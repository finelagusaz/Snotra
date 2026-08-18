# plan: #1122 — `Engine::config` を src-tauri の禁止集合へ載せる（`#[expect]` 形）

ブランチ: `chore/config-read-lint`
調査: `workspace/research.md`（計測 M1〜M5・敵対的調査の採否を含む）
裁定: 2026-08-17 にユーザーが「採択・`#[expect]` 形」を選択（§人間レビューに逐語）

## 目的

#1032 / #1076 の条項「config の live-read は `egui_shell::read_config` を通す。`engine.lock()` を経てはならない」は、機構を持たない規範として運用されている（条項自身が受容残余と明記）。**#1076 で塞いだ穴は 2 サイクル見えなかった**——engine 錠越しの config 読みは検索結果を正しく返し、worker が走っている間だけフレームが返らないので挙動テストにも目視にも現れない。同型の次の 1 件も同じく沈黙する。

`src-tauri/clippy.toml` の `disallowed-methods` へ `snotra_core::engine::Engine::config` を載せ、**規範に機構を与える**。正当な実使用には `#[expect(clippy::disallowed_methods, reason = …)]` を置き、条項が既に課している「呼び出し元を辿ってスレッドを判定する」義務の結果をその場に残す（＝抑制ではなく分類の記録）。**副作用として、注釈は禁止が実効を失ったことのカナリアになる**（M3 / M4 で実測）。

## 受け入れ条件

1. `cargo clippy --workspace --all-targets -- -D warnings` が exit 0（注釈が付いた地点は fulfilled）
2. **新しい違反**を注入すると赤い（例: `egui_shell/` の関数に `engine.lock().unwrap().config()` を 1 行足す＝ M1 と同型で、**これが本来防ぎたい回帰の姿である**）
3. **禁止行を消す**と、注釈側が `unfulfilled_lint_expectations` で赤い（M3 の再現）
4. **禁止行のパスを書き損じる**と赤い（M4 の再現。`G-clippy-disallowed` が原理的に見られない残余がここで閉じる）
5. `npm run governance:check` が exit 0。かつ **`REQUIRED_DISALLOWED_METHODS` から当該行を消す**と G-clippy-disallowed が赤い
6. カテゴリ A + E が全緑（`cargo fmt --check` / `check --workspace` / `clippy` / `test -p snotra` / `cargo doc` / `npm test`）
7. `src-tauri/CLAUDE.md` の当該条項に「機構は無い」という現状と食い違う文が残っていない
8. `clippy.toml` の死経路 3（fingerprint）に、2026-08-17 の観測との食い違いが日付つきで残っている

## 変更ファイルと対象シンボル

| # | ファイル | 変更 |
|---|---|---|
| 1 | `src-tauri/clippy.toml` | 群 3 のコメント節を新設 + `disallowed-methods` へ 1 行 + 死経路 3 へ日付つき追記 + **34 行の全称の修正**（下記） |
| 2 | `scripts/governance/checks/G-clippy-disallowed.mjs` | `REQUIRED_DISALLOWED_METHODS` へ 1 行 + 「守る命題」節を群 1 限定から一般化 + 34 行の fingerprint 機序の再述を正本参照へ寄せる |
| 2b | `scripts/governance/checks/G-clippy-disallowed.test.mjs` | **`CLIPPY_OK` fixture へ 9 件目**（定数だけ足すと `npm test` が赤・実測）+ 16 行の「7 エントリは」を件数なしの形へ |
| 2c | `snotra-core/src/engine.rs` | `Engine::config` の rustdoc へ「製品 crate では `clippy.toml` が禁じている」を 1 行（**改名沈黙への唯一の予防**。先例は `SearchEngine::sorted_by_path` の doc が同じことを書いている・`search.rs:487`） |
| 3 | `src-tauri/src/commands/icon.rs` | `ensure_icon_cache_loaded_if_enabled`（17 行）へ `#[expect]` + 分類理由 |
| 4 | `src-tauri/src/commands/launch.rs` | `resolve_opener`（107 行）/ `resolve_all_openers`（158 行）へ同上 |
| 5 | `src-tauri/src/config_watcher.rs` | `apply_config_change`（87 行）へ同上（**理由の軸が他と違う**——スレッドではなく「適用手続きの一部ゆえ射程外」） |
| 6 | `src-tauri/CLAUDE.md` | 「モジュール構成」の当該条項（57 行）の「**機構は無い**」以降を書き換え |

