//! 検索・履歴・設定を単一ロック下に統合する facade（`Engine`）。
//!
//! `SearchEngine` + `HistoryStore` + `Config` を1つの `Mutex<Engine>` にまとめ、Tauri 側の
//! 多重ロックを解消する。ロック保持時間を最小化するため、ロック外で扱うスナップショット/
//! 構築物 `FolderListContext`・`PrebuiltIndex`・`PreparedHistorySave` を公開する。
//!
//! **設定だけは外側の `Mutex` を経ずに読める**（#1032・`config_handle` の doc が正本）。
//! 検索は `&mut self` を要求するので外側の `Mutex` を長く握り、実運用点では 1 回の
//! `search` が 40〜95 ms 保持する。その間 UI が同じ `Mutex` 越しに設定を読んでいたのが
//! #1032 の主因だった。

use crate::config::{Config, ScanPath};
use crate::folder;
use crate::history::{HistoryStore, PreparedHistorySave};
use crate::indexer::{AppEntry, IndexMaterial};
use crate::search::{SearchEngine, SearchMode, SearchOptions};
use crate::ui_types::SearchResult;
use std::path::Path;
use std::sync::{Arc, RwLock, RwLockReadGuard};

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
    /// `Vec<AppEntry>` から建てる。**製品経路は通らない。**
    ///
    /// **`#[cfg(test)]` で締めてある。** かつては「`snotra-core/tests/` の統合テストが外部クレートとしてリンクするため締められない」と書いていたが、**その呼び出し元は 1 つも存在しなかった**（grep 実測）。締めていなかったせいで「製品コードから新たに呼ばないこと」という規約だけが残り、破っても木を建て直すぶん静かに遅くなるだけで、型は同じ `PrebuiltIndex` を返すのでレビューでも実行結果でも差が見えなかった。
    ///
    /// **「今はコンパイラが拒む」と書いてはならない**（かつてそう書いていた）——`#[cfg(test)]` が閉じるのはこのシンボル名だけで、同じ障害（`Vec<AppEntry>` から建て直す）は `from_material(IndexMaterial::from_tree(IndexTree::build(entries)))` として書けていた。**塞いだのは `IndexTree::build` を `pub(crate)` へ下げたことである**——crate 外から手に入る木は `IndexTree::empty()` だけになり、その連鎖は今はコンパイルを通らない。**crate 内では今も書ける。**
    #[cfg(test)]
    pub fn new(entries: Vec<AppEntry>, migemo_enabled: bool) -> Self {
        Self(SearchEngine::new_with_migemo(entries, migemo_enabled))
    }

    /// [`IndexMaterial`] から構築する。**製品はこの 1 つだけを通る。**
    ///
    /// 派生データの有無で分岐しないのは、材料が組のまま来るからである（分岐は `SearchEngine::from_material` の 1 か所に閉じている）。
    pub fn from_material(material: IndexMaterial, migemo_enabled: bool) -> Self {
        Self(SearchEngine::from_material(material, migemo_enabled))
    }
}

/// インデックス（`SearchEngine`）の構築入力。config から焼き込まれるカテゴリ B の入力一式（issue #347）。
/// `config_watcher` の再構築要否判定（old/new 差分）と `complete_index_drain` の re-diff
/// （ビルド開始時スナップショット/現在）で**この単一定義を共有**する（needs_reindex と in-flight
/// needs_rebuild の二重メンテを解消）。
///
/// **ここに載せてよいのは「変わったら索引を建て直さねばならない入力」だけである。**
/// `appearance.show_icons` は長らく例外で（icon-stale を index-stale に相乗りさせていた）、
/// #996 で索引照合の剪定が消えた後は「アイコンキャッシュを落とす契機」としてしか残って
/// いなかった。**契機を運ぶために全再構築を kick するのは対価が釣り合わない**——判定は
/// この構造体の不一致ゆえ**向きを持たず**、`false → true` でも 313,028 件を建て直したうえで
/// 破棄側は何もしない。ゆえに `show_icons` は外し、破棄は `config_watcher` が
/// true → false のエッジで直接撃つ形へ移した（`snotra` の `icon::drop_icon_cache` が正本）。
#[derive(Debug, Clone, PartialEq)]
pub struct IndexInputs {
    pub scan: Vec<ScanPath>,
    pub show_hidden_system: bool,
    pub include_path_env: bool,
    pub migemo_enabled: bool,
}

impl IndexInputs {
    pub fn from_config(config: &Config) -> Self {
        Self {
            scan: config.paths.scan.clone(),
            show_hidden_system: config.search.show_hidden_system,
            include_path_env: config.search.include_path_env,
            migemo_enabled: config.search.migemo_enabled,
        }
    }
}

