use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM};

use crate::history::HistoryStore;
use crate::search::SearchMode;
use crate::ui_types::SearchResult;

pub fn list_folder(
    dir: &Path,
    filter: &str,
    mode: SearchMode,
    show_hidden_system: bool,
    history: &HistoryStore,
    max_results: usize,
) -> Vec<SearchResult> {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return vec![SearchResult {
            name: "アクセスできません".to_string(),
            path: dir.to_string_lossy().to_string(),
            is_folder: false,
            is_error: true,
        }];
    };

    let mut entries: Vec<SearchResult> = read_dir
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !show_hidden_system && !is_visible_entry(&path) {
                return None;
            }
            let name = entry.file_name().to_string_lossy().to_string();

            if !filter.is_empty() && !matches_filter(&name, filter, mode) {
                return None;
            }

            let is_folder = path.is_dir();
            Some(SearchResult {
                name,
                path: path.to_string_lossy().to_string(),
                is_folder,
                is_error: false,
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

fn matches_filter(name: &str, filter: &str, mode: SearchMode) -> bool {
    let name_lower = name.to_lowercase();
    let filter_lower = filter.to_lowercase();
    match mode {
        SearchMode::Prefix => name_lower.starts_with(&filter_lower),
        SearchMode::Substring => name_lower.contains(&filter_lower),
        SearchMode::Fuzzy => SkimMatcherV2::default()
            .fuzzy_match(&name_lower, &filter_lower)
            .is_some(),
    }
}

fn is_visible_entry(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return true;
    };
    let attrs = meta.file_attributes();
    let hidden = (attrs & FILE_ATTRIBUTE_HIDDEN.0) != 0;
    let system = (attrs & FILE_ATTRIBUTE_SYSTEM.0) != 0;
    !hidden && !system
}

pub fn parent_for_navigation(current_dir: &str) -> Option<PathBuf> {
    if is_navigation_root(current_dir) {
        return None;
    }
    let current = Path::new(current_dir);
    let parent = current.parent()?;
    let parent_str = parent.to_string_lossy();
    if parent_str.is_empty() {
        return None;
    }
    Some(parent.to_path_buf())
}

pub fn is_navigation_root(path: &str) -> bool {
    let normalized = path.trim().replace('/', "\\");
    let trimmed = normalized.trim_end_matches('\\');

    if trimmed.len() == 2 {
        let chars: Vec<char> = trimmed.chars().collect();
        return chars[0].is_ascii_alphabetic() && chars[1] == ':';
    }

    if let Some(rest) = trimmed.strip_prefix("\\\\") {
        let parts: Vec<&str> = rest.split('\\').filter(|p| !p.is_empty()).collect();
        return parts.len() <= 2;
    }

    false
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

        let results = list_folder(&dir, "", SearchMode::Substring, true, &empty_history(), 100);
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

        let results = list_folder(&dir, "", SearchMode::Substring, true, &empty_history(), 100);
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

        let results = list_folder(
            &dir,
            "toml",
            SearchMode::Substring,
            true,
            &empty_history(),
            100,
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "config.toml");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_folder_filter_is_case_insensitive() {
        let dir = temp_dir_with_contents("filter_case");
        fs::write(dir.join("README.TXT"), "").unwrap();

        let results = list_folder(
            &dir,
            "readme",
            SearchMode::Substring,
            true,
            &empty_history(),
            100,
        );
        assert_eq!(results.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_folder_respects_max_results() {
        let dir = temp_dir_with_contents("maxresults");
        for i in 0..10 {
            fs::write(dir.join(format!("file{}.txt", i)), "").unwrap();
        }

        let results = list_folder(&dir, "", SearchMode::Substring, true, &empty_history(), 3);
        assert_eq!(results.len(), 3);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_folder_empty_dir_returns_empty() {
        let dir = temp_dir_with_contents("empty");

        let results = list_folder(&dir, "", SearchMode::Substring, true, &empty_history(), 100);
        assert!(results.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_folder_nonexistent_dir_returns_empty() {
        let dir = std::env::temp_dir().join("snotra_test_nonexistent_zzz");
        let results = list_folder(&dir, "", SearchMode::Substring, true, &empty_history(), 100);
        assert_eq!(results.len(), 1);
        assert!(results[0].is_error);
    }

    #[test]
    fn folders_sorted_alphabetically_when_equal_expansion_count() {
        let dir = temp_dir_with_contents("alpha_dirs");
        fs::create_dir(dir.join("zeta")).unwrap();
        fs::create_dir(dir.join("alpha")).unwrap();
        fs::create_dir(dir.join("mu")).unwrap();

        let results = list_folder(&dir, "", SearchMode::Substring, true, &empty_history(), 100);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "mu", "zeta"]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prefix_mode_matches_only_prefix() {
        let dir = temp_dir_with_contents("prefix_filter");
        fs::write(dir.join("report.txt"), "").unwrap();
        fs::write(dir.join("my_report.txt"), "").unwrap();

        let results = list_folder(&dir, "rep", SearchMode::Prefix, true, &empty_history(), 100);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"report.txt"));
        assert!(!names.contains(&"my_report.txt"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fuzzy_mode_matches_skipped_characters() {
        let dir = temp_dir_with_contents("fuzzy_filter");
        fs::write(dir.join("Visual Studio Code.txt"), "").unwrap();

        let results = list_folder(&dir, "vsc", SearchMode::Fuzzy, true, &empty_history(), 100);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Visual Studio Code.txt");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn substring_mode_does_not_match_skipped_characters() {
        let dir = temp_dir_with_contents("substring_not_fuzzy");
        fs::write(dir.join("Visual Studio Code.txt"), "").unwrap();

        let results = list_folder(
            &dir,
            "vsc",
            SearchMode::Substring,
            true,
            &empty_history(),
            100,
        );
        assert!(results.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_drive_root() {
        assert!(is_navigation_root("C:\\"));
        assert!(is_navigation_root("D:"));
    }

    #[test]
    fn detects_unc_root() {
        assert!(is_navigation_root("\\\\server\\share\\"));
        assert!(!is_navigation_root("\\\\server\\share\\folder"));
    }
}