**`clippy.toml:34` の全称について**: 群 1 のブロックにある「必要になったら `#[allow(clippy::disallowed_methods)]` + 理由コメントで開けること——**それがこの機構で sanctioned な唯一の解消手段である**」は、主語が「この機構」ゆえ群 3 で `#[expect]` を採ると偽になる。**群 1 に限定するか、`#[expect]` を含める形へ書き直す**（`AGENTS.md`「全称表現は前提条件とセットで書く」）。既存の `view.rs:516` の `#[allow]` は**触らない**——群 1 のセーフティネット変更であり本 issue の射程外。

**触らないもの**（根拠つき）:

- `docs/adr/ADR-config-read-exception-discriminator.md` — 案 G′ は「#1122 で決める」と書いてあるが、**ADR は凍結された歴史ゆえ本文を直さない**（`ADR-adr-frozen-history`）。裁定の結果は生きた層（`clippy.toml` の群 3 コメント + `src-tauri/CLAUDE.md` の条項）と issue のクローズが持つ。
- `docs/adr/ADR-clippy-disallowed-enforcement.md` — 同じ理由。fingerprint の残余（39 行）もここでは直さない。
- `docs/architecture.md` — 231 行が「例外の弁別子と射程は `src-tauri/CLAUDE.md` の当該条項が正本——**ここに言い換えを置かない**」と自ら宣言している。機構の有無はその射程内なので変更不要。
- `docs/build-commands.md` — 30 行は既に「禁止を足すときはカナリアへも足すこと（正本は `src-tauri/clippy.toml` 冒頭）」を持ち、件数を書いていないので腐らない。
- `SPEC.md` — 挙動を 1 つも変えない（lint 設定と注釈のみ・`Engine::config` の呼び出し点は増減しない）ため同期不要。
- `snotra-core/src/engine.rs` の `config()` — 可視性は変えない（`src-tauri` に正当な呼び出しが残る・ADR 案 G）。

## 実装順序（中間状態が赤くなることを前提に組む）

**どちらの順でも中間状態は必ず赤い**——禁止行だけ入れれば注釈のない地点が違反になり、注釈だけ入れれば expectation が未達になる。ゆえに順序は「赤の件数が単調に減る」向きに固定し、**PostToolUse hook の赤を追いかけないこと**を明記する。

1. `clippy.toml` へ群 3 のコメント + 配列 1 行（**この編集は hook を 1 つも起動しない**——`selectChecks` が `[]` を返す。沈黙は「何も走らなかった」であって合格ではない）
2. `commands/icon.rs` へ `#[expect]`（hook の clippy は**残り 3 件**で赤い。想定どおり）
3. `commands/launch.rs` へ `#[expect]` × 2（hook は**残り 1 件**で赤い）
4. `config_watcher.rs` へ `#[expect]`（ここで hook が緑になる）
5. `G-clippy-disallowed.mjs` へ登録 + 命題節の一般化 + 34 行の整理
6. `G-clippy-disallowed.test.mjs` の `CLIPPY_OK` fixture へ 9 件目 + 16 行の件数を外す（**5 と 6 は 1 つの単位である**——定数だけ足すと `npm test` が赤い）
7. `snotra-core/src/engine.rs` の `Engine::config` の rustdoc へ 1 行
8. `src-tauri/CLAUDE.md` の条項を書き換え
9. `clippy.toml` の死経路 3 へ日付つき追記 + 34 行の全称を修正
10. 注入で実測（下記「テスト方針と検証」）

## 不変条件と異常系

