use tauri::{AppHandle, State};

use crate::indexing;
use crate::state::AppState;

use super::window::ERR_INDEXING_IN_PROGRESS;

/// `state.is_indexing()` が既に true かどうかだけを見る純粋な分岐。`start_index_build` 自体は
/// `AppHandle` を要求し単体テスト不能なため、`AppHandle` 抜きでテスト可能な部分を切り出す。
fn ensure_not_indexing(state: &AppState) -> Result<(), String> {
    if state.is_indexing() {
        return Err(ERR_INDEXING_IN_PROGRESS.to_string());
    }
    Ok(())
}

// 返り値契約の「失敗しうる操作系」（`commands/mod.rs` の `//!`・#434）。
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

#[cfg(test)]
mod tests {
    use super::*;
    use snotra_core::config::Config;
    use snotra_core::engine::Engine;
    use snotra_core::history::HistoryStore;

    fn test_state(indexing: bool) -> AppState {
        let engine = Engine::new(Vec::new(), HistoryStore::load(), Config::default());
        AppState::new(engine, indexing)
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
