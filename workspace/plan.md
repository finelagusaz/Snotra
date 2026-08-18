# 実装計画: issue #1123 — config の live-read 規範から例外を無くし、機構を lint からコンパイラへ移す

ブランチ: `chore/config-live-read-drop-exception`
調査: `workspace/research.md`（**射程の変更は §0 が正本**）／敵対的調査: `workspace/adversarial-1123.txt`／独立導出: `workspace/plan-review-1123-independent.md`

## 射程について（issue 本文より広い）

**issue #1123 は `config_watcher` の旧 config 読みを「別勘定」として明示的に射程外としている。本計画はそれを射程に入れる。**

ユーザーの裁定による（2026-08-18）。条件は「**例外がなくなってコードで自明になるなら**」であり、3 か所案では自明にならない残りが 3 つある旨（`config_watcher` の `#[expect]` が 1 件残る／guard 内 I/O の禁止は文書契約のまま／`state.config` の直読みを書ける）を提示したうえで、最大値の枝が選ばれた。

**この射程拡大は PR 本文に記録する**（issue 本文が明示的に外していた範囲を入れるため）。

## 目的

config の読みを 4 か所すべて engine 錠の外へ出し、`Engine::config` を `#[cfg(test)]` で閉じる。**規範を守るのが lint ではなくコンパイラになる**ので、条項から例外も射程外も弁別子も `#[expect]` も `clippy.toml` 群 3 も消える。

**得るのは統治だけである。性能は測らない——速くなったとも、遅くなっていないとも主張しない**（4 か所とも egui フレームの外にあり、誰のフレームも止めていない。config `RwLock` の read が増えることも測っていない）。

## 受け入れ条件

1. **製品コードに `Engine::config` の呼び出しがゼロ**になり、`#[expect(clippy::disallowed_methods, …)]` が **4 件 → 0 件**。
2. `Engine::config` が `#[cfg(test)] pub(crate)` になり、**`src-tauri` から綴れない**（型エラーですらなく未定義メソッド）。
3. `clippy.toml` の群 3 と `REQUIRED_DISALLOWED_METHODS` の該当行と `G-clippy-disallowed.test.mjs` の該当 fixture 行が**同じコミットで**消える（`clippy.toml` 群 3 の撤去条件が指定する手順）。
4. 条項に「例外」「射程外」「弁別子」が**1 つも残らない**。残るのは `research.md` §0.5 の全項目だけ（うち 1 項目は `ADR-config-read-exception-discriminator` への短縮引用——**これを落とすと同 ADR が生きた層から孤立し、`G-adr-citations` は一方向検査ゆえ沈黙する**）。
5. 挙動不変。opener 解決の結果・icon キャッシュのロード条件・config 適用の副作用判定が現行と同一。
6. `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra` / `cargo test -p snotra-core` / `cargo doc --workspace --no-deps --document-private-items` / `npm test` / `npm run governance:check` がすべて緑（`npm test` は `G-clippy-disallowed.test.mjs` の fixture を削るため受け入れ条件である）。
7. **新機構（コンパイルエラー）が実際に効くことを故障注入で 1 度測ってある**（下の「機構の乗り換えは測る」）。
8. 弁別子・例外に言及する散文がすべて追随している。
   **この条件は機構が担保しない**——`#[expect]` の reason 内の見出し参照はバックティックが無く `G-heading-refs` の正規表現に掛からない（独立導出が実測）。担保は Phase 2 の機械分割による突き合わせと、変更ファイル一覧の逐条確認である。

## 機構の乗り換えは測る（`.claude/rules/safety-nets.md`）

**測定済みの機構（clippy 群 3・2026-08-18 実測）を、未測定の機構（コンパイルエラー）へ置き換える変更である。**「コンパイラが守る」は書かれた時点の期待であって測定結果ではない。

手順（**作業項目としては Phase 1 に置く**——ここは中身の定義である）:

