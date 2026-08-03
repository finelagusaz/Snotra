//! egui/softbuffer メインウィンドウの外殻（#532 SU2〜SU7・唯一の UI 経路）。
//! window 生成（main/results 両窓）・共有状態（`EguiShellState`）・config の 1 フレーム読み・
//! listener の登録・起動時 updater check。
//! **窓を操作する実体（show/hide・位置・results のサイズ・wake）は `window_coordinator` に
//! ある**（#749 段 1）——ここに残るのは登録と生成であって、駆動ではない。
//! main のサイズだけは 2 か所に分かれている（`window_coordinator` の show 経路と `view.rs` の
//! 毎フレーム。理由は `window_coordinator` の `//!`）。
mod icon_textures;
mod layout;
mod lifecycle;
mod notify;
mod search_state;
// launcher_controller.rs が起動 worker の in-flight 追跡・一時通知で消費する（#532 SU5 Task 4）。
pub(crate) use notify::{LAUNCH_TIMEOUT, NOTICE_LAUNCH, NoticeSlot};
// NOTICE_HOTKEY は launcher_controller.rs（hotkey 失敗通知の duration）が、OverlayKind /
// overlay_kind は view.rs（status 行の優先ラダー）が消費する（#532 SU6 Task 7・#666 段 3 で
// 消費者が 2 モジュールへ割れた）。status_row_present は毎フレーム側（view.rs 予定）と
// show 経路（window_coordinator.rs 予定）の両方が消費する（#755/#801 タスク分割・後続タスクで配線）。
pub(crate) use notify::{NOTICE_HOTKEY, OverlayKind, overlay_kind, status_row_present};
// mod.rs の spawn_update_check が phase 書き込みで、UpdaterUiState が Default で消費する
// （#532 SU5 Task 6）。toast 描画は view.rs が、UpdaterPhase の遷移（install 失敗）は
// launcher_controller.rs が消費する。
pub(crate) use notify::{ToastKind, UpdaterPhase, UpdaterUi};
mod font_stack;
mod launcher_controller;
mod results_view;
mod results_window;
pub(crate) mod strings;
mod view;
mod visual;
mod window_coordinator;

// main.rs（hotkey / tray / setup）・view.rs（結果窓の駆動と wake）・launcher_controller.rs
// （updater install 失敗の wake_main）・results_view.rs（クリック逆流）が
// 消費する。窓操作の実体は window_coordinator.rs へ移した（#749 段 1）。**モジュール外に
// 消費者があるものだけを re-export する**——`read_placement_relative` / `read_metrics` /
// `results_available_height` / `max_results` / `position_on_target_monitor` は同モジュール内
// からしか呼ばれず、`position_results_below_main` は親である本ファイルが
// `window_coordinator::` で直に呼ぶ。
pub(crate) use window_coordinator::{
    DriveResultsInputs, drive_results_window, hide_egui_main, show_egui_main, wake_main,
    wake_results,
};

// mod.rs（窓生成・managed state）が消費する。RowsSnapshot は view.rs（main の snapshot 発行）・
// results_view.rs（update() 描画）が消費する（#646 PR2 Task 4）。ClickTake は view.rs
// （クリック逆流の消費・世代照合の結果で分岐する）が消費する（#699）。
pub(crate) use results_view::ClickTake;
pub(crate) use results_view::ResultsShared;
pub(crate) use results_view::RowsSnapshot;

// main.rs（managed state 化）・window_coordinator.rs（drive / hide）・commands/window.rs
// （topmost）が消費する（#671 PR A′ spec 決定 2。drive は #749 で view.rs から移設）。
pub(crate) use results_window::ResultsWindow;

// view.rs / results_view.rs が毎フレームの描画で消費する（#673 spec 決定 4）。
pub(crate) use visual::{RowTheme, VisualSnapshot};

