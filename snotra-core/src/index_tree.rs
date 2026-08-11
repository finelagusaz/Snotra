//! `target_path` をフォルダ木の接頭辞共有で表す（オンディスクと索引が共有する表現）。
//!
//! 索引の 8 割はフォルダで、パスは親子で接頭辞を共有する。`indexer` の名前導出規則
//! （folder は `file_name()` / file は `file_stem()`）ゆえ、**フォルダの末尾成分は `name`
//! そのもの**であり、ファイルは `name` + 拡張子で組み直せる。親の index と拡張子 id だけを
//! 持てば、フルパスの文字列は親を持たないエントリのぶんしか要らない。
//!
//! # なぜ `search` でも `indexer` でもなくここか
//!
//! この表現は**両者が共有する**。`indexer` は `index.bin` へ書き、読み、PATH スキャンの篩に
//! 使う。`search` は [`crate::search`] の索引本体としてこれを抱える。どちらか一方へ置くと
//! 依存が逆流するので、両者が依存する側へ出してある。
//!
//! # 記憶域の並べ方は 2 通り、辿る規則は 1 つ
//!
//! [`IndexTree`] は並列 Vec（SoA）で持ち、索引側の `PathStore` は 1 件 12 B の構造体の列
//! （AoS）で持つ。**並べ方が違うのは意図的である**——索引は木を辿る段で `parent` と `aux` を
//! 同じ段で読むため、同じキャッシュラインに載るほうがミスが少ない。一方ディスクの形は
//! 並列 Vec のほうが小さい（`Vec<u32>` は postcard の varint で詰まる）。
//!
//! **表示名だけは並べ方が 1 つである。** [`NameArena`] は連結バイト列 + オフセットであり、
//! ディスクも索引もこの同じ物体を持つ——`PathStore::adopt` は写さずに move する。
//!
//! **辿る規則そのものを 2 回書いてはならない。** [`TreeNodes`] が「i 番の親・aux・名前を
//! どう取るか」だけを抽象化し、[`walk_to_root`] と [`raw_path_into`] は両方の並べ方から
//! 同じ 1 つの実装が使われる（単型化されるので索引側の速さは落ちない）。
//!
//! **射程を書く。** 構造が 1 つに保っているのは**原文の組み立て**である——索引側の正規化
//! （`PathStore::normalized_into` と `PathCursor`）は、巻き戻しを効かせるために別の走査を持つ。
//! 両者をまたいで共有されるのは [`push_separator`] の 1 行だけであり、そこが要石ゆえ
//! 関数として括り出してある。セグメントの変換（trim・小文字化）は依然として 2 か所にある。

use std::fmt;

use rayon::prelude::*;
use serde::de::{DeserializeSeed, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::indexer::AppEntry;

/// 親を持たないことを表す番兵。このとき `aux` は [`IndexTree::table`] のフルパスを指す。
pub(crate) const NO_PARENT: u32 = u32::MAX;

/// 祖先の鎖の長さの上限。**構築時に強制する**（[`IndexTree::build`]）ので、組み立て側は
/// 鎖がこれを超えないことを前提にしてよい。
///
/// 上限を組み立て側の打ち切りで実現してはならない——打ち切ると「根でないもの」を根として
/// 扱い、`table[aux]`（拡張子）をパスの先頭として書いてしまう。**短いパスではなく誤った
/// パスになり、しかも黙って通る。** 構築時に上限超過を根へ落とせば（＝自分のフルパスを
/// 持たせれば）、その状態は表現できなくなる。実データの最大深さは 17 段。
pub(crate) const CHAIN_CAP: usize = 64;

/// 表示名を 1 本のバイト列へ連結して持つ（要素ごとの確保を持たない）。
///
/// **1 エントリ 1 確保をやめるための表現である。** `Vec<String>` は 31 万件で 31 万ブロック
/// を占め、ロード段の確保の 76%・常駐ブロックの 76% がここだった（実測は `PERFORMANCE.md`
/// 「採用: 表示名を文字列アリーナで持つ」）。バイト数はほぼ同じでも、**ブロック数はアロケータ
/// 由来の税を決める**——反復 4・5 が観測した「共有 1 反復あたり構築 +17 ms」の傾きは確保・
/// 解放の回数に比例していた。
///
/// # 線上表現は `Vec<String>` と 1 バイトも違わない
///
/// [`Serialize`] は `seq of str` を書き、[`Deserialize`] は同じ列を**要素ごとの `String` を
/// 作らずに** `blob` へ流し込む（[`StrSink`]）。ゆえに `index.bin` の形式は変わっておらず、
/// **`INDEX_CACHE_VERSION` のバンプも旧版フォールバックも要らない**。
///
/// **この不変を固定するのは `arena_wire_format_is_identical_to_vec_of_string` である。**
/// `indexer` の `index_cache_on_disk_format_is_stable`（golden bytes）**では足りない**——
/// あちらの fixture が持つ名前の形しか通らないので、その形に現れないずれは素通りする
/// （実測: `serialize` に `trim_end` を挟む変異は golden を通り、上のテストだけが落ちた）。
/// ゆえに**名前の形を混ぜて持つのは上のテストの責務**であり、fixture を痩せさせてはならない。
///
/// # 不変条件は検査ではなく構成から従う
///
/// `offsets[0] == 0` / 単調非減少 / `offsets.last() == blob.len()` / 各オフセットが UTF-8 の
/// 文字境界であること——どれも「`&str` を丸ごと押し込む」形からしか作れないので、
/// **壊れた `index.bin` でも表現できない**（postcard が str の UTF-8 を検証する）。
/// [`IndexTree::from_parts`] が確かめるのは列の**長さ**の一致だけで足りる。
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct NameArena {
    /// 全要素を連結したバイト列。
    blob: String,
    /// 要素 `i` は `blob[offsets[i]..offsets[i + 1]]`。**末尾に番兵を持つ**ので長さは要素数 + 1
    /// であり、空でも `[0]` が入る（[`Default`] もその形になる）。
    ///
    /// `u32` ゆえ `blob` は 4 GiB までである。溢れた場合は切り詰めでオフセットが単調でなく
    /// なり、[`Self::get`] のスライスが**その場で panic する**——誤った名前を静かに返す形には
    /// ならない。実データの名前は合計 10 MiB である。
    offsets: Vec<u32>,
}

impl NameArena {
    /// 要素を 1 つも持たないアリーナ。
    pub(crate) fn new() -> Self {
        Self {
            blob: String::new(),
            offsets: vec![0],
        }
    }

    /// `n` 要素・合計 `bytes` バイトを見込んで確保する。
    ///
    /// **見込みは `build` が 1 パスで数える。** 伸長に任せると 10 MiB の `blob` が倍々で
    /// 移し替えられ、消したはずの memcpy がそこへ戻る。
    fn with_capacity(n: usize, bytes: usize) -> Self {
        let mut offsets = Vec::with_capacity(n + 1);
        offsets.push(0);
        Self {
            blob: String::with_capacity(bytes),
            offsets,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    /// `i` 番の名前。
    ///
    /// **境界検査は 2 つのスライスが担う**（`offsets[i]` の添字と `blob` の範囲）ので、
    /// 範囲外は panic であって黙った空文字ではない。
    ///
    /// **`get_unchecked` にしても速くならない**（実測）。`String` の範囲添字は両端で UTF-8 の
    /// 文字境界を検査するので、そこがパスクエリの約 5% の出所ではないかと疑って測ったが、
    /// `c:\users` の p50 は 11,905–12,364 µs（安全版 11,760–12,226）で main と重ならないまま、
    /// ばらつきだけが広がった。**ここに `unsafe` を置く理由は無い。**
    ///
    /// 退行の機序そのものは切り分けてある——効いているのは [`raw_path_into`] と
    /// `PathCursor::append` が通る**全件走査の組み立て**であり、`entry_view` ではない
    /// （読み口を差し替える変異 2 つで判定した。表は `PERFORMANCE.md`「採用: 表示名を
    /// 文字列アリーナで持つ」）。**緩和は削減と排他である**——名前を複製して持てば戻るが、
    /// それは 312,108 個の確保を戻すことに等しい。
    #[inline]
    pub(crate) fn get(&self, i: usize) -> &str {
        &self.blob[self.offsets[i] as usize..self.offsets[i + 1] as usize]
    }

    /// 末尾へ 1 件足す（[`IndexTree::build`] と [`IndexTree::extend_with_roots`] が使う）。
    fn push(&mut self, s: &str) {
        self.blob.push_str(s);
        self.offsets.push(self.blob.len() as u32);
    }

    /// **`PathStore` の `shrink_to_fit` と対で呼ぶ**——余剰容量は索引が伸長しないぶん
    /// 最後まで常駐する（理由は `search/build.rs` の `assemble` の doc）。
    pub(crate) fn shrink_to_fit(&mut self) {
        self.blob.shrink_to_fit();
        self.offsets.shrink_to_fit();
    }

    /// 常駐ヒープの内訳を数えるための素（計測専用・勘定の規約は `search/footprint.rs`）。
    /// 返すのは `(blob の確保バイト, offsets の確保バイト)`。
    pub(crate) fn footprint_bytes(&self) -> (usize, usize) {
        (
            self.blob.capacity(),
            self.offsets.capacity() * std::mem::size_of::<u32>(),
        )
    }
}

impl fmt::Debug for NameArena {
    /// **中身を並べない。** 31 万件を展開すると失敗時の出力がターミナルを埋める。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NameArena")
            .field("len", &self.len())
            .field("bytes", &self.blob.len())
            .finish()
    }
}

impl Serialize for NameArena {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let mut seq = ser.serialize_seq(Some(self.len()))?;
        for i in 0..self.len() {
            seq.serialize_element(self.get(i))?;
        }
        seq.end()
    }
}

/// 1 要素ぶんを、`String` を作らずに `blob` へ直接流し込む種。
///
/// **`next_element::<String>()` にしてはならない**——それでは要素ごとの確保が戻り、この型が
/// 存在する理由がなくなる。postcard は借用した `&str` を渡すので写しは 1 回の `push_str` で済む。
struct StrSink<'a>(&'a mut String);

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