- Phase 1 の最後に、`resolve_opener` へ回帰の形（`let _c = state.engine.lock().unwrap().config();`）を**注入**し、`cargo check -p snotra` が**失敗する**ことを確認して戻す。
- **変異が正しい強さであることも見る**: エラーが **E0599（そのメソッドが存在しない）**であることを確かめる。**E0624（private）ではない**——`#[cfg(test)]` はシンボルごと消すからである。別の理由で落ちていれば測っていない。
- **同じ場で「偽の全称」を防ぐ測定も行う**: `engine.lock().unwrap().config_handle().read()` が**今も通る**ことを 1 度測る。通るので、条項に「engine 錠越しの config の読みは書けなくなった」とは**書けない**（受容する残余 3 の根拠）。
- **旧機構との差も記録する**: 旧 lint は `cargo check` では評価されなかった（`clippy::` ツール lint ゆえ）。新機構は `cargo check` で落ちる。
- 稼働中のガードは弱めない——注入するのは**違反側**であり、ガード（`#[cfg(test)]`）には触らない。

## 不変条件と検知手段

| # | 不変条件 | 検知手段 |
|---|---|---|
| I1 | **config の read guard を跨いで I/O も他の錠も取らない。** とくに `is_dir()`（死んだ UNC で最大 21 秒・#524）は guard の外 | 構造（クロージャ形は guard を外へ出せない）＋ doc。**guard の中に I/O を書く形は構造では止まらない**——受容する残余として doc に明記 |
| I2 | icon の 2 値（`show_icons` / `icon_cache_cap()`）を**単一 guard** で読む | doc ＋ 実装（tuple のまま）。`icon_cache_cap()` が純 CPU であることは 3b の P2 が確認 |
| I3 | config guard と `IconCacheState` の錠を**入れ子にしない**（現行の「guard を閉じてから icon lock」を維持） | doc ＋ 実装。両方向の入れ子が無いことは `research.md` §3.3 で実測 |
| I4 | `config_watcher` の**読みと書きの窓は広がらない** | 現行も `.clone()` の一時値で guard が文末で落ち、書きは後段で取り直す（`research.md` §0.4）。`/race-check` で名指し検算 |
| I5 | **製品から `Engine::config` を呼べない** | **コンパイルエラー**（`#[cfg(test)]`）。上の故障注入で実測する |
| I6 | `Engine::config_handle` は残す（`AppState` の構築が通る） | 構築が壊れればコンパイルエラー |

## 異常系

- `AppState` 不在（`.manage` は `.setup` より前ゆえ理論経路のみ）: 4 か所とも `&AppState` を**既に手に持っている**ので構造的に発生しない。`egui_shell::read_config` の `fallback` は `AppHandle` 側の責務として残る。
- config の read guard が poisoned: 現行 `read_config` と同じく `.unwrap()`。方針を変えない。

## 変更ファイルと対象シンボル

### コード

| ファイル | シンボル | 変更 |
|---|---|---|
| `src-tauri/src/state.rs` | `AppState::read_config`（新設） | `fn read_config<T>(&self, read: impl FnOnce(&Config) -> T) -> T`。契約 doc（I1〜I3）の**正本をここに置く** |
| `src-tauri/src/egui_shell/mod.rs` | `read_config` | `AppState::read_config` への委譲へ。doc を「`AppState` 不在の面倒だけを見る層」へ縮め、契約の正本を指す。**冒頭の「UI が config を読む唯一の口（#1032）」（:410）が偽になる**——口は 2 つ（`&AppHandle` 用と `&AppState` 用）になるので、「**唯一**」の主張を正しい階層へ移す: **read guard を取る地点が 1 つ**である |
| `src-tauri/src/commands/launch.rs` | `resolve_opener` / `resolve_all_openers` | `state.read_config(…)` へ。`is_dir()` は**guard の外に維持**。`#[expect]` 2 件削除。doc の理由を「engine ロックを跨いで I/O しない」→「config の read guard を跨いで I/O しない」へ |
| `src-tauri/src/commands/icon.rs` | `ensure_icon_cache_loaded_if_enabled` | 同上。2 値は単一 guard。`IconCache::load` は guard の外。`#[expect]` 1 件削除 |
| `src-tauri/src/config_watcher.rs` | `apply_config_change` | 旧 config の読みを `state.read_config(\|c\| c.clone())` へ。`#[expect]` 1 件削除。**射程外という分類ごと doc から消す** |
| `snotra-core/src/engine.rs` | `Engine::config` | **`#[cfg(test)] pub(crate)` へ**。doc を「製品から呼べないので閉じた」形へ書き直す（先例は同 crate `search.rs` の `sorted_prefix_len`） |