// view.rs の icon texture driver（worker spawn / load_texture 適用）が消費する（#532 SU4 Task 5）。
pub(crate) use icon_textures::{IconMsg, needs_extraction, png_to_color_image, retain_visible};
// `blur_should_hide` は re-export しない——消費点は `blur_grace_action` に一本化され、
// 判定そのものは純粋核の内部で生きている（#711）。2 経路を並走させないための意図的な非公開。
// blur 猶予の 3 件は launcher_controller.rs が、plan_hotkey は main.rs 側が消費する。
pub(crate) use lifecycle::{BLUR_GRACE, BlurAction, HotkeyPlan, blur_grace_action, plan_hotkey};
// launcher_controller.rs が folder 展開（#532 SU3 M2）・Escape ラダー・Enter flush で消費する。
// ViewKind だけは view.rs も読む（入力欄の編集可否と status 行の分岐・#666 段 3）。
pub(crate) use search_state::{
    EscapeOutcome, QueryIntent, SearchState, ViewKind, compute_parent_dir, folder_load_pending,
    should_flush_on_enter,
};
// SlashCmd/find_slash_command は launcher_controller.rs が command 分岐・slash 実行で消費する（#532 SU3 M3 Task 2）。
pub(crate) use search_state::{SlashCmd, find_slash_command};
// needs_index_refresh は launcher_controller.rs（世代検知 → 再検索）が、plain_results_hidden は
// view.rs（表示ゲート）が消費する（#532 SU6 Task 1・#666 段 3 で消費者が割れた）。
pub(crate) use search_state::{needs_index_refresh, plain_results_hidden};
// launcher_controller.rs が検索 debounce（leading + trailing）で消費する。
pub(crate) use layout::Debouncer;
// view.rs が UI 文言（hint/overlay/toast）で、launcher_controller.rs が通知文言（起動失敗・
// 結果不明・hotkey 登録失敗）で消費する（#532 SU5・言語は lang() が毎フレーム live-read）。
pub(crate) use strings as ui_strings;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::Instant;

use snotra_core::config::AppearanceConfig;
use snotra_egui_runtime::EguiRuntime;
use tauri::{Listener, Manager};

use crate::egui_shell::view::SearchWindowView;

/// hotkey 登録失敗の種別（#652）。文言キーが別（i18n.ts `notice.hotkey.*`）ゆえ
/// pending に載せて view 側で整形を分岐する。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HotkeyFailureKind {
    /// 起動時の初回登録失敗（`initial-hotkey-failed`）。
    /// SPEC §10 のとおり窓を能動表示してから通知する
    /// （listener は `register_initial_hotkey_failure_listener`・#652 Task 4）。
    Initial,
    /// 設定変更による再登録失敗（`hotkey-registration-failed`）。旧ホットキーを維持。
    Change,
}

/// egui 経路の show/hide を跨ぐ共有状態（managed state）。
/// - hotkey_generation: alt 解放待ち show の世代。hide が bump して保留 show を無効化する（codex #5/(B)#2）。
/// - hide_pending: view の emit dedup。show がクリアして「hide 後に Focused(true) が来ず抑止が残る」を断つ（codex #8）。
/// - reset_pending: show が立て、view が消費して state.reset()（resetForShow 相当・SU3 M1 Task 9）。
pub(crate) struct EguiShellState {
    pub(crate) hotkey_generation: AtomicU64,
    pub(crate) hide_pending: AtomicBool,
    pub(crate) reset_pending: AtomicBool,
    /// main 窓を外部から起こすハンドル（`create()` = `attach` の戻り値・#671 PR D）。
    /// hidden 中は次 show のフレームで toast 等が読まれるため、wake は可視中のみ意味を持つ
    /// （codex レビュー: 「hidden は次 show でよい」と「visible は repaint が要る」は別条件）。
    /// 旧実装は各 view の `setup` が `egui::Context` の clone を登録する
    /// `Mutex<Option<egui::Context>>` スロットだった——**登録前の窓を「未登録＝no-op」で
    /// 扱う段が消え、この型は窓の存在と同時に有効になる**。
    main_waker: snotra_egui_runtime::WindowWaker,
    /// results 窓を外部から起こすハンドル（main と同型・#646 PR2 の `results_ctx` の後継）。
    results_waker: snotra_egui_runtime::WindowWaker,
    /// hotkey 登録失敗の pending payload（SU6 spec 追補 2 + #652）。種別ごとに文言が違う
    /// ため `(kind, hotkey)` を保持し、view が消費時に lang() live-read で整形する。
    /// **wake の有無は経路で異なる**——Change は wake しない（wake を config-applied に
    /// 委ね、言語同時変更で旧言語整形になる競合窓を閉じる）。この競合窓が閉じる根拠は
    /// `apply_config_change` が engine への `update_config` 適用**後**に `config-applied` を
    /// emit する順序——wake 時の lang() live-read は必ず新言語を読む（旧 `language-changed`
    /// 先行発火の不変条件は #532 SU7 の emit 削除で消滅し、この順序が後継の根拠）。
    /// Initial は wake する（config 変更が随伴せず config-applied が来ないため・#652・SU6.5 決定 2）。
    pub(crate) pending_hotkey_failure: Mutex<Option<(HotkeyFailureKind, String)>>,
}

