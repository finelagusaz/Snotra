//! 疎な派生文字列の列を、要素ごとの確保を持たない表現で保つ（`lower_names` /
//! `lower_file_names`）。
//!
//! **1 エントリ 1 確保をやめるための表現である。** 表示名については
//! [`crate::index_tree::NameArena`] が先に同じことをしており（`PERFORMANCE.md`「採用: 表示名を
//! 文字列アリーナで持つ」）、こちらはその適用先を派生文字列の 2 列へ広げる。違いは 2 つ:
//!
//! 1. **要素が `None` を取りうる**（実データで `lower_names` の 86.6% / `lower_file_names` の
//!    81.9% がそれ）。旗は [`Bits`] で持ち、**「長さ 0 のスライス」で代用しない**（[`OptionalStrArena`]）
//! 2. **列ごとに線上表現が違う**——`lower_names` は `seq of Option<str>`、`lower_file_names` は
//!    3 variant の `seq of LowerFileName` である。記憶域を共有し、種（`DeserializeSeed`）だけを
//!    分ける
//!
//! # 線上表現は変わっていない
//!
//! どちらの列も、`Vec<Option<String>>` / `Vec<LowerFileName>` として書かれたバイト列と 1 バイトも
//! 違わない。ゆえに `INDEX_CACHE_VERSION` のバンプも旧版フォールバックも要らない。
//! **この不変を固定するのは下の `*_wire_format_is_identical_to_*` の 2 本であって
//! golden bytes ではない。** `indexer` の `index_cache_on_disk_format_is_stable` は fixture が
//! 持つ形しか通らないので、その形に現れないずれは素通りする——**2 列とも変異注入で実測した**
//! （2026-08-12。`serialize` に `trim_end` を挟むと、どちらの列でも新設テストだけが落ち、
//! golden を含む他の 569 本は緑のまま通った）。
//!
//! **タグの並べ替えは別である**——`LowerFileNameRef` の variant を入れ替える変異は golden を
//! 含む 5 本が落ちた。「golden では足りない」の射程は**文字列の中身の形**であって、
//! 3 状態の網羅ではない。

use std::fmt;

use serde::de::{DeserializeSeed, Deserializer, EnumAccess, SeqAccess, VariantAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize, Serializer};

// ---------------------------------------------------------------------------
// ビット列
// ---------------------------------------------------------------------------

/// 要素ごとの旗を 1 ビットで持つ列。
///
/// **`Vec<bool>` にしない。** 31 万件で 0.30 MiB を払う一方、ここは 39 KiB で済む——旗 2 本を
/// 足しても、この反復が消す spine（4.76 MiB × 2）に対して無視できる額に収まる。
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct Bits {
    words: Vec<u64>,
    len: usize,
}

impl Bits {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn with_capacity(n: usize) -> Self {
        Self {
            words: Vec::with_capacity(n.div_ceil(64)),
            len: 0,
        }
    }

    fn push(&mut self, bit: bool) {
        if self.len.is_multiple_of(64) {
            self.words.push(0);
        }
        if bit {
            self.words[self.len / 64] |= 1u64 << (self.len % 64);
        }
        self.len += 1;
    }

    /// `i` 番の旗。**呼び出し側が `i < len` を保証する契約である。**
    ///
    /// **「範囲外は panic する」とは書けない。** ビットは 64 個ずつ確保されるので、
    /// `len <= i < words.len() * 64` の窓では `words` の添字が範囲内に収まり、release では
    /// **黙って `false`**（[`OptionalStrArena::get`] としては `None`）を返す。`debug_assert` が
    /// 撃つのはデバッグ実行だけである。
    ///
    /// ここに長さ検査を置かないのは全件走査のホットパスだからで、担保は上位に在る——
    /// `search/build.rs` の `assemble` が並列列の長さを揃え、
    /// [`crate::indexer::IndexMaterial::from_untrusted`] がディスクから来た組の列長を検証する。
    #[inline]
    pub(crate) fn get(&self, i: usize) -> bool {
        debug_assert!(i < self.len, "Bits: 範囲外の添字 {i}（長さ {}）", self.len);
        (self.words[i / 64] >> (i % 64)) & 1 == 1
    }

    fn shrink_to_fit(&mut self) {
        self.words.shrink_to_fit();
    }