### 機構（セーフティネット。撤去条件が手順を指定している）

| ファイル | 変更 |
|---|---|
| `src-tauri/clippy.toml` | **群 3 のセクション（86-145）・配列エントリ（158）・その直前の区切りコメント（157）を削除**。加えて**群 1 のコメント内の相互参照（37 行「群ごとに違う——群 3 は `#[expect]` を要求する」）** も削除 |
| `scripts/governance/checks/G-clippy-disallowed.mjs` | (1) `REQUIRED_DISALLOWED_METHODS` から `snotra_core::engine::Engine::config` の行と群 3 コメント（65-66）を削除、(2) **ヘッダ 21-22 行「群 3（#1122）だけは前提 (3) を例外地点の `#[expect]` が補う…」を削除**（消えた節を指す宙吊りの参照になる）、(3) 前提 (3) の括弧「群 2・3 は snotra-core 側の改名が契機」→ 群 2 だけへ |
| `scripts/governance/checks/G-clippy-disallowed.test.mjs` | 緑 fixture の該当 2 行（コメント＋パス・34-35）を削除。**「群を跨いで持つ」性質は群 1・群 2 で保たれる** |

### 規範・文書

| ファイル | 変更 |
|---|---|
| `src-tauri/CLAUDE.md`（当該条項） | 例外・射程外・弁別子を**全部削る**。残すのは `research.md` §0.5 の**全項目**（数を書かない——§0.5 に項目が増えたときこの行だけが腐る）。機構の記述を lint からコンパイラへ |
| `src-tauri/CLAUDE.md:24` | icon 破棄の機序「engine がまだ `show_icons=true` を返す隙に」を追随（`config_watcher.rs:148` との**写しの対**） |
| `src-tauri/src/config_watcher.rs:148-152` | 同上（写しの対の他方） |
| `src-tauri/src/commands/instant.rs:28-31` | doc の**言い換え**（「条項の例外が名指すのは…弁別子はフレームを止めるかである」）を書き換え。**現時点でも条項とずれている**（却下済みの案 E の言い方）ので、その誤りごと消す |
| `src-tauri/src/egui_shell/launcher_controller.rs:494` / `:765` | 「射程と例外は／射程と例外の定義は」→ 射程だけを指す形へ |
| `docs/architecture.md:231` | 「例外の弁別子と射程は」→ 射程だけを指す形へ |
| `docs/build-commands.md:30` | 末尾の「群 3（#1122）が例外地点の `#[expect]` に持たせた足は `-D warnings` に依存する」が指す対象ごと消える。**要否を実読して判定する**（Phase 2 の作業項目） |
| `docs/adr/ADR-config-read-without-exception.md`（新規） | 否定の知識の記録 |

**変更しない**: `SPEC.md`（挙動不変・SPEC に当該挙動の記述なし）、`PERFORMANCE.md`（性能を測らない）、`docs/adr/ADR-config-read-exception-discriminator.md`（**凍結**・`ADR-adr-frozen-history`。「#1123 で評価する」の一節も残す）、`snotra-core/CLAUDE.md`（独立導出が変更不要を裏取り）、`src-tauri/src/egui_shell/window_coordinator.rs`（害の説明とポインタだけで腐らない）、`G-clippy-disallowed.mjs` の**判定ロジック**（データだけ削る）。

### ADR を書く判断（採用）

書く。否定の知識が実在する。