impl EguiShellState {
    /// wake handle は `create()` が返すものだけを受け取る（`Default` は持たない——窓が
    /// 無いのに wake できる状態を作らないため）。他フィールドは従来の既定値。
    pub(crate) fn new(handles: &EguiShellHandles) -> Self {
        Self {
            hotkey_generation: AtomicU64::new(0),
            hide_pending: AtomicBool::new(false),
            reset_pending: AtomicBool::new(false),
            main_waker: handles.main_waker.clone(),
            results_waker: handles.results_waker.clone(),
            pending_hotkey_failure: Mutex::new(None),
        }
    }
}

/// updater toast の managed 状態（#532 SU5）。view が毎フレーム読む level-triggered
/// （hidden に頑健・launching の channel edge-trigger との構造的対比は spec C 節）。
/// dismissed は view-local に置かない——reset-on-show が view-local を一掃した際に
/// `[閉じる]` 済み toast が復活するため（状態機械レビュー・spec A 節）。
pub(crate) struct UpdaterUiState(
    pub(crate) Mutex<crate::egui_shell::UpdaterUi<Box<tauri_plugin_updater::Update>>>,
);

/// 起動時 updater check（§20.2・spec B 節）。`main.rs` の setup が**無条件で一回だけ**呼び、
/// `auto_update` の判定はこの関数の中で行う（呼び出し側では絞っていない——下の視覚スモーク
/// hatch がその判定より前に置けるのはこのため）。
/// `on_before_exit` に終了保存を登録した builder で check する——ここで得た `Update` の
/// install は「download → 保存 → installer 起動 → exit(0)」となり、保存が構造的に保証される
/// （Windows では downloadAndInstall が復帰しない・updater.rs:865・spec「決着済み: 保存順序」）。
pub(crate) fn spawn_update_check(app: &tauri::AppHandle) {
    use snotra_core::config::AutoUpdateMode;
    use tauri_plugin_updater::UpdaterExt;
    // 視覚スモーク専用の env エスケープハッチ（2 本・`docs/build-commands.md` に手順）:
    // どちらも `auto_update` の判定より**前**にあるため、設定に依らず効く。
    //
    // 失敗トーストの描画（理由の併記 + 末尾省略）を観測する（#654）。実 install 失敗は
    // 実 release への到達 + download 失敗が要り再現できないため、**これが唯一の観測点**である。
    // 理由は既定幅（window_width 600）で省略が起きる長さにしてある——短い理由では `…` が
    // 出ず、省略経路を目視できない。
    if crate::trace::env_flag("SNOTRA_EGUI_FAKE_UPDATE_FAILED") {
        if let Some(st) = app.try_state::<UpdaterUiState>() {
            st.0.lock().unwrap().phase = crate::egui_shell::UpdaterPhase::InstallFailed {
                message: "Network Error: error sending request for url \
                          (https://example.invalid/releases/latest.json)"
                    .into(),
            };
        }
        return;
    }
    // 実 release への依存なしに toast を表示する。install 実体は無い（update: None）。
    if crate::trace::env_flag("SNOTRA_EGUI_FAKE_UPDATE") {
        if let Some(st) = app.try_state::<UpdaterUiState>() {
            st.0.lock().unwrap().phase = crate::egui_shell::UpdaterPhase::Available {
                version: "9.9.9".into(),
                can_install: true,
                update: None,
            };
        }
        return;
    }
    let mode = app
        .try_state::<crate::AppState>()
        .map(|s| s.engine.lock().unwrap().config().general.auto_update)
        // AppState は setup 前に managed されるため実運用では到達不能。到達したら
        // 「設定を読めていない」状態なので、勝手に更新を始めない Disabled へ倒す（#648 F）。
        .unwrap_or(AutoUpdateMode::Disabled);
    if mode == AutoUpdateMode::Disabled {
        return;
    }
    let can_install = mode == AutoUpdateMode::Full;
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(st) = handle.try_state::<UpdaterUiState>() {
            st.0.lock().unwrap().phase = crate::egui_shell::UpdaterPhase::Checking;
        }
        let flush_handle = handle.clone();
        let updater = handle
            .updater_builder()
            .on_before_exit(move || crate::flush_persistent_state(&flush_handle))
            .build();
        let next = match updater {
            Ok(u) => match u.check().await {
                Ok(Some(update)) => crate::egui_shell::UpdaterPhase::Available {
                    version: update.version.clone(),
                    can_install,
                    update: Some(Box::new(update)),
                },
                Ok(None) => crate::egui_shell::UpdaterPhase::UpToDate,
                Err(e) => {
                    // check 失敗は無音（console.warn parity・trace のみ）。
                    crate::trace_main(
                        "egui_update_check_failed",
                        serde_json::json!({ "error": e.to_string() }),
                    );
                    crate::egui_shell::UpdaterPhase::Idle
                }
            },
            Err(e) => {
                crate::trace_main(
                    "egui_update_check_failed",
                    serde_json::json!({ "error": e.to_string() }),
                );
                crate::egui_shell::UpdaterPhase::Idle
            }
        };
        if let Some(st) = handle.try_state::<UpdaterUiState>() {
            st.0.lock().unwrap().phase = next;
        }
        // 可視中に check が完了した場合の wake-up(スパイクの request_repaint と同じ・codex レビュー)。
        wake_main(&handle);
    });
}