pub struct Engine {
    search_engine: SearchEngine,
    history: HistoryStore,
    /// 設定。**外側の `Mutex<Engine>` とは別の錠の内側にある**（#1032）。
    ///
    /// 書き手は `update_config` の 1 本だけで、それは `&mut self` を要求する——**つまり
    /// 書き込みは外側の `Mutex` の内側でしか起きない**。この非対称が意図である: 読みは
    /// 外側の錠を要らないが、書きは要る。`complete_index_drain` が
    /// 「index を swap してから現在の `IndexInputs` と照合する」を原子に行えるのは
    /// この性質による（#347/#348-A の lost-update 対策）。**`update_config` を
    /// `&self` へ変えてはならない**——変えた瞬間にその原子性が崩れる。
    config: Arc<RwLock<Config>>,
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
            config: Arc::new(RwLock::new(config)),
            index_stale: false,
        }
    }

    /// [`IndexMaterial`] から構築する。**起動経路が通る唯一の入口である。**
    ///
    /// **派生データの有無で入口を分けない**（かつては `new_from_tree` / `new_from_cache` の 2 本があり、呼び出し側が `match` で捌いていた）。分岐は `SearchEngine::from_material` の 1 か所に閉じており、**どちらの枝が選ばれるかを決める条件の正本は `indexer::save_cache_sorted` と `indexer::load_cache_in` の分岐である**。
    ///
    /// **版の番号を書かない。** 番号を書くと版を上げるたびにこの散文だけが腐る（`INDEX_CACHE_VERSION` の doc が「現行は v5・実運用点は v6 のまま」というそれ自体矛盾した文が残った事例を記録している）。
    pub fn from_material(material: IndexMaterial, history: HistoryStore, config: Config) -> Self {
        let search_engine = SearchEngine::from_material(material, config.search.migemo_enabled);
        Self {
            search_engine,
            history,
            config: Arc::new(RwLock::new(config)),
            index_stale: false,
        }
    }

    pub fn search(&mut self, query: &str) -> Vec<SearchResult> {
        // **3 つの値を 1 つの読みから取る。** `&mut self` ゆえ外側の `Mutex` は保持されており、
        // 書き手（`update_config`）はそれを要求するので途中で入れ替わることはない——ここで
        // guard を分けても壊れはしないが、分けない形が「同じ config から導いた 3 つ」である
        // ことを表す（#1032）。
        let cfg = self.config.read().unwrap();
        let mode = SearchMode::from(cfg.search.normal_mode);
        let boost = SearchOptions::from(&cfg.search);
        // result_limit が取得上限。visible_rows はウィンドウ可視行数のみを制御する。
        let fetch_limit = cfg.search.effective_result_limit();
        drop(cfg);
        self.search_engine
            .search_with_options(query, fetch_limit, &self.history, mode, boost)
    }

    pub fn recent_history(&self) -> Vec<SearchResult> {
        let max = self.config.read().unwrap().search.effective_recent_limit();
        self.search_engine.recent_history(&self.history, max)
    }

    pub fn capture_folder_list_context(&self) -> FolderListContext {
        let cfg = self.config.read().unwrap();
        FolderListContext {
            mode: SearchMode::from(cfg.search.folder_mode),
            show_hidden_system: cfg.search.show_hidden_system,
            max_results: cfg.search.effective_result_limit(),
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
        let top_n = self.config.read().unwrap().search.effective_result_limit();
        self.history.prepare_save_if_dirty(threshold, top_n)
    }

    pub fn prepare_history_flush(&mut self) -> Option<PreparedHistorySave> {
        let top_n = self.config.read().unwrap().search.effective_result_limit();
        self.history.prepare_flush(top_n)
    }

    /// 現在の設定を読む。**guard を返す**（#1032 で `&Config` から変わった）。
    ///
    /// **保持したまま `update_config` を呼ばないこと**——同じ `RwLock` を読みと書きで
    /// 二重に取ることになり、同一スレッドで自己デッドロックする。値を使い終えたら落とすか、
    /// 必要な値をコピーしてから手放す。
    pub fn config(&self) -> RwLockReadGuard<'_, Config> {
        self.config.read().unwrap()
    }

    /// 設定を差し替える。**`&mut self` である**（外側の `Mutex<Engine>` を要求する）。
    ///
    /// この非対称の理由と、`&self` へ変えてはならない理由は `config` フィールドの doc。
    pub fn update_config(&mut self, config: Config) {
        *self.config.write().unwrap() = config;
    }

    /// 設定の共有ハンドルを渡す（#1032）。
    ///
    /// **UI が毎フレーム行う live-read を、外側の `Mutex<Engine>` の外へ出すための口である。**
    /// 検索の worker は `search` の間じゅう外側の `Mutex` を握る（実運用点で 40〜95 ms）ため、
    /// UI が同じ錠越しに設定を読むと、そのフレームは worker の走査が終わるまで返らない
    /// （`read_window_width` 単独で 43,939 µs の待ちを実測した・`PERFORMANCE.md`）。
    ///
    /// **返すのは同じ `Arc` であって写しではない**——`update_config` の書き込みは、この
    /// ハンドルを持つ読み手へそのまま届く。別々に持って両方へ書く形にすると、書き手が
    /// 片方を忘れた瞬間に UI と検索が違う設定で動く。
    pub fn config_handle(&self) -> Arc<RwLock<Config>> {
        Arc::clone(&self.config)
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
            Some(IndexInputs::from_config(&self.config.read().unwrap()))
        } else {
            None
        }
    }

    /// ロック外で構築した index をスワップし、ビルド開始時スナップショット（`built_from`）と
    /// 現在 config の `IndexInputs` が一致するときだけ stale を落とす。ビルド中に config が
    /// 変わっていれば stale を残し、次の `begin_index_drain` で再ビルドさせる（lost-update を塞ぐ）。
    ///
    /// **swap と照合の原子性は外側の `Mutex<Engine>` が守る**（#1032）。この関数は `&mut self`
    /// を取り、唯一の書き手 `update_config` も `&mut self` を取るので、両者が交錯する並びは
    /// 構築できない。**config を `RwLock` に移してもこの性質は変わらない**——読みだけが錠の
    /// 外へ出ており、書きは外側の `Mutex` の内側に留めてある。
    pub fn complete_index_drain(&mut self, index: PrebuiltIndex, built_from: &IndexInputs) {
        self.apply_prebuilt_index(index);
        // ビルド開始時スナップショットと現在の index 入力が一致するときだけ stale を落とす。
        // ビルド中に config が変わっていれば（!=）stale を残し、次の begin_index_drain で再ビルドさせる。
        if IndexInputs::from_config(&self.config.read().unwrap()) == *built_from {
            self.index_stale = false;
        }
    }

    /// 索引の件数。**`&[AppEntry]` を貸す `entries()` は無い**——索引は `target_path` を
    /// 圧縮して持ち、`AppEntry` の形では存在しない（`search/path_store.rs` の `//!`）。
    pub fn entry_count(&self) -> usize {
        self.search_engine.entry_count()
    }

    /// 索引 `i` の表示名。
    pub fn entry_name(&self, i: usize) -> &str {
        self.search_engine.entry_name(i)
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
        HistoryStore::empty()
    }

    /// `empty_history()` は名前どおり空でなければならない（#963）。
    ///
    /// **この検査は CI では常に緑で、開発機でだけ落ちる。** ランナーには
    /// `%APPDATA%\Snotra` が無いので `HistoryStore::load()` も空を返すためで、
    /// だからこそ食い違いを CI が構造的に検出できない。ここで固定する。
    #[test]
    fn empty_history_fixture_is_actually_empty() {
        assert!(
            empty_history().recent_launches(usize::MAX).is_empty(),
            "empty_history() が空でない — 実 history.bin を読んでいる"
        );
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
        assert_eq!(engine.entry_count(), 1);
        assert_eq!(engine.entry_name(0), "Firefox");
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
        assert_eq!(engine.entry_count(), 1);
        assert_eq!(engine.entry_name(0), "OldApp");

        engine.replace_entries(make_entries(&["NewApp1", "NewApp2"]));
        assert_eq!(engine.entry_count(), 2);
        assert_eq!(engine.entry_name(0), "NewApp1");
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

    /// **`show_icons` は索引を変えないので index 入力ではない**（#996 follow-up）。
    ///
    /// **この 1 本が守るのは「アイコン表示の切替で 313,028 件の全再構築が走らない」ことである。**
    /// 戻すと壊れ方が静かである——検索結果もアイコン表示も正しいまま、チェックボックスを
    /// 触るたびに全走査と `index.bin` の書き直しが走る。しかも `false → true` の向きでは
    /// **払って効果が 0** になる（破棄は `true → false` でしか仕事をしない）。
    #[test]
    fn index_inputs_ignore_show_icons_in_both_directions() {
        for start in [false, true] {
            let mut base = default_config();
            base.appearance.show_icons = start;
            let mut c = base.clone();
            c.appearance.show_icons = !start;
            assert_eq!(
                IndexInputs::from_config(&base),
                IndexInputs::from_config(&c),
                "show_icons {start} → {} は索引を変えない（アイコンの破棄は config_watcher の\
                 エッジ検出が担う）",
                !start
            );
        }
    }
}
