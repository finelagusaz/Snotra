# src-tauri

Tauri v2 バイナリ crate。検索 UI（`egui_shell/`・egui + softbuffer）と Win32 API 統合を担当（WebView2/フロントエンドは #532 SU7 で撤去）。**`[lib]` を持たないため `cargo test -p snotra --lib` は常に失敗する**（`error: no library targets found`。2026-08-03 に 2 セッション連続で踏んだ）——テストは `--lib` なしの `cargo test -p snotra`、絞り込みはテスト名フィルタか `--bin snotra` で行う。

各ルールは「**太字 = 守る指示**、後続 = 理由・経緯」の形式。迷ったら太字部分に従えば安全。

## モジュール構成

責務を持つ個別モジュールの責務宣言は各ファイルの `//!`（module doc）を正本とする（薄いラッパーを集約記述する `commands/`・`platform/` は責務を本節に直接記す例外）。本節はファイル一覧と、`//!` に収まらない**横断不変条件・チェックリスト**を記す（#562）。

- `main.rs` — エントリポイント・Tauri セットアップ・イベントリスナー登録（責務は `//!`）
- `state.rs` — Tauri managed state `AppState`（責務・構成は `//!`）。以下はビルドフラグの規律:
  - **インデックスビルドの開始/終了は `try_begin_index_build()` / `finish_index_build()` メソッド経由で行う** — `indexing`・`index_build_started` を coherent に更新する
  - **config 変更→index 再構築のコヒーレンシ判断は engine の `index_stale` ledger（軸1）に閉じており、この 2 AtomicBool は二重ビルド防止（CAS）と UI 表示専用に純化されている**（#347/#348-A）
- `icon.rs` — アイコンのオンデマンド抽出とキャッシュ永続化（責務は `//!`）。**`invalidate_icon_cache` はメモリ内 `IconCacheState` と `icons.bin` を単一 lock 内で両方無効化する** — lock 外でファイル削除すると、並行ロード（None 検知 → `icons.bin` 再ロード）が削除直前の旧ファイルをメモリへ戻す TOCTOU が起きる（#522、実測 17/2000 回）。片方だけだと終了時 `save_if_dirty` で古いアイコンが復活する
- `indexing.rs` — バックグラウンドインデックス構築（責務は `//!`）。以下は drain / panic 戦略の不変条件:
  - **`start_index_build` は `mark_index_stale`（CAS の前）→ CAS → spawn の順**で、**drain ループ**（`begin_index_drain` で現在 config の `IndexInputs` snapshot → ロック外で `rebuild_and_save` / `PrebuiltIndex::new` → `complete_index_drain` で swap + re-diff）を stale が消えるまで回す
  - **ビルド本体は `catch_unwind` で包む（panic 戦略依存）**: unwind ビルド=debug/test では panic を捕捉し `finish_index_build` で flag 固着 wedge を防ぐ。release は Cargo.toml で `panic="abort"` のため build panic はプロセス abort＝ここに来ないが silent wedge にもならず、再起動で fresh build される。どちらでも UI 永久構築中は起きない
  - **finish 後に `is_index_stale` を再チェック**し、finish 窓で刺さった変更を再 kick で拾う。**unwind の panic 経路では再 kick しない**（決定論 panic の無限リトライ回避）
  - config 変更→index 再構築のコヒーレンシは engine の `index_stale` ledger に一元化（#347/#348-A）
- `config_watcher.rs` — `config.toml` 監視（100ms debounce）と `apply_config_change()` による反映（責務は `//!`）。以下は適用の不変条件と発火イベント:
    - **不変条件: `LoadOutcome::ReadFailed`（一時的・環境的な read 失敗）では `apply_config_change` は何も適用せず早期 return する**（`should_apply_config_change()` で判定）。fallback-default を実行中エンジンへ適用すると、live-read 化した履歴剪定が `history.bin` をデータ損失させ、index 再構築判定（`IndexInputs` 差分）が default scan で誤再構築を起こすため。`Config::load` の「一時的失敗は退避も上書きもしない」保全を適用側にも揃える（#348）
    - ただし早期 return の前に短いバウンドリトライ（`load_with_read_failed_retry`、既定 3 回 × 150ms）で一時的ロック解除を待ち、解ければ正規の変更を取りこぼさず適用する（予算超過時のみ skip。リトライ中は適用しないのでデータ損失安全は不変）
  - **不変条件: index 再構築の要否は `IndexInputs::from_config(old) != IndexInputs::from_config(new)` で判定し、ビルド進行中（`indexing`）でも `!indexing` ゲートなしで常に `start_index_build` を kick する**（`start_index_build` が `mark_index_stale` で stale を立て、in-flight ビルドの drain / finish 後再チェックが取りこぼしを拾う。CAS が二重起動を防ぐ。#347/#348-A）
  - 発火するイベント: `hotkey-registration-failed` / `indexing-started`（indexing.rs から）/ `indexing-complete`（indexing.rs から）/ `config-applied`（egui wake・値なし・SU6）。旧フロント向けの値運搬 emit 群（language-changed 等 7 本）は #532 SU7 で削除——egui は config-applied wake + 毎フレーム live-read で値を拾う