/// `create()` が setup へ引き渡す所有物（#671 PR D・spec 決定 8 の終端形）。
///
/// **窓の生成（`create`）と managed state への載せ替え（`main.rs`）を分ける**ため、間に立つ
/// 型が要る。`create` の中で `app.manage` しないのは、setup の順序制約（どの listener より
/// 前に何が載っているか）を `main.rs` の 1 画面に残すため（spec 決定 8）。
pub(crate) struct EguiShellHandles {
    /// results 窓の所有型（生 Win32 の show/hide/topmost・#671 PR A′）。
    pub(crate) results_window: ResultsWindow,
    /// main 窓の wake handle（`EguiShellState` が保持する）。
    pub(crate) main_waker: snotra_egui_runtime::WindowWaker,
    /// results 窓の wake handle（同上）。
    pub(crate) results_waker: snotra_egui_runtime::WindowWaker,
}

/// フラグ ON の窓生成。EguiRuntime を install し webview 無しの "main" 窓を生成して attach。setup 限定。
/// 宣言窓の全プロパティ（52px 高は初期値〔SU3 で show 前折り畳み + 結果表示時に動的リサイズ・view.rs〕・width は config の window_width・skipTaskbar・
/// alwaysOnTop・decorations:false・resizable:false・visible:false）を再現する（codex #11・(B)#1）。
/// `background_color_hex`: config `visual.background_color`（`#RRGGBB`）。過渡/リサイズ下地の
/// SU2 ハードコード 0x282828 を config へ差し替える（§11・#532 SU4 Task 2）。パース失敗時は
/// パース失敗時は `VisualConfig::default()` の背景色へ fallback（`visual::background_color` =
/// `Color32::from_hex` 1 本・spec 決定 4。リテラルを再手打ちしない）。
///
/// **この初期ブラシが画面に出る機会は無い**——両窓とも `.visible(false)` で生成され、可視化の
/// 直前（`show_egui_main` / `ResultsWindow::show`）が無条件で上書きするためである。**残すのは
/// 安全網としてであり**、show 経路を迂回する窓表示が将来足されたときに白い窓を出さないための
/// もの。config 値を届ける経路としては働いていない。
pub(crate) fn create(
    app: &mut tauri::App,
    window_width: f64,
    background_color_hex: &str,
) -> Result<EguiShellHandles, snotra_egui_runtime::RuntimeError> {
    let runtime = EguiRuntime::new();
    runtime.install(app); // install(&self, &mut App<Wry>)（runtime.rs:77）
    let app_handle = app.handle().clone();
    // parse は描画色と同じ 1 本（`Color32::from_hex`）で、フォールバックの正本は
    // `VisualConfig::default()` である——`#282828` のリテラルをここへ再手打ちしない（spec 決定 4）。
    let bg_color =
        window_coordinator::native_brush_color(visual::background_color(background_color_hex));
    let window = tauri::Window::builder(app, "main")
        .title("Snotra")
        .inner_size(window_width, 52.0) // 保存幅を尊重（codex #11）。高さは初期値。実高は show 時に Metrics で再設定(#646)
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true) // 宣言窓 skipTaskbar:true の再現（(B)#1）
        .always_on_top(true) // 宣言窓 alwaysOnTop:true の再現（(B)#1）
        // 白フラッシュ回避: show 時、最初の softbuffer present 前にネイティブ背景ブラシが一瞬見える。
        // softbuffer の CLEAR_COLOR（renderer.rs=0x282828）に合わせて config テーマ色にし、
        // 白→暗の点滅を消す（既定は従来どおり 0x282828）。
        .background_color(bg_color)
        .visible(false)
        .build()?; // tauri::Error → RuntimeError（#[from]・runtime.rs:46）

    // #646 PR2: 結果リスト窓。focusable(false) で tao が WS_EX_NOACTIVATE を自動適用し
    // (tao window_state.rs: !FOCUSABLE → style_ex |= WS_EX_NOACTIVATE)、クリックしても
    // フォーカスはメインの入力欄から動かない（決定 4）。可視性・サイズ・位置は main の
    // update() が駆動する（hidden 窓は update() が走らないため自分では show できない）。
    let results = tauri::Window::builder(app, "results")
        .title("Snotra Results")
        .inner_size(window_width, 100.0) // 初期値。実高は main が実件数フィットで設定
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .focusable(false)
        .background_color(bg_color)
        .visible(false)
        .build()?;
    #[cfg(windows)]
    {
        apply_rounded_corners(&window); // main にも適用（輪郭言語を揃える・決定 4）
        apply_rounded_corners(&results);
    }
    // #671 PR A′: attach は window を move するため、その**前**に clone から所有型を作る。
    // `tauri::Window` は Arc ベースのハンドルで、clone は同一窓を指す（tauri 2.11 の
    // `impl Clone for Window` を実測）。
    let results_window = ResultsWindow::new(results.clone());
    // attach は窓ごとの wake handle を返す（#671 PR D）。**results を先に attach する順序は
    // 変えない**——`ResultsWindow::new` は attach の move より前でなければならず（PR A′）、
    // main の Moved リスナー登録もこの間に入る。
    let results_waker =
        runtime.attach(results, results_view::ResultsView::new(app_handle.clone()))?;
    // #646 PR2 決定 10: ドラッグ移動中の追従。ネイティブ移動ループ中は egui フレームが
    // 回る保証が無いため、tao の Moved イベント(tauri Window リスナー経由)で直接
    // results を追従させる。通常時の従属は main update() の drive が担う(二重呼びは
    // set_position の同値上書きで無害)。attach で window が move される前に登録する。
    {
        let handle = app_handle.clone();
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Moved(_)) {
                window_coordinator::position_results_below_main(&handle);
            }
        });
    }
    let main_waker = runtime.attach(window, SearchWindowView::new(app_handle))?;
    Ok(EguiShellHandles {
        results_window,
        main_waker,
        results_waker,
    })
}

