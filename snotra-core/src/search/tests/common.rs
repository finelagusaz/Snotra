//! テスト共通 fixture（複数の機能別モジュールが共有する最小のエントリ/履歴生成）。

use crate::history::HistoryStore;
use crate::indexer::AppEntry;

pub(super) fn make_entries(names: &[&str]) -> Vec<AppEntry> {
    names
        .iter()
        .map(|n| AppEntry {
            name: n.to_string(),
            target_path: format!("C:\\fake\\{}.lnk", n),
            is_folder: false,
        })
        .collect()
}

pub(super) fn empty_history() -> HistoryStore {
    HistoryStore::load()
}
