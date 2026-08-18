# 独立導出: #1122（`Engine::config` を src-tauri の clippy 禁止集合へ）

対象 issue: **#1122**（裁定「採択」）。導出はコードと規範のみから行い、`workspace/` 配下は一切読んでいない（`ls` でファイル名だけは見えたが内容は開いていない）。grep は常に `-g '!workspace/**'` 付き。

**母集団を誰が知っているか（自分の決定）**: `#[expect]` を置く地点の母集団は **grep でも issue の表でもなく clippy 自身**である（`.config()` という綴りに一致しない呼び形・`#[cfg(test)]` 内の呼び・re-export 経由を grep は落とす）。そこで **禁止を実際に有効化した clippy の診断**を母集団として採った（下の実測 1）。稼働中のガードは弱めず、`clippy.toml` の複製へ変異を当てて `CLIPPY_CONF_DIR` で差し替えた（`.claude/rules/safety-nets.md`「複製に変異を当てる」）。

## 実測（すべて当セッションで実行・リポジトリのファイルは一切変更していない）

**実測 1 — 母集団の確定（＝`clippy.toml` 冒頭が要求する違反注入そのもの）**
```
# scratchpad/clippyconf/clippy.toml = src-tauri/clippy.toml の複製 + 1 行
#   { path = "snotra_core::engine::Engine::config", reason = "probe #1122" }
cargo clean -p snotra          # clippy.toml は cargo の fingerprint に入らないため必須
CLIPPY_CONF_DIR=<scratchpad>/clippyconf cargo clippy -p snotra --all-targets --message-format=short
```
```
src-tauri\src\commands\icon.rs:17:26: error: use of a disallowed method `snotra_core::engine::Engine::config`
src-tauri\src\commands\launch.rs:107:22: error: use of a disallowed method ...
src-tauri\src\commands\launch.rs:158:22: error: use of a disallowed method ...
src-tauri\src\config_watcher.rs:87:51: error: use of a disallowed method ...
error: could not compile `snotra` (bin "snotra") due to 4 previous errors
error: could not compile `snotra` (bin "snotra" test) due to 4 previous errors
```
- **`error` である**（`warning` ではない）＝ルート `[workspace.lints.clippy] disallowed_methods = "deny"` が新しい群にもそのまま効く。`-D warnings` に依存しない。
- **`src-tauri` のターゲットは bin と bin-test の 2 つだけである**（`src/lib.rs` なし・`src-tauri/tests/` なし・`Cargo.toml` に `[[bin]]`/`[lib]` なし）。両方が同じ 4 件を出したので、**`#[cfg(test)]` 内に追加の呼びは無い**（grep では区別できなかった点）。
- 対照: 変異なしの正準形 `cargo clippy --workspace --all-targets -- -D warnings` は **exit 0**（4 件は複製の設定に由来し、既存の赤ではない）。
- `cargo clean -p snotra` は `target/` にしか触らない（実行中 exe の一部で `os error 5` が出たが、診断が新規に出たこと自体が「replay ではなく実際に適用された」証拠）。

**実測 2 — `#[expect]` の意味論**（scratchpad に別 crate を立てて測定。`[lints.clippy] disallowed_methods = "deny"` + `clippy.toml` で `std::env::var` を禁止し、本番と同じ形を再現）
| 測ったこと | 結果 |
|---|---|
| (a) `let` 文への `#[expect(clippy::disallowed_methods, reason=…)]`（単純形・メソッドチェーン形の両方） | deny された lint を**抑制し、無診断**（`config_watcher.rs:87` と同じチェーン形も可） |
| (b) 抑制対象が無い `#[expect]`（素の `cargo clippy`） | `warning: this lint expectation is unfulfilled` ＋ `= note: <reason>` ＋ `#[warn(unfulfilled_lint_expectations)]` **on by default**、**exit 0** |
| (c) 同じ入力に `-- -D warnings` | `error: this lint expectation is unfulfilled`、**exit 101** |

**実測 3 — カナリアの fixture が load-bearing であること**（`checkClippyDisallowed` を直接呼び、fixture から 1 件抜く＝「定数へ 9 件目を足して fixture を据え置いた」状態と同型）
```
REQUIRED length = 8
full fixture   => []                       # 緑
dropped paths  = 7
dropped fixture=> ["disallowed-methods に snotra_core::engine::Engine::sorted_by_path が無い（…）"]   # 赤
```
**実測 4 — ベースライン**: `npm run governance:check` **exit 0**（`clippy 禁止 8 件`）。

