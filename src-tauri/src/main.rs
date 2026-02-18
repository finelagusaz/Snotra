#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod hotkey;
mod icon;
mod ime;
mod platform;
mod state;

use std::sync::Mutex;

use snotra_core::config::Config;
use snotra_core::history::HistoryStore;
use snotra_core::indexer;
use snotra_core::search::SearchEngine;
use tauri::{Emitter, Listener, Manager};

use crate::platform::{PlatformBridge, PlatformCommand};
use crate::state::AppState;

fn main() {
    let config = Config::load();

    let (entries, _) = indexer::load_or_scan(
        &config.paths.additional,
        &config.paths.scan,
        config.search.show_hidden_system,
    );

    let history = HistoryStore::load(
        config.appearance.top_n_history,
        config.appearance.max_history_display,
    );

    let icon_cache_state = if config.appearance.show_icons {
        icon::init_icon_cache(&entries)
    } else {
        std::sync::Mutex::new(None)
    };

    let engine = SearchEngine::new(entries);
    let show_on_startup = config.general.show_on_startup;
    let show_tray = config.general.show_tray_icon;
    let ime_off = config.general.ime_off_on_show;
    let hotkey_toggle = config.general.hotkey_toggle;
    let hotkey_config = config.hotkey.clone();

    let app_state = AppState {
        engine: Mutex::new(engine),
        history: Mutex::new(history),
        config: Mutex::new(config),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // When a second instance tries to start, show the main window
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
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
            commands::get_icon_base64,
            commands::get_icons_batch,
            commands::get_search_placement,
            commands::save_search_placement,
            commands::get_settings_placement,
            commands::save_settings_placement,
            commands::save_settings_size,
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // Start platform thread (hotkey, tray, IME)
            let platform = PlatformBridge::start(
                app_handle.clone(),
                hotkey_config,
                show_tray,
            );

            // Store platform bridge for later use
            if let Some(bridge) = platform {
                app_handle.manage(Mutex::new(bridge));
            }

            // Listen for hotkey toggle events
            let handle_for_hotkey = app_handle.clone();
            let toggle = hotkey_toggle;
            let ime_control = ime_off;
            app_handle.listen("hotkey-pressed", move |_| {
                if let Some(w) = handle_for_hotkey.get_webview_window("main") {
                    let visible = w.is_visible().unwrap_or(false);
                    if visible && toggle {
                        let _ = w.hide();
                    } else {
                        let _ = w.show();
                        let _ = w.set_focus();

                        // Turn off IME if configured
                        if ime_control {
                            if let Some(bridge) = handle_for_hotkey
                                .try_state::<Mutex<PlatformBridge>>()
                            {
                                if let Ok(b) = bridge.lock() {
                                    b.send_command(PlatformCommand::TurnOffImeForForeground);
                                }
                            }
                        }

                        // Notify frontend to reset search state
                        let _ = handle_for_hotkey.emit("window-shown", ());
                    }
                }
            });

            // Listen for open-settings event from tray
            let handle_for_settings = app_handle.clone();
            app_handle.listen("open-settings", move |_| {
                let _ = commands::open_settings(handle_for_settings.clone());
            });

            // Listen for exit request from tray
            let handle_for_exit = app_handle.clone();
            app_handle.listen("exit-requested", move |_| {
                if let Some(bridge) = handle_for_exit.try_state::<Mutex<PlatformBridge>>() {
                    if let Ok(b) = bridge.lock() {
                        b.send_command(PlatformCommand::Exit);
                    }
                }
                handle_for_exit.exit(0);
            });

            // Show window on startup if configured
            if show_on_startup {
                if let Some(w) = app_handle.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
