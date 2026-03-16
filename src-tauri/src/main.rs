#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod config_watcher;
mod icon;
mod ime;
mod indexing;
mod monitor;
mod platform;
mod state;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use serde_json::json;
use snotra_core::config::Config;
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

fn trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        let Ok(v) = std::env::var("SNOTRA_TRACE") else {
            return false;
        };
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn trace_main(event: &str, data: serde_json::Value) {
    if !trace_enabled() {
        return;
    }
    static TRACE_SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = TRACE_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    let ts_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    eprintln!(
        "[trace] {}",
        json!({
            "seq": seq,
            "ts_ms": ts_ms,
            "event": event,
            "data": data,
        })
    );
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

/// Suspend the WebView2 renderer to reduce memory/CPU while hidden.
///
/// Must be called AFTER `hide()` (`IsVisible=false` required by WebView2).
/// Best-effort: silently ignored if WebView2 runtime is too old (< Edge 88)
/// or `IsVisible` is still true.
#[cfg(windows)]
fn suspend_webview(window: &tauri::WebviewWindow) {
    let _ = window.with_webview(|platform_webview| {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
        use webview2_com::TrySuspendCompletedHandler;
        use windows_core_0_61::Interface;

        let controller = platform_webview.controller();
        let Ok(webview) = (unsafe { controller.CoreWebView2() }) else {
            return;
        };
        let Ok(webview3) = webview.cast::<ICoreWebView2_3>() else {
            return;
        };

        let handler =
            TrySuspendCompletedHandler::create(Box::new(|_result, _is_successful| Ok(())));
        let _ = unsafe { webview3.TrySuspend(&handler) };
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

        let _ = unsafe { webview3.Resume() };
    });
}

#[cfg(not(windows))]
fn suspend_webview(_window: &tauri::WebviewWindow) {}

#[cfg(not(windows))]
fn resume_webview(_window: &tauri::WebviewWindow) {}

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

fn show_main_and_emit(app_handle: &AppHandle, ime_control: bool) {
    let t0 = Instant::now();
    let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;

    if let Some(main) = app_handle.get_webview_window("main") {
        // Resume WebView2 renderer before any window operations.
        // Must precede show() and emit() so the renderer can process
        // DOM updates and receive events.
        // Note: when called from a spawned thread (Alt-wait path),
        // with_webview() dispatches asynchronously, but ordering is
        // maintained because all operations serialize on the main
        // thread's event loop.
        resume_webview(&main);

        trace_main("show_main:start", json!({ "ms": ms(t0.elapsed()) }));

        // Reset window height to search-bar-only (52px) before positioning.
        // This ensures position_on_target_monitor uses the correct (collapsed)
        // height for centering and clamping, not the stale expanded height.
        if let Ok(current) = main.inner_size() {
            let sf = main.scale_factor().unwrap_or(1.0);
            let logical_w = current.width as f64 / sf;
            let _ = main.set_size(tauri::Size::Logical(tauri::LogicalSize::new(logical_w, 52.0)));
        }

        // Position window on the target monitor (cursor or primary) using
        // saved relative coordinates, clamped to the target work area.
        // Must run after height reset so clamp uses the collapsed size.
        #[cfg(windows)]
        position_on_target_monitor(app_handle, &main);

        // show() is idempotent — call unconditionally to skip the costly
        // is_visible() pre-check (61ms + 71ms gap on first invocation).
        trace_main("show_main:show:start", json!({ "ms": ms(t0.elapsed()) }));
        let _ = main.show();
        trace_main("show_main:show:end", json!({ "ms": ms(t0.elapsed()) }));

        // Update tracked visibility (used by hotkey toggle instead of Win32 is_visible).
        if let Some(state) = app_handle.try_state::<AppState>() {
            state.main_visible.store(true, Ordering::SeqCst);
        }

        {
            // set_focus
            trace_main("show_main:set_focus:start", json!({ "ms": ms(t0.elapsed()) }));
            let _ = main.set_focus();
            trace_main("show_main:set_focus:end", json!({ "ms": ms(t0.elapsed()) }));

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

            // Clear lingering Alt modifier after focus is confirmed.
            send_alt_key_up();

            // IME control
            if ime_control {
                trace_main("show_main:ime_control:start", json!({ "ms": ms(t0.elapsed()) }));
                if let Some(bridge) = app_handle.try_state::<Mutex<PlatformBridge>>()
                    && let Ok(b) = bridge.lock()
                {
                    #[cfg(windows)]
                    if let Ok(hwnd) = main.hwnd() {
                        b.send_command(PlatformCommand::TurnOffIme(hwnd.0 as usize));
                    }
                }
                trace_main("show_main:ime_control:end", json!({ "ms": ms(t0.elapsed()) }));
            }

            // emit window-shown
            trace_main(
                "show_main:emit_window_shown:start",
                json!({ "ms": ms(t0.elapsed()) }),
            );
            let _ = app_handle.emit("window-shown", ());
            trace_main(
                "show_main:emit_window_shown:end",
                json!({ "ms": ms(t0.elapsed()) }),
            );

            trace_main("show_main:total", json!({ "ms": ms(t0.elapsed()) }));
        }
    }
}

fn main() {
    let is_first_run = Config::is_first_run();
    let config = Config::load();

    let (entries, initial_indexing, cached_masks) = if is_first_run {
        (Vec::new(), true, None)
    } else {
        #[cfg(debug_assertions)]
        let (entries, _, stats, cached_masks) =
            indexer::load_or_scan_with_stats(&config.paths.scan, config.search.show_hidden_system);
        #[cfg(not(debug_assertions))]
        let (entries, _, _, cached_masks) =
            indexer::load_or_scan_with_stats(&config.paths.scan, config.search.show_hidden_system);
        #[cfg(debug_assertions)]
        eprintln!(
            "[index-load] cache_hit={} total={}ms hash={}ms cache_load={}ms scan={}ms sort={}ms cache_save={}ms",
            stats.cache_hit,
            stats.total_ms,
            stats.hash_ms,
            stats.cache_load_ms,
            stats.scan_ms,
            stats.sort_ms,
            stats.cache_save_ms,
        );
        (entries, false, cached_masks)
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

    let history = HistoryStore::load(config.search.effective_top_n_history());

    let show_on_startup = config.general.show_on_startup;
    let show_tray = config.general.show_tray_icon;
    let ime_off = config.general.ime_off_on_show;
    let hotkey_toggle = config.general.hotkey_toggle;
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

    let ime_off_for_si = ime_off;
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(move |app, _args, _cwd| {
            // When a second instance tries to start, show the main window
            // via show_main_and_emit to ensure height reset, IME control,
            // and window-shown emit are applied consistently.
            show_main_and_emit(app, ime_off_for_si);
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
            commands::get_search_placement,
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
            commands::restart_app,
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // Restore search window width before event loop starts.
            // Position is handled by show_main_and_emit (multi-monitor aware).
            if let Some(w) = app.get_webview_window("main")
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

            // Set WebView2 default background to match the theme to prevent
            // a white flash when the window is resized to show results (#193).
            config_watcher::sync_webview_background(&app_handle, &bg_color);

            // Register AcceleratorKeyPressed handler to suppress WM_SYSKEYDOWN
            // (Alt+char) before TranslateMessage → WM_SYSCHAR → DefWindowProc →
            // MessageBeep(0).  This is the root fix for the hotkey beep issue.
            #[cfg(windows)]
            if let Some(main) = app.get_webview_window("main") {
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

            // Spawn platform thread early to parallelize Win32 init with WebView creation.
            // Tray is NOT created here; SetTrayVisible is sent after full setup (SPEC §7.5).
            let platform_pending = PlatformBridge::begin(app_handle.clone(), hotkey_config, initial_language);

            // Win32 init finishes in a few ms; by the time windows are created it is already done.
            if let Some(bridge) = platform_pending.and_then(PlatformBridgePending::wait) {
                app_handle.manage(Mutex::new(bridge));
            }

            // First-run: launch snotra-settings directly (bypassing the indexing guard
            // in open_settings, since initial_indexing=true during first run).
            // Pass --first-run so SettingsApp opens on the Index tab for onboarding.
            // On failure (exe not found / spawn error), fall back to building the index
            // with default paths so the indexing flag eventually clears and the user
            // can open settings via open_settings once the build finishes.
            if is_first_run
                && commands::launch_settings_process(&app_handle, &["--first-run"]).is_err()
            {
                indexing::start_index_build(&app_handle);
            }

            // Listen for hotkey toggle events
            let handle_for_hotkey = app_handle.clone();
            let toggle = hotkey_toggle;
            let ime_control = ime_off;
            let hotkey_generation = Arc::new(AtomicU64::new(0));
            let hotkey_generation_for_listener = hotkey_generation.clone();
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
                if visible && toggle {
                    if let Some(w) = handle_for_hotkey.get_webview_window("main") {
                        let _ = w.hide();
                        // Suspend WebView2 renderer after hide (IsVisible=false).
                        // Reduces memory/CPU while the launcher is hidden.
                        suspend_webview(&w);
                    }
                    if let Some(state) = handle_for_hotkey.try_state::<AppState>() {
                        state.main_visible.store(false, Ordering::SeqCst);
                    }
                    // Notify JS side so mainVisible signal updates and Blob URLs are released.
                    // Symmetric pair: window-shown is emitted in show_main_and_emit.
                    let _ = handle_for_hotkey.emit("window-hidden", ());
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
                        show_main_and_emit(&handle_for_show, ime_control);
                    });
                } else {
                    trace_main("hotkey:show_direct", json!({}));
                    show_main_and_emit(&handle_for_hotkey, ime_control);
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

            // Listen for open-settings event from tray
            let handle_for_settings = app_handle.clone();
            app_handle.listen("open-settings", move |_| {
                let _ = commands::open_settings(
                    handle_for_settings.state::<AppState>(),
                    handle_for_settings.clone(),
                );
            });

            // Listen for exit request from tray
            let handle_for_exit = app_handle.clone();
            app_handle.listen("exit-requested", move |_| {
                // Flush any unsaved data before exit
                {
                    let app_state = handle_for_exit.state::<AppState>();
                    let mut engine = app_state.engine.lock().unwrap();
                    engine.save_history_if_dirty(1);
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

            // Start config.toml file watcher for external changes (snotra-settings)
            if let Some(watcher) = config_watcher::start(&app_handle) {
                app_handle.manage(Mutex::new(watcher));
            }

            // All windows pre-created and all listeners registered; now safe to show tray.
            // Showing tray before this point would allow right-click menu actions before
            // the windows and listeners are ready (SPEC §7.5 / §9).
            if show_tray
                && let Some(bridge) = app_handle.try_state::<Mutex<PlatformBridge>>()
                && let Ok(b) = bridge.lock()
            {
                b.send_command(PlatformCommand::SetTrayVisible(true));
            }

            // Show window on startup if configured
            if show_on_startup {
                show_main_and_emit(&app_handle, ime_off);
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