- `events.rs` — アプリ内 Tauri イベント名の定数（責務は `//!`）
- `ime.rs` — IME をオフにする Win32 IMM API の薄いラッパー（責務は `//!`）
- `trace.rs` — `SNOTRA_TRACE` 環境変数ゲートの構造化トレースログ（責務は `//!`）
- `monitor.rs`: マルチモニター対応の Win32 ヘルパー（`GetCursorPos` / `MonitorFromPoint` / `GetMonitorInfoW`）。物理座標ベースで作業領域を取得し、ウィンドウ位置のクランプ・中央配置を提供。**基準モニターは必ず点から決める**（`MonitorFromWindow` を使う `window_monitor_work_area` は #835 で消えた）
- `working_set.rs` — 非表示アイドル時のプロセスツリー working set 回収（Windows のみ・非 Windows は no-op。責務は `//!`、適用の詳細は本ファイル「working set の能動回収（EmptyWorkingSet）」）
- `commands/`: ディレクトリモジュール（`mod.rs` + `launch.rs` / `icon.rs` / `window.rs` / `system.rs` / `instant.rs`）。egui view・トレイが共有する core 関数群（旧 `#[tauri::command]` ラッパーと `search.rs` / `config.rs` は #532 SU7 のフロント撤去で消滅）。`launch.rs` は `launch_item_core` / `launch_with_tool_core`（いずれも `pub(crate)`、`instant.rs`・`egui_shell/launcher_controller.rs` から再利用）に加え、トレイメニューからの起動用に `launch_item_with_state` / `launch_with_tool_with_state` / `launch_default_with_state` / `resolve_all_openers` を `pub` で公開
- `platform/`: ディレクトリモジュール（`mod.rs` + `hotkey.rs` / `tray.rs` / `wndproc.rs`）。Win32 メッセージループスレッド + トレイアイコン + ホットキー + ウィンドウプロシージャ。`hotkey.rs` は core の `ParsedHotkey` だけを Win32 modifier/VKへ変換し、永続文字列を再解釈しない。登録と smoke 注入用 `vks` は同じ変換結果から導く
- `egui_shell/`: ディレクトリモジュール（`mod.rs` + `lifecycle.rs` / `search_state.rs` / `layout.rs` / `icon_textures.rs` / `notify.rs` / `strings.rs` / `view.rs` / `launcher_controller.rs` / `results_view.rs` / `results_window.rs` / `visual.rs` / `window_coordinator.rs` / `font_stack.rs`）。製品メインウィンドウ（egui/softbuffer）の外殻 + 検索体験（#532 SU2〜SU7・flip 済みで唯一の UI 経路）。以下はファイル別の索引と、`//!` に収まらない横断不変条件:
  - `font_stack.rs` — フォント解決と `set_fonts` 登録（責務は `//!`）
  - `launcher_controller.rs` — 検索セッション層（show を跨ぐ状態・結果・選択・起動・履歴・期限）の所有者（責務は `//!`）
  - `lifecycle.rs` は純粋核（`plan_hotkey` / `blur_should_hide`）
  - `search_state.rs` は検索状態の純粋核（`SearchState` / `interpret` / `QueryIntent`）
  - `layout.rs` は高さ算出 + results 可視性の導出 + 幾何 + debounce + テキストの中間省略の純粋核（`Metrics` / `results_window_height` / `present_results` / `results_top_y` / `size_delta_exceeds` / `Debouncer` / `truncate_middle_chars` / `fit_middle_by_measure`。旧 `compute_window_height` / `HeightParams` は #646 PR2 で撤去済み・旧 `results_should_show` は #752 で `present_results` へ吸収・旧 `clamp_results_height` / `available_below` は #835 で撤去〔`ADR-results-fixed-height`〕）
  - `icon_textures.rs` — アイコン・テクスチャ層の純粋核（責務は `//!`）
  - `notify.rs` — 通知 primitive の純粋核（責務は `//!`）
  - `strings.rs` — UI 文言テーブル（責務は `//!`）
  - `view.rs` — main 窓の 1 フレーム（入力の読み・描画・OS 窓への適用。責務は `//!`）
  - `results_view.rs` — 結果リスト窓の従属 view（責務は `//!`）
  - `results_window.rs` — results 窓の所有型（責務は `//!`）
  - `visual.rs` — テーマの 1 フレーム分の読み取り値と純粋な導出（責務は `//!`）
  - `window_coordinator.rs` は窓を駆動する責務（main の show/hide（両窓同期）・位置永続と復元・results の毎フレーム driver・wake primitive。**z-order は含まない**——`commands/window.rs` と `ResultsWindow` が持つ。**main のサイズは 2 か所で設定する**——show 経路（ここ）と毎フレーム（`view.rs`）。**両者は同じ高さを導く**: status 行の有無は `status_row_present` を、積算は `main_window_height` を共有する。**main の位置に基準モニターを判断する箇所は 3 つある**（#738）——show（`position_on_target_monitor`）・可視中のクランプ（`clamp_main_into_work_area`。呼ぶのは `view.rs` だが**ポインタ非押下のフレームに限る**——reset-on-show の backstop は実測で却下した・`ADR-main-window-clamp-on-pointer-release`）・hide 時の保存（`read_placement_relative`）。**材料はどれもバー高で共通である**（実高ではない——理由は `layout::bar_rect_height_phys` の doc）。**基準モニターは 2 対 1 に分かれる**: クランプと hide 保存は**バー矩形の中心**が乗るモニター（**両者は `read_bar_anchor` という同じ 1 つの関数を通る**——一致を doc の申し合わせではなく構造で担保する）、show だけがカーソル/プライマリである。show が違うのは「これから出す窓をどこへ置くか」であって既存の窓を戻す話ではないため。**`MonitorFromWindow`（窓全体の矩形）を使ってはならない**——status/toast で伸びた分の重なりで隣モニターが選ばれ、**行の出没でバーが飛ぶ**（正本は `monitor::point_monitor_work_area` の doc）。**唯一の例外だった `results_available_height` は #835 のクランプ撤去で消えた**ため、この crate に窓の矩形から**位置決めの基準モニター**を決める経路はもう無い（`snotra-egui-runtime/src/monitor.rs` は `MonitorFromWindow` を使うが、リフレッシュレートの取得であって位置決めではない）。show 側は reset-on-show 後の状態をリテラルで渡す（畳む高さと描く高さが食い違っても、memo リセットが同じフレームの動的高さ算出で直すため固着はしない——ずれはその 1 フレームだけのスナップとして現れる〔#755 / #801〕。反転の経緯は `ADR-show-path-derives-drawn-height`）・#749）
  - `mod.rs` — 窓生成（main/results 両窓）・共有状態・config の 1 フレーム読み・listener 登録（責務は `//!`）
  - **外部から窓を起こす経路は `wake_main` / `wake_results` の 2 本であり、shell は `egui::Context` を保持しない**（#671 PR D）: wake handle（`snotra_egui_runtime::WindowWaker`）は `create()`（= `attach` の戻り値）から `EguiShellState` へ渡る。**ただし表現不能化ではない**——`EguiShellState` の `main_waker` / `results_waker` は `pub(crate)` ゆえ、crate 内から `app.try_state::<EguiShellState>()` を引いて直接 `.wake()` を呼ぶ書き方はコンパイルが通る（正しい経路を 2 本に定めただけ。results の raw 操作と同じ性格で、wake 経路に計装や dedup を足すなら直呼びの grep も要る）。**managed state に `egui::Context` の clone を置いてはならない**——Context の clone は repaint callback ごと複製し、callback が握る `RepaintScheduler` の Arc が窓の `Destroyed` を越えて worker の停止・join を止める（PR D 以前の実在した破れ・`snotra-egui-runtime/CLAUDE.md` の不変条件）。**自窓の Context を持っている場所（view の `update()` 内・worker へ渡した clone）では従来どおり `ctx.request_repaint()` が正しい**——`WindowWaker` は外部スレッド・別窓からの wake のための経路である
  - **イベント駆動 wake の不変条件（#532 SU5）**: runtime はイベント駆動（`RedrawRequested` 待ち）で通常フレームは勝手に回らない。**フレームの paint より後（遅延 dispatch・クリックハンドラ）や worker スレッドで UI 状態を変えたら、必ず `ctx.request_repaint()` で次フレームを起こす**——欠くと次の無関係な入力まで stale 表示が残る（toast dismiss で実測・PR #647 の e746826 で修正。folder/icon worker の送信毎 repaint と同根）。また **hidden 中は `update()` が走らない**（実測・SU5 要石。機構は tao/OS 層の配送抑止＝worker が送った `RequestRedraw` が hidden な窓には `RedrawRequested` として届かない・#697 実測）——時限処理（timeout・通知期限）の `request_repaint_after` は可視中しか効かず、hide を跨ぐ in-flight 状態は reset-on-show の backstop（クリア）とセットで設計する（blur 猶予は #745 で `BlurGrace::reset` として合流した。**そこに残る受容残余は同 doc が正本**）
  - **期限を待つ状態（armed）は、条件が成立するか解除されるまで毎フレーム残余を再要求する**（#711）: `request_repaint_after(d)` は**フレームの到来を約束しない**（機構は `snotra-egui-runtime/CLAUDE.md`「不変条件」）。1 回きりの予約に賭けると hide や再検索が次の無関係な入力まで宙吊りになる。**armed 期限は 4 つ**（検索 debounce・一時通知・起動タイムアウト・blur 猶予）で全数がこの形。**再要求してよいのは「時間経過で解消する不成立」だけである**——フォーカス・設定値・他プロセスの生死のように時計と無関係な条件で再要求すると `request_repaint_after(ZERO)` の永久スピンになる。それらの変化は**変えた側が wake する責務を負う**（上の規範）
  - **テーマ色・font・行高の読みは 1 フレーム 1 回（#673 spec 決定 4）**: `read_visual` が返す `VisualSnapshot` を各 view の `update()` で 1 回だけ取り、以降はそれを読む。**`[visual]` 全体ではない**——`window_gap` は `position_results_below_main` が別に読む（`Moved` リスナーとも共用しフレームに閉じないため・意図的な除外）。**同じ値を後段で config から読み直さない**——間に `config_watcher` の適用が挟まると、同じフレームの中で新旧が混ざる（新 `font_size` を旧行高で描く等）。**snapshot を `self.` へ保持してもならない**（毎フレーム live-read が config 変更の反映経路そのもの・#576 / #646 決定 2）。導出式の正本は `layout::Metrics::from_config` と `layout::path_size` で、guard 内からそれを呼ぶ。**色のパーサは 1 本である**（#680 の 1 を解消・spec `docs/superpowers/specs/2026-07-28-config-background-color-design.md` 決定 4）——`egui::Color32::from_hex`（`#RGB` 等も受理）だけを使い、tao のネイティブ背景ブラシへは `visual::native_brush_color` が `Color32` から変換する。**ブラシ側の alpha は 255 に固定する**（softbuffer の clear color が `0x00RRGGBB` で alpha を持てず、下地と定常が食い違うため。両者は同じ `Color32` から導くので必ず一致する）。**`#RRGGBBAA` の alpha は「無視」されない**——`from_hex` が RGB を alpha で premultiply する（正本は `visual.rs` の `background_color_premultiplies_alpha_rather_than_ignoring_it`——実際に測っている唯一の場所）。2 本立てだった頃は `#FFF` が描画色だけ通り下地は既定色へ落ちていた。**背景色だけは style を経由しない**——`RuntimeFrame::set_clear_color` が `run_ui` → paint の順序に乗るため同じフレームに届く。**style を経由する 3 値（`extreme_bg_color` / `selection.bg_fill` / `weak_text_color`）も同じフレームに届く**（#751。経路は別のままで、到達フレームの非対称だけが消えた）——ただしそれは **`ui.visuals_mut()` で適用しているからである**。**`ctx.set_visuals` を使ってはならない**: egui 0.35 の root `Ui` は pass 冒頭で `ctx.global_style()` を `Arc` snapshot するため、そこへの書き込みは次の pass からしか効かず、色だけを変えた config 適用フレーム（＝次フレームが来る保証の無い状況と一致する）で入力欄だけが旧色で残る。**帰結が 2 つある。**（1）**`update()` 内の適用位置が correctness の条件になった**——**visuals を読む最初の操作**（ウィジェットの描画・子 Ui の生成）より前に置くこと（「最初のウィジェットより前」ではない——`ui.interact` は `create_widget` を呼ぶが visuals を読まないので上にあってよい）。**この順序に検知手段は無い**（コンパイラ・ユニットテスト・`check:colors`・smoke のいずれも捕まえない受容残余で、正本は `view.rs` の適用点のコメント）。（2）**global style はもう 3 値を持たない**——main 窓へ新しく egui コンテナ（`Area` / `Window` / `CentralPanel` / popup / tooltip）を足すなら、**その Ui へ自分で visuals を渡すこと**。それらは `ctx.global_style()` から Ui を作るため、既定色で描かれる。却下した代替案（runtime のフック・`request_repaint` の対症療法・両方書く・検知器を置く）は `ADR-visuals-application-target`
