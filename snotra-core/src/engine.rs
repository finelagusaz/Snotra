//! 検索・履歴・設定を単一ロック下に統合する facade（`Engine`）。
//!
//! `SearchEngine` + `HistoryStore` + `Config` を1つの `Mutex<Engine>` にまとめ、Tauri 側の
//! 多重ロックを解消する。ロック保持時間を最小化するため、ロック外で扱うスナップショット/
//! 構築物 `FolderListContext`・`PrebuiltIndex`・`PreparedHistorySave` を公開する。

use crate::config::{Config, ScanPath};
use crate::folder;
use crate::history::{HistoryStore, PreparedHistorySave};
use crate::indexer::{AppEntry, CachedMasks};
use crate::search::{SearchEngine, SearchMode, SearchOptions};
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

    /// キャッシュ済みソート結果を現フィルタで同期に絞る（#532 SU3 M2）。SearchMode を
    /// 呼び出し側（src-tauri driver）へ露出させないためのラッパー。
    pub fn filter_sorted(&self, cached: &[SearchResult], filter: &str) -> Vec<SearchResult> {
        folder::filter_sorted(cached, filter, self.mode, self.max_results)
    }
}

/// `SearchEngine` を Mutex 外で事前構築するためのラッパー型。
/// `Engine::apply_prebuilt_index` でロック保持時間を最小化したスワップが可能。
pub struct PrebuiltIndex(SearchEngine);

impl PrebuiltIndex {
    /// `migemo_enabled` に応じて kana_lower_names の構築要否を決める（issue #337）。
    /// 呼び出し側（indexing.rs）はロック内でキャプチャした config の migemo を渡す。
    pub fn new(entries: Vec<AppEntry>, migemo_enabled: bool) -> Self {
        Self(SearchEngine::new_with_migemo(entries, migemo_enabled))
    }
}

/// インデックス（`SearchEngine`）の構築入力。config から焼き込まれるカテゴリ B の入力一式（issue #347）。
/// `config_watcher` の再構築要否判定（old/new 差分）と `complete_index_drain` の re-diff
/// （ビルド開始時スナップショット/現在）で**この単一定義を共有**する（needs_reindex と in-flight
/// needs_rebuild の二重メンテを解消）。`show_icons` は概念的には icon-stale だが、現状は index
/// ビルドのついでにアイコンキャッシュを prune するため含める（設計メモ §6 Q2）。
#[derive(Debug, Clone, PartialEq)]
pub struct IndexInputs {
    pub scan: Vec<ScanPath>,
    pub show_hidden_system: bool,
    pub show_icons: bool,
    pub include_path_env: bool,
    pub migemo_enabled: bool,
}

impl IndexInputs {
    pub fn from_config(config: &Config) -> Self {
        Self {
            scan: config.paths.scan.clone(),
            show_hidden_system: config.search.show_hidden_system,
            show_icons: config.appearance.show_icons,
            include_path_env: config.search.include_path_env,
            migemo_enabled: config.search.migemo_enabled,
        }
    }
}

pub struct Engine {
    search_engine: SearchEngine,
    history: HistoryStore,
    config: Config,
    /// インデックスが現在の config の `IndexInputs` を反映していない可能性を表す（issue #347/#348-A）。
    /// `start_index_build`（ビルド要求）が `mark_index_stale` で立て、`complete_index_drain` が
    /// 「ビルド開始時スナップショット == 現在」のときだけ落とす。コヒーレンシ判断を engine Mutex
    /// （軸1）に閉じ、lost-update 窓を塞ぐための単一の真実。
    index_stale: bool,
}