1. 読み口の 3 案のうち 2 案を却下した理由。とくに**案 3（`&AppHandle` へ替える）は到達しない fallback を 3 つ捏造することになり、`icon_cache_cap` の fallback は `Config::default()` の I/O を招く**（独立導出の指摘。こちらの初期の却下理由より強いので採る）。
2. **撤去条件が指定する `pub(crate)` を採らず `#[cfg(test)]` を採った理由**（`pub(crate)` は `dead_code` で赤くなる。撤去条件を書いた時点でこの帰結が織り込まれていない）。
3. 「例外を残す」（＝ #1076 の判断を保つ）を却下した理由。**案 1（直読み）を却下した理由**も含める（独立導出 v2 はこちらを推奨したので、判断が分かれた点として残す価値がある）。
4. **撤去条件そのものが持っていた欠陥 4 件**（`pub(crate)` では `dead_code` が立つ／test fixture が列挙から漏れている／**散文の掃除が一切列挙されていない**／合図「最後の `#[expect]` が消える」は駆動力を持たない）。撤去条件は本件で消えるので、**教訓の置き場は ADR しかない**。
5. 凍結された `ADR-config-read-exception-discriminator` が「#1123 で評価する」と書いており、凍結ゆえ同 ADR は直せない。**答えは新しい生きた記録が持つしかない。** 新 ADR から旧 ADR を短縮引用する（G-adr-citations が見る実在の辺）。

## 実装順序

### Phase 1 — コードと機構（**1 コミット**）

`clippy.toml` 群 3 の撤去条件が「**同じコミットで**」を要求する。加えて新 API の導入と呼び出し点の移行を分けない（`-D warnings` 下で未使用の新 API は `dead_code`）。

- [ ] `AppState::read_config` を `src-tauri/src/state.rs` へ新設し、契約 doc（I1〜I3）の正本をそこに置く
- [ ] `egui_shell::read_config` を委譲へ書き換え、doc の分担を縮める
- [ ] `resolve_opener` / `resolve_all_openers` を移設（`is_dir()` を guard の外に維持）し `#[expect]` 2 件を削除
- [ ] `ensure_icon_cache_loaded_if_enabled` を移設（2 値を単一 guard・`IconCache::load` は guard の外）し `#[expect]` 1 件を削除
- [ ] `config_watcher::apply_config_change` の旧 config 読みを移設し `#[expect]` 1 件を削除
- [ ] `Engine::config` を `#[cfg(test)] pub(crate)` へ落とし、doc を書き直す
- [ ] `clippy.toml` の群 3（セクション＋配列エントリ＋群 1 冒頭からの言及）を削除
- [ ] `G-clippy-disallowed.mjs` のカナリア行と群 3 コメントを削除
- [ ] `G-clippy-disallowed.test.mjs` の緑 fixture 該当 2 行を削除
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` が緑
- [ ] `cargo test -p snotra` / `cargo test -p snotra-core` が緑
- [ ] `npm test`（`G-clippy-disallowed.test.mjs` を含む vitest）が緑
- [ ] `cargo doc --workspace --no-deps --document-private-items` が緑（`///` を大幅改訂するため必須。**hook は発火しない**）
- [ ] **故障注入**（手順は上の「機構の乗り換えは測る」節が正本）: E0599 で落ちること ＋ `config_handle().read()` が今も通ること を測る
- [ ] `/race-check` を実行する（差分ができて初めて母集団が取れる。主論点は I1〜I4）
- [ ] `/dry-check` を実行する

### Phase 2 — 条項の書き換えと散文の追随（**1 コミット**）

- [ ] `src-tauri/CLAUDE.md` の条項を書き換える（例外・射程外・弁別子を全削除。残すのは `research.md` §0.5 の全項目）
- [ ] `src-tauri/CLAUDE.md:24` と `config_watcher.rs:148-152`（**写しの対**）を同じ変更で直す
- [ ] `src-tauri/src/commands/instant.rs` の doc の言い換えを書き換える
- [ ] `src-tauri/src/egui_shell/launcher_controller.rs` の doc 2 か所の語を追随させる
- [ ] `docs/architecture.md:231` の語を追随させる
- [ ] `docs/build-commands.md:30` の群 3 への言及の要否を実読して判定し、必要なら直す
- [ ] **書き換え後の条項を `**…**` で機械分割し、`research.md` §0.5 の全項目と 1 対 1 で突き合わせる**（3b の P4 が破った当の検算を、今度は書いた側で行う）
- [ ] **`ADR-config-read-exception-discriminator` への生きた層からの引用が 1 件以上残っていることを確かめる**（`grep -rn "ADR-config-read-exception-discriminator" --include=*.md --include=*.rs --include=*.toml .`）。`G-adr-citations` は孤立を検知しない
- [ ] **`grep -rn "弁別子|条項の例外|射程外" --include=*.rs --include=*.md --include=*.toml --include=*.mjs .` を打ち、残存が意図したものだけであることを確かめる**（`.md` 限定の grep で偽の全称を書いた失敗の再発防止）
- [ ] `npm run governance:check` が緑
- [ ] `cargo doc --workspace --no-deps --document-private-items` が緑

