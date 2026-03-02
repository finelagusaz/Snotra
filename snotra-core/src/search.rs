use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use nucleo_matcher::{Config as MatcherConfig, Matcher, Utf32String};
use rayon::prelude::*;

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

/// Search index holding per-entry pre-computed data as parallel Vecs.
///
/// # Why parallel Vecs instead of a single `Vec<CachedEntry>` struct?
///
/// A `CachedEntry` struct grouping all per-entry fields was prototyped and benchmarked
/// (branch `refactor/cached-entry`, issue #110). Results showed **35–120% regression**
/// in Fuzzy full-scan speed across all entry counts:
///
/// | entries | parallel Vec | CachedEntry struct |
/// |--------:|-------------:|-------------------:|
/// |   1,000 |      ~7 ms   |           ~14 ms   |
/// |  50,000 |     ~14 ms   |           ~30 ms   |
/// | 100,000 |     ~22 ms   |           ~29 ms   |
///
/// Root cause: the bitmask pre-filter (`char_masks` / `file_name_char_masks`) sweeps
/// every candidate index before any scoring occurs.  As a compact `Vec<u64>` it fits
/// 8 entries per 64-byte cache line.  Embedded in a `CachedEntry` (~160 bytes/entry)
/// the same sweep loads ~25× more cache lines, degrading L1 locality significantly.
///
/// The parallel-Vec layout is therefore intentional, not accidental tech debt.
/// Adding a new derived field requires updating `new()` and keeping all Vecs in sync —
/// enforce this by running the full test suite after any structural change.
pub struct SearchEngine {
    entries: Vec<AppEntry>,
    lower_names: Vec<String>,
    lower_file_names: Vec<Option<String>>,
    /// Pre-computed normalized keys for history lookups (one per entry).
    normalized_keys: Vec<String>,
    /// Character-presence bitmask for lower_name (a-z: bits 0-25, 0-9: bits 26-35).
    /// Kept as a compact Vec<u64> — 8 entries per cache line — so the pre-filter sweep
    /// that discards non-matching candidates before scoring is L1-cache-friendly.
    /// Merging this into a per-entry struct would inflate the per-entry size ~20× and
    /// cause a measured 35–120% Fuzzy search regression (see struct-level doc comment).
    char_masks: Vec<u64>,
    /// Character-presence bitmask for lower_file_name (same layout as char_masks).
    file_name_char_masks: Vec<u64>,
    /// Incremental search cache: normalized query string from the previous call.
    prev_query: String,
    /// Incremental search cache: entry indices that matched on the previous call,
    /// stored BEFORE truncation to `max_results` so every match is captured.
    prev_candidates: Vec<usize>,
    /// Incremental search cache: search mode of the previous call.
    prev_mode: Option<SearchMode>,
}

/// Lightweight view over all per-entry parallel-Vec fields for index `i`.
/// Bundles 6 immutable references without changing the underlying SoA layout,
/// so all cache-locality properties of the parallel Vecs are preserved.
struct EntryView<'a> {
    entry: &'a AppEntry,
    lower_name: &'a str,
    lower_file_name: Option<&'a str>,
    normalized_key: &'a str,
}

impl SearchEngine {
    pub fn new(entries: Vec<AppEntry>) -> Self {
        // Wave 1: lower_names / lower_file_names / normalized_keys は entries への
        // 純粋な map であり相互依存がないため rayon::join で並列構築する。
        let entries_ref = &entries;
        let ((lower_names, lower_file_names), normalized_keys) = rayon::join(
            || {
                rayon::join(
                    || {
                        entries_ref
                            .iter()
                            .map(|e| to_lower_folded(&e.name))
                            .collect::<Vec<_>>()
                    },
                    || {
                        entries_ref
                            .iter()
                            .map(|e| {
                                std::path::Path::new(&e.target_path)
                                    .file_name()
                                    .and_then(|f| f.to_str())
                                    .map(to_lower_folded)
                            })
                            .collect::<Vec<_>>()
                    },
                )
            },
            || {
                entries_ref
                    .iter()
                    .map(|e| normalize_entry_key(&e.target_path))
                    .collect::<Vec<_>>()
            },
        );

        // Wave 2: char_masks は lower_names に、file_name_char_masks は lower_file_names に
        // それぞれ依存するため Wave 1 完了後に並列構築する。
        // to_lower_folded already folds most Latin accents to ASCII (é→e),
        // so non-ASCII names here are typically CJK, Arabic, etc.
        // u64::MAX ensures (query_mask & u64::MAX) == query_mask for any query_mask,
        // so these entries always pass the bitmask pre-filter regardless of the query.
        let (char_masks, file_name_char_masks) = rayon::join(
            || {
                lower_names
                    .iter()
                    .map(|n| if n.is_ascii() { char_bitmask(n) } else { u64::MAX })
                    .collect::<Vec<_>>()
            },
            || {
                // None → 0: entries without a file_name cannot match via the file_name path,
                // so failing the bitmask check (and being skipped when the name also fails) is correct.
                lower_file_names
                    .iter()
                    .map(|n| {
                        n.as_deref()
                            .map_or(0, |s| if s.is_ascii() { char_bitmask(s) } else { u64::MAX })
                    })
                    .collect::<Vec<_>>()
            },
        );

        debug_assert!(
            lower_names.len() == entries.len()
                && lower_file_names.len() == entries.len()
                && normalized_keys.len() == entries.len()
                && char_masks.len() == entries.len()
                && file_name_char_masks.len() == entries.len(),
            "SearchEngine: all parallel Vecs must have the same length as entries"
        );
        Self {
            entries,
            lower_names,
            lower_file_names,
            normalized_keys,
            char_masks,
            file_name_char_masks,
            prev_query: String::new(),
            prev_candidates: Vec::new(),
            prev_mode: None,
        }
    }