    /// 常駐ヒープの内訳を数えるための素（計測専用・勘定の規約は `search/footprint.rs`）。
    pub(crate) fn footprint_bytes(&self) -> usize {
        self.words.capacity() * std::mem::size_of::<u64>()
    }

    /// 余剰容量（検知器専用・[`OptionalStrArena::excess_capacity_bytes`]）。
    #[cfg(test)]
    pub(crate) fn excess_capacity_bytes(&self) -> usize {
        (self.words.capacity() - self.words.len()) * std::mem::size_of::<u64>()
    }
}

impl fmt::Debug for Bits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bits").field("len", &self.len).finish()
    }
}

// ---------------------------------------------------------------------------
// 疎な文字列アリーナ
// ---------------------------------------------------------------------------

/// 「実体を持つ要素」だけを連結して保つ列。
///
/// # `None` を「長さ 0 のスライス」で表さない
///
/// 空文字と区別が付かなくなる。実データのエントリ名は空でないが、**それは indexer の名前導出
/// 規則の帰結であって列そのものの性質ではない**——`snotra-core/CLAUDE.md` が
/// 「`is_folder` から推論してはならない」で戒めたのと同じ形の推論依存になる。旗（[`Bits`]）は
/// 39 KiB で済むので、区別を型に載せない理由が無い。
///
/// # 不変条件は検査ではなく構成から従う
///
/// `offsets[0] == 0` / 単調非減少 / `offsets.last() == blob.len()` / 各オフセットが UTF-8 の
/// 文字境界であること——どれも「`&str` を丸ごと押し込む」形からしか作れない
/// （[`crate::index_tree::NameArena`] と同じ理屈）。**オフセットの最上位ビットを旗に流用する案は
/// これを壊すので採らない。**
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct OptionalStrArena {
    /// 実体を持つ要素だけを連結したバイト列。
    blob: String,
    /// 要素 `i` は `blob[offsets[i]..offsets[i + 1]]`。**末尾に番兵を持つ**ので長さは
    /// 要素数 + 1 であり、空でも `[0]` が入る。`None` の要素では前後が等しい。
    ///
    /// `u32` ゆえ `blob` は 4 GiB までである。実データの派生文字列は合計 1.2 MiB。
    offsets: Vec<u32>,
    /// ビット `i` = 「`i` 番が自分の文字列を持つ」。
    present: Bits,
}

/// **手書きである。** `#[derive(Default)]` は `offsets` を空にするので [`Self::len`] が
/// `0 - 1` で溢れる——番兵を持つ表現では「空」は `[0]` であって `[]` ではない。
impl Default for OptionalStrArena {
    fn default() -> Self {
        Self::new()
    }
}

impl OptionalStrArena {
    pub(crate) fn new() -> Self {
        Self {
            blob: String::new(),
            offsets: vec![0],
            present: Bits::new(),
        }
    }