## 要対処

### A. `#[expect]` を置く地点 — 全 4 件（母集団は実測 1 の clippy 診断）

| # | 地点 | 関数 | 走るスレッド（呼び出し元を辿った一次証拠） | 規範上の正当性 |
|---|---|---|---|---|
| 1 | `src-tauri/src/commands/launch.rs:107` | `resolve_opener`（同 103 行） | **platform（Win32 トレイ）スレッド** ← `launch_item_with_state`（`launch.rs:113`・唯一の呼び出し元）← `platform/tray.rs:76`（`handle_menu_command`）← `platform/mod.rs:258` の `GetMessageW` ループ ← `mod.rs:107-116` の `std::thread::Builder::spawn`（`platform_thread_loop`） | 条項の例外「イベントループスレッドを止めない場所での読み」に当たる。待つのは platform スレッドだけ |
| 2 | `src-tauri/src/commands/launch.rs:158` | `resolve_all_openers`（同 154 行） | **platform スレッド** ← `platform/tray.rs:441`（`TrayIcon::show_recent_history_menu` のクロージャ）← `handle_tray_message` ← `platform/mod.rs:248` ← 同 spawn | 同上 |
| 3 | `src-tauri/src/commands/icon.rs:17` | `ensure_icon_cache_loaded_if_enabled`（同 7 行） | **icon worker スレッド** ← `load_icon_pngs`（`icon.rs:50`）← `egui_shell/results_view.rs:214`、これは `spawn_icon_load`（同 205 行付近）の `std::thread::spawn` の内側 | 同上（遅れるのは worker の産物＝アイコンだけ） |
| 4 | `src-tauri/src/config_watcher.rs:87` | `apply_config_change`（同 67 行・呼び出し元は `config_watcher.rs:54` の notify コールバック 1 か所のみ、直前に `thread::sleep(100ms)` を持つ） | **config 監視（notify）スレッド** | **弁別子が他の 3 件と違う**——条項は「外れる理由はスレッドではなく手続きで、あれは読みではなく適用の一部であり、`update_config` と同じ錠の内側で取ることに意味がある」と書く。**reason にはスレッドではなく手続きの分類を書くこと**（スレッドも併記してよいが、正当化の主語は手続き） |

- **属性は文（`let`）段に置く**（関数段ではなく）。実測 2(a) で両方の形が抑制されることを確認済み。文段なら「その 1 つの読みの分類」を記録でき、関数へ新しい読みを足したときに黙って覆わない。
- `reason` には**分類結果**を書く（例: 1・2 は「platform スレッド上の読み。イベントループを止めない（#1076 条項の例外）」、3 は「icon worker スレッド上の読み。同」、4 は「読みではなく適用の一部。`update_config` と同じ錠の内側で取ることに意味がある（同条項）」）。#1122 本文は `#[allow]` と書いているが、裁定は `#[expect]`。

### B. コード以外の変更ファイル（完全な一覧）

