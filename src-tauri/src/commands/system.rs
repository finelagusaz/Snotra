use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, Manager, State};

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
pub fn list_system_fonts() -> Vec<String> {
    #[cfg(windows)]
    {
        use std::collections::BTreeSet;
        use windows::Win32::Foundation::LPARAM;
        use windows::Win32::Graphics::Gdi::*;

        unsafe extern "system" fn enum_callback(
            logfont: *const LOGFONTW,
            _text_metric: *const TEXTMETRICW,
            _font_type: u32,
            lparam: LPARAM,
        ) -> i32 {
            unsafe {
                let fonts = &mut *(lparam.0 as *mut BTreeSet<String>);
                let lf = &*logfont;
                let name_len = lf.lfFaceName.iter().position(|&c| c == 0).unwrap_or(32);
                let name = String::from_utf16_lossy(&lf.lfFaceName[..name_len]);
                // @ 始まりは縦書き用フォント、除外
                if !name.starts_with('@') {
                    fonts.insert(name);
                }
                1 // 列挙を続行
            }
        }

        let mut fonts = BTreeSet::<String>::new();
        let hdc = unsafe { GetDC(None) };
        let mut lf: LOGFONTW = unsafe { std::mem::zeroed() };
        lf.lfCharSet = DEFAULT_CHARSET;

        unsafe {
            EnumFontFamiliesExW(
                hdc,
                &lf,
                Some(enum_callback),
                LPARAM(&mut fonts as *mut _ as isize),
                0,
            );
            ReleaseDC(None, hdc);
        }

        fonts.into_iter().collect()
    }
    #[cfg(not(windows))]
    Vec::new()
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    // Reuse the existing exit-requested listener (main.rs)
    // which flushes history/icons, notifies platform, and exits
    let _ = app.emit("exit-requested", ());
}

#[tauri::command]
pub fn record_folder_expansion(path: String, state: State<AppState>) {
    let mut history = state.history.lock().unwrap();
    history.record_folder_expansion(&path);
    history.save_if_dirty(5);
}

#[tauri::command]
pub fn notify_result_clicked(path: String, app: AppHandle) -> Result<(), String> {
    app.emit("result-clicked", path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn notify_result_double_clicked(index: usize, app: AppHandle) -> Result<(), String> {
    app.emit("result-double-clicked", index)
        .map_err(|e| e.to_string())
}
