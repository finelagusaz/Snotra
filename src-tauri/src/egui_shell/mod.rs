//! egui/softbuffer メインウィンドウの外殻（#532 SU2）。WebView2 と並行する
//! egui 専用 window 生成・show/hide・blur 自動非表示・位置永続。WebView2 経路は触らない。
mod icon_textures;
mod lifecycle;
mod search_state;
mod layout;
mod notify;
// view.rs（driver）が起動 worker の in-flight 追跡・一時通知で消費する（#532 SU5 Task 4）。
pub(crate) use notify::{LAUNCH_TIMEOUT, NOTICE_LAUNCH, NoticeSlot};
// mod.rs の spawn_update_check が phase 書き込みで、UpdaterUiState が Default で消費する
// （#532 SU5 Task 6）。toast 描画は view.rs が Task 7 で消費する。
pub(crate) use notify::{ToastKind, UpdaterPhase, UpdaterUi};
pub(crate) mod strings;
mod view;

// view.rs の icon texture driver（worker spawn / load_texture 適用）が消費する（#532 SU4 Task 5）。
pub(crate) use icon_textures::{IconMsg, needs_extraction, png_to_color_image, retain_visible};
pub(crate) use lifecycle::{HotkeyPlan, blur_should_hide, plan_hotkey};
// view.rs（driver）が folder 展開（#532 SU3 M2）で消費する。
pub(crate) use search_state::{
    EscapeOutcome, QueryIntent, SearchState, ViewKind, compute_parent_dir, folder_load_pending,
    should_flush_on_enter,
};
// SlashCmd/find_slash_command は driver（view.rs）が command 分岐・slash 実行で消費する（#532 SU3 M3 Task 2）。
pub(crate) use search_state::{SlashCmd, find_slash_command};
// driver（view.rs）は Task 4 で表示ゲート・再検索トリガとして消費する（#532 SU6 Task 1）。
#[allow(unused_imports)]
pub(crate) use search_state::{needs_index_refresh, plain_results_hidden};
pub(crate) use layout::{Debouncer, HeightParams, compute_window_height};
// view.rs が UI 文言（hint/overlay/toast）で消費する（#532 SU5・言語は起動時一回読み）。
pub(crate) use strings as ui_strings;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use snotra_egui_runtime::EguiRuntime;
use tauri::{Listener, Manager};

use crate::egui_shell::view::SearchWindowView;

/// egui 経路の show/hide を跨ぐ共有状態（managed state）。
/// - hotkey_generation: alt 解放待ち show の世代。hide が bump して保留 show を無効化する（codex #5/(B)#2）。
/// - hide_pending: view の emit dedup。show がクリアして「hide 後に Focused(true) が来ず抑止が残る」を断つ（codex #8）。
/// - reset_pending: show が立て、view が消費して state.reset()（resetForShow 相当・SU3 M1 Task 9）。
#[derive(Default)]
pub(crate) struct EguiShellState {
    pub(crate) hotkey_generation: AtomicU64,
    pub(crate) hide_pending: AtomicBool,
    pub(crate) reset_pending: AtomicBool,
    /// updater check 完了時に可視中の view を起こすための egui Context（view.setup が登録）。
    /// hidden 中は次 show のフレームで toast が読まれるため repaint は可視中のみ意味を持つ
    /// （codex レビュー: 「hidden は次 show でよい」と「visible は repaint が要る」は別条件）。
    pub(crate) egui_ctx: Mutex<Option<egui::Context>>,
}

/// updater toast の managed 状態（#532 SU5）。view が毎フレーム読む level-triggered
/// （hidden に頑健・launching の channel edge-trigger との構造的対比は spec C 節）。
/// dismissed は view-local に置かない——reset-on-show が view-local を一掃した際に
/// `[閉じる]` 済み toast が復活するため（状態機械レビュー・spec A 節）。
pub(crate) struct UpdaterUiState(pub(crate) Mutex<crate::egui_shell::UpdaterUi<Box<tauri_plugin_updater::Update>>>);

