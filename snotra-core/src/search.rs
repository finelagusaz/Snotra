use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use nucleo_matcher::{Config as MatcherConfig, Matcher, Utf32Str, Utf32String};

use crate::config::{SearchConfig, SearchHistoryNormalizationConfig};
use crate::history::HistoryStore;
use crate::indexer::{AppEntry, normalize_entry_key};
use crate::query::{normalize_query, to_lower_folded};
use crate::ui_types::SearchResult;

const GLOBAL_WEIGHT: i64 = 5;
const QUERY_WEIGHT: i64 = 20;
const FOLDER_EXPANSION_WEIGHT: i64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Prefix,
    Substring,
    Fuzzy,
}

impl From<crate::config::SearchModeConfig> for SearchMode {
    fn from(c: crate::config::SearchModeConfig) -> Self {
        match c {
            crate::config::SearchModeConfig::Prefix => SearchMode::Prefix,
            crate::config::SearchModeConfig::Substring => SearchMode::Substring,
            crate::config::SearchModeConfig::Fuzzy => SearchMode::Fuzzy,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HistoryBoostConfig {
    pub normalization: SearchHistoryNormalizationConfig,
    pub fuzzy_history_cap_ratio: f64,
}

impl Default for HistoryBoostConfig {
    fn default() -> Self {
        Self {
            normalization: SearchHistoryNormalizationConfig::Disabled,
            fuzzy_history_cap_ratio: 0.30,
        }
    }
}

impl From<&SearchConfig> for HistoryBoostConfig {
    fn from(config: &SearchConfig) -> Self {
        Self {
            normalization: config.history_normalization,
            fuzzy_history_cap_ratio: config.fuzzy_history_cap_ratio,
        }
    }
}

pub struct SearchEngine {
    entries: Vec<AppEntry>,
    lower_names: Vec<String>,
    lower_file_names: Vec<Option<String>>,
    /// Pre-computed UTF-32 representations of lower_names for nucleo fuzzy matching.
    lower_names_u32: Vec<Utf32String>,
    /// Pre-computed UTF-32 representations of lower_file_names for nucleo fuzzy matching.
    lower_file_names_u32: Vec<Option<Utf32String>>,
    /// Pre-computed normalized keys for history lookups (one per entry).
    normalized_keys: Vec<String>,
    /// Character-presence bitmask per entry (a-z bits 0-25, 0-9 bits 26-35).
    /// Used to skip entries that cannot possibly match the query.
    char_masks: Vec<u64>,
    /// Character-presence bitmask per file_name entry.
    file_name_char_masks: Vec<u64>,
    matcher: Matcher,
}

impl SearchEngine {
    pub fn new(entries: Vec<AppEntry>) -> Self {
        let lower_names: Vec<String> = entries.iter().map(|e| to_lower_folded(&e.name)).collect();
        let lower_file_names: Vec<Option<String>> = entries
            .iter()
            .map(|e| {
                std::path::Path::new(&e.target_path)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .map(to_lower_folded)
            })
            .collect();
        let lower_names_u32 = lower_names
            .iter()
            .map(|n| Utf32String::from(n.as_str()))
            .collect();
        let lower_file_names_u32 = lower_file_names
            .iter()
            .map(|n| n.as_deref().map(Utf32String::from))
            .collect();
        let normalized_keys = entries
            .iter()
            .map(|e| normalize_entry_key(&e.target_path))
            .collect();
        // to_lower_folded already folds most Latin accents to ASCII (é→e),
        // so non-ASCII names here are typically CJK, Arabic, etc.
        // These get u64::MAX so they always pass the bitmask filter.
        let char_masks = lower_names
            .iter()
            .map(|n| if n.is_ascii() { char_bitmask(n) } else { u64::MAX })
            .collect();
        let file_name_char_masks = lower_file_names
            .iter()
            .map(|n| {
                n.as_deref()
                    .map_or(0, |s| if s.is_ascii() { char_bitmask(s) } else { u64::MAX })
            })
            .collect();
        Self {
            entries,
            lower_names,
            lower_file_names,
            lower_names_u32,
            lower_file_names_u32,
            normalized_keys,
            char_masks,
            file_name_char_masks,
            matcher: Matcher::new(MatcherConfig::DEFAULT),
        }
    }

    pub fn search(
        &mut self,
        query: &str,
        max_results: usize,
        history: &HistoryStore,
        mode: SearchMode,
    ) -> Vec<SearchResult> {
        self.search_with_history_boost(
            query,
            max_results,
            history,
            mode,
            HistoryBoostConfig::default(),
        )
    }

    pub fn search_with_history_boost(
        &mut self,
        query: &str,
        max_results: usize,
        history: &HistoryStore,
        mode: SearchMode,
        history_boost_config: HistoryBoostConfig,
    ) -> Vec<SearchResult> {
        if max_results == 0 {
            return Vec::new();
        }

        let norm_query = normalize_query(query);
        if norm_query.is_empty() {
            return Vec::new();
        }

        let has_dot = norm_query.contains('.');
        let query_mask = char_bitmask(&norm_query);

        // Keep only top `max_results` candidates.
        // `rank_cmp_ranked` defines Better as `Ordering::Less`, so in `BinaryHeap<RankedEntry>`
        // `peek()` points to the current Worst item.
        let mut top_k: BinaryHeap<RankedEntry> = BinaryHeap::with_capacity(max_results);

        // Reuse needle UTF-32 conversion buffer across all entries.
        let mut needle_buf: Vec<char> = Vec::new();

        for i in 0..self.entries.len() {
            // Bitmask pre-filter: skip entries that lack query characters (Fuzzy only).
            // Prefix/Substring use cheap str ops, so the bitmask overhead isn't worth it.
            if mode == SearchMode::Fuzzy {
                let name_mask = self.char_masks[i];
                let fn_mask = self.file_name_char_masks[i];
                if (query_mask & name_mask) != query_mask
                    && (!has_dot || (query_mask & fn_mask) != query_mask)
                {
                    continue;
                }
            }

            let entry = &self.entries[i];
            let lower_name = &self.lower_names[i];
            let lower_file_name = &self.lower_file_names[i];
            let name_u32 = &self.lower_names_u32[i];
            let fn_u32 = &self.lower_file_names_u32[i];
            let norm_key = &self.normalized_keys[i];

            let name_score =
                match_score_single_cached(mode, &mut self.matcher, lower_name, &norm_query, name_u32, &mut needle_buf);

            let score = if has_dot {
                if let Some(s) = name_score {
                    if s > 9000 {
                        // High confidence match, skip heavy fuzzy match on file name
                        Some(s)
                    } else {
                        let fn_score = fn_u32.as_ref().and_then(|u32s| {
                            let fn_name = lower_file_name.as_deref().unwrap_or("");
                            match_score_single_cached(mode, &mut self.matcher, fn_name, &norm_query, u32s, &mut needle_buf)
                        });
                        fn_score.map_or(Some(s), |b| Some(s.max(b)))
                    }
                } else {
                    fn_u32.as_ref().and_then(|u32s| {
                        let fn_name = lower_file_name.as_deref().unwrap_or("");
                        match_score_single_cached(mode, &mut self.matcher, fn_name, &norm_query, u32s, &mut needle_buf)
                    })
                }
            } else {
                name_score
            };

            if let Some(base_score) = score {
                let (global_launches, last_launched) = history.get_global_stats_normalized(norm_key);
                let qcount =
                    history.query_count_pre_normalized(&norm_query, norm_key) as i64;

                let folder_boost = if entry.is_folder {
                    history.folder_expansion_count_normalized(norm_key) as i64
                        * FOLDER_EXPANSION_WEIGHT
                } else {
                    0
                };
                
                let raw_history_boost =
                    (global_launches as i64) * GLOBAL_WEIGHT + qcount * QUERY_WEIGHT + folder_boost;
                let history_boost = adjusted_history_boost(
                    mode,
                    base_score,
                    raw_history_boost,
                    history_boost_config,
                );
                let combined = base_score + history_boost;

                let ranked = RankedEntry {
                    score: combined,
                    last_launched,
                    entry,
                    lower_name: lower_name.as_str(),
                };

                if top_k.len() < max_results {
                    top_k.push(ranked);
                } else if let Some(mut worst) = top_k.peek_mut() {
                    // Replace only when the new item is better than the current worst.
                    if rank_cmp_ranked(&ranked, &worst) == Ordering::Less {
                        *worst = ranked;
                    }
                }
            }
        }

        let mut scored: Vec<RankedEntry> = top_k.into_iter().collect();
        scored.sort_by(rank_cmp_ranked);

        scored
            .into_iter()
            .map(|r| SearchResult {
                name: r.entry.name.clone(),
                path: r.entry.target_path.clone(),
                is_folder: r.entry.is_folder,
                is_error: false,
            })
            .collect()
    }

    pub fn recent_history(&self, history: &HistoryStore, max_results: usize) -> Vec<SearchResult> {
        // recent_launches() は正規化済みキーを返すため、照合側も正規化済み normalized_keys を使う
        let path_to_entry: HashMap<&str, &AppEntry> = self
            .normalized_keys
            .iter()
            .zip(self.entries.iter())
            .map(|(k, e)| (k.as_str(), e))
            .collect();

        history
            .recent_launches()
            .into_iter()
            .take(max_results)
            .filter_map(|path| {
                path_to_entry.get(path).map(|entry| SearchResult {
                    name: entry.name.clone(),
                    path: entry.target_path.clone(),
                    is_folder: entry.is_folder,
                    is_error: false,
                })
            })
            .collect()
    }

    pub fn entries(&self) -> &[AppEntry] {
        &self.entries
    }
}

fn adjusted_history_boost(
    mode: SearchMode,
    base_score: i64,
    raw_history_boost: i64,
    config: HistoryBoostConfig,
) -> i64 {
    if mode != SearchMode::Fuzzy
        || config.normalization != SearchHistoryNormalizationConfig::FuzzyRelativeCap
    {
        return raw_history_boost;
    }

    let cap = ((base_score.max(1) as f64) * config.fuzzy_history_cap_ratio).floor() as i64;
    raw_history_boost.min(cap)
}

struct RankedEntry<'a> {
    score: i64,
    last_launched: u64,
    entry: &'a AppEntry,
    lower_name: &'a str,
}

// Implement traits to use RankedEntry in a BinaryHeap
impl<'a> PartialEq for RankedEntry<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score
            && self.last_launched == other.last_launched
            && self.lower_name == other.lower_name
            && self.entry.target_path == other.entry.target_path
    }
}