    /// `n` 要素・合計 `bytes` バイトを見込んで確保する。
    fn with_capacity(n: usize, bytes: usize) -> Self {
        let mut offsets = Vec::with_capacity(n + 1);
        offsets.push(0);
        Self {
            blob: String::with_capacity(bytes),
            offsets,
            present: Bits::with_capacity(n),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    /// `i` 番の要素。
    ///
    /// **`None` の意味は列ごとに違う**——`lower_names` なら「表示名と同一」、
    /// `lower_file_names` なら「file name 成分が無い、または `lower_name` と同一」である
    /// （後者の 2 状態を分けるのは [`LowerFileColumn`] の役目）。
    #[inline]
    pub(crate) fn get(&self, i: usize) -> Option<&str> {
        if !self.present.get(i) {
            return None;
        }
        Some(&self.blob[self.offsets[i] as usize..self.offsets[i + 1] as usize])
    }

    pub(crate) fn push(&mut self, s: Option<&str>) {
        if let Some(s) = s {
            self.blob.push_str(s);
        }
        self.finish_element(s.is_some());
    }

    /// 連結バイト列そのもの（**要素の境界を持たない**）。
    ///
    /// 用途は「この列のどこかに文字 X が在るか」を **1 パスで**問うことだけである
    /// （`SearchEngine::any_name_has_path_sep`・#1057）。要素ごとに [`Self::get`] を引く形との
    /// 差と、そちらを採らない理由は `search/build.rs` の `assemble` が持つ。
    ///
    /// **要素を取り出す用途に使わないこと**——境界が無いので、隣接要素をまたいだ部分文字列が
    /// 偽の一致を作りうる。存在判定は「在れば必ずどれかの要素に在る」向きにしか使えない。
    pub(crate) fn blob(&self) -> &str {
        &self.blob
    }

    /// **`PathStore` の `shrink_to_fit` と対で呼ぶ**——余剰容量は索引が伸長しないぶん
    /// 最後まで常駐する（理由は `search/build.rs` の `assemble` の doc）。
    pub(crate) fn shrink_to_fit(&mut self) {
        self.blob.shrink_to_fit();
        self.offsets.shrink_to_fit();
        self.present.shrink_to_fit();
    }

    /// 常駐ヒープの内訳を数えるための素（計測専用）。`(blob, offsets, present)` の確保バイト。
    pub(crate) fn footprint_bytes(&self) -> (usize, usize, usize) {
        (
            self.blob.capacity(),
            self.offsets.capacity() * std::mem::size_of::<u32>(),
            self.present.footprint_bytes(),
        )
    }

    /// 余剰容量（`shrink_to_fit` が消し残した分）。**検知器専用。**
    ///
    /// `footprint_bytes` では代用できない——あちらは容量しか返さないので、`shrink_to_fit` を
    /// 外しても「その容量が正しい」としか読めない。余剰は `len` との差にしか現れない。
    #[cfg(test)]
    pub(crate) fn excess_capacity_bytes(&self) -> usize {
        (self.blob.capacity() - self.blob.len())
            + (self.offsets.capacity() - self.offsets.len()) * std::mem::size_of::<u32>()
            + self.present.excess_capacity_bytes()
    }

    /// 「1 要素ぶんを `blob` へ書き終えた」ことを記録する。
    ///
    /// 復号の種は `blob` へ直接流し込むので [`Self::push`] を通れない（`&str` を手に持たない）。
    /// **区切りを打つのはここ 1 か所であり**、`offsets` と `present` が同じ回数だけ伸びることは
    /// この関数が 1 つであることから従う。
    fn finish_element(&mut self, present: bool) {
        self.offsets.push(self.blob.len() as u32);
        self.present.push(present);
    }
}

impl fmt::Debug for OptionalStrArena {
    /// **中身を並べない。** 31 万件を展開すると失敗時の出力がターミナルを埋める。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OptionalStrArena")
            .field("len", &self.len())
            .field("bytes", &self.blob.len())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// 復号の共通部品
// ---------------------------------------------------------------------------

/// 1 要素ぶんを、`String` を作らずに連結バイト列へ直接流し込む種。
///
/// **`next_element::<String>()` にしてはならない**——それでは要素ごとの確保が戻り、
/// アリーナという表現が存在する理由がなくなる。postcard は借用した `&str` を渡すので
/// 写しは 1 回の `push_str` で済む。
///
/// **表示名（[`crate::index_tree::NameArena`]）とここが同じものを使う。** 責務は
/// 「`String` を作らずに `&str` を押し込む」ただ 1 つで、列ごとに分岐する理由が無い。
pub(crate) struct StrSink<'a>(pub(crate) &'a mut String);

impl<'de> DeserializeSeed<'de> for StrSink<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_str(self)
    }
}

impl<'de> Visitor<'de> for StrSink<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a string")
    }
    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<(), E> {
        self.0.push_str(v);
        Ok(())
    }
    fn visit_borrowed_str<E: serde::de::Error>(self, v: &'de str) -> Result<(), E> {
        self.0.push_str(v);
        Ok(())
    }
}

/// 壊れた長さプレフィックスで巨大確保しないための上限。
///
/// **理由も額も [`crate::index_tree::NameArena`] と同じである**（serde の `Vec` 訪問子が
/// 1 MiB 相当で頭打ちにするのに倣う）。自前の訪問子はその保護を自分で持たない限り迂回し、
/// 迂回すると「読めなかった → cache-miss → 再走査」へ穏やかに落ちていた壊れ方が、
/// 確保失敗 ＝ release では `panic="abort"` に化ける。
const MAX_PREALLOC_BYTES: usize = 1024 * 1024;

/// 列の長さプレフィックスから、事前確保してよい要素数を出す。
///
/// **オフセット列の要素幅（`u32`）で割るのは、どのアリーナでも同じ導出である**——表示名も
/// 派生文字列も、事前に確保するのは番兵付きのオフセット列だけで、連結バイト列は伸長に任せる
/// （合計バイト数は読み終わるまで分からない）。
pub(crate) fn capped_capacity<'de, A: SeqAccess<'de>>(seq: &A) -> usize {
    seq.size_hint()
        .unwrap_or(0)
        .min(MAX_PREALLOC_BYTES / std::mem::size_of::<u32>())
}