- **trace の presence 検査は状態の検査ではない**（#671 PR A′）: 「操作を要求した」ログは、その操作が効いたことを意味しない。`egui_results:hide` は出るのに窓は残る、という回帰を `smoke:egui` が緑のまま通した。trace で不変条件を守るなら「何が起きたか」ではなく**「起きてはならないことが起きていないか」**（区間内に事象が現れないこと等）を書く

## 実装パターン

- ホットキーは `RegisterHotKey` を `platform/` の Win32 メッセージループスレッドで処理し、`AppHandle.emit()` で Tauri イベントとして通知
- 設定画面は `snotra-settings.exe`（egui 別バイナリ）を子プロセスとして起動。`SettingsProcessState` で重複起動を防止し、子プロセス存命中はメインウィンドウの最前面表示（`set_always_on_top`）を一時解除する
- `commands/` は薄い共有関数に保ち、実処理は `snotra-core` に寄せる（KISS）
- `AppState` は `Mutex<Engine>` で検索エンジン・履歴・設定を一括管理。Phase 2.3 以前の 3重ロック（`Mutex<SearchEngine>` / `Mutex<HistoryStore>` / `Mutex<Config>`）は Engine facade に統合済み
- **インデックスビルドのフラグは `AppState` のメソッド経由で更新する**: `try_begin_index_build()`（`index_build_started` を CAS 取得 → `indexing` を立てる）と `finish_index_build()`（両方を戻す）が唯一の正しい経路。`indexing` / `index_build_started` を直接 `store()` しない——外部からの force-reset は走行中ビルドのガードを踏み倒す競合の原因になる。2フラグは別物（`index_build_started` は CAS 専用ガード、`indexing` は first-run 時にビルドスレッド不在でも true になる UI 表示用）
- Managed state として `IconCacheState`（`Mutex<Option<IconCache>>`、初回アイコン要求で遅延初期化）と `SettingsProcessState`（`Mutex<Option<Child>>`、設定プロセスのハンドル管理）を保持
- **show の操作順序制約（`egui_shell::show_egui_main`）**: `set_size`（バー高）→
  `position_on_target_monitor` → `set_size`（実高。最初のフレームが描く高さ）→ `show()` の順。
  位置計算はウィンドウサイズを OS から読み戻してクランプするため、位置決定に使う 1 回目の
  `set_size` はバー高固定のまま不変——**バーの位置はユーザーが決めるものであり、status 行・
  toast 行の出没で動かしてはならない**。実高への 2 回目の `set_size` は位置決定の後に置くこと
  で、show 後に窓が伸びる／縮んでから伸びる（#755 / #801）を消す