struct NameArenaVisitor;

impl<'de> Visitor<'de> for NameArenaVisitor {
    type Value = NameArena;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a sequence of strings")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<NameArena, A::Error> {
        // **`size_hint` を素で信じてはならない。** これは壊れた `index.bin` が名乗る長さで
        // あり、4 バイトの化けた varint が 40 億件を名乗りうる。serde の `Vec` 訪問子は
        // まさにこの理由で事前確保を 1 MiB 相当で頭打ちにしており（`search/build.rs` の
        // `assemble` の doc がその性質を別の角度から記録している）、**自前の訪問子はその
        // 保護を自分で持たない限り迂回する**。迂回すると、これまで `Err` → cache-miss →
        // 再走査へ穏やかに落ちていた壊れ方が、確保失敗＝ release では `panic="abort"` に化ける。
        //
        // 実データは 312,108 件で上限を超えるが、超過分は `Vec` の伸長が拾う（1 回の再確保）。
        const MAX_PREALLOC_BYTES: usize = 1024 * 1024;
        let cap = seq
            .size_hint()
            .unwrap_or(0)
            .min(MAX_PREALLOC_BYTES / std::mem::size_of::<u32>());
        // 合計バイト数は事前に分からないので `blob` は伸長に任せる（列そのものは
        // 一度きりのロードであり、`build` のように毎回払う区間ではない）。
        let mut arena = NameArena::with_capacity(cap, 0);
        while seq.next_element_seed(StrSink(&mut arena.blob))?.is_some() {
            arena.offsets.push(arena.blob.len() as u32);
        }
        Ok(arena)
    }
}

impl<'de> Deserialize<'de> for NameArena {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        de.deserialize_seq(NameArenaVisitor)
    }
}

/// [`IndexTree`] の列を借りて見る口（保存側が使う）。
///
/// **フィールドに名前を付ける。** `parent` と `aux` はどちらも `Vec<u32>` なので、タプルで
/// 返すと**取り違えてもコンパイルが通る**——そして取り違えた木は panic せず、パスの組み立てが
/// 静かに壊れる（`CachedMasks` を組で束ねているのと同じ理屈）。
pub(crate) struct TreeColumns<'a> {
    pub(crate) names: &'a NameArena,
    pub(crate) is_folder: &'a [bool],
    pub(crate) parent: &'a [u32],
    pub(crate) aux: &'a [u32],
    pub(crate) table: &'a [String],
    pub(crate) sorted_by_path: bool,
}

/// [`IndexTree`] を列へほどいた形（索引側が組み替えるときに使う）。
///
/// **名前を付ける理由は [`TreeColumns`] と同じである。**
pub(crate) struct OwnedTreeColumns {
    pub(crate) names: NameArena,
    pub(crate) is_folder: Vec<bool>,
    pub(crate) parent: Vec<u32>,
    pub(crate) aux: Vec<u32>,
    pub(crate) table: Vec<String>,
    pub(crate) sorted_by_path: bool,
}

