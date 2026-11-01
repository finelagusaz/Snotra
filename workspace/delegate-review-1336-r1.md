# review-1336

anchor sha: 42b77e0122b9e5e35b72b5583a7b0e9efef1fa54（`verify-tree.mjs align` の出力。HEAD 一致・作業ツリー clean を確認）

## 進行
- 差分・旧本書・新本書・plan.md・pr-material.md・research.md「振り分け表」を読了
- 逆向き監査・主張検証・取りこぼし grep は済み（下）。変異注入も済み（末尾）

## 変異注入の結果（CARGO_TARGET_DIR は木ごとの target-review）
- 注入前ハッシュ: `commands/system.rs` = ea73e2b1…、`state.rs` = 1344a1a6…。各変異の後に復元し、同じハッシュに戻ったことを確かめた（最後に `git status --short` で追跡ファイルの差分 0）
- M-I5a（system.rs から `s.indexing.store(..)`）: **CAUGHT** — `cargo check -p snotra --all-targets` が `error[E0616]: field indexing of struct AppState is private`（system.rs:65:42、bin と bin test の両方）
- M-I5b（system.rs から `s.index_build_started.store(..)`）: **CAUGHT** — 同じく `E0616: field index_build_started ... is private`
- M-I5c（`is_indexing` の本体を `false`）: **CAUGHT** — `cargo test -p snotra ensure_not_indexing` で `ensure_not_indexing_rejects_while_indexing_in_progress` が system.rs:46 の `assert_eq!` で FAILED（コンパイルは通り、赤の理由は assert）。`ensure_not_indexing_allows_when_idle` は ok（この変異では落ちないのが正しい）
  - 補足（測っていない）: `is_index_build_started` の中身を測るテストは、名前フィルタ `ensure_not_indexing` の範囲には無い。読み手は `commands/window.rs` の first-run 枝（AppHandle 依存）だけ

## 所見（暫定・変異前に書き出し）

Critical / High: なし

### Medium
- M-1 `search_flow.rs:44` の「射程の定義は `AppState::read_config` の doc が正本」は着地が不完全。旧 #1032 条項の「この規則に条件は付かない——どのスレッドで走る読みかも、何のための読みかも問わない（#1123）」と、害の記述（検索 worker が `engine.search` の間 `Mutex<Engine>` を握る→フレームが返らない）は `state.rs:82-118` の read_config doc に無い。害は `docs/architecture.md:247` と `Engine::config_handle` doc に残るが、「条件が付かない（頻度・スレッドを問わない）」は R3 を「1（E0599）」に分類したことで文として消えた。同じ指しを持つ `activation.rs:348`（射程）・`window_coordinator.rs:206`（「そこで待つのは表示そのもの」の根拠）も同じ指し先へ着地するが、指し先は「待つ」機序（worker の錠保持）を述べない
  - 偽にする手順: `Grep "条件|スレッド|engine.search|走査" src-tauri/src/state.rs` で read_config doc に射程（無条件）か worker の錠保持が書かれていることを示す

