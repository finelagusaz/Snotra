# research: #1122 — config の live-read 規範を lint で守るか（`Engine::config` を disallowed-methods へ）

対象 issue: #1122（`検討:` ラベル相当の裁定 issue・出所は #1076 の `/simplify` 切り口の深さ枠 A1/A2）
ブランチ: `chore/config-read-lint`
計測日: 2026-08-18（rustc 1.97.1 / clippy 0.1.97・GPD WIN MINI 側かは未記録——本 issue に性能測定は無いので機体は無関係）

## 1. issue の要約

#1032 の規範「config の live-read は `egui_shell::read_config` を通す。`engine.lock()` を経てはならない」は、#1076（PR #1125）で例外の弁別子を「`commands/` の操作時の読み」から「イベントループスレッドを止めない場所での読み」へ書き直し、**射程が `commands/` まで広がった**。同じ条項は「**機構は無い**——`engine.lock().config()` は今もコンパイルが通る」を受容残余として明記している。

#1122 が決めたいのは **1 点だけ**である:

> `src-tauri/clippy.toml` の `disallowed-methods` へ `snotra_core::engine::Engine::config` を載せるとき、正当な実使用 4 件に付く `#[allow]` を「**抑制**」と見るか「**分類の記録**」と見るか。

反対側の前例が同じファイルに在る（`clippy.toml` 17-20 行）——`style_ui` / `settings_ui` を禁止しなかった理由は「正当な使用しか無いので偽陽性になり、**解消手段が `#[allow]` しか残らない＝機構を完全に無効化する逃げ道を正当な理由で打鍵させる**」である。

## 2. 関連ファイル・モジュール・関数（すべて実在を確認済み）

| パス | 役割 | 実測 |
|---|---|---|
| `src-tauri/clippy.toml` | 禁止集合の SSOT。群 1（#751 の 7 メソッド）＋群 2（`Engine::sorted_by_path`・#1067） | 76-86 行が `disallowed-methods` 配列 |
| `scripts/governance/checks/G-clippy-disallowed.mjs` | 禁止集合の空洞化・deny の消失を Node の静的読み取りで検知 | `REQUIRED_DISALLOWED_METHODS`（52-62 行）に 8 件 |
| ルート `Cargo.toml` `[workspace.lints.clippy]`（34-35 行） | `disallowed_methods = "deny"` — `-D warnings` からの独立を与える（#950） | `[workspace.lints.rust]` 節は**存在しない**（rustdoc と clippy の 2 節のみ） |
| `src-tauri/src/commands/launch.rs` `resolve_opener` / `resolve_all_openers` | 例外 2 件（tray スレッド） | 107 行 / 158 行 |
| `src-tauri/src/commands/icon.rs` `ensure_icon_cache_loaded_if_enabled` | 例外 1 件（icon worker） | 17 行 |
| `src-tauri/src/config_watcher.rs` `apply_config_change` | 例外 1 件（**射程外**・読みではなく適用手続きの一部） | 87 行 |
| `src-tauri/src/egui_shell/view.rs` `apply_exact_hit_test_style` | **既存の `#[allow(clippy::disallowed_methods)]` はこの 1 件だけ**（群 1 の sanctioned な解消手段の実使用） | 516 行 + 関数 doc に理由 |
| `src-tauri/CLAUDE.md`「モジュール構成」57 行の条項 | 規範の正本。「機構は無い」「残余は以前より広い面に掛かる」を持つ | 採否どちらでも文面の更新対象 |
| `docs/adr/ADR-config-read-exception-discriminator.md` | 案 G（型）却下・案 G′（lint）＝「#1122 で決める」 | 凍結された歴史（`ADR-adr-frozen-history`）ゆえ**本文は直さない** |

`Engine::config` / `config_handle` の呼び出し点（`grep` ＋ clippy の意味解決の両方で確認）:

- `Engine::config`: **4 件のみ**（上表）。`--all-targets` を付けても増えない＝テストターゲットからの呼び出しは 0 件。
- `Engine::config_handle`: 3 件（`main.rs:242` / `state.rs:79` / `commands/system.rs:45`）。**issue の記述「3 つはすべて構築時」は substance は正しいが精度を欠く**——`state.rs:79` と `commands/system.rs:45` は `#[cfg(test)] mod tests` の中の `test_state()` ヘルパーであり、**製品の構築点は `main.rs:242` の 1 件だけ**である。禁止しない判断がより明確に立つ（製品面が 1 点・すべて `AppState` 構築時であり毎フレーム経路に無い）。