### Phase 3 — ADR（**1 コミット**）

- [ ] `docs/adr/ADR-config-read-without-exception.md` を書く（決定・却下した 4 点・帰結・受容する残余）
- [ ] `npm run governance:check` が緑（G-adr-file-names / G-adr-citations）

## テスト方針と検証コマンド

**新しいテストは書かない。** 理由:

- 移設対象 4 関数にテスト席が無い（3b の P1 が全数確認し、主エージェントが LSP `findReferences` で取り直した）。席を作るには `AppHandle` と engine の構築が要る。
- **I1〜I4 はタイミング依存で決定的に再現できない**（21 秒の UNC タイムアウトも writer の待ちも）。測れるのは構造である。
- **I5 は新しくコンパイラが持つ**——そしてそれは上の**故障注入で 1 度測る**（テストではなく実測手順として置く）。
- 既存テストが押さえているもの: `AppState.config` と `Engine.config` が同じ `Arc` であること、UI が engine 錠の保持中に config を読めること（独立導出が名指した 2 本）。
- 挙動不変は `find_matching_tools` / `IndexInputs::from_config` の入力と出力が同一であることから従う。

検証コマンド（`docs/build-commands.md` カテゴリ A / F）:

```
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra
cargo test -p snotra-core
cargo doc --workspace --no-deps --document-private-items
npm test
npm run governance:check
```

## 適用する check スキル

- **`/race-check`** — `AGENTS.md`「条件別チェック」に該当。**計画段階では起動しない**（スキル本文が冒頭で明示・#784。母集団も差分が決める）。**Phase 1 の作業項目。** 主論点は I1〜I4。
- **`/dry-check`** — `AppState::read_config` の新規定義。**Phase 1 の作業項目。** 本件の新関数は `egui_shell::read_config` からの**抽出**であり重複を増やす向きではない。
- **`/plan-review --deep`** — 計画段階で実行（下の結果節）。**射程拡大により再実行が必要**（要件・対象ファイル・シンボル・不変条件がすべて変わった）。
- `.claude/rules/safety-nets.md` — 規範文書ゆえ自動配送されないが**手動で全文参照済み**。加えて `clippy.toml` / `scripts/governance/checks/*.mjs` を触るので**編集時に自動配送される**。
- 非該当: `/persistence-check`（永続形式不変）・`/state-check`（UI モード不変）・`/symmetric-check`（対称ペアの新設なし。guard の取得/解放は I1〜I3 で明示）。

## 受容する残余（doc へ書く）

1. **guard の中に I/O を書く形は構造では止まらない。** クロージャ形が保証するのは「guard を外へ持ち出せない」ことだけ。I1 は文書契約のまま残る（現行の条項が既に持つ責務であり、新設ではない）。
2. **`AppState.config` の read guard 取得点を 1 に保つ検知器は無い。** `pub` フィールドなので直読みを書ける。**新しい残余ではない**——現状も同じである。
3. **規範は機構より広い。** コンパイラが塞ぐのは `Engine::config` という綴りだけで、`engine.lock()` 越しに `config_handle()` を取り直す形と、`Engine` の他メソッド（`search` / `recent_history` / `begin_index_drain`）を錠越しに呼ぶ形は同じだけ待つ。**4 か所案でも消えない。**
4. **icon 破棄の受容残余は形が変わらない**（`config_watcher.rs:150-156` が既に記す窓）。窓の原因は排他の欠如ではなく「読みと icon lock を別々に取る」ことであり、そこは触らない。

## 未確定（実装前に潰す）