/// 起動時 updater check（§20.2・spec B 節）。`auto_update != disabled` で一回だけ呼ぶ。
/// `on_before_exit` に終了保存を登録した builder で check する——ここで得た `Update` の
/// install は「download → 保存 → installer 起動 → exit(0)」となり、保存が構造的に保証される
/// （Windows では downloadAndInstall が復帰しない・updater.rs:865・spec「決着済み: 保存順序」）。
pub(crate) fn spawn_update_check(app: &tauri::AppHandle) {
    use snotra_core::config::AutoUpdateMode;
    use tauri_plugin_updater::UpdaterExt;
    // 視覚スモーク専用（SNOTRA_DISABLE_SUSPEND と同じ E2E エスケープハッチの流儀）:
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
        .unwrap_or(AutoUpdateMode::Full);
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
                    crate::trace_main("egui_update_check_failed", serde_json::json!({ "error": e.to_string() }));
                    crate::egui_shell::UpdaterPhase::Idle
                }
            },
            Err(e) => {
                crate::trace_main("egui_update_check_failed", serde_json::json!({ "error": e.to_string() }));
                crate::egui_shell::UpdaterPhase::Idle
            }
        };
        if let Some(st) = handle.try_state::<UpdaterUiState>() {
            st.0.lock().unwrap().phase = next;
        }
        // 可視中に check が完了した場合の wake-up(スパイクの request_repaint と同じ・codex レビュー)。
        if let Some(sh) = handle.try_state::<EguiShellState>()
            && let Ok(guard) = sh.egui_ctx.lock()
            && let Some(ctx) = guard.as_ref()
        {
            ctx.request_repaint();
        }
    });
}

/// フラグ ON の窓生成。EguiRuntime を install し webview 無しの "main" 窓を生成して attach。setup 限定。
/// 宣言窓の全プロパティ（52px 高は初期値〔SU3 で show 前折り畳み + 結果表示時に動的リサイズ・view.rs〕・width は config の window_width・skipTaskbar・
/// alwaysOnTop・decorations:false・resizable:false・visible:false）を再現する（codex #11・(B)#1）。
/// `background_color_hex`: config `visual.background_color`（`#RRGGBB`）。過渡/リサイズ下地の
/// SU2 ハードコード 0x282828 を config へ差し替える（§11・#532 SU4 Task 2）。パース失敗時は
/// 従来の 0x282828 へ fallback（`config_watcher::parse_hex_color` を再利用・二重実装回避）。
pub(crate) fn create(
    app: &mut tauri::App,
    window_width: f64,
    background_color_hex: &str,
) -> Result<(), snotra_egui_runtime::RuntimeError> {
    let runtime = EguiRuntime::new();
    runtime.install(app); // install(&self, &mut App<Wry>)（runtime.rs:77）
    let app_handle = app.handle().clone();
    let bg_color = crate::config_watcher::parse_hex_color(background_color_hex)
        .unwrap_or(tauri::window::Color(0x28, 0x28, 0x28, 0xff));
    let window = tauri::Window::builder(app, "main")
        .title("Snotra")
        .inner_size(window_width, 52.0) // 保存幅を尊重（codex #11）
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
    runtime.attach(window, SearchWindowView::new(app_handle))
}

