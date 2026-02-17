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