- [x] 射程を 3 か所と 4 か所のどちらにするか — **4 か所**（ユーザー裁定・2026-08-18）。
- [x] `Engine::config` の可視性をどう落とすか — **`#[cfg(test)] pub(crate)`**。`pub(crate)` だけでは移設後に非テストの読み手がゼロになり `dead_code` で赤くなる。先例は `snotra-core/src/search.rs:501` の `sorted_prefix_len`（`research.md` §0.3）。
- [x] `Engine::config` の呼び出し元の母集団 — `.config()` と UFCS の 2 綴りで全 crate を走査。snotra-core のテスト 3 件（`#[cfg(test)] mod tests` は 332 行から）と src-tauri の 4 件だけ（`research.md` §0.2）。
- [x] `cargo doc` が壊れないか — `Engine::config` への intra-doc link は 0 件（実測）。plain な code span が `launch.rs:106` に 1 つあるが、その行は書き換える。
- [x] 撤去条件が発火するか — **する**。4 か所案では最後の `#[expect]` が消えるので、`clippy.toml` の群 3 と `REQUIRED_DISALLOWED_METHODS` を同じコミットで消す（撤去条件が逐語で指定）。
- [x] `G-clippy-disallowed.test.mjs` の fixture をどうするか — **該当 2 行を削除**。カナリアから消しても fixture は上位集合なので緑のままだが、実在しない群を指す死んだ行になる。「群を跨いで持つ」性質は群 1・群 2 で保たれる（`research.md` §0.6）。
- [x] 弁別子・例外に言及する地点の母集団 — `.md` だけでなく `.rs` / `.toml` / `.mjs` / `.ps1` へ当て直した（`research.md` §4.3。初稿の「写しは 0 件」は `--include=*.md` だけで測った偽の全称だった）＋ 独立導出が 3 件を追加。
- [x] `AppState::read_config` が既存の検知器の射程に穴を開けないか — `launcher_controller.rs:1910` の needle は `read_config(` で、新しい綴り `state.read_config(` を部分文字列として含む（`research.md` §4.4）。
- [x] 読み口の設計（3 案） — **案 2（`AppState::read_config` を足して `egui_shell::read_config` を委譲にする）を採る。**
  - 案 3（`&AppHandle` へ替える）は却下。独立導出 v2 の根拠を採る——**到達しない fallback を 3 つ捏造することになり**、とくに `icon_cache_cap` の fallback は `Config::default()` の I/O を招く。
  - **案 1（`state.config.read()` 直読み）を独立導出 v2 は「推奨・最小」としたが、採らない。** 理由 3 点: (a) 直読みでは guard がスコープ末まで生きるローカル束縛になり、**後から guard の後ろへ I/O を足せてしまう**（I1 が最も破られやすい形になる）。クロージャ形は guard の寿命を構文で閉じる。(b) 製品コードの read guard 取得点が 1 → 4 になる（現状 1 は実測）。(c) **src-tauri の config 読みはすべてクロージャ形である**——この 4 か所だけ生 guard にすると、唯一の例外的な書き方になる（例外を消す変更で書き方の例外を作ることになる）。
  - 採否と理由は ADR へ書く。
- [x] 新しい ADR を書くか — **書く**。
- [x] `/plan-review` を `--deep` で回すか — **回す**（ガバナンス文書の圧縮 ＋ セーフティネットの撤去）。射程拡大により**再実行する**。
- [x] `std::sync::RwLock` の公平性を根拠に使うか — **使わない**（`research.md` §3.1）。

## plan-review 結果

- リスク: **高**
- レビュー方式: **独立導出 1 体**（Step 2b・`--deep`）
- エージェント数: 1（本節）／ 通算 2（3b の敵対的調査を含む）
- 成果物: `workspace/plan-review-1123-independent.md`
- **再実行済み**（v2・射程拡大版）。成果物: `workspace/plan-review-1123-independent-v2.md`。以下、v1（3 か所案）と v2 を分けて記す。

### v2（4 か所案）— 要対処（再照合して採用・すべて反映済み）

