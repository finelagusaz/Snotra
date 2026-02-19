use std::path::Path;
use std::sync::atomic::Ordering;

use snotra_core::config::Config;
use snotra_core::folder;
use snotra_core::search::SearchMode;
use snotra_core::ui_types::SearchResult;
use snotra_core::window_data::{self, WindowPlacement, WindowSize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::icon::IconCacheState;
use crate::indexing;
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
        history.save_if_dirty(5);
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

    {
        let mut current = state.config.lock().unwrap();
        *current = config;
    }

    // If indexing flag is set (first run), start the build and close settings
    if state.indexing.load(Ordering::SeqCst) {
        indexing::start_index_build(&app);
        if let Some(w) = app.get_webview_window("settings") {
            let _ = w.close();
        }
    }

    Ok(())
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Config {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
pub fn open_settings(state: State<AppState>, app: AppHandle) -> Result<(), String> {
    if state.indexing.load(Ordering::SeqCst) {
        return Ok(());
    }

    // The settings window is pre-created in setup() and hidden on close,
    // so it always exists. Just show and focus it.
    if let Some(w) = app.get_webview_window("settings") {
        let _ = app.emit("settings-shown", ());
        let _ = w.show();
        let _ = w.set_focus();
    }
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

#[tauri::command]
pub fn set_window_no_activate(app: AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
        };
        if let Some(w) = app.get_webview_window("results") {
            let raw_hwnd = w.hwnd().map_err(|e| e.to_string())?;
            let hwnd = HWND(raw_hwnd.0);
            unsafe {
                let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
                SetWindowLongW(hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as i32);
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn notify_result_clicked(index: usize, app: AppHandle) -> Result<(), String> {
    app.emit("result-clicked", index).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn notify_result_double_clicked(index: usize, app: AppHandle) -> Result<(), String> {
    app.emit("result-double-clicked", index)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_indexing_state(state: State<AppState>) -> bool {
    state.indexing.load(Ordering::SeqCst)
}