/// 木の 1 ノードを読む口。**記憶域の並べ方だけを抽象化する**——規則は [`raw_path_into`] が
/// 唯一持つ。
///
/// 実装は 2 つある（[`IndexTree`] の並列 Vec と、索引側 `PathStore` の構造体の列）。
/// ジェネリックゆえ呼び出しごとに単型化され、索引のホットパスでは今までどおり
/// 構造体のフィールドを直接読む形にコンパイルされる。
pub(crate) trait TreeNodes {
    /// `i` の親の index。[`NO_PARENT`] なら親を持たない。
    ///
    /// **必ず `i` より小さい**——構築時に弾いており、循環は表現できない。深さが有限である
    /// ことがこの不変条件だけで従うので、[`walk_to_root`] は必ず停止する。
    fn parent_of(&self, i: usize) -> u32;
    /// `i` の `table` 添字。親を持つときは拡張子（folder と拡張子なし file は 0 = 空文字）、
    /// 親を持たないときはフルパス。**どちらを指すかは `parent_of` が決める。**
    fn aux_of(&self, i: usize) -> u32;
    /// `i` の表示名（`AppEntry.name` そのもの）。
    fn name_of(&self, i: usize) -> &str;
    /// intern 表の `id` 番。
    fn table_str(&self, id: u32) -> &str;
}

/// 根まで辿って途中の index を積む。返すのは（根, 鎖, 段数）。`chain[0]` が `i` 自身、
/// `chain[depth - 1]` が根の直下である。
///
/// **鎖が配列に収まることは構築時に保証されている**（[`CHAIN_CAP`]）。`parent < self` が
/// 循環を、深さの打ち止めが長さを、それぞれ表現不能にしているので、ここに打ち切りの
/// 分岐は要らない——`debug_assert` は不変条件が壊れたときに黙って誤ったパスを返す
/// のではなく落ちるためにある。
#[inline]
pub(crate) fn walk_to_root<N: TreeNodes + ?Sized>(
    nodes: &N,
    i: usize,
) -> (usize, [u32; CHAIN_CAP], usize) {
    let mut chain = [0u32; CHAIN_CAP];
    let mut depth = 0usize;
    let mut cur = i;
    while nodes.parent_of(cur) != NO_PARENT {
        debug_assert!(
            depth < CHAIN_CAP,
            "鎖が CHAIN_CAP を超えた（構築側の不変条件違反）"
        );
        chain[depth] = cur as u32;
        depth += 1;
        cur = nodes.parent_of(cur) as usize;
    }
    (cur, chain, depth)
}

/// 直前の段が区切りで終わっていなければ `\` を足す。
///
/// **この 1 行が、組み立ての向き（原文 / 正規化キー）をまたいで共有される唯一の規則である。**
/// 原文側（[`raw_path_into`]）と索引側の正規化（`PathStore::normalized_into` と `PathCursor`）は
/// 走査の形が違うが、区切りの入れ方だけは一致していなければならない——ずれても「短いパス」では
/// なく「区切りが 1 つ多い／少ないパス」になり、表示パス・tie-break・履歴照合が
/// **正しく見えるまま**すれる。
///
/// 見るのは `\` の 1 バイトだけである（`/` は見ない）。正規化側のバッファは `/` → `\` 変換後で
/// あり、原文側の親がドライブ直下（`C:\`）なら既に `\` で終わっている。
#[inline]
pub(crate) fn push_separator(buf: &mut String) {
    if buf.as_bytes().last() != Some(&b'\\') {
        buf.push('\\');
    }
}

/// `i` のフルパスを**原文のまま**組み立てる（小文字化も `/` → `\` 変換も trim もしない）。
///
/// 結果は元の `AppEntry.target_path` とバイト一致する。**この一致が木表現の要石である**
/// ——`target_path` をディスクにも索引にも持たない以上、原文を要求する消費者（tie-break の
/// `cmp_paths`・`SearchResult.path`）はすべてここの出力を受け取る。1 バイトずれても
/// **それらしい**パスが返るので挙動テストでは捕まらない。合成 fixture での保証は
/// `search/tests/path.rs` の `path_store_cursor_matches_full_rebuild` が常に持ち、実データの
/// 全件照合は同ファイルの `path_store_raw_matches_target_path_over_real_index` が受け持つ
/// ——**後者は `#[ignore]` で、原文をファイルシステムの走査から取る**（`index.bin` から
/// 取ると組み直し同士の比較になり、どれだけ壊れていても落ちない）。開発機で明示的に走らせる
/// corpus であって保証ではない。
///
/// **CI で走る接地は `indexer` の `index_tree_raw_matches_frozen_v6_specimen` だけである**
/// ——v6 の凍結バイト列（木を 1 度も通っていない原文）と組み直しを突き合わせる。合成の
/// 標本 1 つぶんであり、実データ規模のカバレッジは上の `#[ignore]` 2 本が持つ。
#[inline]
pub(crate) fn raw_path_into<N: TreeNodes + ?Sized>(nodes: &N, buf: &mut String, i: usize) {
    let (root, chain, depth) = walk_to_root(nodes, i);
    buf.clear();
    buf.push_str(nodes.table_str(nodes.aux_of(root)));
    for d in (0..depth).rev() {
        let idx = chain[d] as usize;
        push_separator(buf);
        buf.push_str(nodes.name_of(idx));
        // 空文字（folder・拡張子なし file）の `push_str` は no-op ゆえ分岐を置かない。
        buf.push_str(nodes.table_str(nodes.aux_of(idx)));
    }
}

/// 木の並列 Vec 表現。`index.bin` が持つ形であり、索引側はこれを受け取って組み替える。
///
/// **フィールドは private である。** 列を外から見る口は [`Self::columns`]（借用・保存側が
/// `Cow::Borrowed` で全件 clone を避けるため）と [`Self::into_columns`]（所有・`PathStore` が
/// 組み替えるため）の 2 つだけで、**組む口は [`Self::from_parts`] / [`Self::build`] /
/// [`Self::empty`] しかない**。
///
/// **これは額ではなく構造の変更である。** 以前は `pub(crate)` ゆえ、crate 内から構造体
/// リテラルで木を組めた——つまり `from_parts` の検証（列の長さ・`aux` の範囲・`parent < i`・
/// 深さの上限）を**迂回した木が表現できた**。迂回した木の帰結は `from_parts` の doc が
/// 4 つ挙げているが、どれも「索引が黙って短くなる」「組み立てが止まらない」の側で、
/// 検索結果は正しく見える。**表現できなくすればチェックリストが要らなくなる。**
pub struct IndexTree {
    /// 表示名。`AppEntry.name` をそのまま移す（**要素ごとの確保を持たない**・[`NameArena`]）。
    names: NameArena,
    is_folder: Vec<bool>,
    /// 各要素の意味は [`TreeNodes::parent_of`]。
    parent: Vec<u32>,
    /// 各要素の意味は [`TreeNodes::aux_of`]。
    aux: Vec<u32>,
    /// 拡張子とフルパスを同居させた intern 表（0 番は空文字で固定）。
    ///
    /// 同居させるのは、組み立ての最後の 1 段（根）が必ずここを引くためである。
    /// 別テーブルにして二分探索させると、**全エントリの組み立てにその探索が乗る**。
    table: Vec<String>,
    /// フルパスのバイト順に**狭義の単調増加**で並んでいるか。索引側の tie-break が
    /// 「組み立てずに index を比べる」高速路へ入れるかを決める。
    ///
    /// **仮定ではなく実測である。** 測れなかった経路では偽を入れること——偽は
    /// 組み立てて比べる遅い経路へ落ちるだけで、結果は変わらない。
    pub(crate) sorted_by_path: bool,
}