## 共有 core 関数の返り値契約

旧 IPC コマンドの3系統規約（#434）のうち、フロント撤去（#532 SU7）後も残る契約:

1. **読み取り・検索系**: 素の `T` を返す。エラーは DTO 内の `is_error` フラグ + UI 層で表示文字列を決定する（`snotra-core` の設計と整合）
2. **起動系**: `LaunchResult { status, code, message }` 契約（`launch_item_core` / `launch_with_tool_core` / `execute_instant_action_core`）
3. **失敗しうる操作系**: `Result<T, String>`。「実行できない状態」（インデックス構築中など）も `Err(定数)` で表現する。例: `open_settings` / `rebuild_index`

`bool` 返しは新規関数で使用しない（「成功/失敗」と「実行できない状態」を混同しやすい）。「インデックス構築中で実行できない」の定数は `ERR_INDEXING_IN_PROGRESS`（`commands/window.rs`）を `open_settings` / `rebuild_index` で共有する。新たに「実行できない状態」を追加する場合もこの定数を再利用するか、命名パターン（`ERR_<状態>`）を揃える。

## Win32 メッセージ配送の注意

Shell のトレイコールバック (`uCallbackMessage`) は `SendMessage` で配送される場合があり、`GetMessageW` ループに到達しない。カスタムメッセージ (`WM_APP + N`) をウィンドウプロシージャ (`DefWindowProcW`) だけで処理すると消滅するため、`platform_default_wnd_proc` で検出して `PostThreadMessageW` でスレッドキューに再投入する設計にしている。

