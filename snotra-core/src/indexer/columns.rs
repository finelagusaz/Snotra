//! 索引の派生データ——文字ビットマスク 2 本と、小文字化した派生文字列 2 本の列——の型と導出。
//!
//! 記録側（[`derive_columns`]）と追記側（[`extend_cached_masks`]）をここに同居させてあるのは、
//! 潰しの判定（`crate::query::measure_derived_sharing`）を両者が同じ経路で通ることが、ディスクと
//! メモリで潰れ方が一致する根拠だからである（正本は [`CachedMasks`] の doc）。

use serde::{Deserialize, Serialize};

use crate::index_tree::IndexTree;
use crate::query::{
    file_char_mask, lower_file_name, measure_derived_sharing, name_char_mask, to_lower_folded,
};
use crate::str_arena::{LowerFileColumn, LowerFileSlot, LowerNameColumn};

use super::AppEntry;

#[cfg(test)]
mod tests;

/// 事前計算済みの派生データ。SearchEngine の構築時に渡すことで起動時の計算をスキップする。
///
/// 出所は `index.bin` から読んだもの（キャッシュヒット）と、`save_cache_sorted_in` が書いたその足で返したもの（反復 11 以降）である。**出所を数え上げてはならない**——かつて「**出所は 2 つある**」と書きながら**同じ段落の次の文で数え上げを禁じていた**（`docs/comment-guidelines.md`「第一原則: コメントは理由を書く」が経路の数を書かないよう定めているのに、自分で反した形）。正本は `save_cache_sorted` と `load_cache_in` の分岐であり、数えた散文は枝が増えるたびに腐る。**出所によって表現は変わらない**——潰し方の判定は `query::measure_derived_sharing` の 1 か所を通るので、消費側は出所を区別しない（区別するのは `lower` の variant だけである）。
///
/// - `char_masks` / `file_name_char_masks`: この型が在るなら必ず在る
/// - `lower`: 派生文字列を持たない古い版を読んだときは `None` → Wave 1 計算が走る。
///   **版の番号を書かない**（`Engine::from_material` の doc と同じ理由で、番号を書くと版を上げるたびにこの散文だけが腐る）
///
/// `normalized_keys` は持たない——`target_path` からの導出へ移して索引・オンディスクの
/// 双方から外した（`PERFORMANCE.md`「パスクエリ全走査のコスト — `normalized_keys` を保持するか導出するか」）。
/// `PartialEq` は**テストのときだけ**持つ。「返した組と `index.bin` へ書いた組が同一である」
/// を 1 行で言うためであり、製品はこの型を比べない。
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct CachedMasks {
    pub char_masks: Vec<u64>,
    pub file_name_char_masks: Vec<u64>,
    /// v4+ キャッシュ時のみ `Some`。存在すれば `SearchEngine` の Wave 1 をスキップする。
    pub lower: Option<CachedLower>,
}

/// `lower_file_names` のオンディスク表現（v6）。
///
/// **`Option<String>` では足りない。** `None` には「file name 成分が無い」という先客がおり、
/// そこへ「`lower_names[i]` と同一」を重ねると 2 つの意味が同じ表現に乗る。メモリ側は
/// `CompactEntry::file_name_is_lower_name`（構造体の空きパディング）で解いたが、**ディスクに
/// 空きパディングは無い**——旗を別の `Vec<bool>` で持つと 0.30 MiB 余分にかかり、しかも
/// 2 本の Vec の対応がずれても型は何も言わない。3 状態を 1 つの enum に閉じると、
/// postcard のタグ 1 バイトだけで済み**意味も型に載る**（実測 1.11 MiB 対 1.41 MiB）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LowerFileName {
    /// file name 成分が無い（`query::lower_file_name` が `None` を返した）。
    Absent,
    /// 解決後の `lower_name` とバイト一致する。
    SameAsLowerName,
    /// 独自の文字列。
    Text(String),
}

impl LowerFileName {
    /// 列（[`LowerFileColumn`]）へ積むための借用の形。
    ///
    /// **この対応が 1 対 1 であることが、線上表現が変わっていないことの前提である**
    /// ——3 状態のどれかを別の状態へ写すと、`entry_view` の読み替えが静かにずれる。
    pub(crate) fn as_slot(&self) -> LowerFileSlot<'_> {
        match self {
            Self::Absent => LowerFileSlot::Absent,
            Self::SameAsLowerName => LowerFileSlot::SameAsLowerName,
            Self::Text(s) => LowerFileSlot::Text(s),
        }
    }
}

