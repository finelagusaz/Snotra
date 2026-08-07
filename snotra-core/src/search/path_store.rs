//! `target_path` をフォルダ木の接頭辞共有で保持する索引側の表現（#961/#962 に続く反復 3）。
//!
//! 索引の 8 割はフォルダで、パスは親子で接頭辞を共有する。`indexer` の名前導出規則
//! （folder は `file_name()` / file は `file_stem()`）ゆえ、**フォルダの末尾成分は `name`
//! そのもの**であり、ファイルは `name` + 拡張子で組み直せる。親の index と拡張子 id だけを
//! 持てば、フルパスの文字列は親を持たないエントリのぶんしか要らない。
//!
//! 実測（実 `index.bin` 312,377 エントリ・`PERFORMANCE.md`「`target_path` のフォルダ木
//! 接頭辞共有」）: 文字列 35.56 → 0.22 MiB、`entries` の 1 要素 56 → 32 B、
//! 1 エントリあたりの確保ブロック 1 → 0。常駐 97.51 → 55.02 MiB。
//!
//! **払うのは組み立て直しのコストである。** 構築が 2〜3 → 約 20 ms、パスクエリ全走査が
//! +3 ms、`recent_history` が +4.2 ms。どれも実測で、判定の根拠は `PERFORMANCE.md` にある。
//!
//! # 組み立ての 2 系統
//!
//! 読み手は 2 種類あり、**流用してはならない**:
//!
//! - [`PathStore::raw_into`] — 原文のまま。表示パス（`SearchResult.path`）と tie-break が使う
//! - [`PathStore::normalized_into`] — 小文字化 + `/` → `\`。履歴照合とパスマッチが使う
//!
//! 正規化側は「組み立ててから正規化する」二段払いにせず、正規化バッファへ直接書き出す。

use std::cmp::Ordering;
use std::collections::HashMap;

use rayon::prelude::*;

use crate::indexer::AppEntry;

/// 親を持たないことを表す番兵。このとき `aux` は [`PathStore::table`] のフルパスを指す。
const NO_PARENT: u32 = u32::MAX;

/// 索引 1 件ぶんの圧縮表現（32 B）。`AppEntry`（56 B）から `target_path` の `String` を
/// 落とし、親の index と `table` の id に置き換えたもの。
///
/// `parent` と `aux` を別の並列 Vec へ出さずここに置くのは意図的である——木を辿る段で
/// `entries[cur].parent` を読み、同じ段で `entries[cur].name` も読むため、**同じキャッシュ
/// ラインに載るほうがミスが少ない**（別配列にすると 1 段あたり 2 回になる）。
/// ビットマスク（`char_masks`）は従来どおり独立した `Vec<u64>` のままであり、
/// ここへ巻き込んではならない（#110 の 35〜120% 遅化）。
pub(super) struct CompactEntry {
    /// 表示名。`AppEntry.name` をそのまま移す（伸長しないので `Box<str>`）。
    pub(super) name: Box<str>,
    /// 親エントリの index。[`NO_PARENT`] なら親を持たない。
    ///
    /// **必ず自分より小さい**——構築時に `pi < i` で弾いており、循環は表現できない。
    /// 深さが有限であることがこの不変条件だけで従うので、組み立ては必ず停止する。
    parent: u32,
    /// `table` の添字。親を持つときは拡張子（folder と拡張子なし file は 0 = 空文字）、
    /// 親を持たないときはフルパス。**どちらを指すかは `parent` が決める。**
    aux: u32,
    pub(super) is_folder: bool,
}

/// 索引全体のパス表現。`SearchEngine` が `Vec<AppEntry>` の代わりに持つ。
pub(super) struct PathStore {
    entries: Vec<CompactEntry>,
    /// 拡張子とフルパスを同居させた intern 表（0 番は空文字で固定）。
    ///
    /// 同居させるのは、組み立ての最後の 1 段（根）が必ずここを引くためである。
    /// 別テーブルにして二分探索させると、**全エントリの組み立てにその探索が乗る**。
    table: Vec<Box<str>>,
}

impl PathStore {
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(super) fn get(&self, i: usize) -> &CompactEntry {
        &self.entries[i]
    }

    pub(super) fn shrink_to_fit(&mut self) {
        self.entries.shrink_to_fit();
        self.table.shrink_to_fit();
    }

