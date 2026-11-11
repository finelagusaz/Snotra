# #1365 計画: main の可視性キャッシュ（`MainVisible`）を撤去し、イベントループ上で実状態を読む

調査は `workspace/research.md`（敵対的調査の採否つき）。

## 目的と受け入れ条件

目的: main の可視性の真実源を 1 つ（窓の実状態）にする。写し（`EguiShellState.main_visible`）と、それを保つための順序規範（「show の後に true」「results.hide() の前に false」）を消す。

受け入れ条件:

1. `MainVisible` 型・`EguiShellState.main_visible` フィールド・`set(true)` / `set(false)` が無い（`git grep -n 'MainVisible\|sh\.main_visible'` が `*.rs` で 0 件）
2. main の可視性の読みは `window_coordinator::read_main_visible(app, el)` の 1 つだけで、`&EventLoopProof` を要求する。hotkey（`main.rs`）と `drive_results_window` の両方がそれを呼ぶ
3. 読みは tauri の `Window::is_visible()` を使い、生の `IsWindowVisible` を呼ばない
4. `layout::ResultsInputs.main_visible`（純粋核の入力）は残り、`layout.rs` のテストは無変更で通る
5. 挙動が今と同じ: hotkey のトグル（表示 → 非表示 → 表示）・Escape／フォーカス喪失で hide した後の再表示・single-instance・初回ホットキー失敗時の表示。results が main より先に出ない・main の hide 後に残らない（`smoke:egui` の hide 後不変条件・`manual-smoke.ps1` 項目 2）
6. `is_visible()` のイベントループ上での所要を cold / warm で 1 回測り、機体名つきで `PERFORMANCE.md` へ記録する（issue の任意項目を採る。毎フレームの読みに替わるので、費用を数字で残す）
7. 2026-08-02 spec §9-3 の却下を覆した理由を ADR に残す

## 変更するファイルと対象シンボル

| ファイル | シンボル | 変更 |
|---|---|---|
| `src-tauri/src/egui_shell/window_coordinator.rs` | `MainVisible`（型と impl） | 削除。`AtomicBool` の import が他で使われなければ外す（`Ordering` は他で使う） |
| 同 | `read_main_visible` | `fn read_main_visible(app: &tauri::AppHandle, _el: &snotra_egui_runtime::EventLoopProof) -> bool` へ。本体は `app.get_window("main").and_then(\|w\| w.is_visible().ok()).unwrap_or(false)`。可視性は `pub(crate)`（main.rs から呼ぶ）。doc: 実状態を読む理由（イベントループ上ではインライン・往復しない／tao の show/hide も同期なので順序規範が要らない）と、証人を取る理由（別スレッドから呼ぶと往復が戻る） |
| 同 | `show_egui_main` | `sh.main_visible.set(true)` とその前の 2 行のコメントを削除 |
| 同 | `hide_egui_main` | `sh.main_visible.set(false)` のブロックと直前のコメントを削除。関数 doc の「main_visible=false」「`EguiShellState.main_visible` は更新されず」「臨界区間（`main_visible` の store と 2 枚の `ShowWindow`）」を書き換える——臨界区間は「2 枚の `ShowWindow`（main の hide → results の hide）」になる。受容残余は「Manager 経由の `.hide()` で results が取り残される（main が hidden の間はフレームが来ない）」を残し、「フラグが更新されない」を消す |
| 同 | `drive_results_window` | `read_main_visible(app)` → `read_main_visible(app, el)`。直前のコメント（「hide_egui_main が results.hide() の前に false へ落とす」「`main_visible` を更新しない main の hide は…`MainVisible` の doc を見よ」）を、実状態を読むことと残余の正本（`hide_egui_main` の doc）へ書き換える |
| 同 | `//!` | `MainVisible` を名指す箇所があれば直す（現状の `//!` は名指していない・実装時に grep で確認） |
| `layout.rs`（`present_results` の doc）・`results_window.rs`（`show` の doc と本体のコメント）・`window_coordinator.rs`（`hide_egui_main` 本体・`drive_results_window` 本体） | 「証人型を引数に要求する 5 関数」の 5 か所 | 読みの関数も証人を要求するので、この数え上げは偽になる（Step 2b で発見）。数をやめ、述語で「可視性を変える関数（いずれも証人型を要求する）」と書く。「可視性を変える 5 関数」（`results_window.rs` の `//!`・`hide_request.rs` の `//!`）は真のままなので触らない |
| `src-tauri/src/egui_shell/mod.rs` | `EguiShellState` の doc 箇条・フィールド・`new` | `main_visible` を削除。`pub(crate) use window_coordinator::{…}` に `read_main_visible` を足す |
| `src-tauri/src/main.rs` | hotkey の `on_event_loop` クロージャ | `shell…main_visible.get()` → `egui_shell::read_main_visible(app, el)` |
| 同 | setup のコメント（「main_visible は show と hide が」） | その節を削る |
| `src-tauri/src/egui_shell/layout.rs` | `ResultsInputs.main_visible` の doc | 出所を「`window_coordinator::read_main_visible`（窓の実状態）」へ |
| `src-tauri/src/egui_shell/results_window.rs` | `//!`（連言①）・`visible` の doc「同居する `main_visible` / `hotkey_generation` に合わせ `SeqCst`」 | 後者は `hotkey_generation` だけに合わせる形へ |
| `SPEC.md` | 「8.6」状態図の `[hotkey_toggle && main_visible]`・遷移ルール要約の同じ語 | 識別子をやめて語彙で書く（例: `[hotkey_toggle && メインウィンドウ表示中]`）。挙動は変えない＝仕様変更ではない |
| `PERFORMANCE.md` | 17 行目の一般則「Win32 IPC の cold-call を避ける」 | WebView2 期の前提を外し、「イベントループスレッドからの tauri ウィンドウ getter はインライン実行で往復しない。別スレッドから読むときだけ往復が生じる」へ。3340・3348 行は計測履歴なので直さない |
| 同 | 測定の記録 | 受け入れ条件 6 の値を、既存の「ホットキー表示レイテンシ」節の近くへ機体名つきで追記 |
| `scripts/manual-smoke.ps1` | 項目 2 の `inv` | 「#671 PR A′（main の hide は results の hide より前）」へ |
| `docs/adr/ADR-main-visibility-live-read.md` | 新規 | 下の「ADR の中身」 |