/// キャッシュから復元した派生文字列。**潰し済みか未測定かを型で区別する。**
///
/// **分ける理由は「測り直しが無駄だから」である**（`search/build.rs` の `DerivedStrings` の doc
/// が機序の正本）。潰し済みの列を測定経路へ流しても結果は変わらないが、312,690 回の比較が
/// 丸ごと無駄になる。variant を分けることで、その取り違えはコンパイルを通らない。
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub enum CachedLower {
    /// v6 以降。記録時に `query::measure_derived_sharing` で測って潰してある。
    /// `assemble` は測り直さず、この潰し方をそのまま索引の表現として使う。
    Collapsed {
        /// `None` = `entries[i].name` と同一。
        ///
        /// **型は `Vec<Option<String>>` ではなく [`LowerNameColumn`] だが、線上のバイト列は
        /// 変わっていない**（正本は `crate::str_arena` の doc）——`lower_file_names` も同じ。
        lower_names: LowerNameColumn,
        lower_file_names: LowerFileColumn,
    },
    /// v5 / v4。全件が実体を持つ未測定の列。`assemble` が測って潰す。
    Raw {
        lower_names: Vec<String>,
        lower_file_names: Vec<Option<String>>,
    },
}

/// 木と派生 4 本を導出しただけの中間の姿。**I/O もロック契約も持たない。**
///
/// 分けてあるのは、`index.bin` を書く関数と「潰しの導出」を突き合わせたい検知器が別物だから
/// である——書き込みごと公開すると、検知器がファイルシステムと、型に無いロック契約
/// （→「index.bin 書き込みの排他」）を巻き込む。
///
/// **タプルにしてはならない。** `Vec<u64>` が 2 本隣接するので、名前の無い並びでは取り違えて
/// も型検査を通る（同じ理由で [`CachedMasks`] は組のまま渡す）。
pub(crate) struct DerivedColumns {
    pub(crate) tree: IndexTree,
    pub(crate) char_masks: Vec<u64>,
    pub(crate) file_name_char_masks: Vec<u64>,
    pub(crate) lower_names: LowerNameColumn,
    pub(crate) lower_file_names: LowerFileColumn,
}

impl DerivedColumns {
    /// 列を [`CachedMasks`] へ畳む。
    ///
    /// **`index.bin` へ書き終えた後に呼ぶ。** 書く側は素の列を `Cow::Borrowed` で借りるので、
    /// enum（[`CachedLower`]）へ包むのは借用が終わってからでなければならない——逆順にすると
    /// 包んだ中身を取り出すために `unreachable!()` 付きの `match` が要る。
    pub(crate) fn into_cached_masks(self) -> (IndexTree, CachedMasks) {
        let masks = CachedMasks {
            char_masks: self.char_masks,
            file_name_char_masks: self.file_name_char_masks,
            // **`Collapsed` で渡す。** 列は `collapse_lower_pair`（= `measure_derived_sharing`）
            // を通してあり、`assemble` はこれを測り直さない（variant の意味は [`CachedLower`]）。
            lower: Some(CachedLower::Collapsed {
                lower_names: self.lower_names,
                lower_file_names: self.lower_file_names,
            }),
        };
        (self.tree, masks)
    }
}

/// エントリから木と派生 4 本を導出する。**I/O を持たない。**
pub(crate) fn derive_columns(entries: Vec<AppEntry>) -> DerivedColumns {
    // マスクをここで計算するのは、受け取った側が再計算せずに索引の表現へそのまま使うためである（運ぶ器は [`IndexMaterial`] であり、受け取る経路をここで数えない）。
    //
    // **per-entry の導出そのものは [`derive_entry_collapsed`] が持つ。** 追記側
    // （`extend_cached_masks`）と同じ関数を通ることだけが、ディスクとメモリで潰れ方と
    // マスクが一致する根拠である。ここに列ごとの別実装を書き起こしてはならない。
    //
    // 4 本を 1 周で埋める。列ごとの `collect` に分けると、潰す前の `Vec<String>` /
    // `Vec<Option<String>>` の spine が潰した後の spine と重なって生きる区間ができる。
    //
    // **派生文字列 2 本はアリーナ（`crate::str_arena`）へ直に積む。** `derive_entry_collapsed`
    // が返す `String` はここで写して落ちるので、記録側に per-entry の spine は残らない
    // （`String` そのものの一時確保は残る——それを消すのは別反復である）。
    let len = entries.len();
    let mut char_masks = Vec::with_capacity(len);
    let mut file_name_char_masks = Vec::with_capacity(len);
    let mut collapsed_lower_names = LowerNameColumn::with_capacity(len);
    let mut collapsed_lower_file_names = LowerFileColumn::with_capacity(len);
    for entry in &entries {
        let (char_mask, file_mask, lower_name, lower_file) = derive_entry_collapsed(entry);
        char_masks.push(char_mask);
        file_name_char_masks.push(file_mask);
        collapsed_lower_names.push(lower_name.as_deref());
        collapsed_lower_file_names.push(lower_file.as_slot());
    }

    // **木を建てるのは派生文字列を導出し終えた後である。** 建てる段が `target_path` を
    // 吸い上げるので、順序を入れ替えると `lower_file_name(&e.target_path)` の材料が消える。
    let tree = IndexTree::build(entries);

    DerivedColumns {
        tree,
        char_masks,
        file_name_char_masks,
        lower_names: collapsed_lower_names,
        lower_file_names: collapsed_lower_file_names,
    }
}