impl TreeNodes for IndexTree {
    #[inline]
    fn parent_of(&self, i: usize) -> u32 {
        self.parent[i]
    }
    #[inline]
    fn aux_of(&self, i: usize) -> u32 {
        self.aux[i]
    }
    #[inline]
    fn name_of(&self, i: usize) -> &str {
        self.names.get(i)
    }
    #[inline]
    fn table_str(&self, id: u32) -> &str {
        &self.table[id as usize]
    }
}

impl IndexTree {
    /// ディスクから読んだ 5 本の列を、不変条件を**確かめてから**木にする。
    ///
    /// **確かめずに組んではならない。** 列は `index.bin` から独立に読まれるので、壊れた
    /// ファイルは長さの揃わない列や範囲外の添字を与えうる。それぞれの帰結は:
    ///
    /// - 長さが揃わない → 組み替えの `zip` が短い側で黙って止まり、**索引が静かに短くなる**
    /// - `aux` が `table` の外 → 組み立てが範囲外で panic（release は `panic="abort"`）
    /// - `parent >= i` → [`walk_to_root`] の停止性が崩れる（`parent < i` が唯一の根拠である）
    /// - 深さが [`CHAIN_CAP`] 以上 → `chain` 配列の範囲外
    ///
    /// どれも「読めたが壊れている」形なので、`None` を返して呼び出し側を**再走査へ落とす**。
    /// 深さの検算は `parent < i` のおかげで 1 パスで済む（親の深さは必ず先に確定している）。
    ///
    /// **`sorted_by_path` だけは検証しない。意図的な非対称であり、被害範囲はここに書く。**
    /// 確かめるにはフルパスを全件組み直して比べるしかなく（実測 並列 4.9 ms / 逐次 24.9 ms）、
    /// それは**この旗が存在する理由そのもの**——組み立てずに済ませること——を打ち消す。
    /// 誤って `true` が入ったときの帰結は他の 4 つと違って局所的である: `cmp_paths` が
    /// index 比較の高速路へ入り、**同スコアのエントリどうしの tie-break だけ**がパス順でなく
    /// 格納順になる。エントリは消えず、panic も起きず、順序は安定している。
    /// 上の 4 つ（索引が短くなる・添字 panic・停止しない）とは桁が違うので、全件の組み直しを
    /// 毎起動払う側には倒さない。
    ///
    /// **`pub(crate)` である**——受け取る [`NameArena`] が crate の内側の表現であり、
    /// `index.bin` を読む経路（`indexer::load_cache_in`）以外に木を列から組む入口は無い。
    pub(crate) fn from_parts(
        names: NameArena,
        is_folder: Vec<bool>,
        parent: Vec<u32>,
        aux: Vec<u32>,
        table: Vec<String>,
        sorted_by_path: bool,
    ) -> Option<Self> {
        let n = names.len();
        if is_folder.len() != n || parent.len() != n || aux.len() != n || table.is_empty() {
            return None;
        }
        // 値は高々 `CHAIN_CAP - 1` = 63 ゆえ `u8` で足りる（n = 312,625 で一時確保が半分）。
        let mut depths: Vec<u8> = Vec::with_capacity(n);
        for i in 0..n {
            if aux[i] as usize >= table.len() {
                return None;
            }
            let depth = if parent[i] == NO_PARENT {
                0
            } else {
                let pi = parent[i] as usize;
                if pi >= i {
                    return None;
                }
                depths[pi] + 1
            };
            if depth as usize >= CHAIN_CAP {
                return None;
            }
            depths.push(depth);
        }
        Some(Self {
            names,
            is_folder,
            parent,
            aux,
            table,
            sorted_by_path,
        })
    }

    /// エントリを 1 件も持たない木（初回起動）。
    ///
    /// `table` は空にしない——0 番の空文字は `aux` の既定値が指す先であり、
    /// [`Self::from_parts`] もそれを不変条件として検査する。
    pub fn empty() -> Self {
        Self {
            names: NameArena::new(),
            is_folder: Vec::new(),
            parent: Vec::new(),
            aux: Vec::new(),
            table: vec![String::new()],
            // 空の列は「狭義の単調増加」を空虚に満たすが、`build` が測る値と揃えて真にする。
            sorted_by_path: true,
        }
    }

    /// `i` のフルパスを `buf` へ組み直す（crate の外から使う口）。
    ///
    /// 中身の規則は [`raw_path_into`] が持つ。**バッファを渡す形にしてあるのは、
    /// 全件走査で 1 件ごとに確保させないためである。**
    pub fn path_into(&self, buf: &mut String, i: usize) {
        raw_path_into(self, buf, i);
    }

