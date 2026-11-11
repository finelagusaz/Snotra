# #1365 調査: main の可視性キャッシュ（`main_visible`）を撤去し、イベントループ上で実状態を読む

## issue の要約

- `main_visible` は #156 で入ったキャッシュ（WebView2 期に、別スレッドから `is_visible()` を呼ぶと cold で 61ms / 39ms かかったため）
- #881 / #880 以降、読み手（hotkey の判定・`drive_results_window` の連言①）も書き手（`show_egui_main` / `hide_egui_main`）もイベントループスレッド上にある。そこからの `is_visible()` はインラインで実行され、往復しない
- キャッシュは `show_egui_main` / `hide_egui_main` 以外による可視性の変化を反映しない（潜在的な問題・今そう書いている箇所は無い）
- 変更: キャッシュを撤去し、読みを `app.get_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(false)` へ替える。`EventLoopProof` を受ける形にする。`layout::ResultsInputs.main_visible` は純粋核の入力なので残す。生の `IsWindowVisible` は呼ばない
- 依存 PR #1364 は 2026-11-02 にマージ済み（`gh pr view 1364`）

## issue 起票後に変わった前提

- **キャッシュはもう `AppState` に無い。** #1364 で `EguiShellState.main_visible: window_coordinator::MainVisible`（`AtomicBool` の newtype・`set` はモジュール private）へ移った。issue 本文の「`AppState.main_visible`」「`state.rs` doc」は旧所在である。`state.rs` には `main_visible` も「~35ms」も残っていない（`git grep` 0 件）
- 「~35ms」の記述は `docs/superpowers/specs/2026-08-02-results-visibility-single-thread-design.md` §9-3 の中にだけ残る（凍結された設計書・直さない）

## 上流の一次確認（ローカルの版: Cargo.lock の tao 0.37.2 / tauri-runtime-wry 2.12.3）

- `tauri-runtime-wry-2.12.3/src/lib.rs` `Context::send_user_message`: `current_thread().id() == self.main_thread_id` なら `handle_user_message` を**インラインで**呼ぶ。そうでなければ proxy へ post
- `fn is_visible` は `window_getter!(self, WindowMessage::IsVisible)` → `getter!`（channel を作り send_user_message → recv）。インライン経路では recv の時点で既に送信済み
- `handle_user_message` の `WindowMessage::IsVisible(tx) => tx.send(window.is_visible())`
- tao 0.37.2 `platform_impl/windows/window.rs:628` `is_visible` → `util::is_visible` → `IsWindowVisible(hwnd)`（同期）
- **hide の着弾ラグも無い**: tao `set_visible` は `thread_executor.execute_in_thread(...)`。`event_loop.rs` `execute_in_thread` は `in_event_loop_thread()` ならインライン実行（そうでなければ `PostMessageW`）。`hide_egui_main` はイベントループ上（証人型）なので、`window.hide()` が戻った時点で `WS_VISIBLE` は落ちている
- `handle_user_message` は `windows.0.borrow()` を短く取る。同じ経路（main への getter を毎フレーム・イベントループ上から）は既に使っている——`drive_results_window` → main の位置読み（`window_coordinator.rs` の `get_window("main")` 群）、`hide_egui_main` の `read_placement_relative`。再入で RefCell が二重借用になる経路は新しく生じない

## 関連するファイル・シンボル

| 所在 | シンボル | 役割 |
|---|---|---|
| `src-tauri/src/egui_shell/window_coordinator.rs` | `MainVisible`（型・`new`/`get`/`set`） | キャッシュ本体（撤去対象） |
| 同 | `show_egui_main` | `window.show()` の後に `set(true)`（撤去） |
| 同 | `hide_egui_main` | `window.hide()` の後・`results.hide()` の前に `set(false)`（撤去） |
| 同 | `read_main_visible` | `drive_results_window` の連言①の読み（実状態の読みへ替える） |
| 同 | `drive_results_window` | `read_main_visible(app)` を呼ぶ唯一の呼び手。`el` を持つ |
| `src-tauri/src/egui_shell/mod.rs` | `EguiShellState.main_visible` / `EguiShellState::new` / 型の doc の箇条 | フィールド（撤去） |
| `src-tauri/src/main.rs` | hotkey listener の `on_event_loop` クロージャ（`sh.main_visible.get()`）・setup のコメント「main_visible は show と hide が」 | 読み手（替える）・コメント（直す） |
| `src-tauri/src/egui_shell/layout.rs` | `ResultsInputs.main_visible` の doc（「`EguiShellState.main_visible`」と出所を名指す）・`present_results` の doc | フィールドは残す。doc の出所を直す |
| `src-tauri/src/egui_shell/results_window.rs` | `//!`（連言① `main_visible`）・`visible` の doc「同居する `main_visible` / `hotkey_generation` に合わせ `SeqCst`」 | doc を直す |
| `SPEC.md` | 8.6 の状態図 `[hotkey_toggle && main_visible]`・遷移ルール要約 | 語を「main が可視」へ |
| `PERFORMANCE.md:17` | 一般則「Win32 IPC の cold-call を避ける…`AtomicBool` 等でキャッシュ」 | 現状に合わせて直す |
| `scripts/manual-smoke.ps1` | 項目 2 の `inv`「main_visible=false は results.hide() の前」 | 直す |
| `docs/adr/ADR-results-gate-single-point.md` / `ADR-appstate-config-visibility.md` | `main_visible` に言及 | **直さない**（ADR は凍結された歴史・`.claude/rules/governance-docs.md`）。issue の「同期する文書」とはここが食い違う |