**`app.listen` のコールバックは emit した呼び出し元スレッド上で同期実行される**（tauri 2.11.4 の `event/listener.rs::emit_filter` が別スレッドへ dispatch せず直接呼ぶ・実測）。ゆえに listener の中身は「emit 元のスレッドで走るコード」である——Win32 メッセージループスレッド（hotkey）・config 監視スレッド・index build スレッドが、そのまま managed state や窓 API を触る。**listener を足すことは worker を足すことと同じ**であり、並行境界として扱う（→ `/race-check`）。

NOTIFYICON_VERSION_4 では、キーボード操作（Shift+F10 / Application キー）によるコンテキストメニュー要求は `uCallbackMessage` を経由せずウィンドウプロシージャに直接 `WM_CONTEXTMENU` として届く。`platform_default_wnd_proc` で同様に再投入することで `handle_tray_message` に統一している。

**Win32 メッセージハンドラを削除・変更する前に「そのメッセージが届く全経路」を列挙すること。** 同一メッセージでも発火源が複数ある場合がある（例: `WM_CONTEXTMENU` はマウス右クリック環境とキーボード操作の両経路で届く）。「問題の原因になっている経路」だけを削除しようとすると、問題でない別の経路も同時に消える。

## ウィンドウ生成の制約

ウィンドウの生成は必ず setup フェーズで行い、ランタイムでは show/hide のみで制御する（メイン窓は `egui_shell::create`・setup 限定）。イベントループ中のコールバック（`run_on_main_thread` / `listen` / `RunEvent` 等）はメッセージポンプが 1 イテレーション内で停止しており、ポンプ進行を要する操作（ウィンドウ生成・COM STA 初期化・モーダルダイアログ等）はデッドロックする——「メインスレッドにいる」と「メッセージポンプが自由に回る」は別物（旧 WebView2 期に実測した不変条件・egui 窓でも生成は setup 限定を維持）。メインウィンドウは `decorations: false` で閉じるボタンを持たないため `CloseRequested` ハンドラは不要。