### Low
- L-1 `layout.rs:453`「`window_coordinator::hide_egui_main` の docである」——「doc である」の空白欠落（他の付け替え箇所は「doc」の後に助詞を空けている）。偽にする: 該当行を読む
- L-2 `hide_egui_main` doc（`window_coordinator.rs:486-492`）の「#880 サイクル段 2 時点でこの書き方の呼び出し点は無く」——旧本書には「main に対する `window.hide()` は `hide_egui_main` の 1 か所のみ」という限定があった。新 doc は `hide_egui_main` 自身の doc 内にあり、同関数 `window_coordinator.rs:521-522` が `app.get_window("main")` → `window.hide()` という当の書き方そのものである。自己言及で「無い」と読める。偽にする: 「この関数自身を除き」に相当する限定が doc にあることを示す
- L-3 `.claude/rules/src-tauri.md:10` の節見出し「読む正本（`src-tauri/CLAUDE.md` の該当節）」の下に、本書の節でない `commands/launch.rs` の `launch_item_core` 行が入った（既存の `snotra-core/CLAUDE.md` 行と同じ不一致を 1 本増やした）。偽にする: 見出しが「本書以外も含む」と読める文言であることを示す
- L-4 X10 の命令形「事後検査や『フラグを無視して撃つ hide』を足してはならない」と、その根拠 ADR `ADR-results-gate-single-point` への生きた層からの指しが消えた。台帳は「X7・X10 → `layout::present_results` doc・`results_window.rs` の `//!`」とするが、`results_window.rs:20-27` は撤去の経緯（構築不能になったので撤去した）を述べるだけで、`layout.rs:450-455` も「構築できない」の事実のみ。禁止は推論で辿れるので害は小さい。偽にする: `Grep "事後検査|足してはならない|ADR-results-gate-single-point" src-tauri/src` が禁止文か ADR 名を返すこと（実測では `results_window.rs:22` の経緯 1 件のみ）

- L-5 E2 の「blur 猶予は #745 で `BlurGrace::reset` として合流・そこに残る受容残余は同 doc が正本」への指しが、新しい wake 節から消えた。台帳に行が無い（宣言のない削除）。ただし内容は `lifecycle.rs:135-145` の `BlurGrace::reset` doc に実在するので、命題そのものは失われていない。偽にする: 台帳に E2 の部分削除の行があることを示す
- L-6 計画からのずれ: 計画は `hide_request.rs:5` について「『ウィンドウ生成の制約』の指しは残し、後半だけを付け替える」としていたが、差分は括弧全体を「閉包の射程は `hide_egui_main` の doc」に置き換えた。旧 bullet は「ウィンドウ生成の制約」節の下に無かった（「Win32 / Tauri 注意事項」の下だった）ので旧い指しがそもそもずれており、害は無い。偽にする: `git show 42b77e01:src-tauri/src/egui_shell/launcher_controller/hide_request.rs | sed -n 1,8p`
- L-7 A1（`[lib]` 無しゆえ `cargo test -p snotra --lib` は失敗する）の 5 の理由「正しい形は `docs/build-commands.md` カテゴリ A」は、正しい形は示すが落とし穴を示さない（`git grep -- "--lib" docs/build-commands.md` は 0 件）。2 セッション連続で踏んだという記録は着地先なしで消える。失敗が即時で自己説明的、という理由で 5 は成り立つ。偽にする: build-commands か他の生きた層が `--lib` に触れていることを示す

### ⚠️（確信の持てない所見）
- W-1 `launch_item_core` の新コメント「呼び出しスレッドの COM 状態は当てにせずここで初期化する」（`commands/launch.rs:193-194`）は言い過ぎの可能性。呼び出しスレッドが既に MTA なら `CoInitializeEx(STA)` は `RPC_E_CHANGED_MODE` で `is_ok()==false` になり、STA にはならない（＝当てにしている）。旧本書は「`std::thread::spawn` で新規スレッドに COM 環境を用意する」と書き、rules の旧行も `std::thread::spawn` を含んでいたが、新 rules 行と新コメントからはスレッドの条件が消えた。現行の呼び出し元（egui は worker・トレイは platform スレッド）で MTA 初期化は見当たらない（`CoInitializeEx` の出現は launch.rs の 1 か所のみ）ので実害は未確認。偽にする: 呼び出し元のスレッドがいずれも COM 未初期化であることを列挙し、コメントを「未初期化のスレッドで呼ぶ前提」と読めば真であることを示す
- W-2 X19 の「Tauri コマンドハンドラスレッドは COM 状態が保証されない→新規スレッドで」という条件は台帳 X19（→`launch_item_core` のコメント）に載るが、移設先は条件を持たない。Tauri コマンドハンドラは #532 SU7 で消滅しているので 5 とみなせる可能性が高い
- W-3 `G-heading-refs.mjs:16` は「節ではなく箇条書きのリード文を指す参照の実例: `src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」」と書くが、旧版でもこれは `###` 見出しだった（本 PR 以前からの不一致で、本 PR はそれを `##` にしただけ）。本 PR の所見ではない可能性が高いが、本書を指す散文の例示として腐っている