// ---------------------------------------------------------------------------
// 列 1: `lower_names`（線上は `seq of Option<str>`）
// ---------------------------------------------------------------------------

/// `lower_names` の列。**`None` は「表示名と同一」を意味する**（`assemble` が構築時に測って潰す）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LowerNameColumn(OptionalStrArena);

impl LowerNameColumn {
    pub(crate) fn with_capacity(n: usize) -> Self {
        Self(OptionalStrArena::with_capacity(n, 0))
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    pub(crate) fn get(&self, i: usize) -> Option<&str> {
        self.0.get(i)
    }

    /// 連結バイト列（存在判定専用・[`OptionalStrArena::blob`] の doc が正本）。
    pub(crate) fn blob(&self) -> &str {
        self.0.blob()
    }

    pub(crate) fn push(&mut self, s: Option<&str>) {
        self.0.push(s);
    }

    pub(crate) fn shrink_to_fit(&mut self) {
        self.0.shrink_to_fit();
    }

    /// 先頭から順に走る。
    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = Option<&str>> {
        (0..self.len()).map(|i| self.get(i))
    }

    /// 実体を持つ要素の数（`index.bin` の内訳計器が「共有の効き」を出すために使う）。
    pub(crate) fn count_present(&self) -> usize {
        self.iter().flatten().count()
    }

    /// 常駐ヒープの内訳を数えるための素（計測専用）。`(blob, offsets, present)`。
    pub(crate) fn footprint_bytes(&self) -> (usize, usize, usize) {
        self.0.footprint_bytes()
    }

    /// 余剰容量（検知器専用・[`OptionalStrArena::excess_capacity_bytes`]）。
    #[cfg(test)]
    pub(crate) fn excess_capacity_bytes(&self) -> usize {
        self.0.excess_capacity_bytes()
    }
}

/// 列を 1 行で組む口（テストの fixture と、要素を手に持っている呼び出し側のため）。
impl<'a> FromIterator<Option<&'a str>> for LowerNameColumn {
    fn from_iter<I: IntoIterator<Item = Option<&'a str>>>(it: I) -> Self {
        let it = it.into_iter();
        let mut col = Self::with_capacity(it.size_hint().0);
        for s in it {
            col.push(s);
        }
        col
    }
}

impl Serialize for LowerNameColumn {
    /// **`Option<&str>` を要素として書く。** serde の `Option` は `serialize_none` /
    /// `serialize_some` を呼ぶ形が `Option<String>` と同一なので、線上のバイト列も一致する。
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let mut seq = ser.serialize_seq(Some(self.len()))?;
        for i in 0..self.len() {
            seq.serialize_element(&self.get(i))?;
        }
        seq.end()
    }
}

/// 1 要素（`Option<str>`）をアリーナへ流し込む種。
struct OptionStrSink<'a>(&'a mut OptionalStrArena);

impl<'de> DeserializeSeed<'de> for OptionStrSink<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_option(self)
    }
}

impl<'de> Visitor<'de> for OptionStrSink<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an optional string")
    }
    fn visit_none<E: serde::de::Error>(self) -> Result<(), E> {
        self.0.finish_element(false);
        Ok(())
    }
    fn visit_some<D: Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        StrSink(&mut self.0.blob).deserialize(de)?;
        self.0.finish_element(true);
        Ok(())
    }
}

struct LowerNameColumnVisitor;

impl<'de> Visitor<'de> for LowerNameColumnVisitor {
    type Value = LowerNameColumn;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a sequence of optional strings")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<LowerNameColumn, A::Error> {
        let mut arena = OptionalStrArena::with_capacity(capped_capacity(&seq), 0);
        while seq.next_element_seed(OptionStrSink(&mut arena))?.is_some() {}
        Ok(LowerNameColumn(arena))
    }
}

impl<'de> Deserialize<'de> for LowerNameColumn {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        de.deserialize_seq(LowerNameColumnVisitor)
    }
}

// ---------------------------------------------------------------------------
// 列 2: `lower_file_names`（線上は `seq of LowerFileName`・3 variant）
// ---------------------------------------------------------------------------