触らないもの: `docs/adr/ADR-results-gate-single-point.md`・`ADR-appstate-config-visibility.md`（ADR は凍結された歴史）、`docs/superpowers/**`（凍結された設計書）、`PERFORMANCE.md` の計測履歴、`lifecycle.rs` の `plan_hotkey`（引数名 `visible` のまま）、`layout.rs` の判定とテスト。issue の「同期する文書」に ADR 2 本が挙がっているが、`.claude/rules/governance-docs.md` に従い新 ADR で上書きの関係を示す。

### ADR の中身（`ADR-main-visibility-live-read`）

- 文脈: #156 のキャッシュ、#880/#881 で読み手・書き手がイベントループへ閉じたこと、2026-08-02 spec §9-3 の却下と、その 2 つの理由
- 決定: キャッシュを撤去し、イベントループ上で `Window::is_visible()` を読む。読みの関数は `EventLoopProof` を要求する
- 却下した案と理由:
  - キャッシュを残す: 写しを保つ規範（2 つの順序制約）しか残っていない。`show_egui_main` / `hide_egui_main` 以外の可視性の変化を反映しない
  - 生の `IsWindowVisible` を直接呼ぶ: `PlatformBridge` 経由の規則に当たり、従うと往復が生じる。tauri の getter がイベントループ上でインラインになるので不要
  - 証人なしの読み関数: 別スレッドから呼ぶと往復が戻る（§9-3 の理由 1 が復活する）
- §9-3 の理由が失効した根拠: (1) 別スレッドからの読みはもう無い（読み手 2 つとも `on_event_loop` / フレームの中）。(2)「hide の着弾まで stale な TRUE」は、hide がイベントループ上で同期実行される（tao `execute_in_thread` のインライン枝）ので起きない。版は tao 0.37.2 / tauri-runtime-wry 2.12.3
- 帰結: 順序規範が消える。Manager 経由の `.hide()` による results の取り残しは残る（main が hidden の間はフレームが来ない）
- 旧 ADR との関係: `ADR-results-gate-single-point`（事前ゲートが「`main_visible` を読む」）と `ADR-appstate-config-visibility`（`main_visible` の可視性の議論）は凍結ゆえ直さず、ここで「ゲートが読むのは窓の実状態になった」「`main_visible` は存在しない」と書く（凍結後の先例 #1131 と同じ形）

## 実装の順序

フェーズ 1（コード）→ フェーズ 2（測定）→ フェーズ 3（文書）。フェーズ 1 の終わりで A が緑になる。

## 不変条件と異常系