## 確認済み（所見なし）
- I5: 呼び出し点 5 は `commands/system.rs:13`・`commands/window.rs:167,168,179`・`window_coordinator::read_indexing`。いずれも `load(SeqCst)` → `is_indexing()`/`is_index_build_started()`（SeqCst）で同値、read_indexing は Relaxed→SeqCst（強める向き）。書き手は `new` / `try_begin_index_build` / `finish_index_build` だけ（`state.rs:69-149`）
- 新 doc の主張とコード: `HandleGuard`（`working_set.rs:58-59` の `impl Drop`）・`EguiShellState::main_waker` は private、`EguiShellHandles` の waker は `pub(crate)`（`egui_shell/mod.rs:317-323`）、外部からの `.wake()` は `wake_main`/`wake_results`（`window_coordinator.rs:619-636`）だけ・`commands/mod.rs` の返り値契約（旧本書と同値、`ERR_INDEXING_IN_PROGRESS` 実在）・X5（`commands/window.rs:100-101` が main に `set_always_on_top`）・`SnotraPlatformWindow` の所有（`platform_thread_loop` 内の生成）・`wake_main` doc が hidden 中の配送抑止の機序を持つ（`window_coordinator.rs:611-615`）
- 移設先の実在: M2（`indexing.rs:3,20`）・M5/M6（`config_watcher.rs:103-104,163-167`）・M11（`activation/tests.rs:1-10`）・M14 の「OS からは非クライアント分と scale しか読まない」（`window_coordinator.rs:662`）・M15（`window_coordinator.rs:921`）・M16（`monitor.rs:78`）・I2（`commands/window.rs:99-101,158-159`）・N1/N3（`wndproc.rs:20-30`）・G4 帰結（`main.rs:292-309`）・V1/V2/V3（`visual.rs:1-23`・`window_coordinator.rs:166-173`）・V4（`search_input_ui` doc）・V5（`clippy.toml:21`）・W2（`snotra-egui-runtime/CLAUDE.md:34`）・X6（`results_window.rs:247-252`）・X12（psm1:14 の H1）・X20（`window_coordinator.rs:405,445`）
- advisor 指摘の追加確認: `PreparedHotkey`（`platform/hotkey.rs:19`、`injection_vks` を持つ）・`execute_instant_action_core -> LaunchResult`（`instant.rs:14`）・`ResultsWindow::hide(&self, _el: &EventLoopProof)`（`results_window.rs:178`）・`drive_results_window(.., el: &EventLoopProof)`（`window_coordinator.rs:1407-1409`、doc :1404 に `RuntimeFrame::event_loop()`）・I2（`launch_settings_process` の doc `commands/window.rs:19-23` が重複起動の防止と topmost の一時解除を持つ。`SettingsProcessState` は型エイリアスで doc は 1 行だけだが、命題は関数 doc 側にある）
- 作業ツリー: 自前のビルドディレクトリ target-review は削除済み。未追跡は `.delegate/` だけ
- 付け替えの着地: `docs/architecture.md:90`（listener 登録 → hotkey 登録の順序）・`events.rs` / `visual.rs` / `commands/mod.rs` の `//!`
- 取りこぼし: `git grep "src-tauri/CLAUDE.md"`（ADR・superpowers・design・workspace 除く）の残りはすべて現存見出し（「処置を返す純粋核の強制」「イベント駆動 wake の不変条件」「trace の presence…」「ウィンドウ生成の制約」）かパスのみ。消えた見出し名の grep は 0 件（`commands/mod.rs:3` の `//!` 見出しと SPEC の括弧書きのみ）。`src-tauri/src` のパス無し `CLAUDE.md` は 0 件
