//! アプリケーションのエントリポイント。Tauri のセットアップ・イベントリスナー登録・
//! トレイ/ホットキー起動を行う。
//!
//! 起動時の背景再スキャン（`indexer::load_or_scan_with_stats` が返す `BackgroundRescanTask`）を
//! setup フェーズで低優先度スレッドに spawn し、`RescanOutcome::Changed` なら
//! `icon::invalidate_icon_cache` を呼ぶ。hide 時のレンダラー停止（TrySuspend）もここに置く。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod config_watcher;
mod egui_shell;
mod icon;
mod ime;
mod indexing;
mod monitor;
mod platform;
mod state;
mod trace;
mod working_set;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::json;
use snotra_core::config::{Config, HotkeyConfig, Language, LoadOutcome};
use snotra_core::engine::Engine;
use snotra_core::history::HistoryStore;
use snotra_core::indexer;
use tauri::{AppHandle, Emitter, Listener, Manager};

use crate::commands::SettingsProcessState;
use crate::icon::IconCacheState;

use crate::platform::{PlatformBridge, PlatformBridgePending, PlatformCommand};
use crate::state::AppState;

const ALT_RELEASE_POLL_MS: u64 = 10;
const ALT_RELEASE_TIMEOUT_MS: u64 = 350;

#[cfg(any(test, feature = "e2e-webview-automation"))]
const E2E_WEBVIEW_DATA_DIR_ENV: &str = "SNOTRA_E2E_WEBVIEW_DATA_DIR";
#[cfg(any(test, feature = "e2e-webview-automation"))]
const E2E_WEBVIEW_BROWSER_ARGS: &str = concat!(
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection ",
    "--remote-debugging-port=0"
);

/// Configure the test-only WebView2 environment through the trusted application API.
///
/// WebView2 150 ignores user-writable `WEBVIEW2_*` channels for elevated hosts, so
/// msedgedriver cannot supply its remote-debugging port that way. The Cargo feature
/// keeps this escape hatch out of production builds; the environment value only
/// selects a per-session relative data directory in the feature-enabled E2E binary.
#[cfg(any(test, feature = "e2e-webview-automation"))]
fn configure_e2e_webview(
    windows: &mut [tauri::utils::config::WindowConfig],
    data_dir: Option<&std::ffi::OsStr>,
) -> Result<bool, String> {
    use std::path::{Component, Path};

    let Some(data_dir) = data_dir else {
        return Ok(false);
    };
    let data_dir = Path::new(data_dir);
    let mut components = data_dir.components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(format!(
            "{E2E_WEBVIEW_DATA_DIR_ENV} must be one relative path component, got {}",
            data_dir.display()
        ));
    }

    let main = windows
        .iter_mut()
        .find(|window| window.label == "main")
        .ok_or_else(|| "E2E WebView configuration requires the main window".to_string())?;
    main.additional_browser_args = Some(E2E_WEBVIEW_BROWSER_ARGS.to_string());
    main.data_directory = Some(data_dir.to_path_buf());
    Ok(true)
}

/// Thin wrapper kept so call sites read `trace_main(...)`; logic lives in the
/// shared `crate::trace` module (deduped with `commands::trace_command`, #433).
fn trace_main(event: &str, data: serde_json::Value) {
    trace::trace(event, data);
}

#[cfg(windows)]
fn is_alt_pressed() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_LMENU, VK_MENU, VK_RMENU,
    };
    unsafe {
        GetAsyncKeyState(VK_MENU.0 as i32) < 0
            || GetAsyncKeyState(VK_LMENU.0 as i32) < 0
            || GetAsyncKeyState(VK_RMENU.0 as i32) < 0
    }
}

#[cfg(not(windows))]
fn is_alt_pressed() -> bool {
    false
}

fn wait_alt_release_or_timeout() {
    use std::time::{Duration, Instant};

    if !is_alt_pressed() {
        return;
    }

    let started = Instant::now();
    let timeout = Duration::from_millis(ALT_RELEASE_TIMEOUT_MS);
    let poll = Duration::from_millis(ALT_RELEASE_POLL_MS);

    while started.elapsed() < timeout {
        if !is_alt_pressed() {
            return;
        }
        std::thread::sleep(poll);
    }
}

/// Clear lingering Alt modifier state via `SendInput` before showing the
/// search window.  Uses the AutoHotkey "MenuMaskKey" technique: a dummy
/// key-down/up (vkE8, unassigned) is injected *before* the Alt key-up so
/// that Windows does not treat the Alt release as a bare Alt-up, which
/// would activate the menu bar or trigger a system beep.
#[cfg(windows)]
fn send_alt_key_up() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, SendInput, VK_LMENU, VK_MENU, VK_RMENU, VIRTUAL_KEY,
    };

    const VK_MASK: VIRTUAL_KEY = VIRTUAL_KEY(0xE8); // unassigned — safe dummy key

    let inputs = [
        make_key_input(VK_MASK, false),  // mask key down
        make_key_input(VK_MASK, true),   // mask key up
        make_key_input(VK_MENU, true),   // Alt (generic) up
        make_key_input(VK_LMENU, true),  // Left Alt up
        make_key_input(VK_RMENU, true),  // Right Alt up
    ];
    unsafe {
        let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
    // Brief pause so WebView2 processes the synthetic key-ups before
    // receiving actual user keystrokes.
    std::thread::sleep(std::time::Duration::from_millis(5));
}