1. **`src-tauri/clippy.toml`** — `disallowed-methods` へ 1 行（群 3）＋ 群の見出しコメント。既存の様式（群ごとにコメントで区切る・除外理由の正本をここに置く）に従う。**書くべき内容**: 弁別子が否定形の述語ゆえ型で表せないこと（ADR 案 G）／`#[expect]` を sanctioned な解消手段とすること／**その `#[expect]` の不履行検知は `-D warnings` 依存であること**（要対処 F）。
2. **`src-tauri/clippy.toml:34` の全称の修正（同ファイル内の別件）** — 群 1 のブロックにある「`#[allow(clippy::disallowed_methods)]` + 理由コメントで開けること——**それがこの機構で sanctioned な唯一の解消手段である**」は、主語が「この機構」ゆえ群 3 で `#[expect]` を採ると偽になる。**群 1 に限定して書き直すか、`#[expect]` を含めて書き直す**（規範の全称は前提とセットで書く・`AGENTS.md`「検証の作法」）。
3. **`scripts/governance/checks/G-clippy-disallowed.mjs`** — `REQUIRED_DISALLOWED_METHODS`（52 行〜）へ `"snotra_core::engine::Engine::config"` を追加。登録しないと行の消失・書き損じ・コメントアウトのいずれでも clippy が exit 0 で沈黙する（`docs/build-commands.md:30` が同じ運用を書く）。加えて**同ファイル冒頭の「守る命題」の散文**（15 行付近: 「この検査が緑 ⇒ `Context` 経由の global style 書き込み（#751 / #900）が … error として落ちる」）は群 2（#1067）の時点で既に狭く、群 3 でさらに狭くなる。命題を群に依らない形へ直す。
4. **`scripts/governance/checks/G-clippy-disallowed.test.mjs`** — **issue の表に無い必須ファイル**。`CLIPPY_OK` fixture（20-35 行）は REQUIRED 全件をリテラルで持つ設計で、190-197 行のカナリアが `toHaveLength(REQUIRED_DISALLOWED_METHODS.length)` と両方向で固定する。**定数だけ足すと `npm test` が赤になる**（実測 3）。fixture へ 9 件目のエントリを足し、21 行のコメント（`# 群ごとに…（#751 / #900 / #1067）`）と 16 行の「**7 エントリは**リテラルで書く」（現状すでに 8 件で腐っている）を直す。
5. **`src-tauri/CLAUDE.md`「モジュール構成」の #1032 条項（57 行）** — **この禁止で偽になる散文の本体**。「**機構は無い**——`engine.lock().config()` は今もコンパイルが通るので、UI 層へ新しい読みを足すときは規範として守る」は偽になる。続く「**この条項が `commands/` も覆うようになったぶん、その残余は以前より広い面に掛かる**」も、残余の姿が変わる（残るのは B/§C の射程外だけ）。書き換えは**下限主張**へ倒す（「機構が捕まえるのは engine 越しの `Engine::config` 呼びである。射程はそれだけではなく〜も残る」）——数え上げ・全称を足すと次の反復で腐る。
6. **`docs/adr/ADR-config-read-exception-discriminator.md:26`（案 G′）** — 「**却下ではなく、本サイクルでは決めない。** … **#1122 で決める。**」が現に決まった。ADR は凍結された歴史だが、**この行は未決の宣言であって歴史ではない**——裁定（採択・`#[allow]` ではなく `#[expect]` を「分類の記録」と読む）を書く。#1122 の実装 PR で更新する唯一の ADR。
7. **`snotra-core/src/engine.rs:228-235`（`Engine::config` の rustdoc）** — 先例に従って「製品 crate（`src-tauri`）では `clippy.toml` が禁じている」を 1 行書く。**先例は `snotra-core/src/search.rs:487`**（`sorted_by_path` の契約 doc が同じことを書いている。`Engine::sorted_by_path` 側（318 行）は契約 doc へ委譲する形）。これは**改名沈黙**（`Engine::config` を改名すると禁止パスが解決せず、warning が exit 0 で流れ G-clippy-disallowed は緑・`ADR-clippy-disallowed-enforcement.md:40` の残余 2）に対する唯一の予防である。

**触ってはならない文書と根拠**

- `docs/architecture.md:231` — 当該行は自分で「例外の弁別子と射程は `src-tauri/CLAUDE.md`「モジュール構成」の当該条項が正本——**ここに言い換えを置かない**」と宣言している。写しを持たないので偽にならない。触ると写しが生まれる。
- `PERFORMANCE.md:557〜`「設定の読みを engine lock の外へ出す」— A/B の実測記録。機構の有無に言及していない（値だけ）。測定記録は現在形の主張ではない。
- `docs/adr/ADR-clippy-disallowed-enforcement.md` — 凍結された歴史（#950 の決定）。残余 2・3 は今回そのまま当てはまるので**引く**が、本文は書き換えない（`ADR-adr-frozen-history`）。
- `docs/superpowers/specs/2026-08-09-detector-scope-audit-design.md:278/429` の「禁止メソッドパス **7 件**」「**8 つ目**の禁止対象メソッドを追加しても」— 群 2 の時点で既に腐っている**が**、同 spec 自身が #13 を分類 **③**（「`clippy 禁止 8 件` と数えられるが、8 件目は固定されない」）として件数のドリフトを織り込み済みで、日付つきの設計記録である。**検算済みの非食い違い**として据え置く。
- `docs/build-commands.md:30` — 件数を書いておらず、「禁止を足すときはカナリアへも足すこと」という運用も今回の変更と一致する。読んだが偽になる箇所は無い。
- `snotra-core/CLAUDE.md:192`「engine.rs のロック最小化パターン」— 契約の正本を `config_handle` / `config` フィールドの doc へ委譲しており、機構の主張を持たない。
- `src-tauri/src/egui_shell/view.rs:516` の既存 `#[allow(clippy::disallowed_methods)]`（群 1 の sanctioned な逃げ道）— `#[expect]` へ揃えたくなるが、**#1122 の射程外**（群 1 のセーフティネット変更であり別の合意が要る）。要対処 2 で全称の文言だけ直す。
- **セーフティネットゆえ着手前にチームの合意が要る**: `src-tauri/clippy.toml`・`G-clippy-disallowed*`・規範文書（`src-tauri/CLAUDE.md`）はいずれもルート `CLAUDE.md`「最重要ルール 2」の対象（#1122 本文も「注意」で明記）。