    /// 余剰容量の検知器（`search/tests/build.rs`）が読む確保量。
    /// 余剰は検索結果を変えないため、挙動テストでは捕まらない。
    #[cfg(test)]
    pub(super) fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    /// `i` のフルパスを**原文のまま**組み立てる（小文字化も `/` → `\` 変換も trim もしない）。
    ///
    /// 結果は元の `AppEntry.target_path` とバイト一致する。実 `index.bin` の全 312,377 件で
    /// 固定してある（`tests/path_query_cost.rs` の
    /// `tree_raw_reconstruction_is_byte_identical_to_target_path`）。
    pub(super) fn raw_into(&self, buf: &mut String, i: usize) {
        let (root, chain, depth) = self.walk_to_root(i);
        buf.clear();
        buf.push_str(&self.table[self.entries[root].aux as usize]);
        for d in (0..depth).rev() {
            let idx = chain[d] as usize;
            self.push_separator(buf);
            buf.push_str(&self.entries[idx].name);
            // 空文字（folder・拡張子なし file）の `push_str` は no-op ゆえ分岐を置かない。
            buf.push_str(&self.table[self.entries[idx].aux as usize]);
        }
    }

    /// `i` の正規化キー（`normalize_entry_key` と同じ規則）を組み立てる。
    ///
    /// **組み立ててから正規化する二段払いにはしない**——正規化バッファへ直接書き出す 1 パスで
    /// あり、増分は段数ぶんの読みだけである。規則の正本は
    /// [`crate::indexer::normalize_entry_key_into`] で、記録側と同じバイトになることは
    /// `tests/path_query_cost.rs` の `tree_reconstruction_derives_same_bytes_as_normalize_entry_key`
    /// が実インデックスの全件で固定する。
    pub(super) fn normalized_into(&self, buf: &mut String, i: usize) {
        let (root, chain, depth) = self.walk_to_root(i);
        buf.clear();
        // 根だけ `trim` する。現物は全体へ当てるが、末尾側は最終セグメントが担う。
        push_segment(buf, self.table[self.entries[root].aux as usize].trim());
        for d in (0..depth).rev() {
            let idx = chain[d] as usize;
            self.push_separator(buf);
            push_segment(buf, &self.entries[idx].name);
            push_segment(buf, &self.table[self.entries[idx].aux as usize]);
        }
        // ASCII の小文字化はここで 1 回だけ当てる（`push_segment` の doc を見よ）。
        buf.make_ascii_lowercase();
    }

    /// 2 件のフルパスを**原文のバイト列として**比べる（`ScoredEntry` の最終 tie-break）。
    ///
    /// **セグメント単位で比べてはならない。** 区切り `\`(0x5C) は `-`(0x2D) や `.`(0x2E) より
    /// 大きいため、`C:\a-x\bin` はバイト順では `C:\a\bin` より前に来る。セグメント単位だと
    /// 逆になり、結果の並びが静かに変わる（検知器は `search/tests/ranking.rs` の
    /// `search_result_order_is_stable_across_target_path_representations`）。
    ///
    /// 組み立ては tie が `path` まで落ちたときだけ走る（score / last_launched / lower_name が
    /// すべて等しい場合）。バッファはスレッドローカルで、`Ord` の全順序契約は組み立てが
    /// 決定的であることから保たれる。
    pub(super) fn cmp_paths(&self, a: usize, b: usize) -> Ordering {
        if a == b {
            return Ordering::Equal;
        }
        CMP_BUFS.with(|cell| {
            let (lhs, rhs) = &mut *cell.borrow_mut();
            self.raw_into(lhs, a);
            self.raw_into(rhs, b);
            lhs.as_str().cmp(rhs.as_str())
        })
    }

    /// `i` のフルパスを新しい `String` として返す（`SearchResult` 組み立て用・top-k 確定後の
    /// K 件だけに使う）。ホットループから呼ばない。
    pub(super) fn to_path(&self, i: usize) -> String {
        let mut buf = String::new();
        self.raw_into(&mut buf, i);
        buf
    }

    /// 根まで辿って途中の index を積む。返すのは（根, 鎖, 段数）。
    ///
    /// 上限を切らないのは `parent < self` の不変条件が構築時に強制されているからで、
    /// 段数は必ず index を下回る。配列の長さは実測の最大深さ 17 段に対する余裕である
    /// （越えたぶんは根として扱われ、組み立ては短いパスを返す——止まらなくなることはない）。
    #[inline]
    fn walk_to_root(&self, i: usize) -> (usize, [u32; 64], usize) {
        let mut chain = [0u32; 64];
        let mut depth = 0usize;
        let mut cur = i;
        while self.entries[cur].parent != NO_PARENT && depth < chain.len() {
            chain[depth] = cur as u32;
            depth += 1;
            cur = self.entries[cur].parent as usize;
        }
        (cur, chain, depth)
    }