    /// キャッシュから読み込んだビットマスクを使って SearchEngine を構築する。
    /// char_masks / file_name_char_masks の再計算をスキップし、起動時間を短縮する。
    /// lower_names / lower_file_names / normalized_keys は引き続き並列構築する。
    pub fn new_with_cached_masks(
        entries: Vec<AppEntry>,
        char_masks: Vec<u64>,
        file_name_char_masks: Vec<u64>,
    ) -> Self {
        let entries_ref = &entries;
        let ((lower_names, lower_file_names), normalized_keys) = rayon::join(
            || {
                rayon::join(
                    || {
                        entries_ref
                            .iter()
                            .map(|e| to_lower_folded(&e.name))
                            .collect::<Vec<_>>()
                    },
                    || {
                        entries_ref
                            .iter()
                            .map(|e| {
                                std::path::Path::new(&e.target_path)
                                    .file_name()
                                    .and_then(|f| f.to_str())
                                    .map(to_lower_folded)
                            })
                            .collect::<Vec<_>>()
                    },
                )
            },
            || {
                entries_ref
                    .iter()
                    .map(|e| normalize_entry_key(&e.target_path))
                    .collect::<Vec<_>>()
            },
        );

        debug_assert!(
            lower_names.len() == entries.len()
                && lower_file_names.len() == entries.len()
                && normalized_keys.len() == entries.len()
                && char_masks.len() == entries.len()
                && file_name_char_masks.len() == entries.len(),
            "SearchEngine: all parallel Vecs must have the same length as entries"
        );
        Self {
            entries,
            lower_names,
            lower_file_names,
            normalized_keys,
            char_masks,
            file_name_char_masks,
            prev_query: String::new(),
            prev_candidates: Vec::new(),
            prev_mode: None,
        }
    }