/// `lower_file_names` の 3 状態を呼び出し側へ見せる形。記憶域は [`LowerFileColumn`] が持つ。
///
/// **`crate::indexer::LowerFileName` の借用版である。** あちらが `Text(String)` を持つのに対し
/// こちらは `Text(&str)` で、意味は 1 対 1 に対応する。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LowerFileSlot<'a> {
    /// file name 成分が無い（`query::lower_file_name` が `None` を返した）。
    Absent,
    /// 解決後の `lower_name` とバイト一致する。
    SameAsLowerName,
    /// 独自の文字列。
    Text(&'a str),
}

/// 線上へ書くときの形。**variant の並びを `crate::indexer::LowerFileName` と一致させること**
/// ——postcard は variant を**添字**で書くので、並べ替えると線上表現が黙って変わる（版は
/// 同じまま読めなくなるので、症状は破損ではなく cache-miss ＝ 22〜30 秒の全走査になる）。
/// 一致は `lower_file_column_wire_format_is_identical_to_vec_of_lower_file_name` が固定する。
#[derive(Serialize)]
enum LowerFileNameRef<'a> {
    Absent,
    SameAsLowerName,
    Text(&'a str),
}

const LOWER_FILE_VARIANTS: &[&str] = &["Absent", "SameAsLowerName", "Text"];

/// `lower_file_names` の列。
///
/// **3 状態を 2 本の旗で持つ。** `strings` の present が `Text`、`same_as_lower` が
/// `SameAsLowerName`、どちらも立たないのが `Absent` である。**1 本へ混ぜてはならない**——
/// 「無い」と「同一」は別の状態であり、混ぜると `entry_view` が file name 成分を持たない
/// エントリに対して `lower_name` を返す（`crate::indexer::LowerFileName` の doc が正本）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LowerFileColumn {
    strings: OptionalStrArena,
    same_as_lower: Bits,
}

impl LowerFileColumn {
    pub(crate) fn with_capacity(n: usize) -> Self {
        Self {
            strings: OptionalStrArena::with_capacity(n, 0),
            same_as_lower: Bits::with_capacity(n),
        }
    }

    /// 連結バイト列（存在判定専用・[`OptionalStrArena::blob`] の doc が正本）。
    ///
    /// **`SameAsLowerName` の実体はここに無い**（`lower_names` 側に在る）。存在判定の
    /// 呼び出し側は両方の列を舐めること。
    pub(crate) fn blob(&self) -> &str {
        self.strings.blob()
    }

    pub(crate) fn len(&self) -> usize {
        self.strings.len()
    }

    /// `i` 番が持つ独自の文字列（`Text` なら中身、他は `None`）。
    ///
    /// **索引のホットパスはこちらを使う**（`SearchEngine::entry_view`）——メモリ側の列に
    /// `SameAsLowerName` は残らない（`assemble` が `CompactEntry` の旗へ移す）ので、
    /// 3 状態を組み立て直す必要が無く、旗の側を読まずに済む。
    #[inline]
    pub(crate) fn text_at(&self, i: usize) -> Option<&str> {
        self.strings.get(i)
    }

    /// `i` 番の 3 状態。
    #[inline]
    pub(crate) fn get(&self, i: usize) -> LowerFileSlot<'_> {
        match self.strings.get(i) {
            Some(s) => LowerFileSlot::Text(s),
            None if self.same_as_lower.get(i) => LowerFileSlot::SameAsLowerName,
            None => LowerFileSlot::Absent,
        }
    }

    pub(crate) fn push(&mut self, slot: LowerFileSlot<'_>) {
        match slot {
            LowerFileSlot::Absent => {
                self.strings.push(None);
                self.same_as_lower.push(false);
            }
            LowerFileSlot::SameAsLowerName => {
                self.strings.push(None);
                self.same_as_lower.push(true);
            }
            LowerFileSlot::Text(s) => {
                self.strings.push(Some(s));
                self.same_as_lower.push(false);
            }
        }
    }

    pub(crate) fn shrink_to_fit(&mut self) {
        self.strings.shrink_to_fit();
        self.same_as_lower.shrink_to_fit();
    }

    /// 先頭から順に走る。
    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = LowerFileSlot<'_>> {
        (0..self.len()).map(|i| self.get(i))
    }

    /// 独自の文字列を持つ要素の数（`index.bin` の内訳計器が使う）。
    pub(crate) fn count_text(&self) -> usize {
        self.iter()
            .filter(|s| matches!(s, LowerFileSlot::Text(_)))
            .count()
    }

    /// 常駐ヒープの内訳を数えるための素（計測専用）。`(blob, offsets, present, same_as_lower)`。
    ///
    /// **旗 2 本を足して 1 つで返してはならない。** 呼び出し側（`search/footprint.rs` の
    /// `arena_part`）は「1 行 = 1 確保」でブロックを数えるので、束ねた瞬間に 1 つ数え落とす
    /// ——**未帰属 +1 blocks として実測に出た**（2026-08-12。バイトは合っていたので、
    /// バイトだけを見る検算では捕まらない）。
    pub(crate) fn footprint_bytes(&self) -> (usize, usize, usize, usize) {
        let (blob, offsets, present) = self.strings.footprint_bytes();
        (blob, offsets, present, self.same_as_lower.footprint_bytes())
    }

    /// 余剰容量（検知器専用・[`OptionalStrArena::excess_capacity_bytes`]）。
    #[cfg(test)]
    pub(crate) fn excess_capacity_bytes(&self) -> usize {
        self.strings.excess_capacity_bytes() + self.same_as_lower.excess_capacity_bytes()
    }
}