### C. この禁止が捕まえない経路（機構の射程外）

1. **`engine.lock()` 越しの `config_handle()`** — 禁止していないので、同じ錠を同じだけ待つ形が書ける。現在の呼び出し点 3 件（`src-tauri/src/state.rs:79`・`main.rs:242`・`commands/system.rs:45`）はすべて構築時なので実害は無い（#1122 の穴 1 と一致）。
2. **`Engine` の内側で config を読むメソッド**（実測: `snotra-core/src/engine.rs` の `self.config.read()` は 148 / 159 / 164 / 219 / 224 / 234 / 284 / 302 行）。`search` / `recent_history` / `prepare_history_save` / `prepare_history_flush` / `begin_index_drain` / `complete_index_drain` を**イベントループスレッドから `engine.lock()` 越しに呼ぶと、同じ待ちが同じだけ乗る**のに lint は無診断である（現状の呼び出し点はどれもイベントループ外: `tray.rs:34` は platform、`launcher_controller.rs:827` は folder worker の `std::thread::spawn` 内）。**規範の害（フレームが返らない）に対しては、機構は `Engine::config` という綴りしか見ていない。**
3. **機構は「呼び出し点」を止めるが「スレッド」を判定しない** — 条項は「同じ関数が両方から呼ばれるようになれば分類は変わる」と要求するのに、**`#[expect]` を置いた関数へ新しい呼び出し元がイベントループ側から増えても何も鳴らない**（記録した分類が黙って腐る）。`resolve_opener` は `pub` な `launch_item_with_state`（`launch.rs:112`）経由で、`resolve_all_openers` は `pub`（同 154）で、egui 側から呼べる形になっている。**この足に検知器は無い**（規範が「呼び出し元を辿って判定する」責務を読者に置いたまま）。
4. **`#[allow]` による迂回**（関数段・モジュール段・crate 段）。`G-clippy-disallowed.mjs:33` が射程外と明記する lint 内在の性質。`#[expect]` は不履行が鳴るぶんだけ強いが、**`#[allow]` へ書き換えられたら元の穴に戻る**（その差分を見る機構は無い）。
5. **crate 単位の射程** — `clippy.toml` は `src-tauri` にしか無いので、`snotra-egui-runtime` / `snotra-settings` / `snotra-core`（自身のテスト 3 件: `engine.rs:470 / 479 / 584`）は無影響。これは #1032 の射程と一致する意図的な非対称（禁止集合を `[workspace.lints]` へ移せないことの正本は `clippy.toml:70-75`）。
6. **`clippy.toml` は cargo の fingerprint に入らない** — 実測 1 で `cargo clean -p snotra` が必要だった。既知残余（誰も見ていない）。
7. **`Engine::config` の改名・削除** — 禁止パスが解決しなくなっても `clippy.toml` のテキストは 1 文字も変わらず、warning は `-D warnings` でも exit 0、G-clippy-disallowed は緑（`ADR-clippy-disallowed-enforcement.md:40`）。要対処 B7 の rustdoc がこの足の唯一の予防。
8. **`unfulfilled_lint_expectations` が `-D warnings` 依存である**（要対処 F と同一の穴。ここに再掲するのは射程外の一形だから）。

### D. 検証手段（既存の同型機構 2 群が要求する作法に従う）