    #[inline]
    fn entry_view(&self, i: usize) -> EntryView<'_> {
        EntryView {
            entry: &self.entries[i],
            lower_name: &self.lower_names[i],
            lower_file_name: self.lower_file_names[i].as_deref(),
            normalized_key: &self.normalized_keys[i],
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
        // Bitmask pre-filter is only used in Fuzzy mode; skip the computation for others.
        let query_mask = if mode == SearchMode::Fuzzy { char_bitmask(&norm_query) } else { 0 };

        // Pre-compute needle as UTF-32 once per search call and share it across threads.
        // Reusing the same Utf32String avoids repeated O(|query|) char conversion per entry.
        let needle_u32 = Utf32String::from(norm_query.as_ref());
        let norm_query_str: &str = &norm_query;

        // Phase 4: incremental search – reuse previous match candidates when the query
        // is a monotonic extension of the previous one and the mode is unchanged.
        //
        // Guard: (!has_dot || self.prev_query.contains('.'))
        //   "no-dot → dot" transition must fall back to full scan because prev_candidates
        //   was built without file_name scoring; entries that only match via file_name would
        //   be absent from prev_candidates, causing false negatives.
        let use_incremental = self.prev_mode == Some(mode)
            && !self.prev_candidates.is_empty()
            && !self.prev_query.is_empty()
            && norm_query.starts_with(self.prev_query.as_str())
            && (!has_dot || self.prev_query.contains('.'));

        let candidate_indices: Vec<usize> = if use_incremental {
            std::mem::take(&mut self.prev_candidates)
        } else {
            (0..self.entries.len()).collect()
        };

        // Parallel scoring: each rayon task gets its own Matcher (created in fold init).
        // Matcher::new() is O(alloc_zeroed) — lightweight enough to create per task.
        // Each task builds a local top-k BinaryHeap; tasks are merged in reduce().
        //
        // BinaryHeap<ScoredEntry> is a max-heap where peek() == worst item.
        // ScoredEntry::Ord mirrors rank_cmp_ranked: better entry has Ordering::Less,
        // so the worst (Ordering::Greater) stays at the top of the heap.
        //
        // The fold state includes a Vec<usize> to collect ALL matching entry indices
        // (not just top-k) for the incremental search cache.
        let (top_k, all_match_indices): (BinaryHeap<ScoredEntry>, Vec<usize>) = candidate_indices
            .into_par_iter()
            .fold(
                || {
                    (
                        BinaryHeap::<ScoredEntry>::with_capacity(max_results + 1),
                        Matcher::new(MatcherConfig::DEFAULT),
                        Vec::<usize>::new(),
                    )
                },
                |(mut local_heap, mut matcher, mut local_matches), i| {
                    // Bitmask pre-filter: skip entries that lack query characters (Fuzzy only).
                    // Prefix/Substring use cheap str ops, so the bitmask overhead isn't worth it.
                    if mode == SearchMode::Fuzzy {
                        let name_mask = self.char_masks[i];
                        let fn_mask = self.file_name_char_masks[i];
                        if (query_mask & name_mask) != query_mask
                            && (!has_dot || (query_mask & fn_mask) != query_mask)
                        {
                            return (local_heap, matcher, local_matches);
                        }
                    }

                    let v = self.entry_view(i);

                    // Fuzzy モードのみ UTF-32 へのオンデマンド変換を行う。
                    // ビットマスクで除外された候補には到達しないため、変換コストは
                    // ビットマスク通過分（全体の 1-5%）にのみ発生する。
                    // Option<Utf32String> で保持し as_ref() で &Utf32String を取り出す。
                    let name_u32_owned: Option<Utf32String> = if mode == SearchMode::Fuzzy {
                        Some(Utf32String::from(v.lower_name))
                    } else {
                        None
                    };

                    let name_score = match_score_single_cached(
                        mode,
                        &mut matcher,
                        v.lower_name,
                        norm_query_str,
                        name_u32_owned.as_ref(),
                        &needle_u32,
                    );

                    let score = if has_dot {
                        // Skip file_name scoring only on a high-confidence name match
                        // (avoids heavy fuzzy work).
                        let needs_fn_score = name_score.is_none_or(|s| s <= 9000);
                        let fn_score = if needs_fn_score {
                            v.lower_file_name.and_then(|fn_name| {
                                let fn_u32_owned: Option<Utf32String> =
                                    if mode == SearchMode::Fuzzy {
                                        Some(Utf32String::from(fn_name))
                                    } else {
                                        None
                                    };
                                match_score_single_cached(
                                    mode,
                                    &mut matcher,
                                    fn_name,
                                    norm_query_str,
                                    fn_u32_owned.as_ref(),
                                    &needle_u32,
                                )
                            })
                        } else {
                            None
                        };
                        match (name_score, fn_score) {
                            (None, fn_s) => fn_s,
                            (Some(s), Some(b)) => Some(s.max(b)),
                            (Some(s), None) => Some(s),
                        }
                    } else {
                        name_score
                    };

                    if let Some(base_score) = score {
                        local_matches.push(i); // Record matching index for incremental cache
                        let (global_launches, last_launched) =
                            history.get_global_stats_normalized(v.normalized_key);
                        let qcount =
                            history.query_count_pre_normalized(norm_query_str, v.normalized_key)
                                as i64;

                        let folder_boost = if v.entry.is_folder {
                            history.folder_expansion_count_normalized(v.normalized_key) as i64
                                * FOLDER_EXPANSION_WEIGHT
                        } else {
                            0
                        };

                        let raw_history_boost = (global_launches as i64) * GLOBAL_WEIGHT
                            + qcount * QUERY_WEIGHT
                            + folder_boost;
                        let history_boost = adjusted_history_boost(
                            mode,
                            base_score,
                            raw_history_boost,
                            history_boost_config,
                        );
                        let combined = base_score + history_boost;

                        let scored = ScoredEntry {
                            score: combined,
                            last_launched,
                            lower_name: v.lower_name.to_owned(),
                            name: v.entry.name.clone(),
                            path: v.entry.target_path.clone(),
                            is_folder: v.entry.is_folder,
                        };

                        if local_heap.len() < max_results {
                            local_heap.push(scored);
                        } else if let Some(mut worst) = local_heap.peek_mut() {
                            // Replace only when the new item is better than the current worst.
                            if scored.cmp(&worst) == Ordering::Less {
                                *worst = scored;
                            }
                        }
                    }

                    (local_heap, matcher, local_matches)
                },
            )
            .map(|(heap, _, matches)| (heap, matches))
            .reduce(
                || (BinaryHeap::new(), Vec::new()),
                |(mut a_heap, mut a_matches), (b_heap, b_matches)| {
                    for entry in b_heap {
                        if a_heap.len() < max_results {
                            a_heap.push(entry);
                        } else if let Some(mut worst) = a_heap.peek_mut()
                            && entry.cmp(&worst) == Ordering::Less
                        {
                            *worst = entry;
                        }
                    }
                    a_matches.extend(b_matches);
                    (a_heap, a_matches)
                },
            );

        // Update incremental cache BEFORE sort so all matching indices are captured.
        self.prev_query = norm_query.into_owned();
        self.prev_candidates = all_match_indices;
        self.prev_mode = Some(mode);

        // into_sorted_vec() reuses the heap's internal Vec and sorts in-place (ascending).
        // ScoredEntry::Ord: Less = better, so ascending order puts best first.
        let scored = top_k.into_sorted_vec();

        scored
            .into_iter()
            .map(|r| SearchResult {
                name: r.name,
                path: r.path,
                is_folder: r.is_folder,
                is_error: false,
            })
            .collect()
    }