    /// 列を借りて見る（保存側が `Cow::Borrowed` で全件 clone を避けるための口）。
    pub(crate) fn columns(&self) -> TreeColumns<'_> {
        // **`..` を書かない。** 列を足したらここが compile error になるのが要点である
        // （`SearchEngine::footprint_rows` と同じ規律）。
        let Self {
            names,
            is_folder,
            parent,
            aux,
            table,
            sorted_by_path,
        } = self;
        TreeColumns {
            names,
            is_folder,
            parent,
            aux,
            table,
            sorted_by_path: *sorted_by_path,
        }
    }

    /// 列へほどく（索引側が並べ方を組み替えるための口）。
    ///
    /// **返した列から木へ戻る道は無い**——[`Self::from_parts`] は検証を通すので、ほどいた列を
    /// そのまま構造体リテラルへ流し込む形は書けない。
    pub(crate) fn into_columns(self) -> OwnedTreeColumns {
        let Self {
            names,
            is_folder,
            parent,
            aux,
            table,
            sorted_by_path,
        } = self;
        OwnedTreeColumns {
            names,
            is_folder,
            parent,
            aux,
            table,
            sorted_by_path,
        }
    }

    /// `i` の表示名（アリーナの切り出し）。
    ///
    /// [`TreeNodes::name_of`] と同じものを返すが、あちらは**辿る規則のための口**である。
    /// 木そのものを走査する呼び出し元（kana の構築など）はトレイトを import せずに済むよう
    /// こちらを使う。
    pub(crate) fn name_at(&self, i: usize) -> &str {
        self.names.get(i)
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.len() == 0
    }

    /// 木を `Vec<AppEntry>` へ戻す（フルパスを組み直して実体化する）。
    ///
    /// **実体化は木が消したはずの 312,625 個の `String` をその場で作り直す**ので、通した区間の
    /// ピークには v7 がディスクから消したのと同額（約 36 MiB）が戻る。**壁時計ではなくピークが
    /// 対価である**——`tests/memory_footprint.rs` が測っているのはそちらの側である。
    ///
    /// 通る経路は「派生文字列を自前で導出しなければならない構築」に限られる: `SearchEngine::from_material` が派生データを持たない材料を受けた枝（初回起動と、保存側がそれを返さなかったとき）と、派生文字列を持たない古い版を読んだときの Wave 1。加えて corpus テストと `load_cached_entries` が通る。
    ///
    /// **保存側が `CachedMasks` を返したなら、cache-miss 起動も設定からの再構築もここを通らない**（反復 11 と後続）。**「もう通らない」と無条件に書いてはならない**——返らなかったときの枝が上の列挙に在り、そちらは今もここへ落ちる。`PERFORMANCE.md` が構築段 peak -83.27 MiB として計上しているのはこの実体化が消えたぶんであり、**`materialize` の削減を「cache-miss で効く」と見積もってはならない。**
    ///
    /// **`index.bin` のキャッシュヒット（＝ほとんどの起動）もここを通らない。**
    pub(crate) fn materialize(&self) -> Vec<AppEntry> {
        let mut buf = String::new();
        (0..self.len())
            .map(|i| {
                raw_path_into(self, &mut buf, i);
                AppEntry {
                    name: self.names.get(i).to_owned(),
                    target_path: buf.clone(),
                    is_folder: self.is_folder[i],
                }
            })
            .collect()
    }
    /// `Vec<AppEntry>` を木表現へ組み替える。
    ///
    /// **入力が `target_path` のバイト順に整列していることを前提にするが、要求はしない。**
    /// 親は二分探索で引き、`binary_search_by` が `Ok` を返すのは完全一致のときだけなので、
    /// 整列していない入力でも**別の親を返すことは起こりえない**——起こるのは取りこぼしだけで、
    /// そのエントリは根になり自分のフルパスを持つ。結果は変わらず、記憶域が増えるだけである。
    /// この性質のおかげで整列済みかを事前に走査する必要がなく、実測 6.5 ms を払わずに済む。
    ///
    /// 循環は `pi < i` の 1 比較で構造的に潰す。文書化した契約ではなく順序で担保するので、
    /// 壊れた入力でも [`walk_to_root`] が止まらなくなることはない。
    pub(crate) fn build(entries: Vec<AppEntry>) -> Self {
        let n = entries.len();
        // 整列の判定は並列で 1 回だけ。全件走査の tie-break がこの 1 bit に載る。
        //
        // **`<=` ではなく `<` である。** `<=` は重複パスを許してしまい、そのとき
        // tie-break の高速路（index 比較）は同じパスの 2 件へ `Less`/`Greater` を返す
        // ——正しくは `Equal` である。狭く取れば、重複がある入力は旗が下りて
        // 組み立てて比べる経路へ落ちるだけで済む。
        let sorted_by_path = entries
            .par_windows(2)
            .all(|w| w[0].target_path < w[1].target_path);
        let mut table: Vec<String> = vec![String::new()];
        let mut aux = vec![0u32; n];

        // 拡張子の intern までは `entries` を借りたまま行い、**借用をここで閉じる**——
        // 続く組み立ては `entries` を消費して `String` を move するため、借りたままでは通らない。
        // 親 index だけを借用のない形へ写して次の段へ渡す。
        let parents: Vec<Option<usize>> = {
            let resolved = resolve_all(&entries);
            let mut ext_ids: std::collections::HashMap<&str, u32> =
                std::collections::HashMap::new();
            for (i, r) in resolved.iter().enumerate() {
                if let Some((_, ext)) = r
                    && !ext.is_empty()
                {
                    aux[i] = *ext_ids.entry(ext).or_insert_with(|| {
                        table.push((*ext).to_owned());
                        (table.len() - 1) as u32
                    });
                }
            }
            resolved.iter().map(|r| r.map(|(pi, _)| pi)).collect()
        };

        // **アリーナの合計バイト数は先に数える。** 伸長に任せると 10 MiB の `blob` が倍々で
        // 移し替えられ、最後の 1 回は新旧が同時に生きる——消したはずの確保が peak へ戻る。
        let name_bytes: usize = entries.iter().map(|e| e.name.len()).sum();
        let mut names = NameArena::with_capacity(n, name_bytes);
        let mut is_folder_col = Vec::with_capacity(n);
        let mut parent_col = Vec::with_capacity(n);
        let mut aux_col = Vec::with_capacity(n);
        // 鎖の長さは**ここで**打ち止める。`parent < i` ゆえ親の深さは既に確定しており、
        // 1 引き算で数えられる（[`CHAIN_CAP`] の doc に理由）。
        // 値は高々 `CHAIN_CAP - 1` = 63 ゆえ `u8` で足りる（n = 312,625 で一時確保が半分）。
        let mut depths: Vec<u8> = Vec::with_capacity(n);
        for (i, entry) in entries.into_iter().enumerate() {
            let AppEntry {
                name,
                target_path,
                is_folder,
            } = entry;
            let linked = parents[i].filter(|&pi| (depths[pi] as usize) + 1 < CHAIN_CAP);
            let (parent, aux_id, depth) = match linked {
                Some(pi) => (pi as u32, aux[i], depths[pi] + 1),
                None => {
                    // 親を持たないエントリだけがフルパスを持つ（実データで 0.03%）。
                    table.push(target_path);
                    (NO_PARENT, (table.len() - 1) as u32, 0)
                }
            };
            depths.push(depth);
            names.push(&name);
            is_folder_col.push(is_folder);
            parent_col.push(parent);
            aux_col.push(aux_id);
        }

        Self {
            names,
            is_folder: is_folder_col,
            parent: parent_col,
            aux: aux_col,
            table,
            sorted_by_path,
        }
    }

    /// PATH スキャンが見つけたエントリを、**親を解決せずに根として**足す。
    ///
    /// 根はフルパスを実体で持つので、解決しないぶんの損は PATH の 101 件ぶん
    /// （合計およそ 5 KiB）にしかならない一方、解決すれば「パス → index」の索引を
    /// 全件ぶん建てることになる。反復 9 が PATH スキャンで却下したのと同じ比率の誤りである。
    ///
    /// **整列の旗は必ず下ろす。** 足したパスがバイト順で末尾に来る保証は無く、実運用点では
    /// 実際に崩れる（PATH の実行ファイルは `C:\Windows\System32\…` ゆえ途中に入る・実測）。
    /// 偽は遅い経路へ落ちるだけで結果は変わらない。
    pub(crate) fn extend_with_roots(&mut self, entries: Vec<AppEntry>) {
        if entries.is_empty() {
            return;
        }
        // **`self` を網羅的に分解してから積む。** 手で 5 本を push する形は、列を 1 本足した
        // ときにここだけ触り忘れてもコンパイルが通る——`empty` / `build` / `from_parts` は
        // 構造体リテラルゆえコンパイラが捕まえるのに、この関数だけが漏れる。積み損ねた列は
        // `PathStore::adopt` の連鎖 `zip` が最短で黙って切り、`debug_assert` は release で
        // 消えるので、**PATH 経由でしか届かないプログラムが検索から静かに消える**。
        let Self {
            names,
            is_folder,
            parent,
            aux,
            table,
            sorted_by_path,
        } = self;
        for entry in entries {
            let AppEntry {
                name,
                target_path,
                is_folder: entry_is_folder,
            } = entry;
            table.push(target_path);
            names.push(&name);
            is_folder.push(entry_is_folder);
            parent.push(NO_PARENT);
            aux.push((table.len() - 1) as u32);
        }
        *sorted_by_path = false;
    }

    /// `i` の**末尾成分**を、`indexer` の篩と同じ規則で正規化して `buf` へ書く。
    ///
    /// `seg` は作業用に使い回すバッファで、中身は捨てられる。
    ///
    /// **フルパスを組み立てない。** 木では末尾成分が `name` + 拡張子でそのまま取れるので、
    /// 根まで辿る必要が無い（篩は 312,691 回走るので、ここが区間を支配する）。
    ///
    /// **根だけは別の腕を通る。** 根は `table` にフルパスを持つため、そこから末尾成分を
    /// 切り出す既存の経路（`indexer` 側）へ落とす。実データで根は約 200 件（0.06%）
    /// ——**ほとんど通らない腕ゆえ、壊れても静かである**。
    ///
    /// 両腕が `normalize_file_name_key_into(target_path)` と一致することは
    /// `file_key_matches_normalize_file_name_key_on_both_arms`（合成 fixture・根と非根を
    /// 両方通し、各腕が実際に通ったことも数える）と、`search/tests/path.rs` の
    /// `index_tree_file_key_matches_normalize_file_name_key_over_real_index`（実データ全件・
    /// 開発機限定）が固定する。**合成のほうが機構としての保証を持つ**——corpus は実インデックス
    /// が無ければ自動スキップし、根は実データで 0.06% しか居ない。
    pub(crate) fn file_key_into(&self, buf: &mut String, seg: &mut String, i: usize) {
        if self.parent[i] == NO_PARENT {
            crate::indexer::normalize_file_name_key_into(buf, self.table_str(self.aux[i]));
            return;
        }
        seg.clear();
        seg.push_str(self.names.get(i));
        seg.push_str(self.table_str(self.aux[i]));
        crate::indexer::normalize_entry_key_into(buf, seg);
    }
}