/// egui 経路の show。共有するのは position_on_target_monitor のみ。全 hide は外部化ゆえ
/// runtime.visible は false にならず、show は Focused(true) に依存せず確実に描ける（codex #4）。
/// show 列は WebView2 の show_and_focus_main を egui 用に自前複製（WebView2 本体を触らないため）。
pub(crate) fn show_egui_main(app: &tauri::AppHandle, t0: Instant) {
    let Some(window) = app.get_window("main") else {
        crate::trace_main("egui_show:no_window", serde_json::json!({}));
        return;
    };
    // show のたびに view の emit dedup をリセット（Focused(true) 非依存・codex #8）。
    if let Some(sh) = app.try_state::<EguiShellState>() {
        sh.hide_pending.store(false, Ordering::SeqCst);
        sh.reset_pending.store(true, Ordering::SeqCst); // resetForShow を view に指示
    }
    // 高さリセット → 位置 → show の順（SU2 の show_main_and_emit と同じ制約）。
    // reset-on-show でクエリは空 = 結果なし = 52px。前回 hide 時に展開高（例 300px）のまま
    // だと position クランプが 300px で効き、show 後に view が 52px へ collapse して視覚スナップ +
    // 位置ずれになる。position の前に 52px へ collapse してこれを断つ（SU3 で高さが動的化した
    // ため、旧「52px は create で固定・位置のみ復元」前提は崩れている）。
    #[cfg(windows)]
    {
        let width = window
            .inner_size()
            .ok()
            .map(|s| s.to_logical::<f64>(window.scale_factor().unwrap_or(1.0)).width)
            .unwrap_or(600.0);
        let _ = window.set_size(tauri::LogicalSize::new(width, 52.0));
    }
    #[cfg(windows)]
    crate::position_on_target_monitor(app, &window);
    let _ = window.show();
    // main_visible は show() の後に立てる（WebView2 の show_and_focus_main と同じ「順序不変」
    // 制約）。show 完了前に visible=true を読んだホットキートグルが hide するのを避ける。
    if let Some(state) = app.try_state::<crate::AppState>() {
        state.main_visible.store(true, Ordering::SeqCst);
    }
    let _ = window.set_focus();
    // フォーカス移行の同期待ち（SetForegroundWindow は部分的に非同期・Raymond Chen）。
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{SMTO_NORMAL, SendMessageTimeoutW, WM_NULL};
        let hwnd = HWND(hwnd.0);
        let mut result = 0usize;
        unsafe {
            let _ = SendMessageTimeoutW(
                hwnd,
                WM_NULL,
                WPARAM(0),
                LPARAM(0),
                SMTO_NORMAL,
                100,
                Some(&mut result),
            );
        }
    }
    // 残留 Alt 解除: focus 確定後かつ物理 Alt 解放後のみ（#558）。
    if !crate::is_alt_pressed() {
        crate::send_alt_key_up();
    }
    crate::trace_main(
        "egui_show:done",
        serde_json::json!({ "ms": t0.elapsed().as_secs_f64() * 1000.0 }),
    );
}

/// egui 経路の hide。全 hide の唯一の副作用所有点（codex #7）。外部 window.hide() のみで
/// runtime.visible を false にしない（空白窓回避・codex #4）。
pub(crate) fn hide_egui_main(app: &tauri::AppHandle) {
    // 保留中の alt 解放待ち show を無効化（codex #5/(B)#2）: 世代を bump し、spawn 済み show
    // スレッドの gen 一致チェックを外す。
    if let Some(sh) = app.try_state::<EguiShellState>() {
        sh.hotkey_generation.fetch_add(1, Ordering::SeqCst);
    }
    if let Some(window) = app.get_window("main") {
        save_placement_relative(&window); // save-on-hide
        let _ = window.hide();
    }
    if let Some(state) = app.try_state::<crate::AppState>() {
        state.main_visible.store(false, Ordering::SeqCst);
    }
    crate::trace_main("egui_hide:done", serde_json::json!({}));
}

/// 現在の物理位置をターゲットモニター作業領域原点からの相対座標で window.bin に保存。
/// WebView2 の save_relative_placement（commands/window.rs）と同じ算出を &Window で行う
/// （別モジュールの private fn は参照できないため複製・WebView2 側は不変）。
pub(crate) fn save_placement_relative(window: &tauri::Window) {
    let Ok(pos) = window.outer_position() else {
        return;
    };
    #[cfg(windows)]
    {
        use snotra_core::window_data::{self, WindowPlacement};
        let Ok(hwnd) = window.hwnd() else { return };
        let Some(wa) = crate::monitor::window_monitor_work_area(hwnd.0 as isize) else {
            return;
        };
        window_data::save_search_placement(WindowPlacement {
            x: pos.x - wa.left,
            y: pos.y - wa.top,
        });
    }
    #[cfg(not(windows))]
    {
        use snotra_core::window_data::{self, WindowPlacement};
        window_data::save_search_placement(WindowPlacement { x: pos.x, y: pos.y });
    }
}

/// view からの `egui-hide-requested` を受け、hide_egui_main を実行する（全 hide の合流点・codex #7）。
/// view（イベントループスレッド）→ emit → この listener で hide を 1 経路に集約する。
pub(crate) fn register_hide_listener(app: &tauri::AppHandle) {
    let handle = app.clone();
    app.listen("egui-hide-requested", move |_| {
        hide_egui_main(&handle);
    });
}