#[cfg(windows)]
fn make_key_input(
    vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY,
    is_up: bool,
) -> windows::Win32::UI::Input::KeyboardAndMouse::INPUT {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYBD_EVENT_FLAGS,
    };
    let mut input = INPUT {
        r#type: INPUT_KEYBOARD,
        ..Default::default()
    };
    input.Anonymous.ki = KEYBDINPUT {
        wVk: vk,
        dwFlags: if is_up {
            KEYEVENTF_KEYUP
        } else {
            KEYBD_EVENT_FLAGS::default()
        },
        ..Default::default()
    };
    input
}

#[cfg(not(windows))]
fn send_alt_key_up() {}

/// WebDriver 自動化（E2E）は非表示中のレンダラーへの script 実行で成り立つため、
/// suspend されたレンダラーとは原理的に非互換（`executeScript` が応答せず
/// タイムアウトする）。E2E ハーネスは `SNOTRA_DISABLE_SUSPEND=1` で suspend を
/// 無効化して起動する（`EmptyWorkingSet` trim は従来どおり有効のまま）。
#[cfg(windows)]
fn suspend_disabled() -> bool {
    static DISABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *DISABLED.get_or_init(|| trace::env_flag("SNOTRA_DISABLE_SUSPEND"))
}

/// 非表示中の WebView2 レンダラーを中断し、メモリ・CPU を削減する。
///
/// 必ずウィンドウの hide 完了**後**に呼ぶこと（UX: 停止した webview をユーザーに見せない）。
/// `TrySuspend` の前提は `ICoreWebView2Controller.IsVisible` が false であること——
/// これは HWND の可視性とは**独立**した controller 側のプロパティで、wry は `hide()` で
/// 下げないため、ここで自前で下げる。これを欠くと `TrySuspend` は
/// 0x8007139F (ERROR_INVALID_STATE) で同期的に失敗し、完了ハンドラは呼ばれない
/// （2026-07-17 実測: 導入以来この失敗が全 hide で起きていた）。
/// `resume_webview` が対称に復元する（`SetIsVisible(true)` + `Resume`）。
///
/// best-effort: WebView2 ランタイムが古い場合（Edge 88 未満）は黙って何もしない。
/// 再表示と競合した場合はクロージャ内の `main_visible` ガードで suspend を放棄し、
/// `show_main_and_emit` 末尾の resume 再適用が残余ケース（resume 実行後〜可視フラグ
/// 反映前に滑り込む逆転）を是正する。
#[cfg(windows)]
fn suspend_webview(window: &tauri::WebviewWindow, source: &'static str) {
    if suspend_disabled() {
        return;
    }
    let app_handle = window.app_handle().clone();
    let _ = window.with_webview(move |platform_webview| {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
        use webview2_com::TrySuspendCompletedHandler;
        use windows_core_0_61::Interface;

        // 再表示と競合した場合は suspend を放棄する。旧実装ではこの安全弁を
        // TrySuspend 自身の前提条件チェック（controller.IsVisible=true → 同期 Err）が
        // 無償で担っていたが、IsVisible を自前で下げる本実装ではその検査が機構として
        // 働かないため、メインスレッド実行時点の main_visible で明示的に再設置する。
        // クロージャの外で読むと、ディスパッチ待ちの間に show が完了して古い値を
        // 参照する TOCTOU になるため、必ずクロージャ内（実行時点）で読む。
        let visible = app_handle
            .try_state::<AppState>()
            .map(|s| s.main_visible.load(Ordering::SeqCst))
            .unwrap_or(false);
        if visible {
            trace_main(
                "suspend:skipped",
                json!({ "source": source, "reason": "visible" }),
            );
            return;
        }

        let controller = platform_webview.controller();
        let Ok(webview) = (unsafe { controller.CoreWebView2() }) else {
            return;
        };
        let Ok(webview3) = webview.cast::<ICoreWebView2_3>() else {
            return;
        };

        let _ = unsafe { controller.SetIsVisible(false) };
        let handler = TrySuspendCompletedHandler::create(Box::new(move |result, is_successful| {
            trace_main(
                "suspend:completed",
                json!({
                    "source": source,
                    "successful": is_successful,
                    "hr": format!("{result:?}"),
                }),
            );
            Ok(())
        }));
        // 同期 Err（前提条件不成立など）は完了ハンドラが呼ばれず沈黙する故障モード。
        // SNOTRA_TRACE 有効時は同期戻り値も残し、再発を観測可能にしておく。
        let call_hr = unsafe { webview3.TrySuspend(&handler) };
        trace_main(
            "suspend:call_returned",
            json!({ "source": source, "hr": format!("{call_hr:?}") }),
        );
    });
}

