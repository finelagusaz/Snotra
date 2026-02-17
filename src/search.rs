use std::collections::HashMap;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use crate::history::HistoryStore;
use crate::indexer::AppEntry;
use crate::window::SearchResult;

const GLOBAL_WEIGHT: i64 = 5;
const QUERY_WEIGHT: i64 = 20;

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
    ) -> Vec<SearchResult> {
        if query.is_empty() {
            return Vec::new();
        }

        let norm_query = query.trim().to_lowercase();

        let mut scored: Vec<(i64, &AppEntry)> = self
            .entries
            .iter()
            .filter_map(|entry| {
                self.matcher
                    .fuzzy_match(&entry.name, query)
                    .map(|fuzzy_score| {
                        let global = history.global_count(&entry.target_path) as i64;
                        let qcount = history.query_count(&norm_query, &entry.target_path) as i64;
                        let combined =
                            fuzzy_score + global * GLOBAL_WEIGHT + qcount * QUERY_WEIGHT;
                        (combined, entry)
                    })
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.truncate(max_results);

        scored
            .into_iter()
            .map(|(_, entry)| SearchResult {
                name: entry.name.clone(),
                path: entry.target_path.clone(),
                is_folder: false,
            })
            .collect()
    }

    pub fn recent_history(
        &self,
        history: &HistoryStore,
        max_results: usize,
    ) -> Vec<SearchResult> {
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
                    is_folder: false,
                })
            })
            .collect()
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
            })
            .collect()
    }

    fn empty_history() -> HistoryStore {
        HistoryStore::load(10, 8)
    }

    #[test]
    fn search_empty_query_returns_empty() {
        let engine = SearchEngine::new(make_entries(&["Firefox", "Chrome"]));
        let results = engine.search("", 8, &empty_history());
        assert!(results.is_empty());
    }

    #[test]
    fn search_no_entries_returns_empty() {
        let engine = SearchEngine::new(Vec::new());
        let results = engine.search("fire", 8, &empty_history());
        assert!(results.is_empty());
    }

    #[test]
    fn search_returns_fuzzy_matches() {
        let entries = make_entries(&["Firefox", "Chrome", "Notepad", "Visual Studio Code"]);
        let engine = SearchEngine::new(entries);
        let results = engine.search("fire", 8, &empty_history());
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "Firefox");
    }

    #[test]
    fn search_respects_max_results() {
        let entries = make_entries(&["app1", "app2", "app3", "app4", "app5"]);
        let engine = SearchEngine::new(entries);
        let results = engine.search("app", 3, &empty_history());
        assert!(results.len() <= 3);
    }

    #[test]
    fn search_results_are_not_folders() {
        let entries = make_entries(&["Firefox"]);
        let engine = SearchEngine::new(entries);
        let results = engine.search("fire", 8, &empty_history());
        assert!(!results.is_empty());
        assert!(!results[0].is_folder);
    }

    #[test]
    fn recent_history_empty_when_no_launches() {
        let entries = make_entries(&["Firefox", "Chrome"]);
        let engine = SearchEngine::new(entries);
        let results = engine.recent_history(&empty_history(), 8);
        assert!(results.is_empty());
    }
}