/// DWM に窓の角丸を依頼する（#646 PR2 決定 4）。Windows 11（build 22000+）のみ有効で、
/// Windows 10 ではエラーを黙って握りつぶす（装飾なしで受容・best-effort）。
/// softbuffer は AA を持たず自前角丸は品質が出ないため OS 機構に委ねる。
#[cfg(windows)]
fn apply_rounded_corners(window: &tauri::Window) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DWM_WINDOW_CORNER_PREFERENCE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
        DwmSetWindowAttribute,
    };
    let Ok(hwnd) = window.hwnd() else { return };
    let pref: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND;
    unsafe {
        let _ = DwmSetWindowAttribute(
            HWND(hwnd.0),
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
    }
}

/// 1 フレーム分のテーマ値を **lock 1 回**で読み切る（#673 spec 決定 4）。導出は純関数
/// `visual::visual_snapshot` が持ち、この関数は lock と AppState 不在の面倒だけを見る。
///
/// **`read_metrics` は残す**（統合しない）——`show_egui_main` が show 経路で高さだけを要り、
/// 色 parse を払わせないため。両者とも `Metrics::from_config` を正本とするので導出は 1 つ。
///
/// **戻り値を `self.` へ保持しないこと**（寿命は 1 フレーム・`visual.rs` の `//!`）。
pub(crate) fn read_visual(app: &tauri::AppHandle, applied_font_family: &str) -> VisualSnapshot {
    match app.try_state::<crate::AppState>() {
        Some(s) => {
            let engine = s.engine.lock().unwrap();
            let config = engine.config();
            // guard 内で行うのは hex parse と算術と &str 比較まで。I/O や重い確保を足さない。
            visual::visual_snapshot(
                &config.visual,
                config.appearance.show_icons,
                applied_font_family,
            )
        }
        // AppState 不在（setup 完了前の理論経路のみ）。既定は型から導く——`AppearanceConfig` に
        // `Default` 実装を与えたことで show_icons のリテラルが不要になった（#795）。
        None => visual::visual_snapshot(
            visual::default_visual(),
            AppearanceConfig::default().show_icons,
            applied_font_family,
        ),
    }
}