/// 列を 1 行で組む口（テストの fixture と、要素を手に持っている呼び出し側のため）。
impl<'a> FromIterator<LowerFileSlot<'a>> for LowerFileColumn {
    fn from_iter<I: IntoIterator<Item = LowerFileSlot<'a>>>(it: I) -> Self {
        let it = it.into_iter();
        let mut col = Self::with_capacity(it.size_hint().0);
        for s in it {
            col.push(s);
        }
        col
    }
}

impl Serialize for LowerFileColumn {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let mut seq = ser.serialize_seq(Some(self.len()))?;
        for i in 0..self.len() {
            let e = match self.get(i) {
                LowerFileSlot::Absent => LowerFileNameRef::Absent,
                LowerFileSlot::SameAsLowerName => LowerFileNameRef::SameAsLowerName,
                LowerFileSlot::Text(s) => LowerFileNameRef::Text(s),
            };
            seq.serialize_element(&e)?;
        }
        seq.end()
    }
}

/// variant の添字だけを読む種（serde derive が生成するものと同じ形）。
enum LowerFileTag {
    Absent,
    SameAsLowerName,
    Text,
}

impl<'de> Deserialize<'de> for LowerFileTag {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct TagVisitor;
        impl Visitor<'_> for TagVisitor {
            type Value = LowerFileTag;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a LowerFileName variant")
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<LowerFileTag, E> {
                match v {
                    0 => Ok(LowerFileTag::Absent),
                    1 => Ok(LowerFileTag::SameAsLowerName),
                    2 => Ok(LowerFileTag::Text),
                    _ => Err(E::invalid_value(
                        serde::de::Unexpected::Unsigned(v),
                        &"a LowerFileName variant index (0..=2)",
                    )),
                }
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<LowerFileTag, E> {
                match v {
                    "Absent" => Ok(LowerFileTag::Absent),
                    "SameAsLowerName" => Ok(LowerFileTag::SameAsLowerName),
                    "Text" => Ok(LowerFileTag::Text),
                    _ => Err(E::unknown_variant(v, LOWER_FILE_VARIANTS)),
                }
            }
        }
        de.deserialize_identifier(TagVisitor)
    }
}

/// 1 要素（`LowerFileName`）を列へ流し込む種。
struct LowerFileSink<'a>(&'a mut LowerFileColumn);

impl<'de> DeserializeSeed<'de> for LowerFileSink<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_enum("LowerFileName", LOWER_FILE_VARIANTS, self)
    }
}

impl<'de> Visitor<'de> for LowerFileSink<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a LowerFileName")
    }
    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<(), A::Error> {
        let col = self.0;
        let (tag, variant) = data.variant::<LowerFileTag>()?;
        match tag {
            LowerFileTag::Absent => {
                variant.unit_variant()?;
                col.strings.finish_element(false);
                col.same_as_lower.push(false);
            }
            LowerFileTag::SameAsLowerName => {
                variant.unit_variant()?;
                col.strings.finish_element(false);
                col.same_as_lower.push(true);
            }
            LowerFileTag::Text => {
                variant.newtype_variant_seed(StrSink(&mut col.strings.blob))?;
                col.strings.finish_element(true);
                col.same_as_lower.push(false);
            }
        }
        Ok(())
    }
}