main の可視性を変える経路（`git grep` で数えた）: `window.show()` は `show_egui_main` の 1 か所、`window.hide()` は `hide_egui_main` の 1 か所。生成時は `.visible(false)`（`mod.rs` の 2 窓）。`snotra-egui-runtime` に `set_visible` / `ShowWindow` は無い。

## 再利用できる既存パターン

- `EventLoopProof` を受けて「イベントループ上でしか呼べない」ことを型で示す形（`show_egui_main` / `hide_egui_main` / `drive_results_window`）。読みの関数にも `_el: &EventLoopProof` を取らせれば、別スレッドからの読み（=往復の再発）を型で防げる
- hotkey は既に `on_event_loop(&handle, move |app, el| …)` の中で読んでいる——`el` はそこにある

## 技術的な制約

- 生の `IsWindowVisible` を直接呼ばない（`.claude/rules/src-tauri.md`「Win32 を呼ぶ経路は PlatformBridge 経由」）。tauri の `Window::is_visible()` を使う
- 順序の不変条件（show: 可視になってから「可視」と読ませる / hide: results.hide() の前に「不可視」と読ませる）は、実状態を読むなら `window.show()` / `window.hide()` 自体の同期性で自動的に満たされる（上の tao の確認）
- `ResultsInputs.main_visible` は純粋核の入力として残す（`layout.rs` のテストが使う）
- ADR は書き換えない。2026-08-02 spec §9-3 の却下（「キャッシュを廃止して実状態を真実源にする案は採らない」）を覆すので、否定の知識を新しい ADR に残す

## 未解決の疑問（→ 解消の記録）

- `is_visible()` が `Err` を返すのはどんなときか → 窓が windows map に無い・inner が None のとき tx が drop され recv が Err（ハングしない・3b で確認）。`unwrap_or(false)` は「results を出さない／hotkey は show 側」へ倒れ、今の `EguiShellState` 不在時と同じ向き
- 毎フレームの読みの所要（イベントループ上）→ 未測定。計画で 1 回測る
- `IsWindowVisible` と旧キャッシュの意味が食い違う状態 → 最小化・Win+D・cloak ではどちらも変わらない（3b）。食い違うのは「`show_egui_main` / `hide_egui_main` 以外が可視性を変えたとき」だけで、そこは実状態の側が正しい
- 読みの関数の置き場 → `window_coordinator.rs` の `read_main_visible(app, el)` 1 つにし、hotkey もそれを呼ぶ（計画で決定）

## 敵対的調査（3b・sonnet 1 体・`workspace/adversarial-1365.txt`）の所見と採否

壊せなかった項目（争点 1〜5 と測定環境）:

- 版: Cargo.lock は tao 0.37.2 / tauri 2.12.5 / tauri-runtime-wry 2.12.3。ルート `Cargo.toml` が runtime-wry を `=2.12.3` に固定。ランタイムは Wry（custom runtime ではない）
- `main_thread_id` は `Wry::new` で取られ、イベントループスレッドと一致。`snotra-egui-runtime` の別スレッドは repaint worker 1 本で、proxy へ post するだけ
- インライン実行・hide/show の同期性（tao の `apply_diff` は diff が空でも `SW_HIDE` / `SW_SHOW` を撃つ）
- main の可視性を変える経路は 2 か所だけ（single-instance は `show_egui_main` を呼ぶ）
- キャッシュの読み手は 2 つだけ。テスト・smoke・CI にキャッシュ名への依存は無い（`manual-smoke.ps1` の `inv` は説明文字列）
- RefCell の二重借用は新しく生じない

壊せた項目（採る）:

- **関連ファイル表の取りこぼし**: `PERFORMANCE.md` の 3340・3348 行にも `is_visible()` → `AtomicBool` の記述がある。採否: 採る。ただし両方とも 2026-03-07 の計測履歴なので直さない（直すのは 17 行目の一般則だけ）。`SPEC.md` は 555 行（状態図）と 596 行（遷移ルール要約）の 2 か所
- **効果の過大**: 実状態を読んでも、main が hidden の間は `drive_results_window` が走らないので、外部 hide で results が取り残される残余（#671）は直らない。直るのは hotkey のトグル判定と、results のゲートが実状態を見ることだけ。採否: 採る。issue の「残るもの」と一致。`hide_egui_main` の doc の受容残余は「キャッシュが更新されず」の前提が消えるので書き換える（「拾い直すフレームが来ない」は残す）

⚠️（確信の持てない所見）の裁定:

- ⚠️「`window.show()` の最中に main 自身の `RedrawRequested` が同期で届けば、キャッシュ（show 後に true）と実状態（show 中に true）で観測差が出る」→ **起きない（一次資料で裁定）**。tao 0.37.2 `event_loop.rs` の `WM_PAINT` 枝は、`event_loop_runner.should_buffer()`（イベントハンドラの実行中）なら `RedrawRequested` を出さず `RedrawWindow(RDW_INTERNALPAINT)` で後へ回す。`show_egui_main` は `on_event_loop` のタスク（ユーザーイベントのハンドラ内）か setup の中で走るので、この枝に入る。加えて `ShowWindow` は `WM_PAINT` を同期で送らない（キューで生成される）。入れ子のフレームは構築できない
- ⚠️「外部プロセスが main に `SW_HIDE` した場合、実状態の読みなら hotkey 判定に反映される」→ 採る（望ましい向き）。results の取り残しは上の残余として残る