/// Resume the WebView2 renderer before showing the window.
/// Best-effort: silently ignored if not suspended or runtime too old.
#[cfg(windows)]
fn resume_webview(window: &tauri::WebviewWindow) {
    let _ = window.with_webview(|platform_webview| {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
        use windows_core_0_61::Interface;

        let controller = platform_webview.controller();
        let Ok(webview) = (unsafe { controller.CoreWebView2() }) else {
            return;
        };
        let Ok(webview3) = webview.cast::<ICoreWebView2_3>() else {
            return;
        };

        // suspend_webview が下げた controller.IsVisible を戻してから Resume する
        // （IsVisible=true 自体にも自動 resume の効果があり、両者とも冪等）。
        let _ = unsafe { controller.SetIsVisible(true) };
        let call_hr = unsafe { webview3.Resume() };
        // suspend 側と同型の「同期 Err は沈黙する」故障モードを可観測にしておく
        // （SNOTRA_TRACE ゲート済み）。
        trace_main("resume:call_returned", json!({ "hr": format!("{call_hr:?}") }));
    });
}

#[cfg(not(windows))]
fn suspend_webview(_window: &tauri::WebviewWindow, _source: &'static str) {}

#[cfg(not(windows))]
fn resume_webview(_window: &tauri::WebviewWindow) {}

/// hide 完了後の共通後処理: WebView2 の suspend → working set trim を
/// メインスレッドのイベントループへ積む。全 hide 経路（hotkey トグル /
/// `notify_main_hidden` = フロントエンド起因）が共有する唯一の経路。
///
/// 任意スレッドから呼べる。`with_webview` / `run_on_main_thread` のクロージャは
/// 同一キューで FIFO 直列化されるため、後続 show の resume に追い越されない。
/// 再表示と競合した稀な逆転は suspend_webview 側の `main_visible` ガード +
/// `show_main_and_emit` 末尾の resume 再適用が是正する。trim は可視中でも無害
/// （OS が透過 re-fault するだけ）。すべて best-effort。
/// イベントループへ積めない場合（終了時など）も trim だけは行う。
pub(crate) fn suspend_and_trim_after_hide(app: &AppHandle, source: &'static str) {
    let pid = std::process::id();
    let app_for_main = app.clone();
    let scheduled = app.run_on_main_thread(move || {
        if let Some(w) = app_for_main.get_webview_window("main") {
            suspend_webview(&w, source);
        }
        working_set::trim_idle_working_set(pid);
    });
    if scheduled.is_err() {
        working_set::trim_idle_working_set(pid);
    }
}

/// Position the main window on the target monitor using saved relative coordinates.
///
/// Target monitor is determined by `follow_cursor_monitor` config:
/// - true: monitor containing the mouse cursor
/// - false: primary monitor
///
/// Saved relative coordinates (physical pixels from monitor work area origin)
/// are applied and clamped to the target work area. If no saved position exists,
/// the window is centered on the target monitor.
#[cfg(windows)]
fn position_on_target_monitor(
    app_handle: &AppHandle,
    main: &tauri::WebviewWindow,
) {
    use snotra_core::window_data;

    // Read follow_cursor_monitor from Engine config (refreshed on every show).
    let follow_cursor = app_handle
        .try_state::<AppState>()
        .map(|s| s.engine.lock().unwrap().config().general.follow_cursor_monitor)
        .unwrap_or(true);

    // Determine target monitor work area.
    let target_wa = if follow_cursor {
        monitor::cursor_monitor_work_area()
    } else {
        monitor::primary_monitor_work_area()
    };
    let Some(target_wa) = target_wa else { return };

    // Get current window size (physical) for centering/clamping.
    let Ok(win_size) = main.outer_size() else { return };
    let win_w = win_size.width as i32;
    let win_h = win_size.height as i32;

    // Load saved relative placement and convert to absolute on target monitor.
    let (abs_x, abs_y) = if let Some(placement) = window_data::load_search_placement() {
        // Saved coordinates are physical pixels relative to monitor work area origin.
        let x = target_wa.left + placement.x;
        let y = target_wa.top + placement.y;
        // Clamp to ensure the window stays within the target work area
        // (handles different-sized monitors).
        target_wa.clamp(x, y, win_w, win_h)
    } else {
        // No saved position — center on target monitor.
        target_wa.center(win_w, win_h)
    };

    let _ = main.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
        abs_x, abs_y,
    )));
}

fn elapsed_ms(t0: Instant) -> f64 {
    t0.elapsed().as_secs_f64() * 1000.0
}

/// Reset window height to search-bar-only (52px) before positioning.
/// Must run before `position_on_target_monitor` so clamping/centering uses
/// the collapsed height, not the stale expanded height from a previous show.
fn reset_search_height(main: &tauri::WebviewWindow) {
    if let Ok(current) = main.inner_size() {
        let sf = main.scale_factor().unwrap_or(1.0);
        let logical_w = current.width as f64 / sf;
        let _ = main.set_size(tauri::Size::Logical(tauri::LogicalSize::new(logical_w, 52.0)));
    }
}