1. **`G-clippy-disallowed.mjs` のヘッダ 21-22 行**「群 3（#1122）だけは前提 (3) を例外地点の `#[expect]` が補う…条件は clippy.toml の群 3 が正本」——群 3 が消えると**宙吊りの参照**になる。前提 (3) の括弧「群 2・3 は snotra-core 側の改名が契機」も同様。**実読で確認し、変更ファイル一覧へ追加。**
2. **`egui_shell/mod.rs:410` の「UI が config を読む唯一の口（#1032）」**が偽になる（口が 2 つになる）。**実読で確認。**「唯一」の主張を「read guard を取る地点が 1 つ」へ移す形で解消。
3. **`ADR-config-read-exception-discriminator` への生きた層からの引用は全リポジトリで 2 件しかなく、4 か所案ではその両方が消えて ADR が孤立する。`G-adr-citations` は一方向検査ゆえ沈黙する。** → 条項に短縮引用を 1 件残す（`research.md` §0.5 へ項目として追加）＋ Phase 2 に確認の作業項目を追加。
4. `clippy.toml` の**区切りコメント（157）と群 1 の 37 行の相互参照**も削除対象。→ 一覧を具体化。
5. 新機構の実測は **E0599**（E0624 ではない）で確かめ、あわせて **`config_handle().read()` が今も通ること**を測って「engine 錠越しの読みは書けなくなった」という偽の全称を防ぐ。→ 「機構の乗り換えは測る」節へ反映。

### v2 — 採らなかった提案

- **読みの綴りは α（`state.config.read()` 直読み）を推奨**と提示されたが、**採らない**。理由 3 点は未確定欄「読み口の設計」に記した（guard の寿命がスコープ末まで伸びる／取得点が 1 → 4 になる／src-tauri で唯一の非クロージャ形になる）。

### v2 — 独立に一致したもの（追加対処なし）

- 可視性 `#[cfg(test)] pub(crate)` と `pub(crate)` 単独では `dead_code` が立つこと（**scratch crate で実測**したとのこと。主エージェントは同 crate の先例 `sorted_prefix_len` から導いており、独立の 2 経路で同じ結論）。先例も 3 件挙げている（`PrebuiltIndex::new` / `Engine::replace_entries` / `sorted_prefix_len`）。
- 呼び出し元 7 件（src-tauri 4 ＋ engine.rs の in-file tests 3）。`snotra-core/tests/`・benches・他 crate は 0 件。
- 撤去条件は発火し、その手順に欠陥が 4 件ある（`pub(crate)` 不足／fixture 未列挙／**散文の掃除が一切列挙されていない**／合図が駆動力を持たない）。→ ADR の帰結へ書く。
- 新しい競合・窓・順序制約は生じない。**`config_watcher` の旧 config 読みと `update_config` の間には今日すでに `recv_timeout(2s)` が挟まっている**（窓は移設で広がらない）。
- `cargo doc` は壊れない（intra-doc link 0 件）。**ただし書き直す doc でブラケットリンクにしないこと。**

### v1（3 か所案）— 要対処（すべて反映済み、または 4 か所案が吸収）

### 要対処（再照合して採用・すべて反映済み）

1. `src-tauri/CLAUDE.md:24` と `config_watcher.rs:148-152` が**写しの対**で、どちらも「engine がまだ `show_icons=true` を返す隙に」と書いており偽になる。両方を実読で確認。
2. `snotra-core/src/engine.rs` の `Engine::config` doc が「UI は `egui_shell::read_config` を通す」と書く。**4 か所案ではこの doc ごと書き直す**ので吸収された。
3. `scripts/governance/checks/G-clippy-disallowed.mjs:65` のコメント。**4 か所案では行ごと削除**されるので吸収された。
4. `clippy.toml` 群 3 の撤去条件の支えが 1 本になる旨の追記。**4 か所案では群 3 ごと消える**ので不要になった。

### 軽微

- 設計分岐 (a)/(b)/(c) — 既に案 2 を選択済み。**先方の却下根拠のほうが強い**ので採用（案 3 は到達しない fallback を 3 つ捏造する）。
- `resolve_all_openers` を findReferences で取り直していない → **主エージェントが LSP で 3 関数すべて取り直した。grep と完全一致。⚠️ 解消。**
- 面積計器の報告値が動く（合否なし）→ 対処不要。

### 独立導出が持ち込んだ訂正

