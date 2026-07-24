use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, State};

use crate::indexing;
use crate::state::AppState;

use super::window::ERR_INDEXING_IN_PROGRESS;

/// `state.indexing` が既に true かどうかだけを見る純粋な分岐。`start_index_build` 自体は
/// `AppHandle` を要求し単体テスト不能なため、`AppHandle` 抜きでテスト可能な部分を切り出す。
fn ensure_not_indexing(state: &AppState) -> Result<(), String> {
    if state.indexing.load(Ordering::SeqCst) {
        return Err(ERR_INDEXING_IN_PROGRESS.to_string());
    }
    Ok(())
}

// IPC 返り値契約（src-tauri/CLAUDE.md「IPC コマンドの返り値契約」）の「失敗しうる操作系」:
// `open_settings` と同じ ERR_INDEXING_IN_PROGRESS 定数で「インデックス構築中」を表現し、
// 同一条件を同一契約で揃える（#434）。
#[tauri::command]
pub fn rebuild_index(state: State<AppState>, app: AppHandle) -> Result<(), String> {
    ensure_not_indexing(&state)?;
    if indexing::start_index_build(&app) {
        Ok(())
    } else {
        // try_begin_index_build の CAS に負けた（既に別スレッドがビルド中）場合も
        // 同一のエラーコードで表現する。
        Err(ERR_INDEXING_IN_PROGRESS.to_string())
    }
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
    // Keep history snapshot creation atomic with the record update; defer only
    // the filesystem write so searches are not blocked by storage latency.
    let save = {
        let mut engine = state.engine.lock().unwrap();
        engine.record_folder_expansion(&path);
        engine.prepare_history_save_if_dirty(5)
    };
    if let Some(save) = save {
        let _ = save.save();
    }
}

#[tauri::command]
pub fn notify_main_shown(state: State<AppState>) {
    state.main_visible.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn notify_main_hidden(state: State<AppState>, app: AppHandle) {
    state.main_visible.store(false, Ordering::SeqCst);
    let _ = app.emit("window-hidden", ());
    // フロントエンド起因の hide（フォーカス喪失/Escape/クリック起動/スラッシュ）でも
    // hotkey 経路と同じ共通後処理（suspend → trim）を行う。フロントは `await win.hide()`
    // 完了後に本 IPC を呼ぶ（#361）ため、ここに到達した時点でウィンドウは非表示。
    // 共通の理路（FIFO 直列化・競合時の是正・best-effort 性）はヘルパーの doc を参照。
    crate::suspend_and_trim_after_hide(&app, "notify_main_hidden");
}

#[cfg(test)]
mod tests {
    use super::*;
    use snotra_core::config::Config;
    use snotra_core::engine::Engine;
    use snotra_core::history::HistoryStore;
    use std::sync::atomic::{AtomicBool, AtomicU64};
    use std::sync::Mutex;

    fn test_state(indexing: bool) -> AppState {
        AppState {
            engine: Mutex::new(Engine::new(Vec::new(), HistoryStore::load(), Config::default())),
            indexing: AtomicBool::new(indexing),
            index_build_started: AtomicBool::new(false),
            main_visible: AtomicBool::new(false),
            index_generation: AtomicU64::new(0),
        }
    }

    #[test]
    fn ensure_not_indexing_rejects_while_indexing_in_progress() {
        let state = test_state(true);
        assert_eq!(
            ensure_not_indexing(&state),
            Err(ERR_INDEXING_IN_PROGRESS.to_string()),
            "rebuild_index must reject with ERR_INDEXING_IN_PROGRESS while a build is in flight \
             (same contract as open_settings)"
        );
    }

    #[test]
    fn ensure_not_indexing_allows_when_idle() {
        let state = test_state(false);
        assert!(
            ensure_not_indexing(&state).is_ok(),
            "rebuild_index must allow starting a build when idle"
        );
    }
}