## 3. 計測（違反注入・`clippy.toml` 冒頭が要求している未実施項目）

すべて足場を作って測り、`git checkout --` で撤去済み（`git status` clean を確認）。`clippy.toml` は cargo の fingerprint に入らないため、**toml だけを触った回は毎回 `touch src-tauri/src/main.rs` を挟んだ**（同ファイル冒頭の死経路 3）。

| # | 状態 | 結果 | 判定 |
|---|---|---|---|
| M1 | 禁止行あり・注釈なし | `error` × 4（`icon.rs:17:26` / `launch.rs:107:22` / `launch.rs:158:22` / `config_watcher.rs:87:51`）・`could not compile` | **正の対照が成立**。以降の緑が意味を持つ |
| M1′ | 同上・`-D warnings` **なし** | 同じ 4 件が `error` | 新しい行も #950 の deny 経路に乗る（コマンドラインのフラグに依存しない） |
| M2 | 禁止行あり・4 件に `#[expect(clippy::disallowed_methods, reason = …)]` | clippy exit 0・unfulfilled 0 件・`cargo fmt --check` exit 0 | `#[expect]` は**使える** |
| M3 | 禁止行を消す・`#[expect]` は残す | `error: this lint expectation is unfulfilled` × 4・exit 101 | 禁止が実効を失うと**注釈側が赤くなる** |
| M4 | 禁止行のパスを書き損じる（`Engine::confgi`）・`#[expect]` は残す | `warning: … does not refer to a reachable function`（これ自体は exit 0 の既知経路）**＋ unfulfilled × 4**・exit 101 | **`G-clippy-disallowed` が原理的に見られない残余（「名指したパスが解決し続ける」）が赤に変わる** |
| M5 | 禁止行あり・`#[expect]` あり・非 clippy 経路 | `cargo check --workspace`（`--all-targets` も）unfulfilled 0 件・exit 0／`cargo test -p snotra -q` 292 passed／`cargo doc --workspace --no-deps --document-private-items` unfulfilled 0 件・exit 0 | 素の rustc は `clippy::` ツール lint の expectation を**未達と扱わない**＝ビルドを壊さない |

M3/M4 の**限界**（重要）: unfulfilled の診断は `-D unfulfilled-lint-expectations` implied by `-D warnings` と自ら名乗る。つまり**この赤は `-D warnings` に依存する**——`disallowed_methods` 自身が #950 で獲得した「コマンドラインからの独立」を、`#[expect]` の側は持たない。`ci.yml`（191 行）と `.claude/hooks/post-edit.mjs`（325-326 行）はどちらも `-D warnings` を渡すので**現状は両方で赤い**。独立させるならルート `Cargo.toml` に `[workspace.lints.rust] unfulfilled_lint_expectations = "deny"` を**新設**する必要があり、セーフティネットの変更面がその分広がる（＝裁定の対象に含まれる）。

**`cargo check` では「warn 止まり」ではなく、診断そのものが出ない。** M5 がそれを自分で示している——禁止行が在る状態で `cargo check` を走らせれば clippy lint は一度も発火しないので、rustc から見た expectation は必ず未達である。にもかかわらず出力は 0 件だった。つまり **rustc は `clippy::` ツール lint の expectation を評価しない**（`-D warnings` の有無とは別の話）。ゆえに `#[expect]` のカナリアが現れる場所は clippy 経路（CI の rust-check・PostToolUse hook・手打ちのカテゴリ A の clippy 行）**だけ**である。

**`clippy.toml` の編集は PostToolUse hook を 1 つも起動しない**（`post-edit.mjs` の `selectChecks` を読んで確認）。`isRust` は `.rs` のみ、`CARGO_MANIFEST` は `Cargo.toml` のみ、`config-warn` は `tauri.conf.json` / `config.toml` のみで、`src-tauri/clippy.toml` はどの枝にも当たらず `[]` を返す。**したがってこのファイルだけを編集したあとの沈黙は「合格」ではなく「何も走らなかった」である**（`CLAUDE.md`「フック」の原則そのままだが、禁止集合を触る作業では踏みやすい）。計画の検証手順は clippy を**手で**走らせること。

## 4. 再利用できる既存パターン

