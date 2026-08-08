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

/// ファイルシステムを**実際に走査して**エントリを返す（`#[ignore]` の corpus 専用・実測 75 秒）。
///
/// **[`real_index_entries`] と取り違えてはならない。** あちらは `index.bin` から実体化して
/// 返すので、**組み直しの正しさを問う側の入力にはできない**——v7 はディスクに `target_path` を
/// 持たず、木から組み直した結果が返るため、それを「原文」として突き合わせると**組み直し対
/// 組み直しの不動点**になり、どれだけ壊れても落ちない（件数つきの成功メッセージまで出る）。
/// **原文はここにしか無い。**
///
/// **並びは製品と同じ [`crate::indexer::sort_entries_canonical`] を通す**（親の二分探索が整列を
/// 前提にする）。比較子を書き起こすと、写した側だけが旧い並びで木を建て、「実運用点と別物の
/// 木」に対して一致を報告する。
///
/// scan パス未設定なら理由を出して `None`（CI は必ずここに該当する）。**呼び出し側で
/// メッセージを書き分けないこと**——自動スキップの理由が 1 か所に在ることが、「実データで
/// 固定してある」と読み違えないための唯一の手がかりである。
pub(super) fn real_scanned_entries() -> Option<Vec<AppEntry>> {
    let config = crate::config::Config::load();
    if config.paths.scan.is_empty() {
        println!("実 config に scan パスが無いためスキップします。");
        return None;
    }
    let mut entries =
        crate::indexer::scan_all(&config.paths.scan, config.search.show_hidden_system);
    assert!(!entries.is_empty(), "走査が 0 件では接地にならない");
    crate::indexer::sort_entries_canonical(&mut entries);
    Some(entries)
}
