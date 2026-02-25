#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod hotkey;
mod icon;
mod ime;
mod indexing;
mod platform;
mod state;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use serde_json::json;
use snotra_core::config::Config;
use snotra_core::history::HistoryStore;
use snotra_core::indexer;
use snotra_core::search::SearchEngine;
use snotra_core::window_data;
use tauri::{AppHandle, Emitter, Listener, Manager};

use crate::icon::IconCacheState;

use crate::platform::{PlatformBridge, PlatformCommand};
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

fn ensure_window_with_timing<F>(app_handle: &AppHandle, label: &str, ensure: F) -> tauri::Result<()>
where
    F: FnOnce(&AppHandle) -> tauri::Result<()>,
{
    let started = Instant::now();
    let result = ensure(app_handle);
    match &result {
        Ok(()) => trace_main(
            "main:ensure_window:ok",
            json!({
                "label": label,
                "elapsed_ms": started.elapsed().as_millis(),
            }),
        ),
        Err(e) => trace_main(
            "main:ensure_window:error",
            json!({
                "label": label,
                "elapsed_ms": started.elapsed().as_millis(),
                "error": e.to_string(),
            }),
        ),
    }
    result
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

fn show_main_and_emit(app_handle: &AppHandle, ime_control: bool) {
    if let Some(main) = app_handle.get_webview_window("main") {
        if !main.is_visible().unwrap_or(false) {
            let _ = main.show();
        }

        if main.is_visible().unwrap_or(false) {
            let _ = main.set_focus();

            // Turn off IME if configured
            if ime_control
                && let Some(bridge) = app_handle.try_state::<Mutex<PlatformBridge>>()
                && let Ok(b) = bridge.lock()
            {
                #[cfg(windows)]
                if let Ok(hwnd) = main.hwnd() {
                    b.send_command(PlatformCommand::TurnOffIme(hwnd.0 as usize));
                }
            }

            // Notify frontend to reset search state
            let _ = app_handle.emit("window-shown", ());
        }
    }
}

fn main() {
    let is_first_run = Config::is_first_run();
    let config = Config::load();

    let (entries, initial_indexing) = if is_first_run {
        (Vec::new(), true)
    } else {
        let (entries, _) =
            indexer::load_or_scan(&config.paths.scan, config.search.show_hidden_system);
        (entries, false)
    };

    let icon_cache_state: IconCacheState = if config.appearance.show_icons {
        // Lazy-load icon cache on first icon request to keep startup path short.
        Mutex::new(None)
    } else {
        Mutex::new(None)
    };

    let history = HistoryStore::load(
        config.appearance.top_n_history,
        config.appearance.max_history_display,
    );

    let engine = SearchEngine::new(entries);
    let show_on_startup = config.general.show_on_startup;
    let show_tray = config.general.show_tray_icon;
    let ime_off = config.general.ime_off_on_show;
    let hotkey_toggle = config.general.hotkey_toggle;
    let hotkey_config = config.hotkey.clone();
    let window_width = config.appearance.window_width;

    let app_state = AppState {
        engine: Mutex::new(engine),
        history: Mutex::new(history),
        config: Mutex::new(config),
        indexing: AtomicBool::new(initial_indexing),
        index_build_started: AtomicBool::new(false),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // When a second instance tries to start, show the main window
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .manage(icon_cache_state)
        .invoke_handler(tauri::generate_handler![
            commands::search,
            commands::get_history_results,
            commands::launch_item,
            commands::list_folder,
            commands::load_config,
            commands::save_config,
            commands::get_config,
            commands::open_settings,
            commands::open_about,
            commands::get_icon_base64,
            commands::get_icons_batch,
            commands::get_search_placement,
            commands::save_search_placement,
            commands::get_settings_placement,
            commands::save_settings_placement,
            commands::save_settings_size,
            commands::set_window_no_activate,
            commands::notify_result_clicked,
            commands::notify_result_double_clicked,
            commands::get_indexing_state,
            commands::list_system_fonts,
            commands::rebuild_index,
            commands::quit_app,
            commands::record_folder_expansion,
            commands::get_bootstrap_payload,
            commands::ensure_window,
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // Restore search window position/size before event loop starts
            // to avoid racing with hotkey-show (root cause of first-show input delay).
            if let Some(w) = app.get_webview_window("main") {
                if let Some(placement) = window_data::load_search_placement() {
                    let _ = w.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(
                        placement.x as f64,
                        placement.y as f64,
                    )));
                }
                if window_width > 0 {
                    if let Ok(current) = w.inner_size() {
                        let sf = w.scale_factor().unwrap_or(1.0);
                        let logical_h = current.height as f64 / sf;
                        let _ = w.set_size(tauri::Size::Logical(tauri::LogicalSize::new(
                            f64::from(window_width),
                            logical_h,
                        )));
                    }
                }
            }

            // Pre-create secondary windows for stability.
            ensure_window_with_timing(&app_handle, "results", commands::ensure_results_window)?;
            ensure_window_with_timing(&app_handle, "about", commands::ensure_about_window)?;
            ensure_window_with_timing(&app_handle, "settings", commands::ensure_settings_window)?;

            // Start platform thread (hotkey, tray, IME)
            let platform = PlatformBridge::start(app_handle.clone(), hotkey_config, show_tray);

            // Store platform bridge for later use
            if let Some(bridge) = platform {
                app_handle.manage(Mutex::new(bridge));
            }

            if is_first_run {
                if let Some(settings_window) = app_handle.get_webview_window("settings") {
                    let _ = settings_window.show();
                    let _ = settings_window.set_focus();
                }
            }

            // Listen for hotkey toggle events
            let handle_for_hotkey = app_handle.clone();
            let toggle = hotkey_toggle;
            let ime_control = ime_off;
            let hotkey_generation = Arc::new(AtomicU64::new(0));
            let hotkey_generation_for_listener = hotkey_generation.clone();
            app_handle.listen("hotkey-pressed", move |_| {
                let current_gen = hotkey_generation_for_listener.fetch_add(1, Ordering::SeqCst) + 1;
                if let Some(w) = handle_for_hotkey.get_webview_window("main") {
                    let visible = w.is_visible().unwrap_or(false);
                    if visible && toggle {
                        let _ = w.hide();
                        // Also hide results window
                        if let Some(rw) = handle_for_hotkey.get_webview_window("results") {
                            let _ = rw.hide();
                        }
                    } else {
                        if is_alt_pressed() {
                            let handle_for_show = handle_for_hotkey.clone();
                            let hotkey_generation_for_wait = hotkey_generation_for_listener.clone();
                            std::thread::spawn(move || {
                                wait_alt_release_or_timeout();
                                if hotkey_generation_for_wait.load(Ordering::SeqCst) != current_gen
                                {
                                    return;
                                }
                                show_main_and_emit(&handle_for_show, ime_control);
                            });
                        } else {
                            show_main_and_emit(&handle_for_hotkey, ime_control);
                        }
                    }
                }
            });

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
                    let mut history = app_state.history.lock().unwrap();
                    history.save_if_dirty(1);
                }
                {
                    let icon_state = handle_for_exit.state::<IconCacheState>();
                    let mut cache = icon_state.lock().unwrap();
                    if let Some(c) = cache.as_mut() {
                        c.save_if_dirty();
                    }
                }
                if let Some(bridge) = handle_for_exit.try_state::<Mutex<PlatformBridge>>()
                    && let Ok(b) = bridge.lock()
                {
                    b.send_command(PlatformCommand::Exit);
                }
                handle_for_exit.exit(0);
            });

            // Show window on startup if configured
            if show_on_startup {
                show_main_and_emit(&app_handle, ime_off);
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