1. **`sorted_by_path`（#1067・群 2）** — snotra-core のメソッドを**製品 crate だけ**で塞ぐ形。今回と同型（`Engine::config` は snotra-core 側で、`snotra-core/tests/` は別 crate ゆえ無影響）。群のコメント様式（守る命題・含めなかったもの・死ぬ経路）もここに揃っている。
2. **`view.rs:516` の `#[allow]`** — 群 1 の禁止に対する sanctioned な解消手段が**実際に 1 件使われている**。「注釈は機構の敗北ではなく、機構が要求する分類の記録である」という読みが、既にこのリポジトリで一度採られている（欠陥を原理的に持たない地点であることを関数 doc が論証し、インラインにも 1 行置く形）。
3. **`REQUIRED_DISALLOWED_METHODS` への登録** — 群を足すときの必須手順。登録しないと行の消失・書き損じ・コメントアウトのいずれでも沈黙する（#950）。

## 5. 技術的制約

- **型では表せない**（ADR 案 G）: 弁別子が否定形の述語（「イベントループスレッドを止め**ない**場所」）で、証人型 `EventLoopProof` は肯定側しか証明できない。`Engine` から config を出すのは既に限界（`AppState.config` は同じ `Arc`）、`Engine::config()` の可視性を狭めるのは製品側に正当な呼び出しが残るため不可。
- **機構は規範より狭い**: lint が捕まえるのは `Engine::config` の直呼びだけ。`engine.lock()` 越しに `config_handle()` を取り直す形（穴 1）は同じだけ待つが捕まらない。これは #1032 の射程（＝ engine 錠越しの読み）と一致するので、**「規範は機構より広い」を明記する形は群 1 の条項に先例がある**。
- **`clippy.toml` は cargo の fingerprint に入らない**（既知残余・誰も見ていない）。今回新しく増える残余ではない。
- **セーフティネットの変更**: `clippy.toml` と `G-clippy-disallowed` の両方が該当。ルート `CLAUDE.md`「最重要ルール 2」により**着手前にチームの合意が要る**。`.claude/rules/safety-nets.md` の手順も適用。

## 6. 兄弟 issue との関係

- **#1123（例外を無くす）**: 採択されれば例外 3 件が `read_config` へ移り、本 issue の注釈は **4 件 → 1 件**（`config_watcher`）になる。**ただし 0 件にはならない**——#1123 は `config_watcher` を「別勘定・本 issue の対象ではない」と明示的に射程外にしているため、`Engine::config` を禁じる以上そこには注釈が残る。**#1123 本文の「#1122 の機構化が `#[allow]` 無しで済む」は不正確**（issue コメントで訂正する価値がある）。順序としては #1123 → #1122 の方が注釈が減るが、**#1122 の裁定そのもの（抑制か記録か）は件数では決まらない**（1 件でも「逃げ道を打鍵させる」形は同じである）。
- **#1124（`get_instant_commands` の移設）**: `Engine::config` の呼び出し点を増減させないので本 issue と独立。

## 7. 裁定の材料として整理した論点

**同じファイルに在る 2 つの前例の差は何か。** `style_ui` を禁止しなかった理由と、`all_styles_mut` を禁止して `view.rs:516` で開けた理由の差は「件数」ではない。

- `style_ui` の呼び出し者は **`#751` の欠陥を犯していない**——inspector を描きたいだけで、**呼び出し点ごとに何も判定する義務を負っていない**。禁止すると、義務のない人に無意味な注釈を打鍵させる（機構が生む純粋な雑音）。
- `Engine::config` の呼び出し者は、条項が既に **呼び出し点ごとのスレッド分類義務**を課している（「どこで走るかは呼び出し元を辿って決めること」「同じ関数が両方から呼ばれるようになれば分類が変わる」）。注釈はその**既存の義務の結果をその場に残す**形であり、義務を新設しない。

⚠️ この整理を「正当な使用の集合が閉じているから」という形で書くのは弱い（新しい worker が増えれば正当な使用も増えうる・反証容易）。**義務の有無**の差で書くのが妥当と判断した。

**`#[expect]` は「記録」の読みを構造で裏づける**（M2〜M5 で実測）。`#[allow]` は理由が消えても黙って残るが、`#[expect]` は**違反が消えたら赤くなる**（M3）ので、#1123 が例外を移したときに古い注釈が残ることを機構が拒む。さらに **M4 が示すとおり、注釈は禁止が実効を失ったことのカナリアになる**——これは `G-clippy-disallowed` が「緑は含意しない」と自ら宣言している前提 (3)（名指したパスが解決し続ける）を赤へ変える。**ただし `-D warnings` 依存**（§3 の限界）。

## 8. 未解決の疑問（裁定・計画で潰す）