- `results 可視 ⇒ main 可視`: ゲート（`present_results` の連言①）は実状態を読む。hide 側は `window.hide()` が同期で `WS_VISIBLE` を落とした後に `results.hide()` を撃つので、順序は `hide_egui_main` の文の並びだけで決まる（キャッシュの store が消えても並びは変わらない）
- show の途中で入れ子のフレームは走らない（tao の `WM_PAINT` 枝は `should_buffer()` 中に `RedrawRequested` を出さない・research.md の ⚠️ 裁定）
- `get_window("main")` が `None`・`is_visible()` が `Err`: false（results を出さない・hotkey は show 側）。今の `EguiShellState` 不在時と同じ向き
- `EguiShellState` の他フィールド（`hotkey_generation` 等）の読み書きは変えない

## テスト方針と検証コマンド

- ユニットテストで落とせる箇所は無い（Win32 依存・`.claude/rules/src-tauri.md`）。Red の代わりに、受け入れ条件 1 の `git grep` と、`smoke:egui` の既存シナリオで挙動を固定する。`smoke:egui` は hidden → hotkey で show・results の show・Escape で hide し hide 後に `egui_results:show` が出ない不変条件・hotkey で hide（held-key シナリオの「Z を押したまま hotkey で隠す」）を通る
- 変異注入（`/implement` の委譲先が行う）: `read_main_visible` を常に `false` に → held-key シナリオの hotkey hide が show になり落ちること。常に `true` に → hidden 起動からの最初の hotkey が hide 側（`HideNow`）へ倒れ、`egui_show:done` が観測されず落ちること。どちらも赤の理由が smoke の assert であることを出力で確かめる
- 検証: `docs/build-commands.md` カテゴリ A・C（`npm run verify:tree -- run --sha HEAD --categories A,C`）。文書は F（`governance:check`）。D の手動 smoke（`manual-smoke.ps1` 項目 2）は PR のチェックリストに置き、人が行う
- `/race-check`（フレーム内 live-read・共有状態の撤去）は計画の段階では起動しない（同スキルの冒頭・#784）。実装の差分に対して `/implement` の中で当てる
- `/state-check`（hotkey のガード条件の値の出所が変わる）は計画検証で当てた（下のセルフレビュー）
- 実装時に当てる条件別チェック: `/symmetric-check`（show/hide の対から store の対を外す）・LSP findReferences（`MainVisible` / `read_main_visible` の呼び出し元。削除後は `cargo build` の compile-fail でも漏れを見る）・「重複した読み・冗長に見える状態を束ねる/消す」の行（キャッシュの読み 2 か所が「後で読まれる/立つこと」に依存していないかを 1 行ずつ書き出す——hotkey: 読みの直後に分岐するだけ。drive: 同じフレームで 1 回読み、`present_results` に渡すだけ）
- 自動検査が無いもの: 「hidden 中に走る稀なフレームで results を出さない」（連言①の読みそのもの）。`smoke:egui` の H1 が見るのは hide 後の再 show の区間だけで、初回 show の前は観測しない。PR 本文に残余として書く

## SPEC.md・関連文書

- `SPEC.md`: 語の置き換えだけ（挙動は変えない）
- `docs/architecture.md`: `main_visible` の記述は無い（`git grep` 0 件）。触らない
- `src-tauri/CLAUDE.md`: 記述なし。触らない

## 作業項目

### フェーズ 1 — コード

- [x] `window_coordinator.rs`: `MainVisible` を削除し、`read_main_visible(app, el)` を実状態の読みへ替える（doc 込み）
- [x] `window_coordinator.rs`: `show_egui_main` / `hide_egui_main` の store と関連コメント・doc を直す
- [x] `window_coordinator.rs`: `drive_results_window` の呼び出しとコメントを直す
- [x] `mod.rs`: `EguiShellState.main_visible` を削除し、`read_main_visible` を re-export する
- [x] `main.rs`: hotkey の読みを `egui_shell::read_main_visible(app, el)` へ替え、setup のコメントを直す
- [x] `layout.rs` / `results_window.rs` の doc を直す
- [x] 「証人型を引数に要求する 5 関数」の 5 か所を述語の形へ直す
- [x] `git grep -n 'MainVisible\|main_visible'` を `*.rs` で数え、残りが `ResultsInputs.main_visible` とそのテスト・`present_results` の doc だけであることを確かめる
- [ ] カテゴリ A を通す

### フェーズ 2 — 測定（一時的な足場）

- [x] `read_main_visible` の前後に `Instant` を置く一時コードで、hotkey 1 回目（cold）と以降（warm）、`drive_results_window` の毎フレームの所要を trace へ出し、ローカルの `smoke:egui` で採る。**足場はこのフェーズの中で撤去し、コミットに含めない**（撤去の確認: `git diff` に `Instant` の追加が無いこと）
- [x] 値を機体名（`$env:COMPUTERNAME`）・日付・版（tao 0.37.2）つきで `PERFORMANCE.md` へ記録する

### フェーズ 3 — 文書