- **`#[expect]` の reason 内の見出し参照はバックティックが無く、`G-heading-refs` / `G-near-heading-refs` の正規表現に掛からない**——どの機構も照合していない。→ 受け入れ条件 8 の担保は目視である旨を明記した。**4 か所案では `#[expect]` が全滅するので、この残余自体が消える。**

### 未検証

- I4（`config_watcher` の窓が広がらない）→ Phase 1 の `/race-check` で名指し検算。
- **`notify` のコールバックの並行性**（`apply_config_change` が同時に 2 本走りうるか）は未測定（v2 の ⚠️）。→ `/race-check` の検算対象に含める。**現行も読みと書きは別々の錠取得なので、並行しうるなら移設前から同じ窓が在る**（移設が作る窓ではない）。
- `resolve_opener` が engine 錠を取らなくなる副作用は「挙動不変」だが「**何も変わらない**」ではない（v2 の ⚠️）——tray スレッドが検索 worker の走査を待たなくなる。**これは測らないし、速くなったとも書かない。**
- config `RwLock` の read 競合の増分 → **測らない。「変わらない」とも書かない**（目的節に明記）。
- 条項から削る文の着地先の全数確認 → 「動機と判定を分ける」は例外という装置が在るときだけ必要な区別なので、装置ごと消えれば孤児にならない。凍結 ADR（案 D）が歴史として保持する。

## セルフレビュー

- リスク: **高**（ガバナンス文書の圧縮・セーフティネットの撤去・共有状態の読みの錠の変更・公開 API の可視性変更）
- plan-review: `--deep`（Step 2b 独立導出）を **2 回**（v1: 3 か所案／v2: 4 か所案）
- エージェント数: **3**（3b の敵対的調査 1 体 ＋ 独立導出 v1 1 体 ＋ 独立導出 v2 1 体）
- 要対処: 計 **10 件**（3b 1 件・v1 4 件・v2 5 件）。すべて反映済み、または 4 か所案が吸収
- 主エージェント自身が見つけたもの: §4.3 の**偽の全称**（`.md` 限定 grep）／`/race-check` を計画段階で起動しない旨（Phase 1 へ移設）／**`pub(crate)` では `dead_code` が立つこと**（撤去条件の文言の欠陥。`#[cfg(test)]` の先例を同 crate に発見）
- 未検証: `RwLock` の公平性（根拠から外した）／read 競合の増分（測らないと明記）／I4（`/race-check` へ）／挙動不変を実行時に測る手段（テスト席が無く、入出力同一性から従うものとして受容）

## 人間レビュー

- [x] **問い 1（規範の是非）** — 承認済み — 2026-08-18
  - 問い: "config の live-read 条項から「例外」という装置を無くし、残る 3 か所を engine 錠の外へ出すことに同意なさいますか。**#1076 で入れたばかりの弁別子を開け直す変更**です。"
  - 回答: "問1 例外がなくなってコードで自明になるならOK.理解が違ったら突っ込んで。"
  - **突っ込んだ結果**: 3 か所案では「コードで自明」が部分的にしか達成されない（`config_watcher` の `#[expect]` が 1 件残る／guard 内 I/O の禁止は文書契約のまま／`state.config` の直読みを書ける）と提示し、射程を問い直した。
  - 追加の裁定（射程）: 選択肢「config_watcher も移す（4 か所）— Engine::config を pub(crate) へ落とし、lint も注釈も廃して**コンパイルエラー**が規範を守る形。「コードで自明」の最大値」を選択。
- [x] **問い 2（計画の承認）** — 承認済み — 2026-08-18
  - 問い: "**問い 2（計画の承認）** — この承認には次が含まれます。 / `clippy.toml` の**群 3 の撤去**、ガバナンス検査の**カナリア行と test fixture の削除**（撤去手順は `clippy.toml` 自身が指定しているものです） / `Engine::config` の**可視性を `#[cfg(test)] pub(crate)` へ落とす**こと / **issue #1123 本文が明示的に射程外としていた範囲**を含むこと（PR 本文へ記録します） / セーフティネットの撤去を伴いますので、計画の承認とは別に一度ご確認いただく形にしております。"
  - 回答: "問2 OK"