impl Engine {
    pub fn new(entries: Vec<AppEntry>, history: HistoryStore, config: Config) -> Self {
        let search_engine = SearchEngine::new_with_migemo(entries, config.search.migemo_enabled);
        Self {
            search_engine,
            history,
            config,
            index_stale: false,
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
        let search_engine = SearchEngine::new_with_cached_masks(
            entries,
            cached_masks.char_masks,
            cached_masks.file_name_char_masks,
            cached_masks.lower_names,
            cached_masks.lower_file_names,
            config.search.migemo_enabled,
        );
        Self {
            search_engine,
            history,
            config,
            index_stale: false,
        }
    }

    pub fn search(&mut self, query: &str) -> Vec<SearchResult> {
        let mode = SearchMode::from(self.config.search.normal_mode);
        let boost = SearchOptions::from(&self.config.search);
        // result_limit が取得上限。visible_rows はウィンドウ可視行数のみを制御する。
        let fetch_limit = self.config.search.effective_result_limit();
        self.search_engine
            .search_with_options(query, fetch_limit, &self.history, mode, boost)
    }

    pub fn recent_history(&self) -> Vec<SearchResult> {
        let max = self.config.search.effective_recent_limit();
        self.search_engine.recent_history(&self.history, max)
    }

    pub fn capture_folder_list_context(&self) -> FolderListContext {
        FolderListContext {
            mode: SearchMode::from(self.config.search.folder_mode),
            show_hidden_system: self.config.search.show_hidden_system,
            max_results: self.config.search.effective_result_limit(),
        }
    }

    pub fn finalize_folder_list(
        &self,
        entries: Vec<folder::DirEntryData>,
        ctx: FolderListContext,
    ) -> Vec<SearchResult> {
        // ctx は I/O 開始前にロックなしで取得したスナップショット。
        // 設定変更が並走した場合 result_limit が 1 件ずれる可能性があるが、
        // Mutex 保持時間の最小化を優先する設計判断として許容する。
        // history は常に現在の最新状態を使用する（スコアリングのみへの影響）。
        folder::score_entries(entries, &self.history, ctx.max_results)
    }

    /// folder ナビゲーション時に全件を全順序でソート（上限なし）して返す。driver がキャッシュし、
    /// 打鍵フィルタは `FolderListContext::filter_sorted` で同期に行う（#532 SU3 M2 B）。
    /// history は最新状態を使う（`finalize_folder_list` と同様・スコアリングのみへの影響）。
    pub fn finalize_folder_list_unlimited(
        &self,
        entries: Vec<folder::DirEntryData>,
    ) -> Vec<SearchResult> {
        folder::sort_entries_unlimited(entries, &self.history)
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

    pub fn prepare_history_save_if_dirty(&mut self, threshold: u32) -> Option<PreparedHistorySave> {
        // top_n は現在の config から live-read で渡す（焼き込まない、issue #348）。
        self.history
            .prepare_save_if_dirty(threshold, self.config.search.effective_result_limit())
    }

    pub fn prepare_history_flush(&mut self) -> Option<PreparedHistorySave> {
        self.history
            .prepare_flush(self.config.search.effective_result_limit())
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

    /// インデックスを stale とマークする（ビルドが必要＝要求された）。
    /// `start_index_build`（config 変更 reindex / first-run / 手動 rebuild の全経路）が呼ぶ。
    pub fn mark_index_stale(&mut self) {
        self.index_stale = true;
    }

    /// インデックスが stale か。`start_index_build` の finish 後再チェック（finish 窓の取りこぼし回収）に使う。
    pub fn is_index_stale(&self) -> bool {
        self.index_stale
    }

    /// stale なら現在の `IndexInputs` スナップショットを返す（ロック外ビルドの基準）。stale でなければ None。
    pub fn begin_index_drain(&self) -> Option<IndexInputs> {
        if self.index_stale {
            Some(IndexInputs::from_config(&self.config))
        } else {
            None
        }
    }

    /// ロック外で構築した index をスワップし、ビルド開始時スナップショット（`built_from`）と
    /// 現在 config の `IndexInputs` が一致するときだけ stale を落とす。ビルド中に config が
    /// 変わっていれば stale を残し、次の `begin_index_drain` で再ビルドさせる（lost-update を塞ぐ）。
    pub fn complete_index_drain(&mut self, index: PrebuiltIndex, built_from: &IndexInputs) {
        self.apply_prebuilt_index(index);
        // ビルド開始時スナップショットと現在の index 入力が一致するときだけ stale を落とす。
        // ビルド中に config が変わっていれば（!=）stale を残し、次の begin_index_drain で再ビルドさせる。
        if IndexInputs::from_config(&self.config) == *built_from {
            self.index_stale = false;
        }
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
        HistoryStore::load()
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
    fn search_returns_up_to_result_limit_regardless_of_visible_rows() {
        let mut config = default_config();
        config.appearance.visible_rows = Some(2);
        // result_limit はデフォルト 200 → 4 件全部取得できる。
        // visible_rows はウィンドウ可視行数のみを制御する。
        let mut engine = Engine::new(
            make_entries(&["app1", "app2", "app3", "app4"]),
            empty_history(),
            config,
        );
        let results = engine.search("app");
        assert_eq!(results.len(), 4);
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
        new_config.appearance.visible_rows = Some(42);
        engine.update_config(new_config);
        assert_eq!(engine.config().appearance.visible_rows, Some(42));
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
    fn apply_prebuilt_index_rebuilds_kana_per_migemo() {
        // migemo 有効 config の Engine に migemo ON で構築した PrebuiltIndex をスワップ
        // → kana が eager 構築されローマ字検索がヒットする（必須条件 #1 の eager 経路）。
        let mut config = default_config();
        // migemo_min_chars は default_config() で既に 2。クエリ "dokyu" は min_chars 境界に触れない。
        config.search.migemo_enabled = true;
        let mut engine = Engine::new(Vec::new(), empty_history(), config);
        engine.apply_prebuilt_index(PrebuiltIndex::new(make_entries(&["ドキュメント"]), true));
        let results = engine.search("dokyu");
        assert!(
            results.iter().any(|r| r.name == "ドキュメント"),
            "migemo ON の PrebuiltIndex スワップ後、ローマ字検索がヒット: {:?}",
            results.iter().map(|r| r.name.as_str()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn prebuilt_index_migemo_disabled_no_kana_match_no_panic() {
        // migemo 有効 config でも、migemo OFF で構築した PrebuiltIndex をスワップすると
        // kana 未構築のため panic せず、ローマ字検索はヒットしない（必須条件 #2 の空ガード）。
        let mut config = default_config();
        // migemo_min_chars は default_config() で既に 2。クエリ "dokyu" は min_chars 境界に触れない。
        config.search.migemo_enabled = true;
        let mut engine = Engine::new(Vec::new(), empty_history(), config);
        engine.apply_prebuilt_index(PrebuiltIndex::new(make_entries(&["ドキュメント"]), false));
        let results = engine.search("dokyu");
        assert!(
            results.is_empty(),
            "migemo OFF 構築の PrebuiltIndex では kana 検索がヒットしない（panic せず）: {:?}",
            results.iter().map(|r| r.name.as_str()).collect::<Vec<_>>()
        );
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

    // --- Phase 2: index_stale ledger / drain（issue #347 中核 + #348-A）---

    #[test]
    fn fresh_engine_is_not_index_stale() {
        // 構築直後は現在 config を反映済み＝stale でない。begin は None。
        let engine = Engine::new(make_entries(&["x"]), empty_history(), default_config());
        assert!(!engine.is_index_stale());
        assert!(engine.begin_index_drain().is_none());
    }

    #[test]
    fn mark_index_stale_makes_begin_return_current_inputs() {
        let mut config = default_config();
        config.search.migemo_enabled = true;
        config.search.show_hidden_system = true;
        let mut engine = Engine::new(make_entries(&["x"]), empty_history(), config);
        engine.mark_index_stale();
        assert!(engine.is_index_stale());
        let inputs = engine
            .begin_index_drain()
            .expect("stale なら snapshot を返す");
        assert!(inputs.migemo_enabled);
        assert!(inputs.show_hidden_system);
    }

    #[test]
    fn complete_index_drain_clears_stale_when_config_stable() {
        // ビルド中に config が変わらなければ、complete で stale が落ちる→ begin None。
        let mut engine = Engine::new(make_entries(&["x"]), empty_history(), default_config());
        engine.mark_index_stale();
        let snapshot = engine.begin_index_drain().expect("snapshot");
        engine.complete_index_drain(PrebuiltIndex::new(make_entries(&["x"]), false), &snapshot);
        assert!(!engine.is_index_stale(), "config 安定→stale クリア");
        assert!(engine.begin_index_drain().is_none());
    }

    #[test]
    fn complete_index_drain_keeps_stale_when_config_changed_during_build() {
        // lost-update の核（#348-A「前」）: ビルド開始時スナップショットと現在 config が異なれば
        // complete は stale を残し、次の begin が再び snapshot を返す（取りこぼさない）。
        let mut engine = Engine::new(make_entries(&["x"]), empty_history(), default_config());
        engine.mark_index_stale();
        let snapshot = engine.begin_index_drain().expect("snapshot");
        // ビルド中の config 変更を模倣（index 入力キーを 1 つ反転）
        let mut changed = engine.config().clone();
        changed.search.show_hidden_system = !changed.search.show_hidden_system;
        engine.update_config(changed);
        // 古いスナップショットで complete
        engine.complete_index_drain(PrebuiltIndex::new(make_entries(&["x"]), false), &snapshot);
        assert!(
            engine.is_index_stale(),
            "ビルド中に config が変わったら stale を残す（lost-update 回避）"
        );
        assert!(
            engine.begin_index_drain().is_some(),
            "再ビルドのため begin が再び snapshot を返す"
        );
    }

    #[test]
    fn index_inputs_differ_on_each_index_key() {
        let base = default_config();
        let b = IndexInputs::from_config(&base);

        let mut c = base.clone();
        c.search.show_hidden_system = !base.search.show_hidden_system;
        assert_ne!(b, IndexInputs::from_config(&c), "show_hidden_system");

        let mut c = base.clone();
        c.appearance.show_icons = !base.appearance.show_icons;
        assert_ne!(b, IndexInputs::from_config(&c), "show_icons");

        let mut c = base.clone();
        c.search.include_path_env = !base.search.include_path_env;
        assert_ne!(b, IndexInputs::from_config(&c), "include_path_env");

        let mut c = base.clone();
        c.search.migemo_enabled = !base.search.migemo_enabled;
        assert_ne!(b, IndexInputs::from_config(&c), "migemo_enabled");

        let mut c = base.clone();
        c.paths.scan.push(crate::config::ScanPath {
            path: "C:\\new".into(),
            extensions: vec![".exe".into()],
            include_folders: false,
        });
        assert_ne!(b, IndexInputs::from_config(&c), "scan");
    }

    #[test]
    fn index_inputs_equal_when_unrelated_key_changes() {
        // index 入力でないキー（visible_rows）の変更は IndexInputs を変えない＝再構築不要。
        let base = default_config();
        let mut c = base.clone();
        c.appearance.visible_rows = Some(base.appearance.effective_visible_rows() + 10);
        assert_eq!(
            IndexInputs::from_config(&base),
            IndexInputs::from_config(&c)
        );
    }
}