    #[inline]
    fn push_separator(&self, buf: &mut String) {
        // 親がドライブ直下（`C:\`）なら既に区切りで終わっている。
        if buf.as_bytes().last() != Some(&b'\\') {
            buf.push('\\');
        }
    }
}

thread_local! {
    /// tie-break の組み立て先。**2 本要る**——`cmp_paths` が両辺を同時に持つためである。
    /// 容量を再利用するので暖まったあとの確保は起きない。
    static CMP_BUFS: std::cell::RefCell<(String, String)> =
        const { std::cell::RefCell::new((String::new(), String::new())) };
}

/// 1 セグメントを `/` → `\` だけ直して**追記**する（`clear` も `trim` も小文字化もしない）。
///
/// **ASCII は一括で動かす。** 1 文字ずつ `push` すると `String: Extend<char>` が毎回 UTF-8
/// 符号化の分岐を通り、実測で 2.5-3 倍遅くなる（[`crate::indexer::normalize_entry_key_into`]
/// が同じ理由で同じ形を取っている）。ASCII の小文字化は呼び出し側が最後に 1 回
/// `make_ascii_lowercase` でまとめて当てる。
#[inline]
fn push_segment(buf: &mut String, s: &str) {
    if s.is_ascii() {
        let mut rest = s;
        while let Some(pos) = rest.find('/') {
            buf.push_str(&rest[..pos]);
            buf.push('\\');
            rest = &rest[pos + 1..];
        }
        buf.push_str(rest);
    } else {
        // 非 ASCII はここで小文字化まで済ませる。末尾の `make_ascii_lowercase` は ASCII
        // バイトしか触らないため、ここで書いた結果に二重適用しても変わらない（冪等）。
        for ch in s.chars() {
            if ch == '/' {
                buf.push('\\');
            } else {
                buf.extend(ch.to_lowercase());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 構築
// ---------------------------------------------------------------------------

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

impl PathStore {
    /// `Vec<AppEntry>` を圧縮表現へ組み替える。
    ///
    /// **入力が `target_path` のバイト順に整列していることを前提にするが、要求はしない。**
    /// 親は二分探索で引き、`binary_search_by` が `Ok` を返すのは完全一致のときだけなので、
    /// 整列していない入力でも**別の親を返すことは起こりえない**——起こるのは取りこぼしだけで、
    /// 取りこぼしたエントリは自分のフルパスを持つ（結果は正しく、削減量だけが落ちる）。
    /// この性質のおかげで整列済みかを事前に走査する必要がなく、実測 6.5 ms を払わずに済む。
    ///
    /// 循環は `pi < i` の 1 比較で構造的に潰す。文書化した契約ではなく順序で担保するので、
    /// 壊れた入力でも [`PathStore::walk_to_root`] が止まらなくなることはない。
    pub(super) fn build(entries: Vec<AppEntry>) -> Self {
        let n = entries.len();
        let mut table: Vec<Box<str>> = vec![Box::from("")];
        let mut aux = vec![0u32; n];

        // 拡張子の intern までは `entries` を借りたまま行い、**借用をここで閉じる**——
        // 続く組み立ては `entries` を消費して `String` を move するため、借りたままでは通らない。
        // 親 index だけを借用のない形へ写して次の段へ渡す。
        let parents: Vec<Option<usize>> = {
            let resolved = resolve_all(&entries);
            let mut ext_ids: HashMap<&str, u32> = HashMap::new();
            for (i, r) in resolved.iter().enumerate() {
                if let Some((_, ext)) = r
                    && !ext.is_empty()
                {
                    aux[i] = *ext_ids.entry(ext).or_insert_with(|| {
                        table.push(Box::from(*ext));
                        (table.len() - 1) as u32
                    });
                }
            }
            resolved.iter().map(|r| r.map(|(pi, _)| pi)).collect()
        };

        let mut compact = Vec::with_capacity(n);
        for (i, entry) in entries.into_iter().enumerate() {
            let AppEntry {
                name,
                target_path,
                is_folder,
            } = entry;
            let (parent, aux_id) = match parents[i] {
                Some(pi) => (pi as u32, aux[i]),
                None => {
                    // 親を持たないエントリだけがフルパスを持つ（実データで 0.03%）。
                    table.push(target_path.into_boxed_str());
                    (NO_PARENT, (table.len() - 1) as u32)
                }
            };
            compact.push(CompactEntry {
                name: name.into_boxed_str(),
                parent,
                aux: aux_id,
                is_folder,
            });
        }

        Self {
            entries: compact,
            table,
        }
    }
}

/// 全エントリの（親 index, 拡張子）を導出する。導出どうしは独立ゆえ rayon で回す。
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