- **`#[expect]` は「違反が在ること」の主張である。** 違反が消えれば赤くなる（M3）。ゆえに #1123 が例外を `read_config` へ移すとき、**古い注釈を残したままにはできない**（機構が拒む）。これは意図した性質であり、`#[allow]` を選ばなかった理由そのものである。
- **機構は規範より狭い。** 捕まえるのは `Engine::config` の直呼びだけで、`engine.lock()` 越しに `config_handle()` を取り直す形（issue の穴 1）は同じだけ待つのに捕まらない。**ただしそれは #1032 の射程（engine 錠越しの読み）と一致する**。群 1 の条項に「規範は機構より広い」と書く先例があるので、同じ形で明記する。
- **受容する残余（新しく増えるもの）**: `#[expect]` の赤は `-D warnings` に依存する。`ci.yml`（191 行）と `.claude/hooks/post-edit.mjs`（325-326 行）はどちらも渡すので現状は両方で赤いが、**`cargo check` では診断そのものが評価されない**（rustc は `clippy::` ツール lint の expectation を見ない・M5）。ユーザー裁定により `[workspace.lints.rust] unfulfilled_lint_expectations = "deny"` は**足さない**（変更面を狭く保つ）。この非対称を群 3 のコメントへ書く。
- **受容する残余（既存のもの）**: 新しい違反を「`#[expect]` を写して塞ぐ」逃げ道は残る。これは lint に内在する性質であり、群 1 の `#[allow]` でも同じ（`G-clippy-disallowed` の射程外節が既にそう書いている）。**注釈が要求するのは分類の記録であって、正しい分類ではない。**
- **機構が見るのは綴りであって害ではない。** `Engine` の内側で config を読む他のメソッド（`search` / `recent_history` / `begin_index_drain` 等）を `engine.lock()` 越しにイベントループから呼べば同じ待ちが同じだけ乗るが、lint は無診断である。**規範の害（フレームが返らない）に対して、機構は `Engine::config` という綴りしか見ていない。**
- **記録した分類は黙って腐りうる。** 条項は「同じ関数が両方から呼ばれるようになれば分類は変わる」と要求するのに、`#[expect]` を置いた関数へイベントループ側から新しい呼び出し元が増えても何も鳴らない（`resolve_opener` は `pub` な `launch_item_with_state` 経由で、`resolve_all_openers` は `pub` で egui 側から呼べる形にある）。**この足に検知器は無い**——判定の責務は条項が読者に置いたままである。
- **数を書かない。** 例外地点の件数（現在 4）は #1123 で変わりうるので、コメント・条項のどちらにも件数を書かず「注釈を持つ地点」と書く（`clippy.toml` 冒頭の既存規則に従う）。

## テスト方針と検証コマンド

ユニットテストは追加しない（変更は lint 設定と注釈のみで、製品の挙動を 1 つも変えない）。**保証は注入で測る**——`clippy.toml` 冒頭が「パスを足す・変えるときは、その場で違反を注入して赤くなることを必ず測ること」を要求している。

`.claude/rules/safety-nets.md` の「欠落のパターンごとに検算する」に従い、**足ごとに壊して測る**:

| 足 | 注入 | 期待 |
|---|---|---|
| 本来防ぎたい回帰 | `egui_shell/` の関数へ `engine.lock().unwrap().config()` を 1 行足す | clippy 赤 |
| 分類の記録が効いていること | `#[expect]` を**1 件ずつ**外す | clippy 赤（**その 1 件だけ**。4 件同時の形は M1 で済んでいるので、分解はここで測る） |
| 禁止の消失 | `clippy.toml` の当該行を消す | clippy 赤（unfulfilled・M3） |
| パスの腐り | 当該行のパスを書き損じる | clippy 赤（unfulfilled・M4） |
| カナリアの消失 | `REQUIRED_DISALLOWED_METHODS` の当該行を消す | `governance:check` 赤 |
| コメントアウト | `clippy.toml` の当該行を `#` で潰す | `governance:check` 赤 |
| fixture とカナリアの対応 | 定数へ 9 件目を足して `CLIPPY_OK` fixture を据え置く | `npm test` 赤（`G-clippy-disallowed.test.mjs`） |

**注入の作法**（**実装をコミットしてから注入する**——2026-08-17 に、未コミットの実装が載るファイルへ注入し、撤去に `git checkout -- <path>` を使って `#[expect]` 4 件を全損した。HEAD へ戻るので注入だけでなく実装ごと消える。リポジトリの記録にある #934 と同じ型である）: `clippy.toml` だけを触った回は `.rs` を touch するか `cargo clean -p snotra` を挟む（同ファイルの死経路 3）。**2026-08-17 の観測では touch なしでも診断が出たが、機序を測っていないので手順は従来どおり残す**（上位集合を満たすので安全側）。**稼働中のガードを弱める向きの変異（禁止行の削除・書き損じ・コメントアウト）は複製に当てる**——`clippy.toml` を scratchpad へ複製して変異させ `CLIPPY_CONF_DIR` を向ける（`.claude/rules/safety-nets.md`「複製に変異を当てる」）。作業ツリーへ足す向きの変異（新しい違反・`#[expect]` の一時除去）は作業ツリーで行い、`git checkout -- <個別パス>` で戻して `git status --short` で残っていないことを確認する。