impl<'a> Eq for RankedEntry<'a> {}

impl<'a> PartialOrd for RankedEntry<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> Ord for RankedEntry<'a> {
    fn cmp(&self, other: &Self) -> Ordering {
        rank_cmp_ranked(self, other)
    }
}

fn rank_cmp_ranked(a: &RankedEntry, b: &RankedEntry) -> Ordering {
    // Higher score is better, and better candidates are ordered as `Less`.
    // This keeps final sorting intuitive (`sort_by(rank_cmp_ranked)` puts best first)
    // while making `BinaryHeap<RankedEntry>::peek()` point to the current worst.
    b.score.cmp(&a.score)
        .then_with(|| b.last_launched.cmp(&a.last_launched))
        .then_with(|| a.lower_name.cmp(b.lower_name))
        .then_with(|| a.entry.target_path.cmp(&b.entry.target_path))
}

/// Compute a character-presence bitmask for a lowercase string.
/// Bits 0-25 = 'a'-'z', bits 26-35 = '0'-'9'. All other chars are ignored.
fn char_bitmask(lower: &str) -> u64 {
    let mut mask: u64 = 0;
    for b in lower.bytes() {
        match b {
            b'a'..=b'z' => mask |= 1u64 << (b - b'a'),
            b'0'..=b'9' => mask |= 1u64 << (26 + (b - b'0')),
            _ => {}
        }
    }
    mask
}

