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

pub(crate) fn read_dir_entries(
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

        // Windows では DirEntry::metadata() は GetFileAttributesW を呼び出し、
        // シンボリックリンクを辿った先の属性を返す（fs::metadata(path) と同等）。
        // これにより旧実装（is_visible_entry が fs::metadata を別途呼ぶ）と
        // シンボリックリンク挙動は変わらず、かつ追加の stat 呼び出しが不要になる。
        let meta = entry.metadata().ok();

        if !show_hidden_system
            && meta
                .as_ref()
                .map(is_hidden_or_system)
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

    // k=0 のガード（usize アンダーフロー防止）
    let k = max_results.min(entries.len());
    if k == 0 {
        return vec![];
    }

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

    // ソート順: is_folder 降順 → exp_count 降順 → lower_name 昇順
    // 先頭要素が最良（最優先）エントリになる。
    let cmp = |a: &(String, u32, usize), b: &(String, u32, usize)| {
        let a_entry = &entries[a.2];
        let b_entry = &entries[b.2];
        b_entry
            .is_folder
            .cmp(&a_entry.is_folder)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.0.cmp(&b.0))
    };

    if k < keyed.len() {
        // O(N) 平均の partial select で top-k を前方に集める。
        // 全件ソート O(N log N) の代わりに O(N) + O(K log K) で済む。
        keyed.select_nth_unstable_by(k - 1, &cmp);
        keyed.truncate(k);
    }

    // top-k のみを安定ソートして確定順にする
    keyed.sort_by(&cmp);

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