**層と層の隙間**（`docs/development-principles.md`「検証の層と、層と層の隙間」の問い (2)）: この機構の出力を消費する層は 2 つ——CI の `rust-check`（`ci.yml:191`）と PostToolUse hook（`post-edit.mjs:325-326`）で、**どちらも `-D warnings` を渡す**。届いていることは実装の途中で自然に測れる——実装順序 2〜4 の中間状態では hook の clippy が赤くなり、その診断が会話に届くはずである（届かなければ、その時点で消費層の欠陥として扱う）。**届かない層も名指しておく**: `cargo check --workspace` は `clippy::` ツール lint の expectation を評価しないので、カナリアはそこに現れない（M5・受容残余）。

コマンド（`docs/build-commands.md` カテゴリ A + F）:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra
cargo doc --workspace --no-deps --document-private-items
npm test                  # scripts/ を変更するため必須（カテゴリ E）
npm run governance:check
```

カテゴリ C（smoke）/ D（目視）は不要——UI の挙動・trace イベント名・hotkey・表示経路のいずれも変えない。

## 未確定（実装前に潰す）

- [x] **裁定: 4 件の注釈を抑制と見るか記録と見るか** — 2026-08-17 にユーザーが「採択・`#[expect]` 形」を選択。決め手は M4（注釈が禁止の実効性のカナリアになる）と、`style_ui` 前例との差が件数ではなく**呼び出し点ごとの判定義務の有無**であること。
- [x] **`unfulfilled_lint_expectations` を deny 化するか** — 足さない（ユーザー裁定）。理由: `ci.yml` と hook はどちらも `-D warnings` を渡すので現状は両方で赤く、全 crate に掛かる新しい節を増やす費用に見合わない。`cargo check` で見えないことは残余として群 3 のコメントへ書く。
- [x] **#1123 との順序** — 独立に進める（ユーザー裁定は #1123 を待たない形）。件数が 4 → 1 に減っても「逃げ道を打鍵させる」形は同じなので、裁定は件数で動かない。**#1123 本文の「#1122 の機構化が `#[allow]` 無しで済む」は不正確**（`config_watcher` が射程外として残る）——PR 本文チェックリストで issue コメントによる訂正へ送る。
- [x] **fingerprint の食い違いの扱い** — この PR で `clippy.toml` の死経路 3 へ日付つき追記（ユーザー裁定）。**写しの母集団は grep で数え上げた**: 生きた層は `clippy.toml:46` と `G-clippy-disallowed.mjs:34` の 2 か所、凍結層は `ADR-clippy-disallowed-enforcement.md:39`。`docs/build-commands.md:242` は**別の話**（バイナリの hardlink）で母集団外。追記は正本（`clippy.toml`）にだけ置き、`.mjs` 側は機序の再述をやめて正本参照へ寄せる（同じ事実を 2 か所へ書かない）。
- [x] **`Engine::config_handle` を禁止に含めるか** — 含めない。製品の呼び出し点は `main.rs:242` の 1 件のみ（`state.rs:79` / `commands/system.rs:45` は `#[cfg(test)]` のヘルパー・実測）で、いずれも `AppState` 構築時ゆえフレーム経路に無い。禁止すれば構築そのものが不能になる。群 3 の「含めなかったもの」へ書く。

## フェーズごとの作業項目

### Phase 1 — 機構

- [x] `src-tauri/clippy.toml` に群 3 のコメント節を書く（守る命題と前提 / 含めなかったもの（`config_handle`・`AppState.config` 経由の読み）/ 注釈が記録である理由 / 実測 M1〜M5 の要点 / `-D warnings` 依存の残余 / 規範は機構より広いこと。**件数は書かない**）
- [x] `disallowed-methods` 配列へ `snotra_core::engine::Engine::config` を追加（`reason` に代替手段 `egui_shell::read_config` を書く）
- [x] `scripts/governance/checks/G-clippy-disallowed.mjs` の `REQUIRED_DISALLOWED_METHODS` へ群 3 のコメント付きで追加
- [x] 同ファイル冒頭「守る命題」を群 1 限定の書き方から、禁止集合の各群を覆う形へ一般化（**数を書かない**）
- [x] `G-clippy-disallowed.test.mjs` の `CLIPPY_OK` fixture へ 9 件目のエントリを足す（**定数だけ足すと `npm test` が赤い**）
- [x] 同ファイル 16 行の「**7 エントリ**は…」（群 2 の時点で既に腐っている）を件数なしの形へ直す