/// Show the main window and give it focus.
///
/// Order (must not change): `show()` (idempotent — called unconditionally to
/// skip the costly `is_visible()` pre-check) → update tracked `main_visible`
/// flag → `set_focus()` → synchronous `WM_NULL` wait (`SetForegroundWindow` is
/// partially async per Raymond Chen; this blocks until focus activation
/// messages are fully processed) → Alt key-up (must follow confirmed focus
/// transfer, see `send_alt_key_up` doc comment) — but skipped while the
/// physical Alt is still held (#558), since injecting a key-up then would
/// desync the OS logical modifier from the physical key and dead-zone Alt+Q.
fn show_and_focus_main(app_handle: &AppHandle, main: &tauri::WebviewWindow, t0: Instant) {
    trace_main("show_main:show:start", json!({ "ms": elapsed_ms(t0) }));
    let _ = main.show();
    trace_main("show_main:show:end", json!({ "ms": elapsed_ms(t0) }));

    // Update tracked visibility (used by hotkey toggle instead of Win32 is_visible).
    if let Some(state) = app_handle.try_state::<AppState>() {
        state.main_visible.store(true, Ordering::SeqCst);
    }

    trace_main("show_main:set_focus:start", json!({ "ms": elapsed_ms(t0) }));
    let _ = main.set_focus();
    trace_main("show_main:set_focus:end", json!({ "ms": elapsed_ms(t0) }));

    // Ensure focus transfer is fully processed before sending synthetic
    // key-ups.  SetForegroundWindow is partially asynchronous (Raymond
    // Chen); WM_NULL via SendMessageTimeout blocks until the target has
    // processed all pending activation messages.
    #[cfg(windows)]
    if let Ok(hwnd) = main.hwnd() {
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            SendMessageTimeoutW, SMTO_NORMAL, WM_NULL,
        };
        let hwnd = HWND(hwnd.0);
        let mut result = 0usize;
        unsafe {
            let _ = SendMessageTimeoutW(
                hwnd, WM_NULL, WPARAM(0), LPARAM(0),
                SMTO_NORMAL, 100, Some(&mut result),
            );
        }
    }

    // Clear lingering Alt modifier after focus is confirmed — but only when the
    // physical Alt is already released. Injecting a key-up while Alt is still
    // physically held releases only the OS logical modifier state, desyncing it
    // from the physical key so Alt+Q won't fire until Alt is pressed again
    // (dead-zone observed on the timeout show path, #558). If Alt is still down,
    // the key-up arrives naturally when the user releases it, so no injection is
    // needed. SNOTRA_TRACE makes the skip observable for the manual repro.
    if is_alt_pressed() {
        trace_main("show_main:alt_release_skipped", json!({ "ms": elapsed_ms(t0) }));
        return;
    }
    send_alt_key_up();
}

/// Turn IME composition off on the main window. Must run after focus is
/// confirmed (called from `show_main_and_emit` after `show_and_focus_main`).
fn apply_ime_control(app_handle: &AppHandle, main: &tauri::WebviewWindow, t0: Instant) {
    trace_main("show_main:ime_control:start", json!({ "ms": elapsed_ms(t0) }));
    if let Some(bridge) = app_handle.try_state::<Mutex<PlatformBridge>>()
        && let Ok(b) = bridge.lock()
    {
        #[cfg(windows)]
        if let Ok(hwnd) = main.hwnd() {
            b.send_command(PlatformCommand::TurnOffIme(hwnd.0 as usize));
        }
    }
    trace_main("show_main:ime_control:end", json!({ "ms": elapsed_ms(t0) }));
}

/// Emit `window-shown` to the frontend. Must be the last step of
/// `show_main_and_emit` (frontend reacts to this event assuming the window is
/// already shown, focused, and IME state is settled).
fn emit_window_shown(app_handle: &AppHandle, t0: Instant) {
    trace_main(
        "show_main:emit_window_shown:start",
        json!({ "ms": elapsed_ms(t0) }),
    );
    let _ = app_handle.emit("window-shown", ());
    trace_main(
        "show_main:emit_window_shown:end",
        json!({ "ms": elapsed_ms(t0) }),
    );
}

fn show_main_and_emit(app_handle: &AppHandle) {
    let t0 = Instant::now();

    if let Some(main) = app_handle.get_webview_window("main") {
        // Resume WebView2 renderer before any window operations.
        // Must precede show() and emit() so the renderer can process
        // DOM updates and receive events.
        // Note: when called from a spawned thread (Alt-wait path),
        // with_webview() dispatches asynchronously, but ordering is
        // maintained because all operations serialize on the main
        // thread's event loop.
        resume_webview(&main);

        trace_main("show_main:start", json!({ "ms": elapsed_ms(t0) }));

        // Reset height before positioning (see reset_search_height doc comment).
        reset_search_height(&main);

        // Position window on the target monitor (cursor or primary) using
        // saved relative coordinates, clamped to the target work area.
        // Must run after height reset so clamp uses the collapsed size.
        #[cfg(windows)]
        position_on_target_monitor(app_handle, &main);

        show_and_focus_main(app_handle, &main, t0);

        // 逆転是正: notify_main_hidden の suspend クロージャは hide 完了後に非同期で
        // 積まれるため、直後に show が来ると冒頭の resume より後に実行されうる。
        // suspend 側の main_visible ガードは「show 完了後に実行される」ケースを捕捉するが、
        // 「冒頭 resume の実行後〜main_visible=true 反映前」に滑り込むケースは通り抜ける。
        // show 完了後にもう一度 resume を積む（冪等）ことで、この残余窓も閉じる。
        resume_webview(&main);

        // ime_off_on_show を実行中の config から都度読む（follow_cursor_monitor と同じ
        // 「キャッシュしない」設計）。これにより config_watcher のホットリロードが個別の
        // diff/event 追加なしにこのフラグへ届く（#576）。
        let ime_control = app_handle
            .try_state::<AppState>()
            .map(|s| s.engine.lock().unwrap().config().general.ime_off_on_show)
            .unwrap_or(false); // config.rs の既定値と一致
        if ime_control {
            apply_ime_control(app_handle, &main, t0);
        }

        emit_window_shown(app_handle, t0);

        trace_main("show_main:total", json!({ "ms": elapsed_ms(t0) }));
    }
}