1. **裁定そのもの**（ユーザーの判断が要る・セーフティネット）: 4 件（または #1123 後の 1 件）の注釈を抑制と見るか記録と見るか。
2. `#[expect]` を採るなら、`unfulfilled_lint_expectations` を `[workspace.lints.rust]` で deny 化するか（セーフティネットの変更面が 1 節ぶん広がる）。
3. #1123 との順序（先に #1123 を通せば注釈が 1 件になる）。**裁定を件数で動かさない**なら順序は独立に決めてよい。
4. `config_watcher.rs:87` の注釈の理由文は「スレッド」ではなく「**適用手続きの一部ゆえ射程外**」である（他 3 件と分類の軸が違う）。注釈の文言でこの非対称を書き分けること。

## 9. 敵対的調査（3b）の結果

詳細は `workspace/adversarial-1122.txt`（sonnet 1 体・独立に M1〜M5 を再測定し、一次資料を読み直した）。

### 壊せた項目（2 件・どちらも採用）

1. **§3 の「`cargo check` では warn 止まり」は偽。** 診断そのものが出ない（rustc は `clippy::` ツール lint の expectation を評価しない）。**採用**——ただし機序の裁定は自分で行った: **私自身の M5 がこの主張を反証していた**（禁止行が在る状態の `cargo check` では clippy lint が発火しないので expectation は必ず未達であり、それでも出力は 0 件だった）。§3 に訂正を書いた。所見は正しく、しかも自分の測定の隣に書いた散文が偽だった形である（`own-measurement-refutes-adjacent-prose` と同型）。
2. **`clippy.toml` の編集は PostToolUse hook を 1 つも起動しない**（`selectChecks` が `[]` を返す）。research.md が一切触れていなかった。**採用**——`post-edit.mjs` 125-143 行を自分で読んで確認（`isRust` / `CARGO_MANIFEST` / `config-warn` のどの枝にも当たらない）。§3 に追記した。fingerprint の話（下記 ⚠️）とは別で、こちらの方が作業中に踏みやすい。

### 壊せなかった項目

- **P1**（呼び出し点は正確に 4 件・他 crate は無影響）: 3b が禁止を workspace 全体へ注入して確認。私の M1 とは独立の経路で一致。
- **P2**（`#[expect]` は `check` / `test` / `doc` / `fmt` を壊さない）: 4 コマンドすべてを再実行して一致。
- **P5**（`config_handle` の製品呼び出し点は 1 件・残り 2 件は `#[cfg(test)]`）: コード読みで一致。
- **P6**（#1123 が通っても注釈は 0 件にならない）: #1123 本文が `config_watcher` を明示的に射程外にしていることと突き合わせて一致＝#1123 の当該記述は不正確。
- **P4** は価値判断であって反証可能な事実ではない、と 3b が明言した。**この指摘は妥当**——ゆえに §7 は「実測で裏づけられる部分」（M2〜M5）と「裁定を要する部分」（義務の有無という読み）を分けて書いてある。裁定はユーザーの判断に属する。

### ⚠️ 確信の持てない所見（採用するが、本 issue では扱わない）

**`clippy.toml` が cargo の fingerprint に入らない」という既知残余（同ファイル冒頭の死経路 3・`G-clippy-disallowed.mjs` の射程外節・`docs/build-commands.md`）は、この環境では再現しない。** 3b の指摘を受けて自分で測り直した（2026-08-18・clippy 0.1.97）: 温かいキャッシュで `cargo clippy` が exit 0 を返した直後に **`clippy.toml` だけを編集**して同じコマンドを打つと、`.rs` を touch せず `cargo clean` も挟まずに **4 件の診断が現れた**。

- **観測は採るが、機序は主張しない**（上流 clippy が追随するようになったのか、別の要因かを測っていない）。
- **本 issue の計測は無効化されない**——M1〜M5 は毎回 `.rs` を touch しており、必要条件の上位集合を満たしている。
- **本 issue の対象ではない**が、規範文書 3 か所が「実測」として持つ主張が現行の観測と食い違うので、**別 issue として起票する価値がある**（残余として書かれている以上、次の人はそれを信じて `.rs` を touch し続けるか、逆に「fingerprint に入らない」を根拠に別の判断をしうる）。
- **本 issue で群 3 のコメントを書くなら、死経路 3 を参照する形にしてはならない**（現行観測と食い違う主張を新しい面へ写すことになる）。日付つきの追記で食い違いを残すか、参照しないかの二択であり、前者は同じファイルの編集なので費用は小さい。
