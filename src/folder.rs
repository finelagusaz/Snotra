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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::HistoryStore;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir_with_contents(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("snotra_test_{}", tag));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn empty_history() -> HistoryStore {
        HistoryStore::load(10, 8)
    }

    #[test]
    fn list_folder_returns_files_and_dirs() {
        let dir = temp_dir_with_contents("basic");
        fs::write(dir.join("file1.txt"), "").unwrap();
        fs::write(dir.join("file2.txt"), "").unwrap();
        fs::create_dir(dir.join("subdir")).unwrap();

        let results = list_folder(&dir, "", &empty_history(), 100);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"file1.txt"));
        assert!(names.contains(&"file2.txt"));
        assert!(names.contains(&"subdir"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_folder_folders_come_before_files() {
        let dir = temp_dir_with_contents("order");
        fs::write(dir.join("alpha.txt"), "").unwrap();
        fs::create_dir(dir.join("zsubdir")).unwrap();

        let results = list_folder(&dir, "", &empty_history(), 100);
        assert!(results[0].is_folder);
        assert!(!results.last().unwrap().is_folder);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_folder_filter_excludes_non_matching() {
        let dir = temp_dir_with_contents("filter");
        fs::write(dir.join("readme.txt"), "").unwrap();
        fs::write(dir.join("config.toml"), "").unwrap();
        fs::write(dir.join("build.rs"), "").unwrap();

        let results = list_folder(&dir, "toml", &empty_history(), 100);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "config.toml");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_folder_filter_is_case_insensitive() {
        let dir = temp_dir_with_contents("filter_case");
        fs::write(dir.join("README.TXT"), "").unwrap();

        let results = list_folder(&dir, "readme", &empty_history(), 100);
        assert_eq!(results.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_folder_respects_max_results() {
        let dir = temp_dir_with_contents("maxresults");
        for i in 0..10 {
            fs::write(dir.join(format!("file{}.txt", i)), "").unwrap();
        }

        let results = list_folder(&dir, "", &empty_history(), 3);
        assert_eq!(results.len(), 3);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_folder_empty_dir_returns_empty() {
        let dir = temp_dir_with_contents("empty");

        let results = list_folder(&dir, "", &empty_history(), 100);
        assert!(results.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_folder_nonexistent_dir_returns_empty() {
        let dir = std::env::temp_dir().join("snotra_test_nonexistent_zzz");
        let results = list_folder(&dir, "", &empty_history(), 100);
        assert!(results.is_empty());
    }

    #[test]
    fn folders_sorted_alphabetically_when_equal_expansion_count() {
        let dir = temp_dir_with_contents("alpha_dirs");
        fs::create_dir(dir.join("zeta")).unwrap();
        fs::create_dir(dir.join("alpha")).unwrap();
        fs::create_dir(dir.join("mu")).unwrap();

        let results = list_folder(&dir, "", &empty_history(), 100);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "mu", "zeta"]);

        let _ = fs::remove_dir_all(&dir);
    }
}