### Phase 2 — 注釈（分類の記録）

- [x] `commands/icon.rs:17` へ `#[expect]` + 理由（icon worker スレッド）
- [x] `commands/launch.rs:107` / `:158` へ `#[expect]` + 理由（tray／platform スレッド）
- [x] `config_watcher.rs:87` へ `#[expect]` + 理由（**適用手続きの一部ゆえ射程外**——軸が他と違うことを文言で書き分ける）
- [x] 4 か所すべてで、正本（`src-tauri/CLAUDE.md` の当該条項）を指す
- [x] 属性は**文（`let`）段**に置く（関数段にしない——関数へ新しい読みを足したときに黙って覆わないため）
- [x] `clippy.toml` の `reason`（パスごとに 1 つしか書けない）は**条項へのポインタに留め**、分類は各 `#[expect]` の `reason` に書く

### Phase 3 — 規範の同期と副産物

- [x] `src-tauri/CLAUDE.md` 57 行の「**機構は無い**——`engine.lock().config()` は今もコンパイルが通るので…」を、機構が在ることと**その射程**（直呼びだけ・`config_handle` 取り直しは通る）・注釈が分類の記録であることへ書き換える。「この条項が `commands/` も覆うようになったぶん、その残余は以前より広い面に掛かる」も現状と整合させる
- [x] `clippy.toml` の死経路 3 へ 2026-08-17 の観測を日付つきで追記（**機序は主張しない**）
- [x] `G-clippy-disallowed.mjs:34` の fingerprint 機序の再述を正本参照へ寄せる
- [x] `clippy.toml:34` の全称「それがこの機構で sanctioned な唯一の解消手段である」を、群 1 に限定するか `#[expect]` を含める形へ直す
- [x] `snotra-core/src/engine.rs` の `Engine::config` の rustdoc へ「製品 crate では `clippy.toml` が禁じている」を 1 行（改名沈黙への予防・先例は `SearchEngine::sorted_by_path` の doc）

### Phase 4 — 注入で実測

- [x] 「テスト方針と検証コマンド」の表の**全 7 足**を注入して赤を確認し、結果を本ファイルへ追記（コマンド・診断・exit code）。ガードを弱める向きの変異は複製 + `CLIPPY_CONF_DIR` で行う
- [x] 全注入の撤去後に `git status --short` が clean であることを確認
- [x] カテゴリ A + E + F を全実行して緑を確認（**`clippy.toml` の編集後は hook が沈黙するので clippy は手で走らせる**）
- [ ] 実装差分を確定させる（上記が全緑であること）

## 注入の実測（Phase 4・2026-08-17 / rustc 1.97.1 / clippy 0.1.97）

正準形は `cargo clippy -p snotra --all-targets --message-format short -- -D warnings`。**`-p snotra` に絞るのは計器の要件である**——下記の「計器の欠陥」を参照。

| 足 | 注入 | 観測 |
|---|---|---|
| 1. 本来防ぎたい回帰 | `egui_shell/mod.rs` の `read_config` の中を `s.engine.lock().unwrap().config()` へ書き換える | `egui_shell\mod.rs:430:51: error: use of a disallowed method` |
| 2. 分類の記録 | `#[expect]` を **1 件ずつ**外す（4 回） | 外した 1 件だけが error（`icon.rs:17:26` / `launch.rs:107:22` / `launch.rs:162:22` / `config_watcher.rs:87:51`）。各回とも復元を `git status --porcelain` が空であることで確認 |
| 3. 禁止の消失 | 複製 + `CLIPPY_CONF_DIR` で禁止行を削除 | `error: this lint expectation is unfulfilled` × 4・`could not compile` |
| 4. パスの腐り | 複製 + `CLIPPY_CONF_DIR` でパスを `Engine::confgi` へ | `warning: … does not refer to a reachable function`（それ自体は exit 0 の既知経路）**＋ unfulfilled × 4** |
| 5. カナリアの消失 | メモリ複製で `clippy.toml` から当該エントリを削除し `checkClippyDisallowed` を直接呼ぶ | `disallowed-methods に snotra_core::engine::Engine::config が無い` |
| 6. コメントアウト | 同上・当該行を `#` で潰す | 同上 |
| 7. fixture の据え置き | 定数へ 9 件目を足し `CLIPPY_OK` を据え置いて `npx vitest run` | `Tests 19 failed | 15 passed`（`expected 8 to be 9`） |

