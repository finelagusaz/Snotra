//! フォルダ内エントリの列挙とフィルタ/ソート。
//!
//! ソート順は「`is_folder` 降順 → 展開回数降順 → 小文字名昇順 → path 昇順」で先頭が最優先。
//! `select_nth_unstable_by`（O(N) 平均の top-k 選択）＋安定ソートの2段階で、入力順に
//! 依存しない確定順序を保つ。

use nucleo_matcher::{Config as MatcherConfig, Matcher, Utf32Str};
use std::path::Path;

use crate::history::HistoryStore;
use crate::indexer::is_hidden_or_system;
use crate::query::to_lower_folded;
use crate::search::SearchMode;
use crate::ui_types::{IconSource, SearchResult};

#[derive(Debug, Clone)]
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

        if !show_hidden_system && meta.as_ref().map(is_hidden_or_system).unwrap_or(false) {
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

/// フォルダ列挙の全順序比較器。is_folder 降順 → 展開回数降順 → 小文字名昇順 → path 昇順。
/// path を最終 tie-breaker にして全順序化する（同名畳込みの tie が入力順へ漏れるのを防ぐ・#532 SU3 M2）。
fn folder_entry_cmp(
    entries: &[SearchResult],
    a: &(String, u32, usize),
    b: &(String, u32, usize),
) -> std::cmp::Ordering {
    let a_entry = &entries[a.2];
    let b_entry = &entries[b.2];
    b_entry
        .is_folder
        .cmp(&a_entry.is_folder)
        .then_with(|| b.1.cmp(&a.1))
        .then_with(|| a.0.cmp(&b.0))
        .then_with(|| a_entry.path.cmp(&b_entry.path))
}

/// (lower_name, exp_count, index) の並び替えキーを構築する（score_entries / sort_entries_unlimited 共有）。
fn build_sort_keys(entries: &[SearchResult], history: &HistoryStore) -> Vec<(String, u32, usize)> {
    entries
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
        .collect()
}

/// keyed の順序で entries を並べ替えて所有権を取り出す（clone 回避の mem::replace・共有ヘルパー）。
fn collect_by_keyed(
    mut entries: Vec<SearchResult>,
    keyed: Vec<(String, u32, usize)>,
) -> Vec<SearchResult> {
    keyed
        .into_iter()
        .map(|(_, _, i)| {
            std::mem::replace(
                &mut entries[i],
                SearchResult {
                    name: String::new(),
                    path: String::new(),
                    is_folder: false,
                    is_error: false,
                    icon: IconSource::FromPath,
                },
            )
        })
        .collect()
}

/// フィルタ非依存の確定順序で全件をソートする（上限なし）。egui folder 経路がナビゲーション時に
/// 1 度だけ呼び、driver がキャッシュして打鍵ごとに `filter_sorted` で同期に絞る（#532 SU3 M2 B）。
pub fn sort_entries_unlimited(
    entries: Vec<DirEntryData>,
    history: &HistoryStore,
) -> Vec<SearchResult> {
    let entries: Vec<SearchResult> = entries
        .into_iter()
        .map(|e| SearchResult {
            name: e.name,
            path: e.path,
            is_folder: e.is_folder,
            is_error: false,
            icon: IconSource::FromPath,
        })
        .collect();
    let mut keyed = build_sort_keys(&entries, history);
    keyed.sort_by(|a, b| folder_entry_cmp(&entries, a, b));
    collect_by_keyed(entries, keyed)
}

pub(crate) fn score_entries(
    entries: Vec<DirEntryData>,
    history: &HistoryStore,
    max_results: usize,
) -> Vec<SearchResult> {
    let entries: Vec<SearchResult> = entries
        .into_iter()
        .map(|e| SearchResult {
            name: e.name,
            path: e.path,
            is_folder: e.is_folder,
            is_error: false,
            icon: IconSource::FromPath,
        })
        .collect();
    let k = max_results.min(entries.len());
    if k == 0 {
        return vec![];
    }
    let mut keyed = build_sort_keys(&entries, history);
    if k < keyed.len() {
        keyed.select_nth_unstable_by(k - 1, |a, b| folder_entry_cmp(&entries, a, b));
        keyed.truncate(k);
    }
    keyed.sort_by(|a, b| folder_entry_cmp(&entries, a, b));
    collect_by_keyed(entries, keyed)
}

/// キャッシュ済みソート結果を表示名でフィルタし先頭 max 件を返す（#532 SU3 M2・同期・I/O 無し）。
/// egui folder 経路が打鍵ごとに呼ぶ。`read_dir_entries` のフィルタ段（`matches_filter`）を
/// キャッシュへ再適用する。ソート順はキャッシュ時点で確定済みゆえ再スコア不要(表示名のみ・§6.3)。
pub fn filter_sorted(
    cached: &[SearchResult],
    filter: &str,
    mode: SearchMode,
    max: usize,
) -> Vec<SearchResult> {
    if filter.is_empty() {
        return cached.iter().take(max).cloned().collect();
    }
    let filter_lower = to_lower_folded(filter);
    let mut matcher = Matcher::new(MatcherConfig::DEFAULT);
    cached
        .iter()
        .filter(|r| matches_filter(&r.name, &filter_lower, mode, &mut matcher))
        .take(max)
        .cloned()
        .collect()
}

pub fn error_result(dir: &Path) -> Vec<SearchResult> {
    vec![SearchResult {
        name: String::new(), // 表示文字列はUI層が決める。ロジック層は is_error: true の意味だけを持つ
        path: dir.to_string_lossy().to_string(),
        is_folder: false,
        is_error: true,
        icon: IconSource::FromPath,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::HistoryStore;
    use std::fs;
    use std::path::PathBuf;

    /// テスト用の作業ディレクトリを作り直して返す。
    ///
    /// 名前に `std::process::id()` を含める理由は `indexer.rs` の `temp_dir` の doc を正本とする（#978 / #985）。
    ///
    /// prefix は他モジュールと同じ `snotra_<module>_test_` 形に揃えてある——かつての `snotra_test_` はこの形が定まる前の名残であり、意図的な区別ではない（#985）。
    fn temp_dir_with_contents(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("snotra_folder_test_{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn temp_dir_name_contains_process_id() {
        let dir = temp_dir_with_contents("process_unique");
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .expect("temp dir name");
        assert_eq!(
            name,
            format!("snotra_folder_test_process_unique-{}", std::process::id()),
            "作業ディレクトリ名に自プロセスの pid が入っていない（#985）"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    fn empty_history() -> HistoryStore {
        HistoryStore::empty()
    }

    /// `empty_history()` は名前どおり空でなければならない（#963）。
    /// **CI では常に緑で開発機でだけ落ちる**性質は `engine.rs` の同名テストの doc を参照。
    #[test]
    fn empty_history_fixture_is_actually_empty() {
        assert!(
            empty_history().recent_launches(usize::MAX).is_empty(),
            "empty_history() が空でない — 実 history.bin を読んでいる"
        );
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
        // 名前へ pid を含めるのは、このテストが「存在しないこと」を共有 `%TEMP%` 上の固定名に期待しているからである（誰かが同名を作れば落ちる・#985）。
        let dir = std::env::temp_dir().join(format!(
            "snotra_folder_test_nonexistent_zzz-{}",
            std::process::id()
        ));
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

        let results = list_folder(
            &dir,
            "",
            SearchMode::Substring,
            false,
            &empty_history(),
            100,
        );
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
            println!(
                "[topk_sort] entries={n}, max_results={max_results}, avg={avg_us}µs ({iters} iters)"
            );
            let _ = fs::remove_dir_all(&dir);
        }
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
    fn folder_cmp_is_total_order_by_path() {
        // "Café" と "Cafe" は to_lower_folded で同キー化する（アクセント畳込み）。
        // path tie-breaker が無いと select_nth_unstable の境界順が read_dir 順へ漏れる。
        // empty_history() は既存 folder テストのヘルパー（`HistoryStore::empty()`）。
        let hist = empty_history();
        let entries = vec![
            DirEntryData {
                path: "C:\\d\\Cafe".into(),
                name: "Cafe".into(),
                is_folder: false,
            },
            DirEntryData {
                path: "C:\\d\\Café".into(),
                name: "Café".into(),
                is_folder: false,
            },
        ];
        let a = sort_entries_unlimited(entries.clone(), &hist);
        // 入力順を反転しても同一順序（path 昇順で確定）
        let mut rev = entries;
        rev.reverse();
        let b = sort_entries_unlimited(rev, &hist);
        assert_eq!(
            a.iter().map(|r| r.path.clone()).collect::<Vec<_>>(),
            b.iter().map(|r| r.path.clone()).collect::<Vec<_>>()
        );
        // path 昇順: "Cafe" < "Café"（バイト比較）
        assert_eq!(a[0].path, "C:\\d\\Cafe");
    }

    #[test]
    fn unlimited_filter_take_equals_score_entries() {
        // B の中核: 「全ソート→filter→take-k」＝「filter→score_entries→take-k」。
        // 全順序化した今、両者は同一（境界の tie も path で確定）。
        use crate::search::SearchMode;
        let hist = empty_history();
        let entries = vec![
            DirEntryData {
                path: "C:\\d\\Café".into(),
                name: "Café".into(),
                is_folder: false,
            },
            DirEntryData {
                path: "C:\\d\\Cafe".into(),
                name: "Cafe".into(),
                is_folder: false,
            },
            DirEntryData {
                path: "C:\\d\\dog".into(),
                name: "dog".into(),
                is_folder: false,
            },
        ];
        // 経路 A: 全ソート → filter("cafe") → take(1)
        let sorted = sort_entries_unlimited(entries.clone(), &hist);
        let a = filter_sorted(&sorted, "cafe", SearchMode::Substring, 1);
        // 経路 B: read_dir 相当の filter を read_dir_entries で（filter="cafe"）→ score_entries(max=1)
        let filtered = read_dir_entries_for_test(entries, "cafe", SearchMode::Substring);
        let b = score_entries(filtered, &hist, 1);
        assert_eq!(
            a.iter().map(|r| r.path.clone()).collect::<Vec<_>>(),
            b.iter().map(|r| r.path.clone()).collect::<Vec<_>>()
        );
    }

    /// read_dir_entries のフィルタ段だけをテストで再現する（実 I/O を避ける）。matches_filter は
    /// private だが同一モジュールから呼べる。
    #[cfg(test)]
    fn read_dir_entries_for_test(
        entries: Vec<DirEntryData>,
        filter: &str,
        mode: crate::search::SearchMode,
    ) -> Vec<DirEntryData> {
        let filter_lower = to_lower_folded(filter);
        let mut matcher = Matcher::new(MatcherConfig::DEFAULT);
        entries
            .into_iter()
            .filter(|e| {
                filter.is_empty() || matches_filter(&e.name, &filter_lower, mode, &mut matcher)
            })
            .collect()
    }

    #[test]
    fn filter_sorted_empty_returns_prefix_up_to_max() {
        use crate::search::SearchMode;
        let cached = vec![sr("a"), sr("b"), sr("c")];
        let out = filter_sorted(&cached, "", SearchMode::Substring, 2);
        assert_eq!(
            out.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
            vec!["a", "b"]
        );
    }

    #[test]
    fn filter_sorted_matches_display_name_not_path() {
        use crate::search::SearchMode;
        // path に "zzz" を含むが name には含まない → filter "zzz" は 0 件（表示名のみ・§6.3）
        let cached = vec![SearchResult {
            name: "report".into(),
            path: "C:\\zzz\\report".into(),
            is_folder: false,
            is_error: false,
            icon: IconSource::FromPath,
        }];
        let out = filter_sorted(&cached, "zzz", SearchMode::Substring, 8);
        assert!(out.is_empty());
    }

    // テスト用ヘルパー（既存 tests mod にあれば再利用）
    #[cfg(test)]
    fn sr(name: &str) -> SearchResult {
        SearchResult {
            name: name.into(),
            path: format!("C:\\d\\{name}"),
            is_folder: false,
            is_error: false,
            icon: IconSource::FromPath,
        }
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