**setup フック自身もイベントループの中で走る**（#671 PR D で一次資料を確認・tauri 2.11.4 `src/app.rs` の `make_run_event_loop_callback` が `RuntimeRunEvent::Ready` の arm で setup を呼ぶ）。**「setup はイベントループより前」ではない。** 帰結が 2 つある:

- setup ブロックの実行中は wry plugin の `on_event` が回らないため、**egui フレームは 1 枚も走らない**。ゆえに窓生成（`egui_shell::create`）より**後**に managed state を載せてよい（`EguiShellState` の manage 位置がこれに依る）
- 上段のポンプ停止の話は setup にも当てはまる。setup 内で「ポンプが回ること」を期待する操作（`run_on_main_thread` の完了待ち等）を足してはならない

## working set の能動回収（EmptyWorkingSet）

`working_set::trim_idle_working_set()` が hide 経路（`egui_shell::hide_egui_main` 合流点）で Win32 `EmptyWorkingSet` をプロセスツリー（自プロセス + 子孫。設定サイドカー存命中はそれも含む）へ能動適用し、hide 直後の物理 RSS を即時に落とす（egui-hidden の PrivWS ~1MiB・#532 SU6.5 実測。旧 WebView2 suspend 層は SU7 で消滅）。

- **show 側に逆操作は不要**: trim されたページは show 時に OS が透過的に re-fault する。明示 untrim API は存在しない。trim が hide 前後どちらで走っても無害（再 fault するだけ）
- **best-effort・物理 RAM のみ**: Toolhelp / `OpenProcess` / `EmptyWorkingSet` の全失敗は黙ってスキップ（機能影響ゼロ）。HANDLE は RAII ガードで解放。削減対象は working set であって commit ではない。`EmptyWorkingSet` はスレッド非依存で任意スレッドから呼べる

## フォント登録（混在スクリプトのベースライン）

Latin と CJK が混在する行のベースラインずれは、**softbuffer 期に固有の顕在化条件**を持つ。規則自体は `egui_shell/font_stack.rs` の `font_definitions_*` テスト群が固定しているので、ここには**なぜ壊れるか**だけを置く（規則の正本はテストと `font_stack.rs` の `//!`）。

- **前提**（混在を 2 フォントで積むとずれること自体）は `snotra-settings/CLAUDE.md`（#399）が正本
- **softbuffer 固有の増幅**: `raster.rs` の `fill_mesh` は**カバレッジ AA を持たない**ため、2 フォント間の分数 px のベースライン差を整数 px へ丸めて**目に見える段差**にする。glow / wgpu 期は sub-pixel AA が同じ差を吸収して隠していた——**描画バックエンドを替えたことで顕在化した**類のバグである
- **再発の経路**: フォント登録を書くたびに「末尾 fallback（`push`）で足す」形へ戻り、#399 → #579 と繰り返した。**新しく bin や窓を足すときに再導入されやすい**
- **ゆえにフォント登録に触る変更では `cargo run -p snotra` の目視（`docs/build-commands.md` カテゴリ D）を省略してはならない**（検知手段が視覚スモークだけであること・受容残余は `snotra-settings/CLAUDE.md` と `SPEC.md` のフォント節が正本）

## Win32 / Tauri 注意事項