    pub fn recent_history(&self, history: &HistoryStore, max_results: usize) -> Vec<SearchResult> {
        // recent_launches() は正規化済みキーを返すため、照合側も正規化済み normalized_keys を使う
        let path_to_entry: HashMap<&str, &AppEntry> = (0..self.entries.len())
            .map(|i| {
                let v = self.entry_view(i);
                (v.normalized_key, v.entry)
            })
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

/// Owned scored entry used in the parallel top-k heap.
/// Clones only strings that make it into the heap (at most `max_results` per rayon task).
struct ScoredEntry {
    score: i64,
    last_launched: u64,
    lower_name: String, // tie-breaking key (alphabetical)
    name: String,       // for SearchResult
    path: String,       // for SearchResult and tie-breaking (= target_path)
    is_folder: bool,    // for SearchResult
}

// Higher score is better; better entries are ordered as `Ordering::Less`.
// This makes BinaryHeap::peek() point to the current worst (max by Ord = least good).
// scored.sort() (ascending) then puts the best entry first.
impl PartialEq for ScoredEntry {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score
            && self.last_launched == other.last_launched
            && self.lower_name == other.lower_name
            && self.path == other.path
    }
}

impl Eq for ScoredEntry {}

impl PartialOrd for ScoredEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScoredEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .score
            .cmp(&self.score)
            .then_with(|| other.last_launched.cmp(&self.last_launched))
            .then_with(|| self.lower_name.cmp(&other.lower_name))
            .then_with(|| self.path.cmp(&other.path))
    }
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

