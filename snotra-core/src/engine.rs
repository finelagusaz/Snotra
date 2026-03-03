use crate::config::Config;
use crate::folder;
use crate::history::HistoryStore;
use crate::indexer::{AppEntry, CachedMasks};
use crate::search::{HistoryBoostConfig, SearchEngine, SearchMode};
use crate::ui_types::SearchResult;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct FolderListContext {
    mode: SearchMode,
    show_hidden_system: bool,
    max_results: usize,
}

impl FolderListContext {
    pub fn read_dir_entries(
        &self,
        dir: &Path,
        filter: &str,
    ) -> std::io::Result<Vec<folder::DirEntryData>> {
        folder::read_dir_entries(dir, filter, self.mode, self.show_hidden_system)
    }
}

/// `SearchEngine` を Mutex 外で事前構築するためのラッパー型。
/// `Engine::apply_prebuilt_index` でロック保持時間を最小化したスワップが可能。
pub struct PrebuiltIndex(SearchEngine);

impl PrebuiltIndex {
    pub fn new(entries: Vec<AppEntry>) -> Self {
        Self(SearchEngine::new(entries))
    }
}

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

    /// v3/v4 キャッシュヒット時に使用するコンストラクタ。
    /// - v4 ヒット: ビットマスク + lower names を渡し Wave 1/2 を完全スキップ（A-3）
    /// - v3 フォールバック: ビットマスクのみ渡し Wave 1 は SearchEngine 内で実行
    pub fn new_from_cache(
        entries: Vec<AppEntry>,
        cached_masks: CachedMasks,
        history: HistoryStore,
        config: Config,
    ) -> Self {
        Self {
            search_engine: SearchEngine::new_with_cached_masks(
                entries,
                cached_masks.char_masks,
                cached_masks.file_name_char_masks,
                cached_masks.lower_names,
                cached_masks.lower_file_names,
                cached_masks.normalized_keys,
            ),
            history,
            config,
        }
    }

    pub fn search(&mut self, query: &str) -> Vec<SearchResult> {
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

    pub fn capture_folder_list_context(&self) -> FolderListContext {
        FolderListContext {
            mode: SearchMode::from(self.config.search.folder_mode),
            show_hidden_system: self.config.search.show_hidden_system,
            max_results: self.config.appearance.max_results,
        }
    }

    pub fn finalize_folder_list(
        &self,
        entries: Vec<folder::DirEntryData>,
        ctx: FolderListContext,
    ) -> Vec<SearchResult> {
        // ctx は I/O 開始前にロックなしで取得したスナップショット。
        // 設定変更が並走した場合 max_results が 1 件ずれる可能性があるが、
        // Mutex 保持時間の最小化を優先する設計判断として許容する。
        // history は常に現在の最新状態を使用する（スコアリングのみへの影響）。
        folder::score_entries(entries, &self.history, ctx.max_results)
    }

    /// フォルダ内エントリを同期的に列挙してスコアリング済み結果を返す。
    /// Tauri コマンドは `capture_folder_list_context` + `finalize_folder_list` の
    /// 非同期2フェーズ版を使う。こちらは `folder.rs` のユニットテスト向け同期ラッパー。
    pub fn list_folder(&self, dir: &str, filter: &str) -> Vec<SearchResult> {
        let ctx = self.capture_folder_list_context();
        folder::list_folder(
            Path::new(dir),
            filter,
            ctx.mode,
            ctx.show_hidden_system,
            &self.history,
            ctx.max_results,
        )
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

    /// テスト専用。本番コードは `apply_prebuilt_index` を使う（H-1）。
    #[cfg(test)]
    pub(crate) fn replace_entries(&mut self, entries: Vec<AppEntry>) {
        self.search_engine = SearchEngine::new(entries);
    }

    /// Mutex 外で事前構築した SearchEngine を高速スワップする。
    /// インデックス再構築時のロック保持時間を最小化するために使う。
    pub fn apply_prebuilt_index(&mut self, index: PrebuiltIndex) {
        self.search_engine = index.0;
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
        let engine = Engine::new(
            make_entries(&["Firefox"]),
            empty_history(),
            default_config(),
        );
        assert_eq!(engine.entries().len(), 1);
        assert_eq!(engine.entries()[0].name, "Firefox");
    }

    #[test]
    fn search_returns_matching_results() {
        let mut engine = Engine::new(
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
        let mut engine = Engine::new(
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
        let mut engine = Engine::new(
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
        let mut engine = Engine::new(make_entries(&["OldApp"]), empty_history(), default_config());
        assert_eq!(engine.entries().len(), 1);
        assert_eq!(engine.entries()[0].name, "OldApp");

        engine.replace_entries(make_entries(&["NewApp1", "NewApp2"]));
        assert_eq!(engine.entries().len(), 2);
        assert_eq!(engine.entries()[0].name, "NewApp1");
    }

    #[test]
    fn replace_entries_search_uses_new_entries() {
        let mut engine = Engine::new(make_entries(&["OldApp"]), empty_history(), default_config());
        let results = engine.search("old");
        assert_eq!(results.len(), 1);

        engine.replace_entries(make_entries(&["NewApp"]));
        let results = engine.search("old");
        assert!(results.is_empty());
        let results = engine.search("new");
        assert_eq!(results.len(), 1);
    }
}