- Win32 関連の不具合では、まず `config.toml`（テーマ含む）を確認し、次にウィンドウライフサイクル順序、最後に API 呼び出しを調査する（白画面バグの真因がテーマ設定だった事例あり）
- Rust クレートをバージョン昇格する際は、対象バージョンが crates.io に実在・正当であることを確認する。大版ジャンプを前提にしない（例: `bincode 3.0.0` は `compile_error!` のみを含むジョークパッケージでコンパイル不能）
- `windows` クレート（現在 v0.62）はバージョンごとに API シグネチャが変わる（`Result` 型の有無、ハンドル型の変更など）。コードを書く前に、使用中のバージョンで対象 API が利用可能か・型が一致するかを確認する
- **宣言的なウィンドウ属性（`focusable(false)` 等）で挙動を代替させる判断は、その属性を読む側の「全分岐」を確かめてから確定する。** tao はスタイル計算（`window_state.rs` の `to_window_styles`）で `!FOCUSABLE → WS_EX_NOACTIVATE` を付ける一方、`apply_diff` の `ShowWindow` 分岐は**別の条件**（`MARKER_DONT_FOCUS`・窓生成時に 1 回だけ立ち初回 show で消費）で `SW_SHOW`（活性化する）と `SW_SHOWNOACTIVATE` を選ぶ。前者だけ読んで「この属性で足りる」と結論すると、**クリックでは奪われないのに表示で奪われる**非対称を踏む（#646 PR2・実機スモークでのみ露見）。属性が効く経路と、同じフラグを読む他の経路は別物である
- **ある窓の show / hide / topmost のいずれか 1 つが tao を迂回したら、残り 2 つも必ず迂回側へ寄せる。混在は許されない。** `apply_diff` はフラグ差分がゼロなら早期 return し、`VISIBLE` を持たない窓には `SW_HIDE` を副作用で撃つ。片方だけ raw にすると「`hide()` が何もしない」「`set_always_on_top` で窓が消える」が同時に生まれる（#646 PR2）。窓ごとの層は次で固定する:
  - main（主窓）= 3 操作すべて tao 経由（tauri `show` / `hide` / `set_always_on_top`）
  - results（従属窓）= 3 操作すべて raw（`SW_SHOWNOACTIVATE` / `SW_HIDE` / `SetWindowPos`）。実装は `egui_shell::ResultsWindow` に集約する（#671 PR A′）。**ただし表現不能化ではない**——`Manager` から results の生ハンドルを引いて `.hide()` を呼ぶ書き方は依然コンパイルが通り、黙って no-op する（正しい経路を 1 つにしただけ・spec §7-1）
  - **「main の show だけ raw にして統一する」は禁止。** main の tao `VISIBLE` が stale 化し、`set_always_on_top` が main を消す（`commands/window.rs` の topmost 対称がその瞬間に凶器になる）
  - **新しい操作を raw へ寄せるかの判定基準は「`apply_diff` を通るか」ではなく「フラグ差分が生じるか」である。** `set_size` / `set_position` も `set_window_flags(MAXIMIZED=false)` 経由で `apply_diff` に**入る**が、results では MAXIMIZED が元から false ゆえ差分が空になり冒頭 return で助かる（tao 0.35.3 で実測）。ゆえに tao 経由のままでよい。一方**差分を生む操作**（`set_resizable` 等）は `apply_diff` 末尾の `if !new.contains(VISIBLE) { ShowWindow(SW_HIDE) }` に到達し results 窓を消す
  - **可視性は「誰が撃つか」だけでは閉じない。** main が hidden の間に results が出る事故は show 述語側のゲート（`egui_shell::layout::present_results` が `AppState.main_visible` を連言①として合流させる）が塞ぐ。`ResultsWindow` は raw 操作の所有点であって、撃ってよい状況かは判定しない（#671 PR A′ で実機発見）
  - **可視性を変える操作はイベントループスレッドに閉じてある。** `show_egui_main` / `hide_egui_main` / `drive_results_window` / `ResultsWindow::{show, hide}` は `&snotra_egui_runtime::EventLoopProof`（`!Send + !Sync`・crate 外で構築不能）を引数に要求し、**別スレッドからの呼び出しはコンパイルが通らない**。フレームの中は `RuntimeFrame::event_loop()`、外は `on_event_loop` が唯一の口である。**証人を引数から外してはならない**——外した瞬間に「フラグ = false・窓 = 可視」の並び（下の「かつては述語のゲートが…」の項が記す）が再び構築可能になる
  - **上の閉包は表現不能化ではない。閉じたのはその 5 関数であって、`tauri::Window` の生の面ではない。** `Manager` から main のハンドルを引いて `.hide()` / `.show()` を呼ぶ書き方は任意のスレッドからコンパイルが通り、**results と違って実際に効く**（main は 3 操作すべて tao 経由ゆえ `VISIBLE` が正確である）。そのとき `AppState.main_visible` は更新されないため、**results が既に可視であれば最前面に取り残される**——main が hidden の間は `RedrawRequested` が配送されず `drive_results_window` が走らないので、拾い直すフレームが来ない（#671 PR A′ で実機発見した症状）。**results が新たに出ることはない**（同じ理由でフレームが走らない）ので、危険なのは既に可視だった場合に限る。**#880 サイクル段 2 時点でこの書き方の呼び出し点は無く**（main に対する `window.hide()` は `hide_egui_main` の 1 か所のみ。`results_window.rs` の非 Windows fallback は results 窓ゆえ別・grep 実測）、ゆえに現状の欠陥ではなく**受容する残余**である。main を隠す新しい経路が要るなら `hide_egui_main` を通すこと
  - **かつては述語のゲートが「読んだ時刻」しか守らなかった。** ゲートの読みと raw `ShowWindow` の間には Win32 呼び出しが挟まり、`hide_egui_main` が**別スレッド**（hotkey listener は Win32 メッセージループスレッド上で走る——本ファイル「Win32 メッセージ配送の注意」）からその隔たりへ割り込めたためである。ゆえに当時は `results 可視 ⇒ main 可視` を 3 点で守っていた: ①事前ゲート（`present_results`）②事後検査（撃った後に `main_visible` を読み直し、失われていれば撤回する）③hide の権威性（main が可視でないことを理由とする hide は可視フラグを無視して raw 操作を撃つ）。**②と③が要った理由は「フラグ=false・窓=可視」の食い違いである**——その状態で main が消えると、フラグを見る hide は黙って no-op し、main が hidden の間は `update()` が走らない以上、拾い直すフレームが来ない。**②と③は証人型の導入で不要になり、#880 サイクル段 2 で撤去した**——上の 3 点のうち残るのは①事前ゲートだけである（`hide_egui_main` の hide 側同期は別軸として引き続き必要——上の「可視性は『誰が撃つか』だけでは閉じない」の bullet と #646 PR2 決定 6）
  - **この種の race を lock で囲んではならない。** 窓を所有しないスレッドからの `ShowWindow`（`set_topmost` はいまも `commands/window.rs` のポーリングスレッドから撃つ、真の cross-thread 経路である）は所有スレッドのメッセージポンプ待ちでブロックしうるため、イベントループ側が取る lock で囲むと race がデッドロックへ化ける。**当時撤去した事後検査・hide の権威性は、この原則に従い `SeqCst` の全順序（撃った**後**に読み直す）だけで封鎖していた**——両者自体は #880 サイクル段 2 で撤去済みだが、cross-thread な Win32 呼び出しを lock で囲まないという原則は今も有効である
  - **この事故は presence 検査では捕まらない。** 検出器は `scripts/lib/SnotraTraceInvariants.psm1` の H1（hidden 区間に `egui_results:show` が現れたら異常）であり、ユニットテストが測れるのは述語の決定ロジックだけである（raw 操作は `#[cfg(windows)]`）
