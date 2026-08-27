//! 索引の材料を組のまま運ぶ器（[`IndexMaterial`]）と、スキャン・`index.bin` 入出力・PATH 走査を
//! 責務ごとに分けた子モジュールの索引。
//!
//! 木と派生データを 1 つの型へ束ねるのはこのファイルの責務である——**両者の長さが揃うこと**
//! を、消費側の規約ではなく型で持つ（`index.bin` から来た組は `IndexMaterial::from_untrusted`
//! が検証し、揃わなければ全走査へ落とす）。索引を建てる側は `crate::search` にあり、そちらは組を
//! ほどかずに受け取る（理由は [`IndexMaterial`] の doc）。
//!
//! **crate の外から見える名前はここの re-export が決める。** 子モジュールは private ゆえ、
//! `snotra_core::indexer::X` という呼び出しの形はこのファイルを通って保たれる。子モジュールの
//! 責務はそれぞれの `//!` が正本であり、ここでは数え上げない（下の `mod` 宣言が一覧である）。

use serde::{Deserialize, Serialize};

use crate::index_tree::IndexTree;

mod cache;
mod columns;
mod keys;
mod path_env;
mod scan;

pub use cache::{
    CacheByteBreakdown, CacheByteRow, INDEX_CACHE_VERSION, LoadOrScanResult, LoadOrScanStats,
    cache_byte_breakdown_in, index_built_at_in, load_cached_entries, load_or_scan_with_stats,
    rebuild_and_save, scan_identity_hash,
};
pub use columns::{CachedLower, CachedMasks, LowerFileName};
pub(crate) use columns::{derive_columns, extend_cached_masks};
pub(crate) use keys::normalize_file_name_key_into;
pub use keys::{normalize_entry_key, normalize_entry_key_into};
pub use path_env::scan_path_env;
pub use scan::scan_all;
pub(crate) use scan::{is_hidden_or_system, sort_entries_canonical};

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppEntry {
    pub name: String,
    pub target_path: String,
    pub is_folder: bool,
}

/// 索引の材料。**木と、その木に対して導出した派生データを組のまま運ぶ。**
///
/// **ほどいて 2 つの値として持ち回してはならない。** 束ねる理由は [`CachedMasks`] が 4 本を束ねるのと同じ（`SearchEngine::new_with_cached_masks` の doc「境界を跨ぐ手前でほどかない」）で、こちらが消すのは「**木を伸ばしたのにマスクへ追記し忘れる**」誤りである。フィールドが private ゆえ木だけを伸ばす経路は書けない——その誤りは `SearchEngine` の長さ検証が `debug_assert` ゆえ **release で沈黙し**、再構築を終えた後の初回検索で並列 Vec の添字外 panic として出る（`panic = "abort"` ゆえプロセスごと終了）。
///
/// **`Option` を消費側へ配らない。** かつては呼び出し点が `match` で `Some` / `None` を捌き、同じ分岐が `PrebuiltIndex` / `Engine` / `SearchEngine` の **3 層 5 か所**へ写っていた。`PrebuiltIndex::from_parts` を足すだけの案は**そのうち 1 か所しか直せない**（根は最下層の 2 コンストラクタで、上の 2 層はその写しだから）ので採らなかった。
///
/// **ディスクから来た組は `from_untrusted` を通す**（列長を検証する）。構成上正しいのは `derive_columns` の出力だけである。
pub struct IndexMaterial {
    tree: IndexTree,
    masks: Option<CachedMasks>,
}

impl IndexMaterial {
    /// 派生データを持たない材料（初回起動と、保存側がマスクを返さなかったとき）。
    pub fn from_tree(tree: IndexTree) -> Self {
        Self { tree, masks: None }
    }

    /// 導出したその足の組。**長さを検証しない**——[`derive_columns`] が 1 周で 4 本を埋めるので構成上一致する。
    pub(crate) fn derived(tree: IndexTree, masks: CachedMasks) -> Self {
        Self {
            tree,
            masks: Some(masks),
        }
    }

