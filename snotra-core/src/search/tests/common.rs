//! テスト共通 fixture（複数の機能別モジュールが共有する最小のエントリ/履歴生成）。

use crate::history::HistoryStore;
use crate::indexer::AppEntry;

pub(super) fn make_entries(names: &[&str]) -> Vec<AppEntry> {
    names
        .iter()
        .map(|n| AppEntry {
            name: n.to_string(),
            target_path: format!("C:\\fake\\{}.lnk", n),
            is_folder: false,
        })
        .collect()
}

pub(super) fn empty_history() -> HistoryStore {
    HistoryStore::empty()
}

/// 実 `%APPDATA%\Snotra\index.bin` に**載っているときだけ**エントリを返す。
/// scan パス未設定・キャッシュ不在・キャッシュが config と食い違うときは `None`。
///
/// **これは corpus であって保証ではない。** 上の 3 条件のいずれかで呼び出し側のテストは自動
/// スキップし（CI は 1 つめで必ず該当する）、そこに残る保証は同じ論点を合成 fixture で押さえる
/// テストのほうである。合成が届かない多様さ（ドライブ直下・UNC 共有・非 ASCII・深い木・親が
/// 索引に不在のエントリ）を開発機の実データで舐めるのがこの corpus の役目で、両者は代替では
/// なく補完である。**「実データの全件で固定してある」と書くときは、この自動スキップを併記すること。**
///
/// 走査しない入口（[`crate::indexer::load_cached_entries`]）を通すのは必須である——理由は
/// その doc にある。
pub(super) fn real_index_entries() -> Option<Vec<AppEntry>> {
    let config = crate::config::Config::load();
    if config.paths.scan.is_empty() {
        return None;
    }
    crate::indexer::load_cached_entries(&config.paths.scan, config.search.show_hidden_system)
}