fn main() {
    let is_first_run = Config::is_first_run();
    let (config, load_outcome) = Config::load_reporting();

    let (entries, initial_indexing, cached_masks, rescan_task) = if is_first_run {
        (Vec::new(), true, None, None)
    } else {
        let result =
            indexer::load_or_scan_with_stats(&config.paths.scan, config.search.show_hidden_system);
        #[cfg(debug_assertions)]
        {
            let s = &result.stats;
            eprintln!(
                "[index-load] cache_hit={} total={}ms hash={}ms cache_load={}ms scan={}ms sort={}ms cache_save={}ms",
                s.cache_hit,
                s.total_ms,
                s.hash_ms,
                s.cache_load_ms,
                s.scan_ms,
                s.sort_ms,
                s.cache_save_ms,
            );
        }
        (result.entries, false, result.cached_masks, result.rescan_task)
    };

    // PATH エントリのスキャン + マージ
    let (mut entries, mut cached_masks) = (entries, cached_masks);
    if config.search.include_path_env {
        let path_entries = indexer::scan_path_env(&entries, config.search.show_hidden_system);
        if !path_entries.is_empty() {
            if let Some(ref mut masks) = cached_masks {
                indexer::extend_cached_masks(masks, &path_entries);
            }
            entries.extend(path_entries);
        }
    }

    // Lazy-load icon cache on first icon request to keep startup path short.
    let icon_cache_state: IconCacheState = Mutex::new(None);

    let history = HistoryStore::load();

    let show_on_startup = config.general.show_on_startup;
    let show_tray = config.general.show_tray_icon;
    let hotkey_config = config.hotkey.clone();
    let initial_language = config.general.language;
    let window_width = config.appearance.window_width;
    let bg_color = config.visual.background_color.clone();

    let engine = if let Some(masks) = cached_masks {
        Engine::new_from_cache(entries, masks, history, config)
    } else {
        Engine::new(entries, history, config)
    };

    let app_state = AppState {
        engine: Mutex::new(engine),
        indexing: AtomicBool::new(initial_indexing),
        index_build_started: AtomicBool::new(false),
        main_visible: AtomicBool::new(false),
    };

    let app_context = tauri::generate_context!();
    #[cfg(feature = "e2e-webview-automation")]
    let app_context = {
        let mut app_context = app_context;
        let data_dir = std::env::var_os(E2E_WEBVIEW_DATA_DIR_ENV);
        configure_e2e_webview(
            &mut app_context.config_mut().app.windows,
            data_dir.as_deref(),
        )
        .expect("invalid E2E WebView configuration");
        app_context
    };

    // フラグ ON: 宣言窓 "main"（WebView2）を除去して egui が置き換える（#532 SU2・codex #2）。
    // tauri.conf.json は変えず config を実行時ミューテート（E2E 注入と同じ経路）。flag OFF は不変。
    #[allow(unused_mut)]
    let mut app_context = app_context;
    if crate::trace::env_flag("SNOTRA_EGUI_MAIN") {
        app_context
            .config_mut()
            .app
            .windows
            .retain(|w| w.label != "main");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(move |app, _args, _cwd| {
            // When a second instance tries to start, show the main window
            // via show_main_and_emit to ensure height reset, IME control,
            // and window-shown emit are applied consistently.
            show_main_and_emit(app);
        }))
        .manage(app_state)
        .manage(icon_cache_state)
        .manage(SettingsProcessState::default())
        .invoke_handler(tauri::generate_handler![
            commands::search,
            commands::get_history_results,
            commands::launch_item,
            commands::get_matching_tools,
            commands::launch_with_tool,
            commands::list_folder,
            commands::open_settings,
            commands::get_icons_batch,
            commands::save_search_placement,
            commands::notify_main_shown,
            commands::notify_main_hidden,
            commands::get_indexing_state,
            commands::rebuild_index,
            commands::quit_app,
            commands::record_folder_expansion,
            commands::get_bootstrap_payload,
            commands::instant::get_instant_commands,
            commands::instant::execute_instant_command,
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // Restore search window width before event loop starts.
            // Position is handled by show_main_and_emit (multi-monitor aware).
            setup_window_geometry(&app_handle, window_width);

            // Set WebView2 default background to match the theme to prevent
            // a white flash when the window is resized to show results (#193).
            config_watcher::sync_webview_background(&app_handle, &bg_color);

            // Register AcceleratorKeyPressed handler to suppress WM_SYSKEYDOWN
            // (Alt+char) before TranslateMessage → WM_SYSCHAR → DefWindowProc →
            // MessageBeep(0).  This is the root fix for the hotkey beep issue.
            setup_accelerator_handler(&app_handle);

            // Spawn platform thread early to parallelize Win32 init with WebView creation.
            // Tray is NOT created here; SetTrayVisible is sent after full setup (SPEC §7.5).
            // Must precede setup_hotkey_listener below, which sends RegisterInitialHotkey
            // through the platform bridge managed here.
            setup_platform_thread(&app_handle, hotkey_config, initial_language);

            // 窓生成: フラグ ON は egui（platform thread spawn 後・SPEC §8.5 で Win32 初期化と並列化）、
            // OFF は宣言窓が Tauri により既に生成済み（何もしない・flag OFF は不変）。
            if crate::trace::env_flag("SNOTRA_EGUI_MAIN") {
                // show/hide を跨ぐ共有状態（世代・emit dedup）。view/hotkey/hide が参照するので窓生成前に管理下へ。
                app.manage(egui_shell::EguiShellState::default());
                egui_shell::create(app, window_width as f64)?;
            }

            // First-run: launch snotra-settings directly (bypassing the indexing guard
            // in open_settings, since initial_indexing=true during first run).
            setup_first_run(&app_handle, is_first_run);

            // Listen for hotkey-pressed events, then activate the hotkey on the
            // platform thread. Registering the listener before activating the
            // hotkey ensures no event is emitted before there is a receiver to
            // handle it — this order must not change (src-tauri/CLAUDE.md).
            setup_hotkey_listener(&app_handle);

            // Listen for open-settings event from tray
            setup_open_settings_listener(&app_handle);

            // Listen for exit request from tray
            setup_exit_listener(&app_handle);

            // Start config.toml file watcher for external changes (snotra-settings)
            setup_config_watcher(&app_handle);

            // 背景再スキャン（SPEC §3.3 ハイブリッド方式）。キャッシュヒット時のみ。
            // ロジックは snotra-core、spawn と結果の後始末（アイコン無効化）は src-tauri。
            setup_background_rescan(&app_handle, rescan_task);

            // All windows pre-created and all listeners registered; now safe to show tray.
            // Showing tray before this point would allow right-click menu actions before
            // the windows and listeners are ready (SPEC §7.5 / §9). Must run after every
            // setup_*_listener call above.
            setup_tray(&app_handle, show_tray, load_outcome);

            // Show window on startup if configured. Must run last: relies on the
            // platform bridge (IME control) and listeners registered above.
            setup_startup_display(&app_handle, show_on_startup);

            Ok(())
        })
        .run(app_context)
        .expect("error while running tauri application");
}

