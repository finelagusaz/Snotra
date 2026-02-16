use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use crate::indexer::AppEntry;
use crate::window::SearchResult;

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

    pub fn search(&self, query: &str, max_results: usize) -> Vec<SearchResult> {
        if query.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<(i64, &AppEntry)> = self
            .entries
            .iter()
            .filter_map(|entry| {
                self.matcher
                    .fuzzy_match(&entry.name, query)
                    .map(|score| (score, entry))
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.truncate(max_results);

        scored
            .into_iter()
            .map(|(_, entry)| SearchResult {
                name: entry.name.clone(),
                path: entry.target_path.clone(),
            })
            .collect()
    }
}