/// Score using pre-computed lowercase name and an optional UTF-32 haystack.
/// `haystack_u32` is `Some` only in Fuzzy mode (computed on-demand after bitmask pre-filter).
/// `needle_u32` must be the UTF-32 encoding of `query` (pre-computed once per search call).
fn match_score_single_cached(
    mode: SearchMode,
    matcher: &mut Matcher,
    lower_name: &str,
    query: &str,
    haystack_u32: Option<&Utf32String>,
    needle_u32: &Utf32String,
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
            let h = haystack_u32.expect("Fuzzy mode requires UTF-32 haystack");
            let haystack = h.slice(..);
            let needle = needle_u32.slice(..);
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
        let ra = ScoredEntry {
            score: 100,
            last_launched: 200,
            lower_name: "tool".to_string(),
            name: "Tool".to_string(),
            path: "C:\\B\\tool.exe".to_string(),
            is_folder: false,
        };
        let rb = ScoredEntry {
            score: 100,
            last_launched: 200,
            lower_name: "tool".to_string(),
            name: "Tool".to_string(),
            path: "C:\\A\\tool.exe".to_string(),
            is_folder: false,
        };
        let mut scored = vec![ra, rb];
        scored.sort();
        assert_eq!(scored[0].path, "C:\\A\\tool.exe");
        assert_eq!(scored[1].path, "C:\\B\\tool.exe");
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

    // --- パフォーマンス計測 ---
    // `cargo test -p snotra-core bench_ -- --ignored --nocapture` で実行

    fn make_bench_entries(n: usize) -> Vec<AppEntry> {
        // 実際のアプリ名に近い多様な文字列を生成する
        let prefixes = [
            "Microsoft", "Adobe", "Google", "Apple", "Mozilla", "Visual", "Windows",
            "System", "App", "Tool", "Launcher", "Manager", "Explorer", "Editor",
        ];
        let suffixes = [
            "Studio", "Reader", "Player", "Code", "Settings", "Control", "Panel",
            "Viewer", "Browser", "Assistant", "Helper", "Updater", "Installer",
        ];
        (0..n)
            .map(|i| {
                let name = format!(
                    "{} {} {}",
                    prefixes[i % prefixes.len()],
                    suffixes[i % suffixes.len()],
                    i
                );
                AppEntry {
                    target_path: format!("C:\\Program Files\\App{}\\app{}.lnk", i, i),
                    name,
                    is_folder: false,
                }
            })
            .collect()
    }

    fn bench_search(label: &str, n: usize, queries: &[&str]) {
        use std::time::Instant;
        let entries = make_bench_entries(n);
        let mut engine = SearchEngine::new(entries);
        let history = empty_history();

        // ウォームアップ（rayon スレッドプールの初期化を除外）
        for q in queries {
            let _ = engine.search(q, 10, &history, SearchMode::Fuzzy);
        }

        let iters = 20usize;
        let mut total_ns = 0u128;
        for _ in 0..iters {
            for q in queries {
                let t = Instant::now();
                let _ = engine.search(q, 10, &history, SearchMode::Fuzzy);
                total_ns += t.elapsed().as_nanos();
            }
        }

        let avg_us = total_ns / (iters * queries.len()) as u128 / 1000;
        println!("[{label}] entries={n}, avg={avg_us}µs ({} queries × {iters} iters)", queries.len());
    }

    #[test]
    #[ignore]
    fn bench_fuzzy_search_scaling() {
        let queries = ["vis", "code", "micro", "app", "sett"];
        for &n in &[1_000, 10_000, 50_000, 100_000, 300_000] {
            bench_search("fuzzy", n, &queries);
        }
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

    // --- インクリメンタル検索テスト ---

    #[test]
    fn incremental_search_gives_correct_results_on_extension() {
        let names = &["Firefox", "Final Cut", "Chrome", "Finder", "Fire TV"];
        let mut engine = SearchEngine::new(make_entries(names));
        let h = empty_history();

        // 連続する monotonic 拡張で incremental パスを使う
        let _ = engine.search("fi", 8, &h, SearchMode::Fuzzy);
        let _ = engine.search("fir", 8, &h, SearchMode::Fuzzy);
        let incremental = engine.search("fire", 8, &h, SearchMode::Fuzzy);

        // 新鮮なエンジンでの結果と一致するか確認
        let mut fresh = SearchEngine::new(make_entries(names));
        let fresh_result = fresh.search("fire", 8, &h, SearchMode::Fuzzy);

        let inc_names: Vec<&str> = incremental.iter().map(|r| r.name.as_str()).collect();
        let fresh_names: Vec<&str> = fresh_result.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(inc_names, fresh_names);
    }

    #[test]
    fn incremental_search_fallback_on_backspace() {
        // "Firm" は fuzzy "fir" にマッチするが "fire" にはマッチしない（'e' がない）。
        // "fire" → "fir" はバックスペースなので incremental パスは使えない。
        // full scan にフォールバックしていなければ "Firm" が結果から漏れる。
        let mut engine =
            SearchEngine::new(make_entries(&["Firefox", "Final Cut", "Chrome", "Firm"]));
        let h = empty_history();

        let _ = engine.search("fire", 8, &h, SearchMode::Fuzzy);
        // "fir" は "fire" の拡張ではない → full scan にフォールバック
        let results = engine.search("fir", 8, &h, SearchMode::Fuzzy);

        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"Firefox"), "full scan で Firefox が返る必要がある");
        assert!(
            names.contains(&"Firm"),
            "Firm は 'fire' にマッチしないため前回の candidates に不在。full scan でのみ検出可能"
        );
    }

    #[test]
    fn incremental_search_fallback_on_mode_change() {
        let mut engine = SearchEngine::new(make_entries(&["Firefox", "Final Cut", "Chrome"]));
        let h = empty_history();

        let _ = engine.search("fi", 8, &h, SearchMode::Fuzzy);
        // 同じクエリでもモード変更 → full scan
        let results = engine.search("fi", 8, &h, SearchMode::Prefix);

        // Prefix "fi": "firefox" / "final cut" は先頭一致、"chrome" は不一致
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"Firefox"), "Prefix 'fi' で Firefox が返る必要がある");
        assert!(names.contains(&"Final Cut"), "Prefix 'fi' で Final Cut が返る必要がある");
        assert!(
            !names.contains(&"Chrome"),
            "Chrome は 'fi' で始まらないため除外される必要がある"
        );
    }

    #[test]
    fn incremental_search_dot_to_dot_uses_incremental() {
        // dot → dot の拡張は incremental パスを使用できる（no-dot→dot ガードを通過する）。
        // "ssp." → "ssp.e" はどちらもドットを含む単調拡張。
        let entries = vec![
            AppEntry {
                name: "SSP".to_string(),
                target_path: "C:\\fake\\SSP.exe".to_string(),
                is_folder: false,
            },
            AppEntry {
                name: "AnotherApp".to_string(),
                target_path: "C:\\fake\\ssp.data".to_string(),
                is_folder: false,
            },
        ];
        let mut engine = SearchEngine::new(entries.clone());
        let h = empty_history();

        // "ssp." で両エントリが候補にキャッシュされる（prev_candidates に両インデックスが入る）
        let _ = engine.search("ssp.", 8, &h, SearchMode::Fuzzy);
        // "ssp.e" はドットあり拡張 → incremental で prev_candidates を再利用
        let incremental = engine.search("ssp.e", 8, &h, SearchMode::Fuzzy);

        // fresh エンジンとの比較で正確性を担保
        let mut fresh = SearchEngine::new(entries);
        let fresh_result = fresh.search("ssp.e", 8, &h, SearchMode::Fuzzy);

        let inc_names: Vec<&str> = incremental.iter().map(|r| r.name.as_str()).collect();
        let fresh_names: Vec<&str> = fresh_result.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            inc_names,
            fresh_names,
            "dot→dot incremental は fresh 結果と一致する必要がある"
        );
    }

    #[test]
    fn incremental_search_no_dot_to_dot_falls_back_to_full_scan() {
        // "AnotherApp" は名前では "ssp" にマッチしないが、
        // file_name "ssp.data" は "ssp." クエリにマッチする
        let entries = vec![
            AppEntry {
                name: "SSP".to_string(),
                target_path: "C:\\fake\\SSP.exe".to_string(),
                is_folder: false,
            },
            AppEntry {
                name: "AnotherApp".to_string(),
                target_path: "C:\\fake\\ssp.data".to_string(),
                is_folder: false,
            },
        ];
        let mut engine = SearchEngine::new(entries);
        let h = empty_history();

        // "ssp"（ドットなし）→ prev_candidates には名前マッチした SSP だけが入る
        let _ = engine.search("ssp", 8, &h, SearchMode::Fuzzy);

        // "ssp."（ドットあり）→ no-dot→dot ガードにより full scan
        // AnotherApp の file_name "ssp.data" が "ssp." にマッチするはず
        let results = engine.search("ssp.", 8, &h, SearchMode::Fuzzy);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"SSP"), "SSP.exe が ssp. にマッチするはず");
        assert!(
            names.contains(&"AnotherApp"),
            "AnotherApp の file_name ssp.data が ssp. にマッチするはず（full scan 必須）"
        );
    }

    #[test]
    fn incremental_search_empty_prev_candidates_falls_back() {
        let mut engine = SearchEngine::new(make_entries(&["Firefox", "Chrome"]));
        let h = empty_history();

        // マッチなし → prev_candidates が空になる
        let r1 = engine.search("xyz", 8, &h, SearchMode::Fuzzy);
        assert!(r1.is_empty());

        // "xyzw" は "xyz" の拡張だが prev_candidates 空 → full scan
        let r2 = engine.search("xyzw", 8, &h, SearchMode::Fuzzy);
        assert!(r2.is_empty());

        // キャッシュが壊れていなければ通常クエリも機能する
        let r3 = engine.search("fire", 8, &h, SearchMode::Fuzzy);
        assert!(!r3.is_empty());
        assert_eq!(r3[0].name, "Firefox");
    }
}