pub fn error_result(dir: &Path) -> Vec<SearchResult> {
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

fn is_hidden_or_system(meta: &std::fs::Metadata) -> bool {
    let attrs = meta.file_attributes();
    let hidden = (attrs & FILE_ATTRIBUTE_HIDDEN.0) != 0;
    let system = (attrs & FILE_ATTRIBUTE_SYSTEM.0) != 0;
    hidden || system
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
        HistoryStore::load(10)
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

    #[test]
    fn hidden_files_excluded_when_show_hidden_system_false() {
        let dir = temp_dir_with_contents("hidden_filter");
        fs::write(dir.join("visible.txt"), "").unwrap();
        let hidden_path = dir.join("hidden.txt");
        fs::write(&hidden_path, "").unwrap();
        std::process::Command::new("attrib")
            .args(["+H", hidden_path.to_str().unwrap()])
            .status()
            .expect("attrib command failed");

        let results = list_folder(&dir, "", SearchMode::Substring, false, &empty_history(), 100);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(results.len(), 1);
        assert!(names.contains(&"visible.txt"));
        assert!(!names.contains(&"hidden.txt"));

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
    #[ignore]
    fn bench_folder_topk_sort() {
        // max_results << N のケース: top-k 選択の効果を確認する
        // N=10_000 エントリを max_results=50 で絞り込む
        for &n in &[1_000, 5_000, 10_000] {
            use std::time::Instant;
            let tag = format!("bench_topk_{n}");
            let dir = temp_dir_with_contents(&tag);
            for i in 0..n {
                fs::write(dir.join(format!("file_{i:05}.txt")), "").unwrap();
            }
            let history = empty_history();
            let max_results = 50;
            // warmup
            let warmup = list_folder(&dir, "", SearchMode::Substring, true, &history, max_results);
            assert_eq!(warmup.len(), max_results);
            let iters = 20usize;
            let mut total_ns = 0u128;
            for _ in 0..iters {
                let t = Instant::now();
                let results =
                    list_folder(&dir, "", SearchMode::Substring, true, &history, max_results);
                total_ns += t.elapsed().as_nanos();
                assert_eq!(results.len(), max_results);
            }
            let avg_us = total_ns / iters as u128 / 1000;
            println!("[topk_sort] entries={n}, max_results={max_results}, avg={avg_us}µs ({iters} iters)");
            let _ = fs::remove_dir_all(&dir);
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

    // --- score_entries の top-k 境界テスト ---

    fn make_file(name: &str) -> DirEntryData {
        DirEntryData {
            name: name.to_string(),
            path: format!("C:\\test\\{}", name),
            is_folder: false,
        }
    }

    fn make_folder(name: &str) -> DirEntryData {
        DirEntryData {
            name: name.to_string(),
            path: format!("C:\\test\\{}", name),
            is_folder: true,
        }
    }

    #[test]
    fn score_entries_fewer_entries_than_max_returns_all() {
        // N=3 < max_results=10 → 全 3 件が返る
        let entries = vec![make_file("a"), make_file("b"), make_file("c")];
        let results = score_entries(entries, &empty_history(), 10);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn score_entries_max_results_zero_returns_empty() {
        // max_results=0 は k=0 ガードで即返却する
        let entries = vec![make_file("a"), make_file("b")];
        let results = score_entries(entries, &empty_history(), 0);
        assert!(results.is_empty());
    }

    #[test]
    fn score_entries_k_equals_n_returns_all() {
        // N==max_results → truncate なしで全件返る
        let entries = vec![make_file("a"), make_file("b"), make_file("c")];
        let results = score_entries(entries, &empty_history(), 3);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn score_entries_k_one_returns_top_folder_over_files() {
        // k=1 のとき最優先（フォルダ）だけが返る
        let entries = vec![make_file("alpha"), make_folder("zeta"), make_file("beta")];
        let results = score_entries(entries, &empty_history(), 1);
        assert_eq!(results.len(), 1);
        assert!(results[0].is_folder);
        assert_eq!(results[0].name, "zeta");
    }

    #[test]
    fn score_entries_top_k_contains_correct_entries_in_order() {
        // 2 フォルダ + 3 ファイルで k=3: フォルダ 2 件 + ファイル先頭 1 件が
        // ソート順（フォルダ降順 → 名前昇順）で返る
        let entries = vec![
            make_file("delta"),
            make_folder("zeta"),
            make_file("alpha"),
            make_folder("alpha_dir"),
            make_file("beta"),
        ];
        let results = score_entries(entries, &empty_history(), 3);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["alpha_dir", "zeta", "alpha"]);
    }

    #[test]
    fn score_entries_top_k_order_independent_of_input_order() {
        // select_nth_unstable_by は不安定なため、最終 sort_by がなければ
        // 入力順によって結果順序が変わる。この回帰テストでその不変条件を固定する。
        let entries1 = vec![
            make_file("charlie"),
            make_file("alpha"),
            make_file("echo"),
            make_file("bravo"),
            make_file("delta"),
        ];
        let entries2 = vec![
            make_file("echo"),
            make_file("delta"),
            make_file("bravo"),
            make_file("alpha"),
            make_file("charlie"),
        ];
        let r1 = score_entries(entries1, &empty_history(), 3);
        let r2 = score_entries(entries2, &empty_history(), 3);
        let names1: Vec<&str> = r1.iter().map(|r| r.name.as_str()).collect();
        let names2: Vec<&str> = r2.iter().map(|r| r.name.as_str()).collect();
        // 入力順によらず同一の top-3 が同一順で返る
        assert_eq!(names1, vec!["alpha", "bravo", "charlie"]);
        assert_eq!(names1, names2);
    }

    #[test]
    fn score_entries_expansion_count_prioritizes_frequently_opened_folder() {
        // expansion_count が高いフォルダは同一グループ内で先頭に来る
        let mut history = empty_history();
        history.record_folder_expansion("C:\\test\\often");
        history.record_folder_expansion("C:\\test\\often");
        history.record_folder_expansion("C:\\test\\often");
        history.record_folder_expansion("C:\\test\\rarely");

        let entries = vec![
            DirEntryData {
                name: "rarely".to_string(),
                path: "C:\\test\\rarely".to_string(),
                is_folder: true,
            },
            DirEntryData {
                name: "often".to_string(),
                path: "C:\\test\\often".to_string(),
                is_folder: true,
            },
            DirEntryData {
                name: "never".to_string(),
                path: "C:\\test\\never".to_string(),
                is_folder: true,
            },
        ];
        // k=2: often(3回) > rarely(1回) > never(0回) の順で上位 2 件が選ばれる
        let results = score_entries(entries, &history, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "often");
        assert_eq!(results[1].name, "rarely");
    }
}
