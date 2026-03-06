use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, State};

use crate::indexing;
use crate::state::AppState;

#[tauri::command]
pub fn rebuild_index(state: State<AppState>, app: AppHandle) -> bool {
    if state.indexing.load(Ordering::SeqCst) {
        return false;
    }
    // Reset the guard so start_index_build can proceed
    state.index_build_started.store(false, Ordering::SeqCst);
    indexing::start_index_build(&app)
}

#[tauri::command]
pub fn get_indexing_state(state: State<AppState>) -> bool {
    state.indexing.load(Ordering::SeqCst)
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    // Reuse the existing exit-requested listener (main.rs)
    // which flushes history/icons, notifies platform, and exits
    let _ = app.emit("exit-requested", ());
}

#[tauri::command]
pub fn record_folder_expansion(path: String, state: State<AppState>) {
    let mut engine = state.engine.lock().unwrap();
    engine.record_folder_expansion(&path);
    engine.save_history_if_dirty(5);
}

#[tauri::command]
pub fn notify_main_shown(state: State<AppState>) {
    state.main_visible.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn notify_main_hidden(state: State<AppState>) {
    state.main_visible.store(false, Ordering::SeqCst);
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
pub fn notify_result_hovered(index: usize, app: AppHandle) -> Result<(), String> {
    app.emit("result-hovered", index).map_err(|e| e.to_string())
}