/// Restore the search window's saved width before the event loop starts.
/// Position is handled separately by `position_on_target_monitor`
/// (multi-monitor aware, applied on every show).
fn setup_window_geometry(app_handle: &AppHandle, window_width: u32) {
    if let Some(w) = app_handle.get_webview_window("main")
        && window_width > 0
        && let Ok(current) = w.inner_size()
    {
        let sf = w.scale_factor().unwrap_or(1.0);
        let logical_h = current.height as f64 / sf;
        let _ = w.set_size(tauri::Size::Logical(tauri::LogicalSize::new(
            f64::from(window_width),
            logical_h,
        )));
    }
}

/// Register the AcceleratorKeyPressed handler used to suppress WM_SYSKEYDOWN
/// (Alt+char) before TranslateMessage → WM_SYSCHAR → DefWindowProc →
/// MessageBeep(0). Root fix for the hotkey beep issue. Must run in the setup
/// phase: `with_webview()` needs the message pump, which only runs freely
/// before the event loop starts (src-tauri/CLAUDE.md WebView2 constraints).
fn setup_accelerator_handler(app_handle: &AppHandle) {
    #[cfg(windows)]
    if let Some(main) = app_handle.get_webview_window("main") {
        main.with_webview(move |platform_webview| {
            use webview2_com::Microsoft::Web::WebView2::Win32::{
                COREWEBVIEW2_KEY_EVENT_KIND,
                COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
            };
            use webview2_com::AcceleratorKeyPressedEventHandler;
            use windows::Win32::UI::Input::KeyboardAndMouse::VK_F4;

            let controller = platform_webview.controller();
            let handler = AcceleratorKeyPressedEventHandler::create(Box::new(
                move |_controller, args| {
                    if let Some(args) = args {
                        let mut kind = COREWEBVIEW2_KEY_EVENT_KIND(0);
                        unsafe { args.KeyEventKind(&mut kind)? };
                        if kind == COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN {
                            let mut vk = 0u32;
                            unsafe { args.VirtualKey(&mut vk)? };
                            // Let Alt+F4 through for window close
                            if vk != VK_F4.0 as u32 {
                                unsafe { args.SetHandled(true)? };
                            }
                        }
                    }
                    Ok(())
                },
            ));
            let mut token = 0i64;
            unsafe {
                let _ = controller.add_AcceleratorKeyPressed(&handler, &mut token);
            }
        }).expect("AcceleratorKeyPressed handler must register in setup phase");
    }
}

/// Spawn the Win32 platform thread early to parallelize its init with WebView
/// creation, and manage the resulting `PlatformBridge` once ready. The tray
/// is NOT created here; `setup_tray` sends `SetTrayVisible` later, after all
/// windows and listeners are ready (SPEC §7.5).
fn setup_platform_thread(app_handle: &AppHandle, hotkey_config: HotkeyConfig, initial_language: Language) {
    let platform_pending = PlatformBridge::begin(app_handle.clone(), hotkey_config, initial_language);

    // Win32 init finishes in a few ms; by the time windows are created it is already done.
    if let Some(bridge) = platform_pending.and_then(PlatformBridgePending::wait) {
        app_handle.manage(Mutex::new(bridge));
    }
}