**計器の欠陥を 1 つ見つけた**（足 3 の初回で発覚）: `CLIPPY_CONF_DIR` は **workspace 全 crate に効く**ため、通常は `clippy.toml` を持たない `snotra-core` / `snotra-settings` が群 1・群 2 の禁止に当たって先に落ち、`snotra` の診断まで到達しない（`unfulfilled` が 0 件に見えた）。**測る枝と変更が触る枝が違った**形であり、`-p snotra` に絞って測り直した。**複製 + `CLIPPY_CONF_DIR` で注入するときは必ずパッケージを絞ること。**

**事故を 1 件起こした**（記録として残す）: 足 2 の初回で、**未コミットの実装が載るファイルへ注入し、撤去に `git checkout -- <path>` を使って `#[expect]` 4 件を全損した**。`checkout` は HEAD へ戻すので、注入だけでなく実装ごと消える。テキストが会話に残っていたため再適用で回復したが、**注入は実装をコミットしてから行うこと**（リポジトリの記録にある #934 と同じ型）。

## code-reviewer（4b・ラウンド 1）

Critical / High **0 件**。Medium 2 / Low 4 / ⚠️ 5。**製品の挙動に関する指摘は 0 件で、全件が散文の精度**である。

| # | 指摘 | 対応 |
|---|---|---|
| M1 | 「群 3 だけは前提が閉じている」に前提が 1 つ足りない——**注釈を持つ地点が 1 つ以上残る間だけ**であり、全例外が移行すれば注釈ごと消えて群 1・2 と同じ沈黙へ戻る | 採用。`clippy.toml` 群 3 と `G-clippy-disallowed.mjs` の両方へ前提を追記。**規範が成功した瞬間に計器が黙る形**（`instrument-breaks-when-the-fix-lands` と同型）であることも書いた |
| M2 | 全称を縮めた結果、**群 2 の解消手段が無指定**になった | 採用。「群 2 には解消手段を定めていない——製品に正当な使用が生じない設計だから」を明記 |
| L1 | 追記した実例（`apply_exact_hit_test_style`）が、同じ段落の「歴史上そこに style 書き込みは一度も無い」を反証している | 採用（`view.rs:527` で一次確認）。「#900 の時点では」と時点を明示し、成り立たなくなったことを書いた |
| L2 | 「唯一の実使用」は無監視の数え上げ | 採用。数え上げを落として名指しの例示へ |
| L3 | `engine.rs` の rustdoc が「誰も捕まえない」と読める | 採用。注釈が残る間は clippy が鳴ること・その条件を併記 |
| L4 | 群 2 → 群 3 の境界に区切りが無く、全群に掛かる段が群 2 固有に読める | 採用。区切りを入れ、当該段に「全群に掛かる」と明記 |
| ⚠️4 | `docs/build-commands.md:30` の「`-D warnings` のおかげではない」を読者が群 3 全体へ一般化しうる | 採用。1 節追加。**その追記で `unfulfilled_lint_expectations` を綴ったところ `governance:check` が赤になった**（この repo のソースに存在しない識別子＝ deny 化しない裁定の帰結）ので、識別子を外した表現へ直した |
| ⚠️5 | `config_handle` の除外理由に**呼び出し元の数え上げ**が入っている（`docs/comment-guidelines.md`「書かないもの」） | 採用。構造的理由（禁止すると構築が不能）だけに縮めた |
| ⚠️2 | エイリアス経由の呼び（`let f = Engine::config;`）が未測定 | **測った**。`launch.rs` へ注入したところ当該行が error になる＝**捕まる**。散文の「直呼びだけ」は狭すぎたので、`clippy.toml` と `src-tauri/CLAUDE.md` の両方を「綴りへの参照（パス式も含む）」へ直した |
| ⚠️1 | トレイのスレッド分類は #1076 / #1125 の裁定に依拠しており本レビューで再測していない | 対応不要（分類の軸そのものは本 issue の対象外。呼び出し元の一意性は再測済みと明記されている） |
| ⚠️3 | `plan.md` の最後の `- [ ]` が未チェックだと `gh pr create` が #749 のガードで止まる | 実装差分の確定後に閉じる（この行がその項目である） |