/// 直前に解決した親を憶えておく move-to-front の小さなキャッシュ。
///
/// ソート順では兄弟がほぼ連続して現れるので、同じ親を何度も探索し直すのが元の無駄である。
/// 部分木が割り込むと外れる（`dir\a`, `dir\a\x`, `dir\b` で `dir` が押し出される）ため
/// 直近の祖先の鎖を覆えるだけの段数を持つ。**段数は実測で決めた**（312,377 件・並列・
/// 各 3 回の最小値）: 2 段 25.3 ms / **4 段 23.8 ms** / 8 段 25.6 ms。深さの平均は 6.05 段
/// だが、8 段では線形走査が伸びて損が勝つ。
#[derive(Default)]
struct ParentCache<'a> {
    slots: [Option<(&'a str, usize)>; 4],
}

impl<'a> ParentCache<'a> {
    fn get(&self, par: &str) -> Option<usize> {
        self.slots
            .iter()
            .flatten()
            .find(|(p, _)| *p == par)
            .map(|(_, i)| *i)
    }

    /// 既に居るものは前へ出すだけで増やさない——同じ親が複数の段を占めると、
    /// 実効の段数が減って割り込みに弱くなる。
    fn put(&mut self, par: &'a str, i: usize) {
        if let Some(pos) = self
            .slots
            .iter()
            .position(|s| s.map(|(p, _)| p) == Some(par))
        {
            self.slots[..=pos].rotate_right(1);
            return;
        }
        self.slots.rotate_right(1);
        self.slots[0] = Some((par, i));
    }
}

/// 全件の親と拡張子を導出する。
///
/// **分割は要素ごとではなく連続した塊ごとにする。** [`ParentCache`] は隣り合う要素が同じ親を
/// 持つことに賭けるので、要素を撒くと賭けが成立しない（実測 30.0 → 23.0 ms）。
fn resolve_all(entries: &[AppEntry]) -> Vec<Option<(usize, &str)>> {
    const CHUNK: usize = 8192;
    (0..entries.len())
        .step_by(CHUNK)
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|start| {
            let end = (start + CHUNK).min(entries.len());
            let mut cache = ParentCache::default();
            (start..end)
                .map(|i| resolve_one(entries, i, &mut cache))
                .collect::<Vec<_>>()
        })
        .flatten()
        .collect()
}