/// First-run: launch snotra-settings directly (bypassing the indexing guard
/// in open_settings, since initial_indexing=true during first run).
/// Pass --first-run so SettingsApp opens on the Index tab for onboarding.
/// On failure (exe not found / spawn error), fall back to building the index
/// with default paths so the indexing flag eventually clears and the user
/// can open settings via open_settings once the build finishes.
fn setup_first_run(app_handle: &AppHandle, is_first_run: bool) {
    if is_first_run && commands::launch_settings_process(app_handle, &["--first-run"]).is_err() {
        indexing::start_index_build(app_handle);
    }
}

/// Register the `hotkey-pressed` listener, then activate the hotkey on the
/// platform thread. **Order must not change**: registering the listener
/// before sending `RegisterInitialHotkey` ensures no event is emitted before
/// there is a receiver to handle it (src-tauri/CLAUDE.md).
fn setup_hotkey_listener(app_handle: &AppHandle) {
    let handle_for_hotkey = app_handle.clone();
    let hotkey_generation = Arc::new(AtomicU64::new(0));
    let hotkey_generation_for_listener = hotkey_generation;
    app_handle.listen("hotkey-pressed", move |_| {
        let t0 = Instant::now();
        trace_main("hotkey:listener_enter", json!({}));
        // Ignore the hotkey while snotra-settings is running: the user may be
        // pressing the current hotkey combination to configure a new one.
        if let Some(proc_state) = handle_for_hotkey.try_state::<SettingsProcessState>()
            && proc_state.lock().unwrap().is_some()
        {
            return;
        }
        let current_gen = hotkey_generation_for_listener.fetch_add(1, Ordering::SeqCst) + 1;
        // Use tracked AtomicBool instead of Win32 is_visible() to avoid
        // ~35ms cold-call overhead on first hotkey press.
        let visible = handle_for_hotkey
            .try_state::<AppState>()
            .map(|s| s.main_visible.load(Ordering::SeqCst))
            .unwrap_or(false);
        trace_main("hotkey:visible_check", json!({ "visible": visible }));
        // hotkey_toggle を実行中の config から都度読む（follow_cursor_monitor と同じ
        // 「キャッシュしない」設計）。config_watcher のホットリロードが個別の diff/event
        // 追加なしにこのフラグへ届く（#576）。
        // `visible &&` で短絡し、非表示（=show 経路・最も遅延に敏感）では engine ロックを
        // 取らない（hotkey_toggle は可視時の hide 判定にしか使わないため）。
        let hide_on_toggle = visible
            && handle_for_hotkey
                .try_state::<AppState>()
                .map(|s| s.engine.lock().unwrap().config().general.hotkey_toggle)
                .unwrap_or(true); // config.rs の既定値と一致
        if hide_on_toggle {
            if let Some(w) = handle_for_hotkey.get_webview_window("main") {
                let _ = w.hide();
            }
            if let Some(state) = handle_for_hotkey.try_state::<AppState>() {
                state.main_visible.store(false, Ordering::SeqCst);
            }
            // Notify JS side so mainVisible signal updates and Blob URLs are released.
            // Symmetric pair: window-shown is emitted in show_main_and_emit.
            // Must precede suspend so the JS cleanup handler is queued in the
            // renderer before TrySuspend pauses it.
            let _ = handle_for_hotkey.emit("window-hidden", ());
            suspend_and_trim_after_hide(&handle_for_hotkey, "hotkey");
        } else if is_alt_pressed() {
            trace_main("hotkey:alt_wait_start", json!({}));
            let handle_for_show = handle_for_hotkey.clone();
            let hotkey_generation_for_wait = hotkey_generation_for_listener.clone();
            std::thread::spawn(move || {
                wait_alt_release_or_timeout();
                trace_main("hotkey:alt_wait_done", json!({ "waited_ms": t0.elapsed().as_secs_f64() * 1000.0 }));
                if hotkey_generation_for_wait.load(Ordering::SeqCst) != current_gen {
                    return;
                }
                show_main_and_emit(&handle_for_show);
            });
        } else {
            trace_main("hotkey:show_direct", json!({}));
            show_main_and_emit(&handle_for_hotkey);
        }
    });

    // hotkey-pressed listener is now registered; activate hotkey on platform thread.
    // Registering the hotkey only after the listener is ready ensures no event
    // is emitted before there is a receiver to handle it.
    if let Some(bridge) = app_handle.try_state::<Mutex<PlatformBridge>>()
        && let Ok(b) = bridge.lock()
    {
        b.send_command(PlatformCommand::RegisterInitialHotkey);
    }
}

/// Listen for the `open-settings` event emitted by the tray menu.
fn setup_open_settings_listener(app_handle: &AppHandle) {
    let handle_for_settings = app_handle.clone();
    app_handle.listen("open-settings", move |_| {
        let _ = commands::open_settings(
            handle_for_settings.state::<AppState>(),
            handle_for_settings.clone(),
        );
    });
}

