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
pub fn notify_main_hidden(state: State<AppState>, app: AppHandle) {
    state.main_visible.store(false, Ordering::SeqCst);
    let _ = app.emit("window-hidden", ());
}
