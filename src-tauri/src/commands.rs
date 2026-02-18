use snotra_core::config::Config;
use snotra_core::folder;
use snotra_core::search::SearchMode;
use snotra_core::ui_types::SearchResult;
use snotra_core::window_data::{self, WindowPlacement, WindowSize};
use std::path::Path;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::icon::IconCacheState;
use crate::platform::{PlatformBridge, PlatformCommand};
use crate::state::AppState;

#[tauri::command]
pub fn search(query: String, state: State<AppState>) -> Vec<SearchResult> {
    let config = state.config.lock().unwrap();
    let engine = state.engine.lock().unwrap();
    let history = state.history.lock().unwrap();
    let mode: SearchMode = config.search.normal_mode.into();
    engine.search(&query, config.appearance.max_results, &history, mode)
}

#[tauri::command]
pub fn get_history_results(state: State<AppState>) -> Vec<SearchResult> {
    let config = state.config.lock().unwrap();
    let engine = state.engine.lock().unwrap();
    let history = state.history.lock().unwrap();
    engine.recent_history(&history, config.appearance.max_history_display)
}

#[tauri::command]
pub fn launch_item(path: String, query: String, state: State<AppState>) {
    {
        let mut history = state.history.lock().unwrap();
        history.record_launch(&path, &query);
    }
    #[cfg(windows)]
    {
        use windows::core::HSTRING;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        unsafe {
            ShellExecuteW(
                None,
                &HSTRING::from("open"),
                &HSTRING::from(&path),
                None,
                None,
                SW_SHOWNORMAL,
            );
        }
    }
}

#[tauri::command]
pub fn list_folder(
    dir: String,
    filter: String,
    state: State<AppState>,
) -> Vec<SearchResult> {
    let config = state.config.lock().unwrap();
    let history = state.history.lock().unwrap();
    let mode: SearchMode = config.search.folder_mode.into();
    folder::list_folder(
        Path::new(&dir),
        &filter,
        mode,
        config.search.show_hidden_system,
        &history,
        config.appearance.max_results,
    )
}

#[tauri::command]
pub fn load_config() -> Config {
    Config::load()
}

#[tauri::command]
pub fn save_config(
    config: Config,
    state: State<AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let old_config = state.config.lock().unwrap().clone();
    config.save();

    // Notify platform bridge of hotkey/tray changes
    if let Some(bridge) = app.try_state::<std::sync::Mutex<PlatformBridge>>() {
        if let Ok(b) = bridge.lock() {
            if config.hotkey != old_config.hotkey {
                let (tx, rx) = std::sync::mpsc::channel();
                b.send_command(PlatformCommand::SetHotkey {
                    config: config.hotkey.clone(),
                    reply: tx,
                });
                // Wait for hotkey registration result
                if let Ok(false) = rx.recv() {
                    // Re-register failed, revert in-memory but still save to disk
                }
            }
            if config.general.show_tray_icon != old_config.general.show_tray_icon {
                b.send_command(PlatformCommand::SetTrayVisible(
                    config.general.show_tray_icon,
                ));
            }
        }
    }

    let mut current = state.config.lock().unwrap();
    *current = config;
    Ok(())
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Config {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
pub fn open_settings(app: AppHandle) -> Result<(), String> {
    // If settings window already exists, just focus it
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.set_focus();
        return Ok(());
    }

    WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App(Default::default()))
        .title("Snotra 設定")
        .inner_size(760.0, 560.0)
        .min_inner_size(520.0, 360.0)
        .resizable(true)
        .visible(true)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn get_icon_base64(path: String, icons: State<IconCacheState>) -> Option<String> {
    let cache = icons.lock().unwrap();
    cache.as_ref()?.get_base64(&path).cloned()
}

#[tauri::command]
pub fn get_icons_batch(
    paths: Vec<String>,
    icons: State<IconCacheState>,
) -> std::collections::HashMap<String, String> {
    let cache = icons.lock().unwrap();
    match cache.as_ref() {
        Some(c) => c.get_base64_batch(&paths),
        None => std::collections::HashMap::new(),
    }
}

#[tauri::command]
pub fn get_search_placement() -> Option<WindowPlacement> {
    window_data::load_search_placement()
}

#[tauri::command]
pub fn save_search_placement(x: i32, y: i32) {
    window_data::save_search_placement(WindowPlacement { x, y });
}

#[tauri::command]
pub fn get_settings_placement() -> (Option<WindowPlacement>, Option<WindowSize>) {
    (
        window_data::load_settings_placement(),
        window_data::load_settings_size(),
    )
}

#[tauri::command]
pub fn save_settings_placement(x: i32, y: i32) {
    window_data::save_settings_placement(WindowPlacement { x, y });
}

#[tauri::command]
pub fn save_settings_size(width: i32, height: i32) {
    window_data::save_settings_size(WindowSize { width, height });
}
