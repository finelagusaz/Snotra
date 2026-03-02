use nucleo_matcher::{Config as MatcherConfig, Matcher, Utf32Str};
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM};

use crate::history::HistoryStore;
use crate::query::to_lower_folded;
use crate::search::SearchMode;
use crate::ui_types::SearchResult;

#[derive(Debug)]
pub struct DirEntryData {
    path: String,
    name: String,
    is_folder: bool,
}

pub fn list_folder(
    dir: &Path,
    filter: &str,
    mode: SearchMode,
    show_hidden_system: bool,
    history: &HistoryStore,
    max_results: usize,
) -> Vec<SearchResult> {
    let entries = match read_dir_entries(dir, filter, mode, show_hidden_system) {
        Ok(entries) => entries,
        Err(_) => return error_result(dir),
    };

    score_entries(entries, history, max_results)
}

pub fn read_dir_entries(
    dir: &Path,
    filter: &str,
    mode: SearchMode,
    show_hidden_system: bool,
) -> std::io::Result<Vec<DirEntryData>> {
    let read_dir = std::fs::read_dir(dir)?;
    let mut entries = Vec::new();
    let mut matcher = Matcher::new(MatcherConfig::DEFAULT);
    let filter_lower = to_lower_folded(filter);

    for entry in read_dir {
        let Ok(entry) = entry else {
            continue;
        };

        let name = entry.file_name().to_string_lossy().to_string();

        if !filter.is_empty() && !matches_filter(&name, &filter_lower, mode, &mut matcher) {
            continue;
        }

        let meta = entry.metadata().ok();

        if !show_hidden_system
            && meta
                .as_ref()
                .map(|meta| !is_visible_metadata(meta))
                .unwrap_or(false)
        {
            continue;
        }

        let path = entry.path();
        let is_folder = meta
            .as_ref()
            .map(|meta| meta.is_dir())
            .unwrap_or_else(|| path.is_dir());

        entries.push(DirEntryData {
            path: path.to_string_lossy().to_string(),
            name,
            is_folder,
        });
    }

    Ok(entries)
}

pub(crate) fn score_entries(
    entries: Vec<DirEntryData>,
    history: &HistoryStore,
    max_results: usize,
) -> Vec<SearchResult> {
    let mut entries: Vec<SearchResult> = entries
        .into_iter()
        .map(|entry| SearchResult {
            name: entry.name,
            path: entry.path,
            is_folder: entry.is_folder,
            is_error: false,
        })
        .collect();

    // Schwartzian transform: pre-compute sort keys to avoid repeated to_lowercase()
    let mut keyed: Vec<(String, u32, usize)> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let lower = to_lower_folded(&e.name);
            let exp_count = if e.is_folder {
                history.folder_expansion_count(&e.path)
            } else {
                0
            };
            (lower, exp_count, i)
        })
        .collect();

    keyed.sort_by(|a, b| {
        let a_entry = &entries[a.2];
        let b_entry = &entries[b.2];
        b_entry
            .is_folder
            .cmp(&a_entry.is_folder)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.0.cmp(&b.0))
    });

    keyed.truncate(max_results);

    keyed
        .into_iter()
        .map(|(_, _, i)| {
            // Take ownership by swapping with a dummy to avoid clone
            std::mem::replace(
                &mut entries[i],
                SearchResult {
                    name: String::new(),
                    path: String::new(),
                    is_folder: false,
                    is_error: false,
                },
            )
        })
        .collect()
}

fn error_result(dir: &Path) -> Vec<SearchResult> {
    vec![SearchResult {
        name: String::new(), // 表示文字列はUI層が決める。ロジック層は is_error: true の意味だけを持つ
        path: dir.to_string_lossy().to_string(),
        is_folder: false,
        is_error: true,
    }]
}

fn matches_filter(name: &str, filter_lower: &str, mode: SearchMode, matcher: &mut Matcher) -> bool {
    let name_lower = to_lower_folded(name);
    match mode {
        SearchMode::Prefix => name_lower.starts_with(filter_lower),
        SearchMode::Substring => name_lower.contains(filter_lower),
        SearchMode::Fuzzy => {
            let mut haystack_buf = Vec::new();
            let mut needle_buf = Vec::new();
            let haystack = Utf32Str::new(&name_lower, &mut haystack_buf);
            let needle = Utf32Str::new(filter_lower, &mut needle_buf);
            matcher.fuzzy_match(haystack, needle).is_some()
        }
    }
}

fn is_visible_metadata(meta: &std::fs::Metadata) -> bool {
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
        assert_eq!(results[0].name, ""); // UI 文字列はロジック層に持たない
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

    // --- パフォーマンス計測 ---
    // `cargo test -p snotra-core bench_folder_ -- --ignored --nocapture` で実行

    fn bench_folder_search(
        label: &str,
        n: usize,
        filter: &str,
        show_hidden_system: bool,
        expected_results: usize,
    ) {
        use std::time::Instant;

        let tag = format!("bench_folder_{}_{}", label, n);
        let dir = temp_dir_with_contents(&tag);

        for i in 0..n {
            let file_name = if i + 1 == n {
                format!("needle_match_{i}.txt")
            } else {
                format!("filler_file_{i}.txt")
            };
            fs::write(dir.join(file_name), "").unwrap();
        }

        let history = empty_history();

        let warmup = list_folder(
            &dir,
            filter,
            SearchMode::Substring,
            show_hidden_system,
            &history,
            n.max(100),
        );
        assert_eq!(warmup.len(), expected_results);
        if expected_results == 1 {
            assert_eq!(warmup[0].name, format!("needle_match_{}.txt", n - 1));
        }

        let iters = 20usize;
        let mut total_ns = 0u128;
        for _ in 0..iters {
            let t = Instant::now();
            let results = list_folder(
                &dir,
                filter,
                SearchMode::Substring,
                show_hidden_system,
                &history,
                n.max(100),
            );
            total_ns += t.elapsed().as_nanos();
            assert_eq!(results.len(), expected_results);
        }

        let avg_us = total_ns / iters as u128 / 1000;
        println!("[{label}] entries={n}, avg={avg_us}µs ({iters} iters)");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore]
    fn bench_folder_narrow_filter() {
        for &n in &[1_000, 5_000, 10_000] {
            bench_folder_search("folder_narrow", n, "needle", true, 1);
        }
    }

    #[test]
    #[ignore]
    fn bench_folder_hidden_filter_all() {
        for &n in &[1_000, 5_000, 10_000] {
            bench_folder_search("folder_hidden_all", n, "", false, n);
        }
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