作法の正本は **`src-tauri/clippy.toml` 冒頭**「**パスを足す・変えるときは、その場で違反を注入して赤くなることを必ず測ること**——G-clippy-disallowed が見るのは `REQUIRED_DISALLOWED_METHODS` が名指す件の在否であって、**そのパスが解決することは見ない**」と、**`.claude/rules/safety-nets.md`**「効いていることはフォールトインジェクションで一度は実測する」「**足ごとに壊して測り、捕まらない足は機構へ載せる**」「稼働中のガードを弱めない——複製に変異を当てる」。足は 5 本ある。

| 足 | 注入 | 期待 | 備考 |
|---|---|---|---|
| 1. 禁止が実効（パスが解決する） | 4 件の `#[expect]` を**1 件ずつ**外す／UI 側（例 `egui_shell/view.rs`）へ `engine.lock().unwrap().config()` を 1 行足す | `cargo clippy --workspace --all-targets -- -D warnings` が**その行で error** | **`cargo clean -p snotra` か `.rs` の touch を挟む**（fingerprint 残余）。実測 1 で 4 件同時の形は済んでいる——**1 件ずつの分解は実装 PR で行う**（`#[expect]` が入った後は 4 件が抑制されるので、注入なしでは何も出ない） |
| 2. `#[expect]` の不履行が鳴る | `#[expect]` を残したまま下の `config()` 呼びを消す（＝読みを `read_config` へ移した将来の姿） | `-D warnings` 付きで **error: this lint expectation is unfulfilled**（実測 2c・exit 101） | **素の `cargo clippy` では warning ＝ exit 0**（実測 2b）。PostToolUse hook は exit code しか見ないので、`-D warnings` が落ちた瞬間にこの足は沈黙する（要対処 F） |
| 3. カナリア（設定の空洞化） | `clippy.toml` の新エントリを消す／`#` でコメントアウト／パスを 1 文字書き損じる | `npm run governance:check` が**赤**（`disallowed-methods に … が無い`） | 実測 3 で同型（`sorted_by_path` を抜いた形）を確認済み |
| 4. deny レベル | ルート `[workspace.lints.clippy]` へ `all = "allow"` を足す／`disallowed_methods` の行を消す | `governance:check` が**赤** | 既存機構（#950）。群 3 を足しても同じ 1 か所が効く |
| 5. fixture とカナリアの対応 | 定数へ 9 件目を足して `CLIPPY_OK` fixture を据え置く | `npm test`（`G-clippy-disallowed.test.mjs`）が**赤** | 実測 3 |

注入は**複製に当てる**（実測 1 のように `CLIPPY_CONF_DIR` へ複製した `clippy.toml` を向ける／変異は作業ツリーに残さない）。足 1 の「1 件ずつ」は `#[expect]` の実物が必要なので実装 PR のチェックリストへ送る。

### E. issue #1122 と現状（HEAD = a4726834、#1125 マージ後）の食い違い

- **行番号は食い違わない**（実測 1）: `commands/launch.rs:107` / `:158` / `commands/icon.rs:17` / `config_watcher.rs:87` は issue 本文と一致。**件数も 4 件で一致**。#1125（config の live-read を engine lock の外へ出し切る）はこの 4 件を動かしていない。**検算済みの非食い違いとして報告する。**
- **issue の表は不完全である**（実装ファイルの数え上げが 2 件足りない）: (a) `G-clippy-disallowed.test.mjs` の fixture（実測 3 で赤になることを確認）、(b) 偽になる散文＝`src-tauri/CLAUDE.md:57`・`clippy.toml:34` の全称・`G-clippy-disallowed.mjs` 冒頭の命題・`ADR-config-read-exception-discriminator.md:26`。
- **issue と裁定で抑制の綴りが違う**（issue: `#[allow]` / 裁定: `#[expect]`）。ADR 案 G′ も `#[allow]` と書いているので、要対処 B6 の更新で綴りごと記録する。
- **`#1123` と対象が重なる**（`ADR-config-read-exception-discriminator.md`「検討していない代替」）: #1123 は「例外を置かない——残る 3 か所（`launch.rs` の 2 つと `icon.rs`）も `read_config` へ移す」を評価する。**採択されれば #1122 の `#[expect]` 4 件のうち 3 件が消え、群 3 のコメントで名指した外延も同時に腐る。** ゆえに群 3 の散文は**件数と地点を数え上げない**書き方にする（足 2 の不履行検知が移行漏れを鳴らす側に回る、という関係も書ける）。