/// エントリ 1 件の親 index と拡張子を導出する。組み替えの規則はここが唯一の定義である。
fn resolve_one<'a>(
    entries: &'a [AppEntry],
    i: usize,
    cache: &mut ParentCache<'a>,
) -> Option<(usize, &'a str)> {
    let e = &entries[i];
    let path = e.target_path.as_str();
    // **`Path` を経由しない。** Windows の `Path` は `OsStr`（WTF-8）を包み、`to_str()` が
    // UTF-8 検証の走査を全件に乗せる（実測 167 → 117 ms）。パスは既に `&str` である。
    let cut = path.rfind(['\\', '/'])?;
    let tail = &path[cut + 1..];
    // 親はドライブ直下だけ区切りを含む（`C:\`）。それ以外は含まない（`C:\foo`）。
    let par_end = if cut == 0 || path.as_bytes()[cut - 1] == b':' {
        cut + 1
    } else {
        cut
    };
    let par = &path[..par_end];

    let pi = match cache.get(par) {
        Some(hit) => hit,
        None => {
            let found = entries
                .binary_search_by(|c| c.target_path.as_str().cmp(par))
                .ok()?;
            cache.put(par, found);
            found
        }
    };
    // **親は必ず自分より前に居る。** ここで弾くことで循環が構造的に生じえなくなる。
    if pi >= i {
        return None;
    }

    // **連結して比べ直さない。** `par` も `tail` も `path` の部分スライスなので、確かめるのは
    // 間に挟まる区切りだけでよい（1 エントリ 1 確保が消える・実測 249.6 → 167 ms）。
    let sep = &path[par_end..cut + 1];
    if sep != "\\" && !(sep.is_empty() && par.ends_with(['\\', '/'])) {
        return None;
    }
    // file の `name` は `file_stem()`（拡張子なし）ゆえ末尾成分の接頭辞になる。
    // folder は `file_name()` そのものなので差分は空文字になる。
    let ext = tail.strip_prefix(e.name.as_str())?;
    Some((pi, ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `from_parts` が取る 5 列（names / is_folder / parent / aux / table）。
    type Parts = (NameArena, Vec<bool>, Vec<u32>, Vec<u32>, Vec<String>);

    /// 名前の列をアリーナへ詰める（テストの読みやすさのためだけの補助）。
    fn arena(names: &[&str]) -> NameArena {
        let mut a = NameArena::new();
        for n in names {
            a.push(n);
        }
        a
    }

    /// 3 件（根 → 子 → 孫）の健全な列。各テストが 1 か所だけ壊して使う。
    fn healthy_parts() -> Parts {
        (
            arena(&["Projects", "sub", "app"]),
            vec![true, true, false],
            vec![NO_PARENT, 0, 1],
            vec![2, 0, 1],
            vec![String::new(), ".exe".into(), "C:\\Projects".into()],
        )
    }

    /// **アリーナの線上表現は `Vec<String>` と 1 バイトも違わない。**
    ///
    /// これが `INDEX_CACHE_VERSION` を上げずに `IndexCache.names` の型を替えられた根拠であり、
    /// 破れると**全ユーザーの `index.bin` が黙って読めなくなる**（版が同じまま形式だけ変わる
    /// ので、フォールバックの鎖はどの枝も拾わず cache-miss ＝ 22〜30 秒の全走査へ落ちる）。
    ///
    /// **`index_cache_on_disk_format_is_stable`（golden bytes）と役割が違う。** あちらは
    /// `IndexCache` 全体の凍結バイト列を守り、こちらは**なぜそれが凍結されたままでいられるか**
    /// ——アリーナ単体が seq of str であること——を名指しで固定する。両向きを見る:
    /// 書いた側が `Vec<String>` と一致することと、`Vec<String>` として書かれた旧バイト列を
    /// 読み戻せることは別の主張である。
    #[test]
    fn arena_wire_format_is_identical_to_vec_of_string() {
        // 非 ASCII・空文字・区切りを含む名前を混ぜる（**オフセットが UTF-8 の文字境界に
        // 載ることは、要素を丸ごと押し込む形からしか従わない**）。
        let names = [
            "Projects",
            "",
            "Ünïcode 名前",
            "sub",
            "アプリ",
            "x",
            "trailing space ",
        ];
        let owned: Vec<String> = names.iter().map(|s| (*s).to_string()).collect();

        let via_vec = postcard::to_allocvec(&owned).expect("Vec<String> を書ける");
        let via_arena = postcard::to_allocvec(&arena(&names)).expect("NameArena を書ける");
        assert_eq!(
            via_vec, via_arena,
            "アリーナの線上表現が `Vec<String>` とずれた（版を上げずに型を替えた前提が崩れる）"
        );

        // 逆向き: `Vec<String>` として書かれたバイト列をアリーナで読み戻す。
        let back: NameArena = postcard::from_bytes(&via_vec).expect("旧バイト列を読める");
        assert_eq!(back.len(), names.len());
        for (i, want) in names.iter().enumerate() {
            assert_eq!(back.get(i), *want, "{i} 番の切り出しがずれた");
        }
    }

    /// **壊れた長さプレフィックスは、巨大確保ではなく `Err` で落ちる。**
    ///
    /// `index.bin` は自分が書いたファイルだが、化けた 5 バイトの varint は 40 億件を名乗れる。
    /// `Vec<String>` のときは serde の訪問子が事前確保を頭打ちにしていたので、この入力は
    /// 「読めなかった」＝ cache-miss ＝再走査へ穏やかに落ちていた。**自前の訪問子は同じ上限を
    /// 自分で持たない限りそれを迂回する**——迂回すると確保失敗になり、release は
    /// `panic="abort"` ゆえ**起動そのものが落ちる**（検索結果が変わる形ではないので挙動
    /// テストでは捕まらない）。
    #[test]
    fn corrupt_length_prefix_fails_instead_of_preallocating() {
        // 要素数だけを名乗り、中身を 1 バイトも持たないバイト列（postcard の varint で
        // 約 40 億）。**上限を外すと 16 GiB の確保を試みる。**
        let hostile = [0xFF, 0xFF, 0xFF, 0xFF, 0x0F];
        let got: Result<NameArena, _> = postcard::from_bytes(&hostile);
        assert!(got.is_err(), "中身の無い巨大な列を受理してはならない");
    }

    /// 空のアリーナ（初回起動の [`IndexTree::empty`]）も往復する。
    ///
    /// **番兵を持つ表現ゆえ「空」は `offsets == [0]` である。** `Vec::new()` にすると
    /// `len()` が `0 - 1` で溢れるので、空の形を別に固定しておく。
    #[test]
    fn empty_arena_roundtrips_and_reports_zero_len() {
        let empty = NameArena::new();
        assert_eq!(empty.len(), 0);
        let bytes = postcard::to_allocvec(&empty).expect("空を書ける");
        assert_eq!(bytes, postcard::to_allocvec(&Vec::<String>::new()).unwrap());
        let back: NameArena = postcard::from_bytes(&bytes).expect("空を読める");
        assert_eq!(back.len(), 0);
        assert_eq!(IndexTree::empty().len(), 0);
    }

    /// **PATH スキャンの追記でアリーナが伸びる**（[`IndexTree::extend_with_roots`]）。
    ///
    /// 追記側を忘れると、足したエントリの名前が**直前のエントリの名前として読まれる**——
    /// オフセットが伸びないので添字は範囲内に収まり、`debug_assert` にも掛からない。
    /// 件数だけを見るテストでは緑のまま通るので、**組み直したフルパスまで見る**。
    #[test]
    fn extend_with_roots_grows_the_name_arena() {
        let mut tree = IndexTree::build(vec![AppEntry {
            name: "Projects".into(),
            target_path: "C:\\Projects".into(),
            is_folder: true,
        }]);
        tree.extend_with_roots(vec![AppEntry {
            name: "curl".into(),
            target_path: "C:\\Windows\\System32\\curl.exe".into(),
            is_folder: false,
        }]);

        assert_eq!(tree.len(), 2);
        assert_eq!(tree.name_at(0), "Projects");
        assert_eq!(tree.name_at(1), "curl");
        let mut buf = String::new();
        tree.path_into(&mut buf, 1);
        assert_eq!(buf, "C:\\Windows\\System32\\curl.exe");
    }

    /// 土台。**これが通らないと下の「壊したら弾く」は空虚になる**——`None` はどんな理由でも
    /// 返るので、健全な入力が `Some` になることを先に固定しないと「検査が効いている」証拠に
    /// ならない（何でも弾く実装でも赤にならない）。
    #[test]
    fn from_parts_accepts_healthy_columns_and_rebuilds_paths() {
        let (names, is_folder, parent, aux, table) = healthy_parts();
        let tree = IndexTree::from_parts(names, is_folder, parent, aux, table, false)
            .expect("健全な列は受理される");

        let mut buf = String::new();
        let want = [
            "C:\\Projects",
            "C:\\Projects\\sub",
            "C:\\Projects\\sub\\app.exe",
        ];
        for (i, expected) in want.iter().enumerate() {
            tree.path_into(&mut buf, i);
            assert_eq!(&buf, expected, "index {i}");
        }
    }

    /// **`index.bin` の 5 列は独立に読まれる**ので、壊れたファイルは長さの揃わない列や
    /// 範囲外の添字を与えうる。弾かなかったときの帰結は静かで、しかも条件ごとに違う:
    ///
    /// - 列の長さ不一致 → 組み替えの `zip` が短い側で止まり、**索引が黙って短くなる**
    /// - `aux` が範囲外 → 組み立てが添字 panic（release は `panic="abort"` ＝ 起動不能）
    /// - `parent >= i` → `walk_to_root` の停止性が崩れる（**止まらない**）
    /// - 深さが `CHAIN_CAP` 以上 → `chain` 配列の範囲外
    ///
    /// （角括弧のリンクを使わないのは、`#[cfg(test)]` 内の doc を rustdoc が一切見ないため
    /// である——`cargo doc` の intra-doc link 検査に載らないので、書けば「検査済みの参照」に
    /// 見えるだけの飾りになる。）
    ///
    /// **版が読めたことは中身が健全であることを意味しない。** 5 条件を 1 本ずつ潰すのは、
    /// 1 つ消しても残りが通るせいで**検査の欠落が緑のまま残る**ためである。
    #[test]
    fn from_parts_rejects_each_broken_invariant() {
        // 1. 列の長さが揃わない（`is_folder` だけ 1 件短い）。
        let (names, mut is_folder, parent, aux, table) = healthy_parts();
        is_folder.pop();
        assert!(
            IndexTree::from_parts(names, is_folder, parent, aux, table, false).is_none(),
            "列の長さ不一致を受理してはならない"
        );

        // 2. `aux` が `table` の外を指す。
        let (names, is_folder, parent, mut aux, table) = healthy_parts();
        aux[1] = table.len() as u32;
        assert!(
            IndexTree::from_parts(names, is_folder, parent, aux, table, false).is_none(),
            "table 範囲外の aux を受理してはならない"
        );

        // 3. `table` が空。0 番の空文字は `aux` の既定値が指す先であり、必ず居る
        //    （`IndexTree::empty` も空にしない）。**n = 0 の形で見る**——エントリがあると
        //    `aux[i] >= table.len()` の検査が先に弾いてしまい、`table.is_empty()` を消しても
        //    このケースは緑のままになる（列が空なら aux のループは 1 度も回らない）。
        assert!(
            IndexTree::from_parts(
                NameArena::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                true
            )
            .is_none(),
            "空の table を受理してはならない（0 件でも 0 番の空文字は要る）"
        );

        // 4. `parent >= i`（ここでは自己参照）。**停止性の唯一の根拠がこれである。**
        let (names, is_folder, mut parent, aux, table) = healthy_parts();
        parent[1] = 1;
        assert!(
            IndexTree::from_parts(names, is_folder, parent, aux, table, false).is_none(),
            "parent >= i を受理してはならない"
        );

        // 5. 深さが CHAIN_CAP 以上。**鎖を実際に伸ばして測る**——上限の値を書き写すと、
        //    定数を変えたときにこのテストだけが古い前提のまま緑になる。
        let n = CHAIN_CAP + 1;
        let deep: Vec<String> = (0..n).map(|i| format!("d{i}")).collect();
        let names = arena(&deep.iter().map(String::as_str).collect::<Vec<_>>());
        let parent: Vec<u32> = std::iter::once(NO_PARENT)
            .chain((0..n - 1).map(|i| i as u32))
            .collect();
        assert!(
            IndexTree::from_parts(
                names,
                vec![true; n],
                parent,
                vec![0; n],
                vec![String::new(), "C:\\d0".into()],
                false,
            )
            .is_none(),
            "CHAIN_CAP 以上の深さを受理してはならない"
        );
    }

    /// **PATH の篩のキーは、両腕とも `normalize_file_name_key_into` と一致しなければならない。**
    ///
    /// 篩は `reject_existing` が既存エントリを素通しさせないための唯一の機構であり、
    /// ずれると `by_file_key.get()` が外れて**索引に既にある PATH 実行ファイルが二重に
    /// 入る**（件数が増えるだけで panic も失敗も起きない）。
    ///
    /// **両腕が実際に通ったことを数える。** 非根の腕は実データの 99.94% を占めるのに、
    /// `scan_path_dirs_*` の fixture はどれも 1 要素の木を渡すので**その 1 件は必ず根になる**
    /// ——つまり非根の腕はどのテストからも実行されていなかった。腕ごとの件数を assert しないと、
    /// 木の建ち方が変わって全件が根に落ちてもこのテストは緑のままになる。
    #[test]
    fn file_key_matches_normalize_file_name_key_on_both_arms() {
        // `target_path` のバイト昇順。`name` は indexer の導出規則（folder は `file_name()`、
        // file は `file_stem()`）に合わせる——`resolve_one` は `tail.strip_prefix(name)` で
        // 拡張子を切るので、規則がずれると親が解決されず全件が根になる。
        let entries = [
            ("Projects", "C:\\Projects", true),
            // 名前に空白を含む folder（拡張子は空文字）。
            ("My App", "C:\\Projects\\My App", true),
            ("run", "C:\\Projects\\My App\\run.exe", false),
            // 非 ASCII の名前 + 大文字の拡張子（小文字化の両分岐を通す）。
            ("Ünïcode", "C:\\Projects\\Ünïcode.LNK", false),
            ("apps", "C:\\apps", true),
            // 区切りの直後に空白があるパス（`normalize_file_name_key_into` の doc が
            // 「写像と `trim` が可換」を前提として名指している形）。
            (" tool", "C:\\apps\\ tool.EXE", false),
        ];
        let want: Vec<String> = entries
            .iter()
            .map(|(_, path, _)| {
                let mut b = String::new();
                crate::indexer::normalize_file_name_key_into(&mut b, path);
                b
            })
            .collect();

        let tree = IndexTree::build(
            entries
                .iter()
                .map(|(name, path, is_folder)| AppEntry {
                    name: (*name).to_string(),
                    target_path: (*path).to_string(),
                    is_folder: *is_folder,
                })
                .collect(),
        );

        let (mut roots, mut children) = (0usize, 0usize);
        let (mut buf, mut seg) = (String::new(), String::new());
        for (i, (_, path, _)) in entries.iter().enumerate() {
            if tree.parent[i] == NO_PARENT {
                roots += 1;
            } else {
                children += 1;
            }
            tree.file_key_into(&mut buf, &mut seg, i);
            assert_eq!(buf, want[i], "篩のキーが正規化とずれている（{path}）");
        }
        assert_eq!(
            roots, 2,
            "根の腕が想定どおり通っていない（fixture の前提崩れ）"
        );
        assert_eq!(
            children, 4,
            "非根の腕が想定どおり通っていない（fixture の前提崩れ）"
        );
    }
}