struct LowerFileColumnVisitor;

impl<'de> Visitor<'de> for LowerFileColumnVisitor {
    type Value = LowerFileColumn;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a sequence of LowerFileName")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<LowerFileColumn, A::Error> {
        let mut col = LowerFileColumn::with_capacity(capped_capacity(&seq));
        while seq.next_element_seed(LowerFileSink(&mut col))?.is_some() {}
        Ok(col)
    }
}

impl<'de> Deserialize<'de> for LowerFileColumn {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        de.deserialize_seq(LowerFileColumnVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::LowerFileName;

    /// 名前の形を混ぜた fixture。**痩せさせてはならない**——空文字・非 ASCII・末尾空白・
    /// 区切りのどれが欠けても、そこに現れるずれだけが素通りする（golden bytes が
    /// 役に立たないのと同じ理由）。
    const NAMES: [Option<&str>; 8] = [
        Some("projects"),
        None,
        Some(""),
        Some("ünïcode 名前"),
        None,
        Some("アプリ"),
        Some("trailing space "),
        Some("c:\\dir\\sub"),
    ];

    fn name_column() -> LowerNameColumn {
        let mut c = LowerNameColumn::default();
        for s in NAMES {
            c.push(s);
        }
        c
    }

    /// **線上表現が `Vec<Option<String>>` と 1 バイトも違わない。**
    ///
    /// これが `INDEX_CACHE_VERSION` を上げずに `IndexCache.lower_names` の型を替えられた
    /// 根拠であり、破れると**全ユーザーの `index.bin` が黙って読めなくなる**（版が同じまま
    /// 形式だけ変わるので、フォールバックの鎖はどの枝も拾わず cache-miss ＝ 全走査へ落ちる）。
    ///
    /// **両向きを見る。** 書いた側が一致することと、`Vec<Option<String>>` として書かれた
    /// 旧バイト列を読み戻せることは別の主張である。
    #[test]
    fn lower_name_column_wire_format_is_identical_to_vec_of_option_string() {
        let owned: Vec<Option<String>> = NAMES.iter().map(|s| s.map(str::to_string)).collect();

        let via_vec = postcard::to_allocvec(&owned).expect("Vec<Option<String>> を書ける");
        let via_col = postcard::to_allocvec(&name_column()).expect("列を書ける");
        assert_eq!(
            via_vec, via_col,
            "線上表現が `Vec<Option<String>>` とずれた（版を上げずに型を替えた前提が崩れる）"
        );

        let back: LowerNameColumn = postcard::from_bytes(&via_vec).expect("旧バイト列を読める");
        assert_eq!(back.len(), NAMES.len());
        for (i, want) in NAMES.iter().enumerate() {
            assert_eq!(back.get(i), *want, "{i} 番の切り出しがずれた");
        }
    }

    /// **`None` と `Some("")` は別の状態である。**
    ///
    /// 旗を持たず「長さ 0 のスライス」で `None` を表す実装にすると、この 2 つが同じに見える
    /// ——`lower_names` では「表示名と同一」と「小文字化したら空になった」が混ざり、
    /// `entry_view` が別の文字列を返す。実データにこの形は無いが、それは indexer の名前導出
    /// 規則の帰結であって列の性質ではない。
    #[test]
    fn absent_and_empty_string_are_distinct() {
        let mut c = LowerNameColumn::default();
        c.push(None);
        c.push(Some(""));
        assert_eq!(c.get(0), None);
        assert_eq!(c.get(1), Some(""));

        let back: LowerNameColumn =
            postcard::from_bytes(&postcard::to_allocvec(&c).unwrap()).expect("往復できる");
        assert_eq!(back.get(0), None, "往復で `None` が空文字へ化けた");
        assert_eq!(back.get(1), Some(""), "往復で空文字が `None` へ化けた");
    }

    /// **3 状態すべてに加えて、`Text` の中身の形も混ぜる。** variant のタグだけを網羅した
    /// fixture では、タグの並べ替えは捕まえられても**文字列の中身をいじる変異**（`trim_end`
    /// 等）が素通りする——`Absent` / `SameAsLowerName` / `Text("x")` だけの fixture でそれを
    /// 実測した（2026-08-12）。末尾空白と空文字を持たせるのはそのためであり、
    /// **痩せさせてはならない。**
    const FILES: [LowerFileSlot<'static>; 8] = [
        LowerFileSlot::Absent,
        LowerFileSlot::SameAsLowerName,
        LowerFileSlot::Text("tool.exe"),
        LowerFileSlot::Absent,
        LowerFileSlot::Text("アプリ.lnk"),
        LowerFileSlot::SameAsLowerName,
        LowerFileSlot::Text("trailing space "),
        LowerFileSlot::Text(""),
    ];

    fn file_column() -> LowerFileColumn {
        let mut c = LowerFileColumn::default();
        for s in FILES {
            c.push(s);
        }
        c
    }

    /// **線上表現が `Vec<LowerFileName>` と 1 バイトも違わない。**
    ///
    /// postcard は variant を添字で書くので、[`LowerFileNameRef`] の並びが本体とずれた瞬間に
    /// 破れる。**3 variant すべてを混ぜて持つ**のは、`Text` だけの fixture では並べ替えを
    /// 検出できないためである。
    #[test]
    fn lower_file_column_wire_format_is_identical_to_vec_of_lower_file_name() {
        let owned: Vec<LowerFileName> = FILES
            .iter()
            .map(|s| match s {
                LowerFileSlot::Absent => LowerFileName::Absent,
                LowerFileSlot::SameAsLowerName => LowerFileName::SameAsLowerName,
                LowerFileSlot::Text(t) => LowerFileName::Text((*t).to_string()),
            })
            .collect();

        let via_vec = postcard::to_allocvec(&owned).expect("Vec<LowerFileName> を書ける");
        let via_col = postcard::to_allocvec(&file_column()).expect("列を書ける");
        assert_eq!(
            via_vec, via_col,
            "線上表現が `Vec<LowerFileName>` とずれた（variant の並びを疑うこと）"
        );

        let back: LowerFileColumn = postcard::from_bytes(&via_vec).expect("旧バイト列を読める");
        assert_eq!(back.len(), FILES.len());
        for (i, want) in FILES.iter().enumerate() {
            assert_eq!(back.get(i), *want, "{i} 番の状態がずれた");
        }
    }

    /// 空の列（初回起動）も往復し、`Default` が番兵を持つ形になる。
    #[test]
    fn empty_columns_roundtrip_and_report_zero_len() {
        assert_eq!(LowerNameColumn::default().len(), 0);
        assert_eq!(LowerFileColumn::default().len(), 0);

        let names = postcard::to_allocvec(&LowerNameColumn::default()).unwrap();
        assert_eq!(
            names,
            postcard::to_allocvec(&Vec::<Option<String>>::new()).unwrap()
        );
        assert_eq!(
            postcard::from_bytes::<LowerNameColumn>(&names)
                .unwrap()
                .len(),
            0
        );

        let files = postcard::to_allocvec(&LowerFileColumn::default()).unwrap();
        assert_eq!(
            files,
            postcard::to_allocvec(&Vec::<LowerFileName>::new()).unwrap()
        );
        assert_eq!(
            postcard::from_bytes::<LowerFileColumn>(&files)
                .unwrap()
                .len(),
            0
        );
    }

    /// **壊れた長さプレフィックスは、巨大確保ではなく `Err` で落ちる。**
    ///
    /// 化けた 5 バイトの varint は 40 億件を名乗れる。上限を外すと 16 GiB の確保を試み、
    /// release は `panic="abort"` ゆえ**起動そのものが落ちる**（検索結果が変わる形ではないので
    /// 挙動テストでは捕まらない）。
    #[test]
    fn corrupt_length_prefix_fails_instead_of_preallocating() {
        let hostile = [0xFF, 0xFF, 0xFF, 0xFF, 0x0F];
        assert!(
            postcard::from_bytes::<LowerNameColumn>(&hostile).is_err(),
            "中身の無い巨大な列を受理してはならない（lower_names）"
        );
        assert!(
            postcard::from_bytes::<LowerFileColumn>(&hostile).is_err(),
            "中身の無い巨大な列を受理してはならない（lower_file_names）"
        );
    }

    /// 未知の variant 添字は `Err`（黙って `Absent` へ落とさない）。
    #[test]
    fn unknown_lower_file_variant_is_rejected() {
        // 要素 1 件・variant 添字 3（存在しない）。
        let bytes = [0x01u8, 0x03];
        assert!(postcard::from_bytes::<LowerFileColumn>(&bytes).is_err());
    }
}