## plan-review 結果

- リスク: **高**（セーフティネット＝`clippy.toml` / `G-clippy-disallowed` / 規範文書を変更する）
- レビュー方式: 独立導出 1 体（Step 2b。`workspace/` の読み取りとリポジトリ全体 grep を禁じ、issue の WHAT だけを渡した）
- エージェント数: 2（3b の敵対的調査 1 体 + 本レビュー 1 体）
- 成果物: `workspace/plan-review-config-read-lint.md`（対象 issue・導出ファイル・シンボル・3 分類を含む。API エラーで枠自体は異常終了したが、成果物は完成済みだったため再起動しなかった）

### 要対処（すべて根拠を再照合したうえで採用）

- **`G-clippy-disallowed.test.mjs` の fixture** — 計画へ追加（変更ファイル 2b・Phase 1）。根拠を実物で確認: `CLIPPY_OK` が REQUIRED 全件をリテラルで持ち（14-35 行）、カナリアが実 `clippy.toml` に対して `toHaveLength(REQUIRED_DISALLOWED_METHODS.length)` を課す（196 行）。**定数だけ足すと `npm test` が赤い。**
- **`clippy.toml:34` の全称** — 計画へ追加（Phase 3）。「それがこの機構で sanctioned な唯一の解消手段である」の主語が「この機構」ゆえ `#[expect]` を採ると偽になる。
- **`Engine::config` の rustdoc** — 計画へ追加（変更ファイル 2c・Phase 3）。先例を実物で確認: `SearchEngine::sorted_by_path` の doc（`search.rs:487`）が「そこは `src-tauri/clippy.toml` が製品 crate で禁じている」を書いている。**改名沈黙（禁止パスが解決しなくなっても全層が緑）への唯一の予防。**
- **射程外の 2 件**（`Engine` 内側の config 読み・記録した分類が黙って腐ること）— 不変条件の節へ追加。
- **注入は複製に当てる**（`CLIPPY_CONF_DIR`）— 検証節の作法を修正。
- **`npm test` の欠落** — 検証コマンドへ追加。

### 軽微

- `G-clippy-disallowed.test.mjs:16` の「7 エントリ」は群 2 の時点で既に腐っている——fixture を触るついでに件数なしの形へ（Phase 1）。
- `clippy.toml` の群 2 の全称「製品 crate から届く綴りは `Engine::sorted_by_path` の 1 本だけ」は群 2 に閉じており偽にならない（読んで確認）。

### 降格（要対処 → 却下・理由つき）

- **ADR 案 G′ の更新** — 導出は「あの行は未決の宣言であって歴史ではない」と主張したが、`ADR-adr-frozen-history` が「ADR 本文は決定日時点の世界の記述として凍結」と定め、**覆された記述を持つ `ADR-stale-identifier-detector-scope` を「凍結ゆえ編集しない——それ自体が本契約の初適用である」として実際に据え置いた先例**を持つ。よって触らない。裁定の結果は生きた層（`clippy.toml` の群 3・`src-tauri/CLAUDE.md` の条項）と issue のクローズが持つ。

### 未検証（PR 本文チェックリストへ送る）

- **CI 上での実測**（`rust-check` / `governance-check` job）— `.claude/rules/safety-nets.md` のとおり PR が在って初めて測れる。計画の作業項目に置くと `gh pr create` のガード（#749）と循環する。
- **エイリアス経由の呼び**（`let f = Engine::config;` のようなパス式）が `disallowed_methods` に捕まるか — 未測定。射程外として扱い、実装 PR で 1 度測る。
- **#1123 本文の不正確な記述**（「#1122 の機構化が `#[allow]` 無しで済む」）の issue コメントによる訂正。

### 判断

- 実装着手: **可**（未確定欄は空・人間の承認待ちのみ）

## セルフレビュー

- リスク: 高
- plan-review: 独立導出 1 体
- エージェント数: 1（3b を含めると 2）
- 要対処: 6 件（すべて反映済み）+ 降格 1 件（理由を記録）
- 未検証: 3 件（いずれも PR 本文チェックリストへ送る。理由は上記）

## 人間レビュー

- [x] 承認済み — 2026-08-17 / 問い: "`workspace/plan.md` を読んで、**注釈を追加する**か**明示的に承認**してください。" / 回答: "OK"