- `webview2-com` は `windows-core 0.61` に依存するが、プロジェクトの `windows` クレート（v0.62）は `windows-core 0.62` を使う。`Interface::cast()` 等を呼ぶ際は `windows-core_0_61 = { package = "windows-core", version = "0.61" }` のエイリアス依存を使い、`use windows_core_0_61::Interface` とする
- 必要な feature フラグ（`Win32_UI_WindowsAndMessaging` 等）が `Cargo.toml` に宣言されているか確認してから実装する
- `UpdateWindow` など一部 API は windows クレートのバージョンによっては未提供。代替 API（`RedrawWindow` 等）の存在を事前に調べる
- Windows パスの正規化では `C:` と `C:\` の違いに注意する（ドライブルートは末尾 `\` が必須）
- ファイルメタデータ取得時、シンボリックリンクを考慮する場合は `symlink_metadata` を使う（`metadata()` はリンク先を辿る）
- **Win32 の「サイズ取得 → バッファ充填」2回呼び出しパターン**（`ExpandEnvironmentStringsW` 等）では、2回目の戻り値（書込長）を必ず**バッファ長で clamp してからスライス**する（`written.min(buf.len())`）。値が2呼び出し間で伸びると戻り値 > バッファ長になり `buf[..written-1]` が境界外 panic、release は `Cargo.toml` で `panic="abort"` のためプロセス abort に化ける（#394）
- Tauri プラグインの新機能を使う際は `capabilities/*.json` の権限宣言を確認する
- `ShellExecuteW` でフォルダ・画像・文書ファイルを開く場合は COM STA が必要。Tauri コマンドハンドラスレッドは COM 状態が保証されないため、`std::thread::spawn` + `CoInitializeEx(None, COINIT_APARTMENTTHREADED)` + `ShellExecuteW` + `if com_ok { CoUninitialize() }` パターンで新規スレッドに COM 環境を用意する。`is_ok()` は S_OK(0) と S_FALSE(1) を両方 true とし、どちらも CoUninitialize が必要。EXE ファイルは COM 不要なため同問題を起こさない
- `SendInput` はシステム入力キューに注入し、ルーティングはキュー取り出し時に決定される。**フォーカス移行直後の `SendInput` は対象ウィンドウに届かない場合がある**（`SetForegroundWindow` は部分的に非同期）。**この同期待ちに `SendMessageTimeoutW(hwnd, WM_NULL, …)`（Raymond Chen 推奨パターン）を使えるのは、宛先窓が他スレッドの所有であるときだけである**——`SendMessage` 系は宛先が**呼び出しスレッド自身の所有**なら窓プロシージャを直接サブルーチンとして呼んで即座に戻り、キューを 1 通も排出せず、タイムアウトも意味を持たない（＝**完全な no-op**）。ゆえに宛先ごとに当否が分かれる:
  - **tao の窓（main / results）はイベントループスレッドの所有**であり、可視性を変える 5 関数も証人型で同スレッドへ閉じてある（**このパターンの当否を決めるのは宛先窓の所有スレッドと呼び出しスレッドが同じかであって、その閉包ではない**）。**そこを宛先にこのパターンは当たらない**（#880 サイクル段 2 で `show_egui_main` から撤去済み）。**自分のキューが進むのを待つ手段はイベントループのコールバック内には無い**——ポンプを回すことは「ウィンドウ生成の制約」が禁じている。ゆえに `set_focus()` 直後の `SendInput` / IME 操作が依存できるのは**呼び出し順だけ**である
  - **例外は `platform/mod.rs` が `platform_thread_loop` の中で生成する `SnotraPlatformWindow`** である（Win32 メッセージループスレッドの所有）。イベントループ側からそこを宛先にする限り、このパターンは**今も有効**である（`tray.rs` は既に `PostMessageW(hwnd, WM_NULL, …)` をこの窓へ撃っている）