/// view からの `egui-hide-requested` を受け、hide_egui_main を実行する（**main の** hide の
/// 合流点・codex #7）。
/// view（イベントループスレッド）→ emit → この listener で hide を 1 経路に集約する。
pub(crate) fn register_hide_listener(app: &tauri::AppHandle) {
    let handle = app.clone();
    app.listen(crate::events::EGUI_HIDE_REQUESTED, move |_| {
        // emit 元は view の `update()` の中（イベントループスレッド）ゆえ、
        // `on_event_loop` はインライン実行へ倒れる——**今日と同じフレーム内順序が保たれる**
        // （`proof.rs` の `on_event_loop` の doc）。
        snotra_egui_runtime::on_event_loop(&handle, hide_egui_main);
    });
}

/// config 変更・index 状態変化の wake 合図（#532 SU6 spec 決定 1）。値は運ばず request_repaint
/// のみ——次フレームの live-read が最新値を拾う。空振りは benign（初 show フレームの live-read が
/// 最新を描く）。**「値を運ばない」はこの benign 性の load-bearing 前提**——将来イベントに値を
/// 載せる変更はこの前提を壊す（spec 決定 1）。
pub(crate) fn register_config_wake_listeners(app: &tauri::AppHandle) {
    for event in [
        crate::events::CONFIG_APPLIED,
        crate::events::INDEXING_STARTED,
        crate::events::INDEXING_COMPLETE,
    ] {
        let handle = app.clone();
        app.listen(event, move |_| {
            wake_main(&handle);
        });
    }
}