/// Score using pre-computed lowercase name and pre-cached UTF-32 representation.
fn match_score_single_cached(
    mode: SearchMode,
    matcher: &mut Matcher,
    lower_name: &str,
    query: &str,
    haystack_u32: &Utf32String,
    needle_buf: &mut Vec<char>,
) -> Option<i64> {
    match mode {
        SearchMode::Prefix => {
            if lower_name.starts_with(query) {
                Some(10_000 - lower_name.len() as i64)
            } else {
                None
            }
        }
        SearchMode::Substring => lower_name.find(query).map(|idx| 5_000 - idx as i64),
        SearchMode::Fuzzy => {
            let haystack = haystack_u32.slice(..);
            let needle = Utf32Str::new(query, needle_buf);
            matcher.fuzzy_match(haystack, needle).map(|s| s as i64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::HistoryStore;
    use crate::indexer::AppEntry;

    fn make_entries(names: &[&str]) -> Vec<AppEntry> {
        names
            .iter()
            .map(|n| AppEntry {
                name: n.to_string(),
                target_path: format!("C:\\fake\\{}.lnk", n),
                is_folder: false,
            })
            .collect()
    }

    fn empty_history() -> HistoryStore {
        HistoryStore::load(10, 8)
    }

    #[test]
    fn search_empty_query_returns_empty() {
        let mut engine = SearchEngine::new(make_entries(&["Firefox", "Chrome"]));
        let results = engine.search("", 8, &empty_history(), SearchMode::Fuzzy);
        assert!(results.is_empty());
    }

    #[test]
    fn search_no_entries_returns_empty() {
        let mut engine = SearchEngine::new(Vec::new());
        let results = engine.search("fire", 8, &empty_history(), SearchMode::Fuzzy);
        assert!(results.is_empty());
    }

    #[test]
    fn search_returns_fuzzy_matches() {
        let entries = make_entries(&["Firefox", "Chrome", "Notepad", "Visual Studio Code"]);
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("fire", 8, &empty_history(), SearchMode::Fuzzy);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "Firefox");
    }

    #[test]
    fn search_respects_max_results() {
        let entries = make_entries(&["app1", "app2", "app3", "app4", "app5"]);
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("app", 3, &empty_history(), SearchMode::Fuzzy);
        assert!(results.len() <= 3);
    }

    #[test]
    fn search_top_k_replaces_worst_when_better_arrives_late() {
        let entries = make_entries(&[
            "appabcdefghij",
            "appabcdefghi",
            "appx",
            "app",
        ]);
        let mut engine = SearchEngine::new(entries);

        let results = engine.search("app", 2, &empty_history(), SearchMode::Prefix);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();

        assert_eq!(names, vec!["app", "appx"]);
    }

    #[test]
    fn search_top_k_is_independent_from_input_order() {
        let ordered = make_entries(&[
            "appabcdefghij",
            "appabcdefghi",
            "appx",
            "app",
        ]);
        let reversed = make_entries(&[
            "app",
            "appx",
            "appabcdefghi",
            "appabcdefghij",
        ]);
        let mut ordered_engine = SearchEngine::new(ordered);
        let mut reversed_engine = SearchEngine::new(reversed);

        let ordered_results = ordered_engine.search("app", 2, &empty_history(), SearchMode::Prefix);
        let reversed_results = reversed_engine.search("app", 2, &empty_history(), SearchMode::Prefix);

        let ordered_names: Vec<&str> = ordered_results.iter().map(|r| r.name.as_str()).collect();
        let reversed_names: Vec<&str> = reversed_results.iter().map(|r| r.name.as_str()).collect();

        assert_eq!(ordered_names, vec!["app", "appx"]);
        assert_eq!(reversed_names, vec!["app", "appx"]);
    }

    #[test]
    fn search_results_are_not_folders() {
        let entries = make_entries(&["Firefox"]);
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("fire", 8, &empty_history(), SearchMode::Fuzzy);
        assert!(!results.is_empty());
        assert!(!results[0].is_folder);
    }

    #[test]
    fn search_prefix_mode_matches_only_prefix() {
        let entries = make_entries(&["Notepad", "Pad Tool"]);
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("pad", 8, &empty_history(), SearchMode::Prefix);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Pad Tool");
    }

    #[test]
    fn search_substring_mode_matches_middle() {
        let entries = make_entries(&["Visual Studio Code"]);
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("studio", 8, &empty_history(), SearchMode::Substring);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_with_extension_matches_stem_entry() {
        // "SSP.exe" と入力して、name="SSP", target_path="C:\\fake\\SSP.exe" にマッチする
        let entries = vec![AppEntry {
            name: "SSP".to_string(),
            target_path: "C:\\fake\\SSP.exe".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("SSP.exe", 8, &empty_history(), SearchMode::Prefix);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "SSP");
    }

    #[test]
    fn search_with_extension_substring_mode() {
        let entries = vec![AppEntry {
            name: "SSP".to_string(),
            target_path: "C:\\fake\\SSP.exe".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("ssp.exe", 8, &empty_history(), SearchMode::Substring);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_with_extension_fuzzy_mode() {
        let entries = vec![AppEntry {
            name: "SSP".to_string(),
            target_path: "C:\\fake\\SSP.exe".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("ssp.exe", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_without_extension_still_works() {
        let entries = make_entries(&["SSP"]);
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("SSP", 8, &empty_history(), SearchMode::Prefix);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "SSP");
    }

    #[test]
    fn search_with_extension_does_not_match_unrelated_exe() {
        // "ssp.exe" で FileZilla.exe はヒットしない（stem "ssp" が fuzzy でも一致しない）
        let entries = vec![
            AppEntry {
                name: "SSP".to_string(),
                target_path: "C:\\fake\\SSP.exe".to_string(),
                is_folder: false,
            },
            AppEntry {
                name: "FileZilla".to_string(),
                target_path: "C:\\fake\\FileZilla.exe".to_string(),
                is_folder: false,
            },
        ];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("ssp.exe", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "SSP");
    }

    #[test]
    fn search_with_extension_filters_by_ext() {
        // "ssp.exe" は .lnk の SSP にはヒットしない（ファイル名 "SSP.lnk" と "ssp.exe" は不一致）
        let entries = vec![AppEntry {
            name: "SSP".to_string(),
            target_path: "C:\\fake\\SSP.lnk".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("ssp.exe", 8, &empty_history(), SearchMode::Prefix);
        assert!(results.is_empty());
    }

    #[test]
    fn search_partial_ext_dot_only() {
        // "SSP." → target_path のファイル名 "SSP.exe" に fuzzy 一致
        let entries = vec![AppEntry {
            name: "SSP".to_string(),
            target_path: "C:\\fake\\SSP.exe".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("SSP.", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "SSP");
    }

    #[test]
    fn search_partial_ext_dot_e() {
        // "SSP.e" → target_path のファイル名 "SSP.exe" に fuzzy 一致
        let entries = vec![AppEntry {
            name: "SSP".to_string(),
            target_path: "C:\\fake\\SSP.exe".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("SSP.e", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "SSP");
    }

    #[test]
    fn search_partial_ext_dot_ex() {
        // "SSP.ex" → target_path のファイル名 "SSP.exe" に fuzzy 一致
        let entries = vec![AppEntry {
            name: "SSP".to_string(),
            target_path: "C:\\fake\\SSP.exe".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("SSP.ex", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "SSP");
    }

    #[test]
    fn search_name_with_dot_matches() {
        // name にドットを含むエントリが、ドット入りクエリでヒットする
        let entries = vec![AppEntry {
            name: "Dr.Web".to_string(),
            target_path: "C:\\fake\\drweb32w.exe".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("Dr.Web", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Dr.Web");
    }

    #[test]
    fn search_name_with_dot_prefers_name() {
        // name にドットを含むエントリが、部分一致クエリでもヒットする
        let entries = vec![AppEntry {
            name: "Dr.Web".to_string(),
            target_path: "C:\\fake\\drweb32w.exe".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("dr.w", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Dr.Web");
    }

    #[test]
    fn search_double_ext_file() {
        // 二重拡張子のファイルに対して部分一致でヒットする
        let entries = vec![AppEntry {
            name: "hoge".to_string(),
            target_path: "C:\\fake\\hoge.exe.bak".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("hoge.exe", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "hoge");
    }

    #[test]
    fn search_double_ext_full() {
        // 二重拡張子のファイルに対して完全一致でヒットする
        let entries = vec![AppEntry {
            name: "hoge".to_string(),
            target_path: "C:\\fake\\hoge.exe.bak".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("hoge.exe.bak", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "hoge");
    }

    #[test]
    fn recent_history_empty_when_no_launches() {
        let entries = make_entries(&["Firefox", "Chrome"]);
        let engine = SearchEngine::new(entries);
        let results = engine.recent_history(&empty_history(), 8);
        assert!(results.is_empty());
    }

    #[test]
    fn rank_cmp_breaks_full_tie_with_target_path() {
        let a = AppEntry {
            name: "Tool".to_string(),
            target_path: "C:\\B\\tool.exe".to_string(),
            is_folder: false,
        };
        let b = AppEntry {
            name: "Tool".to_string(),
            target_path: "C:\\A\\tool.exe".to_string(),
            is_folder: false,
        };
        let ra = RankedEntry {
            score: 100,
            last_launched: 200,
            entry: &a,
            lower_name: "tool",
        };
        let rb = RankedEntry {
            score: 100,
            last_launched: 200,
            entry: &b,
            lower_name: "tool",
        };
        let mut scored = vec![ra, rb];
        scored.sort_by(rank_cmp_ranked);
        assert_eq!(scored[0].entry.target_path, "C:\\A\\tool.exe");
        assert_eq!(scored[1].entry.target_path, "C:\\B\\tool.exe");
    }

    #[test]
    fn has_dot_uses_cached_lower_file_name() {
        let entries = vec![AppEntry {
            name: "Dummy".to_string(),
            target_path: "C:\\fake\\Tool.EXE".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("tool.exe", 8, &empty_history(), SearchMode::Substring);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Dummy");
    }

    #[test]
    fn has_dot_handles_missing_file_name_without_panic() {
        let entries = vec![AppEntry {
            name: "Dummy".to_string(),
            target_path: "C:\\".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("dummy.exe", 8, &empty_history(), SearchMode::Fuzzy);
        assert!(results.is_empty());
    }

    #[test]
    fn adjusted_history_boost_disabled_uses_raw_value() {
        let adjusted =
            adjusted_history_boost(SearchMode::Fuzzy, 100, 250, HistoryBoostConfig::default());
        assert_eq!(adjusted, 250);
    }

    #[test]
    fn adjusted_history_boost_caps_only_in_fuzzy_mode() {
        let config = HistoryBoostConfig {
            normalization: SearchHistoryNormalizationConfig::FuzzyRelativeCap,
            fuzzy_history_cap_ratio: 0.30,
        };
        let adjusted = adjusted_history_boost(SearchMode::Prefix, 100, 250, config);
        assert_eq!(adjusted, 250);
    }

    #[test]
    fn adjusted_history_boost_caps_fuzzy_history_by_base_ratio() {
        let config = HistoryBoostConfig {
            normalization: SearchHistoryNormalizationConfig::FuzzyRelativeCap,
            fuzzy_history_cap_ratio: 0.30,
        };
        let adjusted = adjusted_history_boost(SearchMode::Fuzzy, 100, 250, config);
        assert_eq!(adjusted, 30);
    }

    #[test]
    fn adjusted_history_boost_zeroes_when_base_is_non_positive() {
        let config = HistoryBoostConfig {
            normalization: SearchHistoryNormalizationConfig::FuzzyRelativeCap,
            fuzzy_history_cap_ratio: 0.30,
        };
        let adjusted = adjusted_history_boost(SearchMode::Fuzzy, -50, 250, config);
        assert_eq!(adjusted, 0);
    }

    #[test]
    fn search_with_history_boost_disabled_matches_legacy_search() {
        let entries = make_entries(&["alpha", "alpaca", "alpine"]);
        let mut engine = SearchEngine::new(entries);
        let mut history = empty_history();
        for _ in 0..50 {
            history.record_launch("C:\\fake\\alpaca.lnk", "alp");
        }

        let legacy = engine.search("alp", 8, &history, SearchMode::Fuzzy);
        let explicit = engine.search_with_history_boost(
            "alp",
            8,
            &history,
            SearchMode::Fuzzy,
            HistoryBoostConfig::default(),
        );
        assert_eq!(legacy, explicit);
    }

    // --- 大文字パス正規化テスト ---

    #[test]
    fn recent_history_matches_case_insensitive_path() {
        // 大文字パスで記録した起動履歴が、元ケース AppEntry と照合できる
        let entries = vec![AppEntry {
            name: "App".to_string(),
            target_path: "C:\\Fake\\App.lnk".to_string(),
            is_folder: false,
        }];
        let engine = SearchEngine::new(entries);
        let mut history = empty_history();
        history.record_launch("C:\\FAKE\\APP.LNK", "app");

        let results = engine.recent_history(&history, 8);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "App");
    }

    #[test]
    fn query_boost_matches_case_insensitive_path() {
        // 大文字パスで記録したクエリ別履歴がスコアブーストに反映され、
        // 同スコアの競合エントリより上位に来る
        let entries = vec![
            AppEntry {
                name: "App".to_string(),
                target_path: "C:\\Fake\\App.lnk".to_string(),
                is_folder: false,
            },
            AppEntry {
                name: "AppX".to_string(),
                target_path: "C:\\Other\\appx.lnk".to_string(),
                is_folder: false,
            },
        ];
        let mut engine = SearchEngine::new(entries);
        let mut history = empty_history();
        for _ in 0..10 {
            history.record_launch("C:\\FAKE\\APP.LNK", "app");
        }

        let results = engine.search_with_history_boost(
            "app",
            8,
            &history,
            SearchMode::Prefix,
            HistoryBoostConfig::default(),
        );
        assert!(!results.is_empty());
        // 履歴ブーストにより "App" が "AppX" より上位に来る
        assert_eq!(results[0].name, "App");
    }

    // --- char_bitmask テスト ---

    #[test]
    fn bitmask_lowercase_letters() {
        let mask = char_bitmask("abc");
        assert_ne!(mask & (1 << 0), 0); // a
        assert_ne!(mask & (1 << 1), 0); // b
        assert_ne!(mask & (1 << 2), 0); // c
        assert_eq!(mask & (1 << 3), 0); // d not present
    }

    #[test]
    fn bitmask_digits() {
        let mask = char_bitmask("a1b2");
        assert_ne!(mask & (1 << 0), 0);      // a
        assert_ne!(mask & (1 << 1), 0);      // b
        assert_ne!(mask & (1 << 27), 0);     // '1' = bit 26+1
        assert_ne!(mask & (1 << 28), 0);     // '2' = bit 26+2
    }

    #[test]
    fn bitmask_non_alnum_ignored() {
        let mask = char_bitmask("a-b.c");
        assert_ne!(mask & (1 << 0), 0); // a
        assert_ne!(mask & (1 << 1), 0); // b
        assert_ne!(mask & (1 << 2), 0); // c
        // '-' and '.' don't set any bits
        assert_eq!(mask.count_ones(), 3);
    }

    #[test]
    fn bitmask_filter_skips_non_matching_entries() {
        // "xyz" のクエリでビットマスクが "Firefox" にマッチしないことを確認
        let entries = make_entries(&["Firefox", "Chrome", "Xyzer"]);
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("xyz", 8, &empty_history(), SearchMode::Fuzzy);
        // "Firefox" と "Chrome" は x,y,z の全文字を含まないのでスキップされる
        // "Xyzer" は x,y,z を含むのでマッチ候補になる
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Xyzer");
    }

    #[test]
    fn bitmask_filter_does_not_skip_accented_entries() {
        // nucleo はアクセント正規化（é→e）でマッチするため、
        // ビットマスクフィルタがアクセント付きエントリを除外してはならない
        let entries = vec![AppEntry {
            name: "Café".to_string(),
            target_path: "C:\\fake\\Café.lnk".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("cafe", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Café");
    }

    // --- アクセント正規化統一テスト ---

    #[test]
    fn prefix_matches_accented_entry() {
        let entries = vec![AppEntry {
            name: "Café".to_string(),
            target_path: "C:\\fake\\Café.lnk".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("cafe", 8, &empty_history(), SearchMode::Prefix);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Café");
    }

    #[test]
    fn substring_matches_accented_entry() {
        let entries = vec![AppEntry {
            name: "Résumé Builder".to_string(),
            target_path: "C:\\fake\\Résumé Builder.lnk".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("resume", 8, &empty_history(), SearchMode::Substring);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Résumé Builder");
    }

    #[test]
    fn history_boost_unified_across_accent_variants() {
        // "résumé" で起動記録 → "resume" で検索時に履歴ブーストが効く
        let entries = vec![
            AppEntry {
                name: "Résumé Builder".to_string(),
                target_path: "C:\\fake\\Résumé Builder.lnk".to_string(),
                is_folder: false,
            },
            AppEntry {
                name: "Resume Helper".to_string(),
                target_path: "C:\\fake\\Resume Helper.lnk".to_string(),
                is_folder: false,
            },
        ];
        let mut engine = SearchEngine::new(entries);
        let mut history = empty_history();
        // "résumé" で Résumé Builder を多数起動
        for _ in 0..20 {
            history.record_launch("C:\\fake\\Résumé Builder.lnk", "résumé");
        }
        // "resume"（アクセントなし）で検索 → 履歴ブーストが効いて Résumé Builder が上位
        let results = engine.search("resume", 8, &history, SearchMode::Fuzzy);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "Résumé Builder");
    }
}
