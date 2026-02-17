use std::collections::HashMap;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use crate::history::HistoryStore;
use crate::indexer::AppEntry;
use crate::query::normalize_query;
use crate::window::SearchResult;

const GLOBAL_WEIGHT: i64 = 5;
const QUERY_WEIGHT: i64 = 20;
const FOLDER_EXPANSION_WEIGHT: i64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Prefix,
    Substring,
    Fuzzy,
}

pub struct SearchEngine {
    entries: Vec<AppEntry>,
    matcher: SkimMatcherV2,
}

impl SearchEngine {
    pub fn new(entries: Vec<AppEntry>) -> Self {
        Self {
            entries,
            matcher: SkimMatcherV2::default(),
        }
    }

    pub fn search(
        &self,
        query: &str,
        max_results: usize,
        history: &HistoryStore,
        mode: SearchMode,
    ) -> Vec<SearchResult> {
        let norm_query = normalize_query(query);
        if norm_query.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<(i64, u64, &AppEntry)> = self
            .entries
            .iter()
            .filter_map(|entry| {
                match_score(mode, &self.matcher, &entry.name, &norm_query).map(|base_score| {
                    let global = history.global_count(&entry.target_path) as i64;
                    let qcount = history.query_count(&norm_query, &entry.target_path) as i64;
                    let folder_boost = if entry.is_folder {
                        history.folder_expansion_count(&entry.target_path) as i64
                            * FOLDER_EXPANSION_WEIGHT
                    } else {
                        0
                    };
                    let combined =
                        base_score + global * GLOBAL_WEIGHT + qcount * QUERY_WEIGHT + folder_boost;
                    let last = history.last_launched(&entry.target_path).unwrap_or(0);
                    (combined, last, entry)
                })
            })
            .collect();

        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.name.to_lowercase().cmp(&b.2.name.to_lowercase()))
        });
        scored.truncate(max_results);

        scored
            .into_iter()
            .map(|(_, _, entry)| SearchResult {
                name: entry.name.clone(),
                path: entry.target_path.clone(),
                is_folder: entry.is_folder,
                is_error: false,
            })
            .collect()
    }

    pub fn recent_history(&self, history: &HistoryStore, max_results: usize) -> Vec<SearchResult> {
        let path_to_entry: HashMap<&str, &AppEntry> = self
            .entries
            .iter()
            .map(|e| (e.target_path.as_str(), e))
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

fn match_score(mode: SearchMode, matcher: &SkimMatcherV2, name: &str, query: &str) -> Option<i64> {
    match mode {
        SearchMode::Prefix => {
            let lname = name.to_lowercase();
            if lname.starts_with(query) {
                Some(10_000 - lname.len() as i64)
            } else {
                None
            }
        }
        SearchMode::Substring => {
            let lname = name.to_lowercase();
            lname.find(query).map(|idx| 5_000 - idx as i64)
        }
        SearchMode::Fuzzy => matcher.fuzzy_match(&name.to_lowercase(), query),
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
        let engine = SearchEngine::new(make_entries(&["Firefox", "Chrome"]));
        let results = engine.search("", 8, &empty_history(), SearchMode::Fuzzy);
        assert!(results.is_empty());
    }

    #[test]
    fn search_no_entries_returns_empty() {
        let engine = SearchEngine::new(Vec::new());
        let results = engine.search("fire", 8, &empty_history(), SearchMode::Fuzzy);
        assert!(results.is_empty());
    }

    #[test]
    fn search_returns_fuzzy_matches() {
        let entries = make_entries(&["Firefox", "Chrome", "Notepad", "Visual Studio Code"]);
        let engine = SearchEngine::new(entries);
        let results = engine.search("fire", 8, &empty_history(), SearchMode::Fuzzy);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "Firefox");
    }

    #[test]
    fn search_respects_max_results() {
        let entries = make_entries(&["app1", "app2", "app3", "app4", "app5"]);
        let engine = SearchEngine::new(entries);
        let results = engine.search("app", 3, &empty_history(), SearchMode::Fuzzy);
        assert!(results.len() <= 3);
    }

    #[test]
    fn search_results_are_not_folders() {
        let entries = make_entries(&["Firefox"]);
        let engine = SearchEngine::new(entries);
        let results = engine.search("fire", 8, &empty_history(), SearchMode::Fuzzy);
        assert!(!results.is_empty());
        assert!(!results[0].is_folder);
    }

    #[test]
    fn search_prefix_mode_matches_only_prefix() {
        let entries = make_entries(&["Notepad", "Pad Tool"]);
        let engine = SearchEngine::new(entries);
        let results = engine.search("pad", 8, &empty_history(), SearchMode::Prefix);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Pad Tool");
    }

    #[test]
    fn search_substring_mode_matches_middle() {
        let entries = make_entries(&["Visual Studio Code"]);
        let engine = SearchEngine::new(entries);
        let results = engine.search("studio", 8, &empty_history(), SearchMode::Substring);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn recent_history_empty_when_no_launches() {
        let entries = make_entries(&["Firefox", "Chrome"]);
        let engine = SearchEngine::new(entries);
        let results = engine.recent_history(&empty_history(), 8);
        assert!(results.is_empty());
    }
}
