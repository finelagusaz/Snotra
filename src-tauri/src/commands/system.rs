use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, State};

use crate::indexing;
use crate::state::AppState;

#[tauri::command]
pub fn rebuild_index(state: State<AppState>, app: AppHandle) -> bool {
    if state.indexing.load(Ordering::SeqCst) {
        return false;
    }
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
    // フロントエンド起因の hide（フォーカス喪失/Escape/クリック起動/スラッシュ）も
    // working set を回収する。EmptyWorkingSet はスレッド非依存ゆえ tokio IPC スレッドから
    // 安全に呼べる（suspend_webview の with_webview 非同期制約がない）。best-effort。
    crate::working_set::trim_idle_working_set(std::process::id());
}