/// hotkey 登録失敗の payload 受け口（spec 追補 2・wake は config-applied に委ねる）。
pub(crate) fn register_hotkey_failure_listener(app: &tauri::AppHandle) {
    let handle = app.clone();
    app.listen(crate::events::HOTKEY_REGISTRATION_FAILED, move |event| {
        // emit 側は String を渡すため payload は JSON 文字列（引用符付き）。
        let hotkey: String = serde_json::from_str(event.payload()).unwrap_or_default();
        if let Some(sh) = handle.try_state::<EguiShellState>() {
            *sh.pending_hotkey_failure.lock().unwrap() = Some((HotkeyFailureKind::Change, hotkey));
        }
    });
}

/// 起動時 hotkey 登録失敗の受け口（#652・SU6.5 決定 2）。**格納 → show → wake** の順で処理する。
///
/// - **格納が先**: show が起こすフレームは reset-on-show の `notice.clear()` を通ってから
///   pending を消費する（view の順序不変条件）。逆順にすると clear と store の間にフレームが
///   挟まりうるため、通知が消えたまま二度と出ない。
/// - **show する**: ホットキーが登録できていない＝ユーザーが窓を開く手段がトレイしか無い。
///   SPEC §10「初回ホットキー登録失敗時は操作不能回避のため検索 UI を表示し」の実装で、
///   旧 TS フロントが担っていた経路の egui 版（当該フロントは #532 SU7 で撤去済み）。
/// - **wake する**: `show_on_startup=true` で既に可視なら `show()` は再描画を生まない。
///   `register_hotkey_failure_listener`（変更失敗）が wake しないのと**意図的に逆**——
///   あちらは必ず `config-applied` が随伴するが、起動時失敗には config 変更が無く
///   `config-applied` は来ない。ここで起こさないと「hidden 中は update() が走らない」
///   不変条件（SU5）により通知が永遠に描かれない。**この非対称ゆえ 2 つの listener を
///   統合してはならない**（`/simplify` 対象外）。
pub(crate) fn register_initial_hotkey_failure_listener(app: &tauri::AppHandle) {
    let handle = app.clone();
    app.listen(crate::events::INITIAL_HOTKEY_FAILED, move |event| {
        // emit 側は String を渡すため payload は JSON 文字列（引用符付き）。
        // `register_hotkey_failure_listener` と同じ流儀（#673 spec 決定 3 で袋を解体）。
        let hotkey: String = serde_json::from_str(event.payload()).unwrap_or_default();
        if let Some(sh) = handle.try_state::<EguiShellState>() {
            *sh.pending_hotkey_failure.lock().unwrap() = Some((HotkeyFailureKind::Initial, hotkey));
        }
        // **`wake_main` も同じクロージャの中へ入れる**（`register_hide_listener` との差）。
        // このイベントの emit 元は platform スレッド（`platform/mod.rs` の
        // `RegisterInitialHotkey`）であり、`on_event_loop` はインラインに倒れず **post** に
        // なる。wake をクロージャの外に残すと wake が show を追い越し、上の doc が要求する
        // 「格納 → show → wake」が崩れる——show より前に届いたフレームが pending を消費し、
        // その後の reset-on-show（`show_egui_main` が立てた `reset_pending` の消費）の
        // `notice.clear()` が消してしまい、通知が二度と出ない。
        snotra_egui_runtime::on_event_loop(&handle, |app, el| {
            show_egui_main(app, el, Instant::now());
            wake_main(app);
        });
    });
}