/// エントリ 1 件の派生（**潰す前**）。マスク 2 本と、潰す前の派生文字列 1 組を返す。
///
/// **マスクは潰す前の完全な文字列から導出する。** 潰した後に取ると
/// `file_char_mask(None) == 0` になり、pre-filter が false negative を出す
/// （`search/build.rs` の `compute_wave2` と同じ不変条件——あちらは「ビットマスクより後に
/// 潰す」という順序で守っており、ここでは「潰す前に導出する」という順序で守る）。
///
/// 潰さない列（`CachedLower::Raw`）へ追記する経路と、マスクだけを要する経路（`lower` が
/// `None`）がこれを直接呼ぶ。潰した形が要るなら [`derive_entry_collapsed`] を呼ぶ。
fn derive_entry_lowers(entry: &AppEntry) -> (u64, u64, String, Option<String>) {
    let lower_name = to_lower_folded(&entry.name);
    let lower_file = lower_file_name(&entry.target_path);
    let char_mask = name_char_mask(&lower_name);
    let file_mask = file_char_mask(lower_file.as_deref());
    (char_mask, file_mask, lower_name, lower_file)
}

/// エントリ 1 件の派生（**潰した形**）。マスク 2 本と、畳んだ派生文字列 1 組を返す。
///
/// **潰す前にマスクを取るという順序は、この 1 か所にしかない。**（`derive_entry_lowers` が
/// 返した後でだけ `collapse_lower_pair` を当てる。）記録側（[`derive_columns`]）と追記側
/// （`extend_cached_masks` の `Collapsed` 枝）が同じここを通ることが、ディスクとメモリで
/// 潰れ方とマスクが一致する根拠である。検知器は `derived_masks_come_from_the_uncollapsed_strings`。
fn derive_entry_collapsed(entry: &AppEntry) -> (u64, u64, Option<String>, LowerFileName) {
    let (char_mask, file_mask, lower_name, lower_file) = derive_entry_lowers(entry);
    let (lower_name, lower_file) = collapse_lower_pair(&entry.name, lower_name, lower_file);
    (char_mask, file_mask, lower_name, lower_file)
}

/// 派生文字列 1 組を、`measure_derived_sharing` の判定に従って潰した形へ畳む。
///
/// **唯一の呼び出し元は [`derive_entry_collapsed`] である**（記録側・追記側はそこを通る）。
/// 別実装で書き起こすと、その経路の分だけが索引の読み替えとずれる——`assemble` は
/// `Collapsed` を測り直さないので、ずれは**検索結果のスコアという形で静かに現れる**。
fn collapse_lower_pair(
    name: &str,
    lower_name: String,
    lower_file: Option<String>,
) -> (Option<String>, LowerFileName) {
    let sharing = measure_derived_sharing(name, &lower_name, lower_file.as_deref());
    let file = match (sharing.file_name_is_lower_name, lower_file) {
        (true, _) => LowerFileName::SameAsLowerName,
        (false, None) => LowerFileName::Absent,
        (false, Some(s)) => LowerFileName::Text(s),
    };
    let name = if sharing.lower_name_is_name {
        None
    } else {
        Some(lower_name)
    };
    (name, file)
}

/// CachedMasks の各 Vec に新しいエントリの分を追記する。
/// インデックスキャッシュの恩恵を維持しつつ、PATH エントリ等の追加分を補完する。
///
/// `char_masks` / `file_name_char_masks` は常に追記。派生文字列は `lower` が `Some` の場合のみ、
/// **その variant が持つ表現に合わせて**追記する。`kana_lower_names` は SearchEngine 側で
/// entries から直接計算されるためここでは扱わない。
pub(crate) fn extend_cached_masks(masks: &mut CachedMasks, new_entries: &[AppEntry]) {
    for entry in new_entries {
        // per-entry の導出は記録側（`derive_columns`）と同じ [`derive_entry_lowers`] /
        // [`derive_entry_collapsed`] を通す。ここに関数列を書き起こしてはならない。
        match masks.lower {
            // v3 以下: 派生文字列を持たない（Wave 1 が全件を計算する）。マスクだけ足す。
            None => {
                let (char_mask, file_mask, _, _) = derive_entry_lowers(entry);
                masks.char_masks.push(char_mask);
                masks.file_name_char_masks.push(file_mask);
            }
            // 潰さない列。**この枝は潰す段を持たないので順序の不変条件も持たない**
            // （`assemble` が後で測る）。
            Some(CachedLower::Raw {
                ref mut lower_names,
                ref mut lower_file_names,
            }) => {
                let (char_mask, file_mask, lower, lower_file) = derive_entry_lowers(entry);
                masks.char_masks.push(char_mask);
                masks.file_name_char_masks.push(file_mask);
                lower_names.push(lower);
                lower_file_names.push(lower_file);
            }
            // **潰し済みの列へは、同じ判定を通した値だけを足す。**
            Some(CachedLower::Collapsed {
                ref mut lower_names,
                ref mut lower_file_names,
            }) => {
                let (char_mask, file_mask, name, file) = derive_entry_collapsed(entry);
                masks.char_masks.push(char_mask);
                masks.file_name_char_masks.push(file_mask);
                lower_names.push(name.as_deref());
                lower_file_names.push(file.as_slot());
            }
        }
    }
}
