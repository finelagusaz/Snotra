# 台帳: src-tauri/src/egui_shell/mod.rs（基準 0265463c・候補 61 行・41 ブロック）

分類: 残す／縮める／書き直す／消す。行番号は基準 sha のもの。原文は候補行だけで、ブロックの全文は基準のファイルで読む。

| ID | 行 | 候補行（原文） | 分類 | 処置後の文（要旨） | 根拠（コードの位置・#NNN） | 範囲外メモ |
|---|---|---|---|---|---|---|
| B1 | 1-1 | //! egui/softbuffer メインウィンドウの外枠（#532 SU2〜SU7・唯一の UI 経路）。 | | | | |
| B2 | 5-5 | //! ある**（#749 段 1）——ここに残るのは登録と生成であって、駆動ではない。 | | | | |
| B3 | 15-18 | // launcher_controller が起動 worker の in-flight 追跡・一時通知で消費する（#532 SU5 Task 4）。 ⏎ pub(crate) use notify::{LAUNCH_TIMEOUT, NOTICE_LAUNCH, NoticeSlot}; ⏎ // NOTICE_HOTKEY は launcher_controller（hotkey 失敗通知の duration）が、OverlayKind / ⏎ // overlay_kind は view.rs（status 行の優先ラダー）が消費する（#532 SU6 Task 7・#666 段 3 で | | | | |
| B4 | 24-24 | // （#532 SU5 Task 6）。toast 描画は view.rs が、UpdaterPhase の遷移（install 失敗）は | | | | |
| B5 | 38-38 | // 消費する。ウィンドウ操作の実体は window_coordinator.rs へ移した（#749 段 1）。**モジュール外に | | | | |
| B6 | 55-61 | // （topmost）が消費する（#671 PR A′ spec 決定 2。drive は #749 で view.rs から移設）。 ⏎ pub(crate) use results_window::ResultsWindow; ⏎  ⏎ // view.rs / results_view.rs が毎フレームの描画で消費する（#673 spec 決定 4）。 ⏎ pub(crate) use visual::{RowTheme, VisualSnapshot}; ⏎  ⏎ // results_view.rs の icon texture driver（worker spawn / load_texture 適用）が消費する（#532 SU4 Task 5・#646 PR2 で移管）。 | | | | |
| B7 | 76-77 | // launcher_controller が folder 展開（#532 SU3 M2）・Escape ラダー・Enter flush で消費する。 ⏎ // ViewKind だけは view.rs も読む（入力欄の編集可否と status 行の分岐・#666 段 3）。 | | | | |
| B8 | 82-82 | // SlashCmd/find_slash_command は launcher_controller が command 分岐・slash 実行で消費する（#532 SU3 M3 Task 2）。 | | | | |
| B9 | 86-86 | // （#532 SU6 Task 1・#666 段 3 で消費者が割れた）。**表示と起動が同じ述語を通ることが | | | | |
| B10 | 104-104 | // view.rs が `update()` のフレーム所要と間隔の計測コードとして消費する（#1004 PR 1）。 | | | | |
| B11 | 109-112 | // launcher_controller が Plain 検索の要求送信・結果受信（`drain_search`）で消費する（#1004 PR 2）。 ⏎ pub(crate) use search_worker::{SearchMsg, SearchRequest, spawn_search_worker}; ⏎ // view.rs が UI 文言（hint/overlay/toast）で、launcher_controller が通知文言（起動失敗・ ⏎ // 結果不明・hotkey 登録失敗）で消費する（#532 SU5・言語は lang() が毎フレーム live-read）。 | | | | |
| B12 | 133-133 | /// （listener は `register_initial_hotkey_failure_listener`・#652 Task 4）。 | | | | |
| B13 | 140-144 | /// - hotkey_generation: alt 解放待ち show の世代。hide が bump して保留 show を無効化する（codex #5/(B)#2）。 ⏎ /// - main_visible: main ウィンドウが可視かのキャッシュ。書き手・順序・適用範囲は ⏎ ///   `window_coordinator::MainVisible` の doc。 ⏎ /// - hide_pending: view の emit dedup。show がクリアして「hide 後に Focused(true) が来ず抑止が残る」を断つ（codex #8）。 ⏎ /// - reset_pending: show が立て、view が消費して state.reset()（resetForShow 相当・SU3 M1 Task 9）。 | | | | |
| B14 | 170-174 | /// main ウィンドウを外部から起こすハンドル（`create()` = `attach` の戻り値・#671 PR D）。 ⏎ /// hidden 中は次 show のフレームで toast 等が読まれるため、wake は可視中のみ意味を持つ ⏎ /// （codex レビュー: 「hidden は次 show でよい」と「visible は repaint が要る」は別条件）。 ⏎ /// 旧実装は各 view の `setup` が `egui::Context` の clone を登録する ⏎ /// `Mutex<Option<egui::Context>>` スロットだった——**登録前のウィンドウを「未登録＝no-op」で | | | | |
| B15 | 181-183 | /// results ウィンドウを外部から起こすハンドル（main と同型・#646 PR2 の `results_ctx` の後継）。 ⏎ results_waker: snotra_egui_runtime::WindowWaker, ⏎ /// hotkey 登録失敗の pending payload（SU6 spec 追補 2 + #652）。種別ごとに文言が違う | | | | |
| B16 | 188-190 | /// emit する順序——wake 時の lang() live-read は必ず新言語を読む（旧 `language-changed` ⏎ /// 先行発火の不変条件は #532 SU7 の emit 削除で消滅し、この順序が後継の根拠）。 ⏎ /// Initial は wake する（config 変更が随伴せず config-applied が来ないため・#652・SU6.5 決定 2）。 | | | | |
| B17 | 196-196 | /// 無いのに wake できる状態を作らないため）。他フィールドは従来の既定値。 | | | | |
| B18 | 217-217 | /// updater toast の managed 状態（#532 SU5）。view が毎フレーム読む level-triggered | | | | |
| B19 | 242-242 | // 止めるのにもこれを使う。判定より後ろへ動かす・撤去するときは、そちらも直す（#1306）。 | | | | |
| B20 | 318-318 | /// `create()` が setup へ引き渡す所有物（#671 PR D・spec 決定 8 の終端形）。 | | | | |
| B21 | 322-324 | /// 前に何が載っているか）を `main.rs` の 1 画面に残すため（spec 決定 8）。 ⏎ pub(crate) struct EguiShellHandles { ⏎ /// results ウィンドウの所有型（生 Win32 の show/hide/topmost・#671 PR A′）。 | | | | |
| B22 | 334-339 | /// 宣言によるウィンドウは 1 つも無い。ここが与える全プロパティ（52px 高は初期値〔SU3 で show 前折り畳み + 結果表示時に動的リサイズ・view.rs〕・width は config の window_width・skipTaskbar・ ⏎ /// alwaysOnTop・decorations:false・resizable:false・visible:false）がウィンドウの姿である（codex #11・(B)#1）。 ⏎ /// `background_color_hex`: config `visual.background_color`（`#RRGGBB`）。過渡/リサイズ下地の ⏎ /// SU2 ハードコード 0x282828 を config へ差し替える（§11・#532 SU4 Task 2）。 ⏎ /// パース失敗時は `VisualConfig::default()` の背景色へ fallback（`visual::background_color` = ⏎ /// `Color32::from_hex` 1 本・spec 決定 4。リテラルを再手打ちしない）。 | | | | |
| B23 | 354-354 | // `VisualConfig::default()` である——`#282828` のリテラルをここへ再手打ちしない（spec 決定 4）。 | | | | |
| B24 | 359-359 | .inner_size(window_width, 52.0) // 保存幅を尊重（codex #11）。高さは初期値。実高は show 時に Metrics で再設定(#646) | | | | |
| B25 | 366-366 | // 白→暗の点滅を消す（既定は従来どおり 0x282828）。 | | | | |
| B26 | 371-373 | // #646 PR2: 結果リストウィンドウ。focusable(false) で tao が WS_EX_NOACTIVATE を自動適用し ⏎ // (tao window_state.rs: !FOCUSABLE → style_ex \|= WS_EX_NOACTIVATE)、クリックしても ⏎ // フォーカスはメインの入力欄から動かない（決定 4）。可視性・サイズ・位置は main の | | | | |
| B27 | 388-391 | apply_rounded_corners(&window); // main にも適用（輪郭言語を揃える・決定 4） ⏎ apply_rounded_corners(&results); ⏎ } ⏎ // #671 PR A′: attach は window を move するため、その**前**に clone から所有型を作る。 | | | | |
| B28 | 395-396 | // attach はウィンドウごとの wake handle を返す（#671 PR D）。**results を先に attach する順序は ⏎ // 変えない**——`ResultsWindow::new` は attach の move より前でなければならず（PR A′）、 | | | | |
| B29 | 400-400 | // #646 PR2 決定 10: ドラッグ移動中の追従。ネイティブ移動ループ中は egui フレームが | | | | |
| B30 | 420-420 | /// DWM にウィンドウの角丸を依頼する（#646 PR2 決定 4）。Windows 11（build 22000+）のみ有効で、 | | | | |
| B31 | 455-455 | /// （`read_visual` がテーマ値 3 種を 1 回で読み切るのはそのためである・#673 spec 決定 4）。 | | | | |
| B32 | 468-468 | /// 1 フレーム分のテーマ値を **読み 1 回**で読み切る（#673 spec 決定 4）。導出は純関数 | | | | |
| B33 | 487-487 | // リテラルが不要になった（#795）。 | | | | |
| B34 | 499-499 | /// 合流点・codex #7）。 | | | | |
| B35 | 504-504 | // emit 元は view の `update()` の中（イベントループスレッド）ゆえ、 | | | | |
| B36 | 510-513 | /// config 変更・index 状態変化の wake 合図（#532 SU6 spec 決定 1）。値は運ばず request_repaint ⏎ /// のみ——次フレームの live-read が最新値を拾う。空振りは benign（初 show フレームの live-read が ⏎ /// 最新を描く）。**「値を運ばない」はこの benign 性の load-bearing 前提**——将来イベントに値を ⏎ /// 載せる変更はこの前提を壊す（spec 決定 1）。 | | | | |
| B37 | 539-539 | /// 起動時 hotkey 登録失敗の受け口（#652・SU6.5 決定 2）。**格納 → show → wake** の順で処理する。 | | | | |
| B38 | 546-546 | ///   旧 TS フロントが担っていた経路の egui 版（当該フロントは #532 SU7 で撤去済み）。 | | | | |
| B39 | 551-551 | ///   不変条件（SU5）により通知が永遠に描かれない。**この非対称ゆえ 2 つの listener を | | | | |
| B40 | 557-557 | // `register_hotkey_failure_listener` と同じ流儀（#673 spec 決定 3 で袋を解体）。 | | | | |
| B41 | 563-563 | // このイベントの emit 元は platform スレッド（`platform/mod.rs` の | | | | |