- [x] `SPEC.md`「8.6」の状態図と遷移ルール要約の語を直す
- [x] `PERFORMANCE.md` 17 行目の一般則を直す
- [x] `scripts/manual-smoke.ps1` 項目 2 の `inv` を直す
- [x] `docs/adr/ADR-main-visibility-live-read.md` を書く
- [x] `git grep -n 'main_visible'`（`*.rs` 以外）で残りが凍結文書（ADR 2 本・`docs/superpowers/**`）と `PERFORMANCE.md` の計測履歴だけであることを確かめる
- [ ] カテゴリ A・C・F を `verify:tree` で通す

## 未確定（実装前に潰す）

（なし）

## 人間レビュー

- [x] 承認済み — 2026-11-11 / 問い: "計画は `workspace/plan.md` にございます。注釈を書き込んでいただくか、このまま承認してくださるか、どちらかをお願いいたしますわ。" / 回答: "ok"

## セルフレビュー

- リスク: 高（共有状態 `EguiShellState` のフィールド撤去・状態遷移のガードの値の出所の変更・複数モジュールにまたがる読み口の変更）
- plan-review: 独立レビュー1体（Step 2b・独立導出・`workspace/plan-review-1365-derive.md`）
- エージェント数: 1（3b の敵対的調査を含めると 2）
- 要対処: 2 件を反映
  - 「証人型を引数に要求する 5 関数」の数え上げ 5 か所が、読みの関数に証人を足すと偽になる → 変更表と作業項目へ追加（`git grep '5 関数'` で 5 か所を再照合した）
  - 新 ADR に旧 ADR 2 本との関係の節が要る → ADR の中身へ追加
- 軽微（採ったもの）: 実装時の条件別チェック（`/symmetric-check`・LSP findReferences・重複した読みの行）を明記した。H1 の観測範囲外（初回 show 前の hidden フレーム）を残余として明記した
- 軽微（採らなかったもの）:
  - 関数名を `main_is_visible` へ改名 → 採らない。既存の `read_main_visible` を残せば改名の差分が出ない。名前の `read_` は live-read の慣用（`read_metrics` / `read_visible_rows`）と揃う
  - SPEC の語を替えない案 → 採らない。`main_visible` という識別子は消えるので、SPEC が存在しない識別子を名指すことになる（SPEC は語彙層で書く）
  - `proof.rs` の `//!` に「読みにも使う」と足す → 採らない。`proof.rs` は証人型の定義と入口であり、どの関数が証人を要求するかの一覧は持っていない
  - 検証カテゴリに E を足す → `manual-smoke.ps1` は `scripts/**` なので E（`npm test`）に当たる。`verify:tree` は A・C の指定で E・F も走らせる（#1427 の実行で `categories_run` に E・F が出たことを確認）ので、作業は変わらない
- 判断の不一致: issue は ADR 2 本の同期を求めるが、ADR は凍結（`.claude/rules/governance-docs.md`・`ADR-adr-frozen-history`）。両者とも直さないと導出したので一致。issue との食い違いは承認の問いで示す
- `/state-check`: 状態モデルとの不整合なし。新しいモード・シグナルは無く、変わるのは hotkey のガード（`plan_hotkey` の `visible`）と results のゲート（連言①）の値の出所だけ。窓の可視性は `search_state` のモード（`FolderExpansionMode` / `ToolSelectionMode` / `QueryIntent` / `indexing` / `launching`）と直交（どれも窓の可視性を読まない）。リセット経路: 撤去する状態は reset-on-show の対象ではなかった（`show_egui_main` / `hide_egui_main` が直接書いていた）。ガードが true になる状況: `show_egui_main` の `window.show()` の後から `hide_egui_main` の `window.hide()` までで、キャッシュと同じ。初回起動（`show_on_startup` は `show_egui_main` 経由）・初回ホットキー失敗（listener が `show_egui_main` を呼ぶ）・setup 完了前（`get_window` が `None` → false、旧も `EguiShellState` 不在 → false）を塞がない。SPEC「8.6」は語の置き換えだけ
- 未検証: 毎フレームの `is_visible()` の所要（フェーズ 2 で測る）
- 散文の全件列挙: 語 5 / 総件数 88 行 / ★未掲載 12（計画へ足した 0）——★の 12 件は凍結文書 8（`ADR-appstate-config-visibility`・`docs/superpowers/**` 7）と同名の別物 4（`snotra-core/src/folder.rs` と `src-tauri/gen/schemas/*.json` の `is_visible`）。「5 関数」は語に入れていなかったので sweep に出ず、Step 2b が拾った
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2b / 設計変更 0 / 主の見落とし 2