/// Listen for the `exit-requested` event emitted by the tray menu: flush
/// unsaved data, kill the snotra-settings child process if running, and exit.
fn setup_exit_listener(app_handle: &AppHandle) {
    let handle_for_exit = app_handle.clone();
    app_handle.listen("exit-requested", move |_| {
        // Flush any unsaved data before exit
        // Capture a consistent final snapshot under the Engine lock, then flush
        // it without holding the lock through filesystem I/O.
        let history_save = {
            let app_state = handle_for_exit.state::<AppState>();
            let mut engine = app_state.engine.lock().unwrap();
            engine.prepare_history_flush()
        };
        if let Some(save) = history_save {
            let _ = save.save();
        }
        {
            let icon_state = handle_for_exit.state::<IconCacheState>();
            let mut cache = icon_state.lock().unwrap();
            if let Some(c) = cache.as_mut() {
                c.save_if_dirty();
            }
        }
        // Kill snotra-settings child process if running.
        if let Some(proc_state) = handle_for_exit.try_state::<SettingsProcessState>()
            && let Ok(mut guard) = proc_state.lock()
            && let Some(mut child) = guard.take()
        {
            let _ = child.kill();
        }
        if let Some(bridge) = handle_for_exit.try_state::<Mutex<PlatformBridge>>()
            && let Ok(b) = bridge.lock()
        {
            b.send_command(PlatformCommand::Exit);
        }
        handle_for_exit.exit(0);
    });
}

/// Start the `config.toml` file watcher for external changes (snotra-settings).
fn setup_config_watcher(app_handle: &AppHandle) {
    if let Some(watcher) = config_watcher::start(app_handle) {
        app_handle.manage(Mutex::new(watcher));
    }
}

/// キャッシュヒット時のみ `Some` で渡される背景再スキャンタスクを低優先度スレッド
/// で実行する（SPEC §3.3 ハイブリッド方式）。ロジックは snotra-core、spawn と結果の
/// 後始末（アイコン無効化）は src-tauri の責務。
fn setup_background_rescan(app_handle: &AppHandle, rescan_task: Option<indexer::BackgroundRescanTask>) {
    if let Some(task) = rescan_task {
        let handle_for_rescan = app_handle.clone();
        let _ = std::thread::Builder::new()
            .name("snotra-index-rescan".to_string())
            .spawn(move || {
                indexer::lower_current_thread_priority();
                if task.run() == indexer::RescanOutcome::Changed
                    && let Some(icons) = handle_for_rescan.try_state::<IconCacheState>()
                {
                    icon::invalidate_icon_cache(&icons);
                }
            });
    }
}

/// Show the tray icon now that all windows are pre-created and all event
/// listeners are registered. Showing tray before this point would allow
/// right-click menu actions before the windows/listeners are ready (SPEC
/// §7.5 / §9).
fn setup_tray(app_handle: &AppHandle, show_tray: bool, load_outcome: LoadOutcome) {
    if show_tray
        && let Some(bridge) = app_handle.try_state::<Mutex<PlatformBridge>>()
        && let Ok(b) = bridge.lock()
    {
        b.send_command(PlatformCommand::SetTrayVisible(true));
        // config が壊れて既定値に復旧した場合、トレイ生成直後に通知する。
        // 復旧時は config=default=show_tray ON のためこの分岐に入る。
        // SetTrayVisible→ShowConfigRecoveryBalloon を同一チャネルに順に積むので、
        // process_commands はトレイ生成後にバルーンを処理する。
        if load_outcome == LoadOutcome::RecoveredFromCorrupt {
            b.send_command(PlatformCommand::ShowConfigRecoveryBalloon);
        }
    }
}

/// Show the main window on startup if configured. Must run after tray/listener
/// setup: `show_main_and_emit` depends on the platform bridge (IME control).
fn setup_startup_display(app_handle: &AppHandle, show_on_startup: bool) {
    if show_on_startup {
        show_main_and_emit(app_handle);
    }
}

#[cfg(test)]
mod e2e_webview_config_tests {
    use std::ffi::OsStr;

    use super::{E2E_WEBVIEW_BROWSER_ARGS, configure_e2e_webview};
    use tauri::utils::config::WindowConfig;

    fn main_window() -> WindowConfig {
        WindowConfig {
            label: "main".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn e2e_webview_config_sets_trusted_arguments_and_isolated_data_directory() {
        let mut windows = [main_window()];

        let configured =
            configure_e2e_webview(&mut windows, Some(OsStr::new("snotra-e2e-session-1"))).unwrap();

        assert!(configured);
        assert_eq!(
            windows[0].additional_browser_args.as_deref(),
            Some(E2E_WEBVIEW_BROWSER_ARGS)
        );
        assert_eq!(
            windows[0].data_directory.as_deref(),
            Some(std::path::Path::new("snotra-e2e-session-1"))
        );
    }

    #[test]
    fn e2e_webview_config_without_session_directory_preserves_production_defaults() {
        let mut windows = [main_window()];

        let configured = configure_e2e_webview(&mut windows, None).unwrap();

        assert!(!configured);
        assert!(windows[0].additional_browser_args.is_none());
        assert!(windows[0].data_directory.is_none());
    }

    #[test]
    fn e2e_webview_config_rejects_unsafe_session_directories() {
        for invalid in ["", "..", "../shared", "nested/session", r"C:\absolute"] {
            let mut windows = [main_window()];
            assert!(
                configure_e2e_webview(&mut windows, Some(OsStr::new(invalid))).is_err(),
                "{invalid} must be rejected"
            );
        }
    }
}
