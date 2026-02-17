use std::path::Path;

use crate::history::HistoryStore;
use crate::window::SearchResult;

pub fn list_folder(
    dir: &Path,
    filter: &str,
    history: &HistoryStore,
    max_results: usize,
) -> Vec<SearchResult> {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut entries: Vec<SearchResult> = read_dir
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if !filter.is_empty() && !matches_filter(&name, filter) {
                return None;
            }

            let is_folder = path.is_dir();
            Some(SearchResult {
                name,
                path: path.to_string_lossy().to_string(),
                is_folder,
            })
        })
        .collect();

    entries.sort_by(|a, b| {
        // Folders before files
        b.is_folder
            .cmp(&a.is_folder)
            .then_with(|| {
                // Higher expansion count first (for folders)
                let b_count = if b.is_folder {
                    history.folder_expansion_count(&b.path)
                } else {
                    0
                };
                let a_count = if a.is_folder {
                    history.folder_expansion_count(&a.path)
                } else {
                    0
                };
                b_count.cmp(&a_count)
            })
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    entries.truncate(max_results);
    entries
}

fn matches_filter(name: &str, filter: &str) -> bool {
    name.to_lowercase().contains(&filter.to_lowercase())
}