### F. `#[expect]` の自己清掃が `-D warnings` に依存する（決定により意図的に開けた穴）

`unfulfilled_lint_expectations` は **warn 既定**（実測 2b）。裁定でルート `Cargo.toml` へ `[workspace.lints.rust]` を新設しない（deny 化しない）と決めたので、この足が赤くなるのは `.github/workflows/ci.yml:191` と `.claude/hooks/post-edit.mjs:323-326` が `-- -D warnings` を渡している間だけである（両方を読んで確認）。**これは #950 が `disallowed_methods` について塞いだ「沈黙経路 0」と同じ形であり、`G-clippy-disallowed` はこちらを見張っていない**（見張るのは `disallowed_methods` のレベルだけ・同 `.mjs:34` が射程外と明記）。**受容するなら、その残余を `clippy.toml` の群 3 のコメントに名指しで書くこと**（誰も見ていない残余として）。

## 軽微

- `scripts/governance/checks/G-clippy-disallowed.test.mjs:16` の「**7 エントリ**は…」は群 2（#1067）の時点で既に 8 で腐っている。今回 fixture を触るので同時に直せる（数を書かない形へ）。
- `src-tauri/clippy.toml` の群 2 ブロックにある「**製品 crate から届く綴りは `Engine::sorted_by_path` の 1 本だけである**」は群 2 に閉じた全称なので偽にならない（読んで確認）。
- 群 3 の reason 文言は 4 件で 1 種類にならない（3 件はスレッド・1 件は手続き）。`clippy.toml` の `reason` は**呼び出し点ごとではなくパスごと**に 1 つしか書けないので、**パス側の reason は条項へのポインタに留め、分類は各 `#[expect]` の `reason` に書く**のが素直（群 1・2 の様式と同じ）。
- `src-tauri/src/commands/system.rs:45` / `state.rs:79` / `main.rs:242`（`config_handle()`）は今回**触らない**。禁止すると構築時の 3 件へ `#[expect]` が増えるだけで、待ちの実害が無い（射程外 1 として記録するだけでよい）。
- `docs/hooks.md` は今回の変更で偽にならない（PostToolUse の発火一覧はファイル種で決まり、`clippy.toml` の中身に依存しない）。ただし `clippy.toml` 編集時に **hook は `cargo check` も clippy も走らせない**（`.toml` の写像）——実装時に手で正準形を打つこと。

## 未検証

- **実物への `#[expect]` の適用**: ファイルを変更しない制約のため、`#[expect]` の挙動は**同型の probe crate**でしか測れていない（実測 2）。本物の 4 地点でコンパイルが通ること・4 件すべてが抑制されること・`clippy --workspace --all-targets -- -D warnings` が exit 0 に戻ることは実装 PR で測る必要がある（とくに `icon.rs:17` はブロック式の中の `let`、`config_watcher.rs:87` はチェーン形。どちらも probe では通ったが実物では未確認）。
- **足 1 の 1 件ずつの分解**: 実測 1 は 4 件同時（`#[expect]` 不在の現状）である。「各 `#[expect]` を外すとその 1 件だけが赤くなる」は未測定。
- **エイリアス経由の呼び**（`let f = Engine::config; f(&e)` のようなパス式、trait 経由の間接呼び）が `disallowed_methods` に捕まるかは測っていない。射程外 3・4 と同じ足なら実害は小さいが、未確認。
- **CI 上での挙動**（`governance-check` job と `rust-check` job の実測）は PR が無いと測れない（`.claude/rules/safety-nets.md`「CI の実測は PR が在って初めて行える」）。PR 本文のチェックリストへ送るべき項目。
- **`npm test` の実走**: 実測 3 は `checkClippyDisallowed` を直接呼ぶ同型再現であり、vitest 本体（`G-clippy-disallowed.test.mjs`）は走らせていない（定数を変更できないため）。
- **群 3 追加後の `governance:check` の evidence 文言**（`clippy 禁止 8 件` → 9 件）が他の検査の期待値に触れないか。`governance-check.mjs` 側に件数リテラルが無いことは確認していない（`clippyDisallowedCount` は動的だが、呼び出し側の総括行の形は未確認）。
- **`snotra-egui-runtime` / `snotra-settings` が `Engine::config` を呼ぶか**は clippy で測っていない（`clippy.toml` が無いため測っても診断が出ない）。grep では 0 件。