    /// **ディスクから読んだ組を検証してから受け取る。** 列長が木と揃わなければ `None` を返し、呼び出し側は全走査へ落ちる。
    ///
    /// **見るのは列の長さだけである。** マスクの中身が正しいかは検証できない（正しさの定義が「その木から導出した値と一致する」であり、それを確かめるには導出し直すことになる——検証のために削減を捨てる形になる）。検証をここに置くのは、`load_cache_in` が [`IndexTree::from_parts`] で**木の整合しか**見ていなかったためである——切り詰められた `index.bin` は「木より短いマスク」として起動経路へ入り、`SearchEngine` の長さ検証は `debug_assert` ゆえ release で消えて初回検索の添字外 panic になっていた。**版が読めたことは中身が健全であることを意味しない**（`from_parts` の doc と同じ理屈）。
    pub(crate) fn from_untrusted(tree: IndexTree, masks: CachedMasks) -> Option<Self> {
        let n = tree.len();
        if masks.char_masks.len() != n || masks.file_name_char_masks.len() != n {
            return None;
        }
        let lower_ok = match &masks.lower {
            None => true,
            Some(CachedLower::Collapsed {
                lower_names,
                lower_file_names,
            }) => lower_names.len() == n && lower_file_names.len() == n,
            Some(CachedLower::Raw {
                lower_names,
                lower_file_names,
            }) => lower_names.len() == n && lower_file_names.len() == n,
        };
        lower_ok.then_some(Self {
            tree,
            masks: Some(masks),
        })
    }

    /// PATH スキャンが見つけたエントリをマージする。**マスクへの追記と木への追加を対で行う唯一の場所である。**
    ///
    /// 起動経路（`src-tauri` の `main`）と背景の再構築（同 `drain_index`）が同じここを通る。片方だけ書く形は**この型の外からは書けない**——それがフィールドを private にしてある理由である。検知器は `search/tests/build.rs` の `path_merge_after_cache_miss_agrees_with_deriving_over_the_extended_tree` と `path_merge_extends_the_tree_even_without_derived_data`（**どちらも変異を注入して落ちることを実測してある**）。
    ///
    /// **スキャンは呼び出し側に残してある。** 実 PATH 環境変数を読む [`scan_path_env`] を内側へ入れると、この操作が決定的なユニットテストに乗らない。スキャンの呼び忘れは `entries` が手元に無いという形で目に見えるので、閉じる価値があるのはマージの側だけである。
    pub fn extend_with_path_entries(&mut self, entries: Vec<AppEntry>) {
        // **追記が先に来るのは所有権の帰結であって規約ではない。** `extend_with_roots` は `entries` を move で取るため、逆順に書くと clone が要る——順序の取り違えは「うっかり」では書けない。ゆえにこの順序に検知器を置いていない。
        if let Some(masks) = self.masks.as_mut() {
            extend_cached_masks(masks, &entries);
        }
        self.tree.extend_with_roots(entries);
    }

    /// 木の読み取り（件数・パスの組み直し）。**伸ばす操作は公開しない。**
    pub fn tree(&self) -> &IndexTree {
        &self.tree
    }

    /// 索引を建てる側だけがほどく。**crate 外へ出さない**——ほどいた 2 値を持ち回せるようになると、この型の存在理由が消える。
    pub(crate) fn into_parts(self) -> (IndexTree, Option<CachedMasks>) {
        (self.tree, self.masks)
    }

    /// 派生データを持っているか。**検知器と計測ハーネスのためだけに在る**（`SearchEngine::footprint_rows` と同じ `#[doc(hidden)] pub` の扱い）。
    ///
    /// **製品コードはこれを読んで分岐してはならない。** 建て方の分岐は `SearchEngine::from_material` の 1 か所に閉じており、そこへ戻すために `Option` を外へ出していない。ここが要るのは 2 つの理由による: (1) `extend_with_path_entries` が**マスクを取り落とさない**ことを検知器が名指しで測れるようにする——落とすと `from_material` が黙って木からの導出へ切り替わり、A/B 一致は**成立したまま**削減だけが消える（挙動テストでは捕まらない類の退行）。(2) `tests/memory_footprint.rs` が「どちらの枝を測ったか」を出力に添える——添えないとその数字は読めない。
    #[doc(hidden)]
    pub fn has_masks(&self) -> bool {
        self.masks.is_some()
    }
}
