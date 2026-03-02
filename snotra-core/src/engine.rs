use crate::config::Config;
use crate::folder;
use crate::history::HistoryStore;
use crate::indexer::AppEntry;
use crate::search::{HistoryBoostConfig, SearchEngine, SearchMode};
use crate::ui_types::SearchResult;
use std::path::Path;

pub struct Engine {
    search_engine: SearchEngine,
    history: HistoryStore,
    config: Config,
}

impl Engine {
    pub fn new(entries: Vec<AppEntry>, history: HistoryStore, config: Config) -> Self {
        Self {
            search_engine: SearchEngine::new(entries),
            history,
            config,
        }
    }

    pub fn search(&self, query: &str) -> Vec<SearchResult> {
        let mode = SearchMode::from(self.config.search.normal_mode);
        let boost = HistoryBoostConfig::from(&self.config.search);
        let max = self.config.appearance.max_results;
        self.search_engine
            .search_with_history_boost(query, max, &self.history, mode, boost)
    }

    pub fn recent_history(&self) -> Vec<SearchResult> {
        let max = self.config.appearance.max_history_display;
        self.search_engine.recent_history(&self.history, max)
    }

    pub fn list_folder(&self, dir: &str, filter: &str) -> Vec<SearchResult> {
        let mode = SearchMode::from(self.config.search.folder_mode);
        let show_hidden = self.config.search.show_hidden_system;
        let max = self.config.appearance.max_results;
        folder::list_folder(Path::new(dir), filter, mode, show_hidden, &self.history, max)
    }

    pub fn record_launch(&mut self, path: &str, query: &str) {
        self.history.record_launch(path, query);
    }

    pub fn record_folder_expansion(&mut self, path: &str) {
        self.history.record_folder_expansion(path);
    }

    pub fn save_history_if_dirty(&mut self, threshold: u32) {
        self.history.save_if_dirty(threshold);
    }

    pub fn flush_history(&mut self) {
        self.history.save();
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn update_config(&mut self, config: Config) {
        self.config = config;
    }

    pub fn replace_entries(&mut self, entries: Vec<AppEntry>) {
        self.search_engine = SearchEngine::new(entries);
    }

    pub fn entries(&self) -> &[AppEntry] {
        self.search_engine.entries()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
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

    fn make_folder_entries(names: &[&str]) -> Vec<AppEntry> {
        names
            .iter()
            .map(|n| AppEntry {
                name: n.to_string(),
                target_path: format!("C:\\fake\\{}", n),
                is_folder: true,
            })
            .collect()
    }

    fn empty_history() -> HistoryStore {
        HistoryStore::load(10, 8)
    }

    fn default_config() -> Config {
        Config::default()
    }

    #[test]
    fn new_creates_engine() {
        let engine = Engine::new(make_entries(&["Firefox"]), empty_history(), default_config());
        assert_eq!(engine.entries().len(), 1);
        assert_eq!(engine.entries()[0].name, "Firefox");
    }

    #[test]
    fn search_returns_matching_results() {
        let engine = Engine::new(
            make_entries(&["Firefox", "Chrome", "Notepad"]),
            empty_history(),
            default_config(),
        );
        let results = engine.search("fire");
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "Firefox");
    }

    #[test]
    fn search_empty_query_returns_empty() {
        let engine = Engine::new(
            make_entries(&["Firefox"]),
            empty_history(),
            default_config(),
        );
        let results = engine.search("");
        assert!(results.is_empty());
    }

    #[test]
    fn search_respects_max_results_from_config() {
        let mut config = default_config();
        config.appearance.max_results = 2;
        let engine = Engine::new(
            make_entries(&["app1", "app2", "app3", "app4"]),
            empty_history(),
            config,
        );
        let results = engine.search("app");
        assert!(results.len() <= 2);
    }

    #[test]
    fn recent_history_empty_when_no_launches() {
        let engine = Engine::new(
            make_entries(&["Firefox"]),
            empty_history(),
            default_config(),
        );
        let results = engine.recent_history();
        assert!(results.is_empty());
    }

    #[test]
    fn record_launch_and_search_boost() {
        let mut engine = Engine::new(
            make_entries(&["Alpha", "Alpaca"]),
            empty_history(),
            default_config(),
        );
        // Record many launches for Alpaca to boost it
        for _ in 0..20 {
            engine.record_launch("C:\\fake\\Alpaca.lnk", "alp");
        }
        let results = engine.search("alp");
        assert!(!results.is_empty());
        // Alpaca should be boosted above Alpha due to history
        assert_eq!(results[0].name, "Alpaca");
    }

    #[test]
    fn record_folder_expansion_increments() {
        let mut engine = Engine::new(
            make_folder_entries(&["Projects"]),
            empty_history(),
            default_config(),
        );
        engine.record_folder_expansion("C:\\fake\\Projects");
        // No panic, and the engine still works
        let results = engine.search("proj");
        assert!(!results.is_empty());
    }

    #[test]
    fn config_returns_current_config() {
        let config = default_config();
        let engine = Engine::new(Vec::new(), empty_history(), config.clone());
        assert_eq!(engine.config().hotkey.key, config.hotkey.key);
    }

    #[test]
    fn update_config_changes_config() {
        let mut engine = Engine::new(Vec::new(), empty_history(), default_config());
        let mut new_config = default_config();
        new_config.appearance.max_results = 42;
        engine.update_config(new_config);
        assert_eq!(engine.config().appearance.max_results, 42);
    }

    #[test]
    fn replace_entries_updates_search() {
        let mut engine = Engine::new(
            make_entries(&["OldApp"]),
            empty_history(),
            default_config(),
        );
        assert_eq!(engine.entries().len(), 1);
        assert_eq!(engine.entries()[0].name, "OldApp");

        engine.replace_entries(make_entries(&["NewApp1", "NewApp2"]));
        assert_eq!(engine.entries().len(), 2);
        assert_eq!(engine.entries()[0].name, "NewApp1");
    }

    #[test]
    fn replace_entries_search_uses_new_entries() {
        let mut engine = Engine::new(
            make_entries(&["OldApp"]),
            empty_history(),
            default_config(),
        );
        let results = engine.search("old");
        assert_eq!(results.len(), 1);

        engine.replace_entries(make_entries(&["NewApp"]));
        let results = engine.search("old");
        assert!(results.is_empty());
        let results = engine.search("new");
        assert_eq!(results.len(), 1);
    }
}
