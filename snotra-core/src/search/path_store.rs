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
//! **払うのは構築の 2〜3 → 66〜78 ms だけである。** 検索経路はどれも遅くなっていない
//! ——`recent_history` は 9.9 → 6.4 ms、パスクエリのフレームコストは全件で改善した。
//! 判定の根拠はすべて `PERFORMANCE.md` の実測にある。
//!
//! # 組み立ての 2 系統
//!
//! 読み手は 2 種類あり、**流用してはならない**:
//!
//! - [`PathStore::raw_into`] — 原文のまま。表示パス（`SearchResult.path`）と tie-break が使う
//! - [`PathCursor::normalized`] — 小文字化 + `/` → `\`。履歴照合とパスマッチが使う
//!
//! 正規化側は「組み立ててから正規化する」二段払いにせず、正規化バッファへ直接書き出す。
//!
//! # 全件走査は 1 件ずつ独立に組み立てない
//!
//! **素直に「1 件ごとに根まで辿って書き直す」と全経路が 1.6〜1.9 倍遅くなる**（実測）。
//! 速さは走査の形から来る:
//!
//! - 索引はソート順ゆえ**隣り合うエントリは祖先を共有する** → [`PathCursor`] が鎖を持ち回り、
//!   バッファを巻き戻して 1 段だけ書き足す
//! - 整列済みなら**index の順序がフルパスのバイト順と一致する** → [`PathStore::cmp_paths`] は
//!   組み立てずに index を比べる（tie-break は `c:\` のようなクエリで総当たりに発火する）
//!
//! どちらも**仮定ではなく構築時の実測**に載っている（`sorted_by_path`）。外れた入力は
//! 遅い経路を通るだけで、結果は変わらない。

use std::cmp::Ordering;
use std::collections::HashMap;

use rayon::prelude::*;

use crate::indexer::AppEntry;

/// 親を持たないことを表す番兵。このとき `aux` は [`PathStore::table`] のフルパスを指す。
const NO_PARENT: u32 = u32::MAX;

/// 祖先の鎖の長さの上限。**構築時に強制する**（[`PathStore::build`]）ので、組み立て側は
/// 鎖がこれを超えないことを前提にしてよい。
///
/// 上限を組み立て側の打ち切りで実現してはならない——打ち切ると「根でないもの」を根として
/// 扱い、`table[aux]`（拡張子）をパスの先頭として書いてしまう。**短いパスではなく誤った
/// パスになり、しかも黙って通る。** 構築時に上限超過を根へ落とせば（＝自分のフルパスを
/// 持たせれば）、その状態は表現できなくなる。実データの最大深さは 17 段。
const CHAIN_CAP: usize = 64;

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
    /// `target_path` のバイト順に並んでいるか。**仮定ではなく構築時の実測である。**
    ///
    /// 真なら index の順序がフルパスのバイト順と一致するので、[`Self::cmp_paths`] は
    /// 組み立てずに index を比べるだけでよい（`sort_entries_canonical` は第 1 キーが
    /// `target_path` であり、dedup により `target_path` は一意ゆえ第 1 キーだけで全順序が
    /// 決まる）。偽なら組み立てて比べる経路へ落ち、結果は変わらない。
    ///
    /// **「本番は必ず整列している」という文書上の契約に寄りかからない**のが要点である
    /// ——`SearchEngine::new` は任意順を受け取れるので、契約にすると破れたとき静かに
    /// 順序が変わる。測って持てば、破れた入力は遅い経路を通るだけになる。
    sorted_by_path: bool,
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
    ///
    /// **製品はこれを呼ばない**——全件走査は [`PathCursor`] が祖先の鎖を持ち回る形で組み立てる。
    /// ここに残すのは**カーソルの正しさを固定する参照実装**としてであり、鎖の状態に依らない
    /// この素直な形と 1 バイトも違わないことを `path_store_cursor_matches_full_rebuild` が
    /// 順・逆順・乱順の 3 通りで検証する（カーソルは最適化であって意味の変更ではない）。
    #[cfg(test)]
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
        // 整列済みなら index の順序がフルパスのバイト順と一致する（[`Self::sorted_by_path`]）。
        // **これは効き所である**——`c:\` のような全件が同スコアになるクエリでは tie-break が
        // 総当たりで発火し、1 回ごとに両辺を組み立て直すと走査全体を支配する。
        if self.sorted_by_path {
            return a.cmp(&b);
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

    /// 根まで辿って途中の index を積む。返すのは（根, 鎖, 段数）。`chain[0]` が `i` 自身、
    /// `chain[depth - 1]` が根の直下である。
    ///
    /// **鎖が配列に収まることは構築時に保証されている**（[`CHAIN_CAP`]）。`parent < self` が
    /// 循環を、深さの打ち止めが長さを、それぞれ表現不能にしているので、ここに打ち切りの
    /// 分岐は要らない——`debug_assert` は不変条件が壊れたときに黙って誤ったパスを返す
    /// のではなく落ちるためにある。
    #[inline]
    fn walk_to_root(&self, i: usize) -> (usize, [u32; CHAIN_CAP], usize) {
        let mut chain = [0u32; CHAIN_CAP];
        let mut depth = 0usize;
        let mut cur = i;
        while self.entries[cur].parent != NO_PARENT {
            debug_assert!(
                depth < CHAIN_CAP,
                "鎖が CHAIN_CAP を超えた（構築側の不変条件違反）"
            );
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

/// 全件走査のあいだ、直前に組み立てた祖先の鎖を持ち回るカーソル。
///
/// **索引はソート順に並んでいるので、隣り合うエントリは祖先をほぼ共有する。** 毎回根まで
/// 辿って全部書き直すと、共有している部分を何度も書き直すことになる。鎖を持ち回れば、
/// 大半のエントリは**バッファを巻き戻して 1 段だけ書き足す**だけで済む。
///
/// 親が鎖に載っているかの判定は `u32` の線形走査（高々 [`CHAIN_CAP`] 個・実測の深さは平均
/// 6.05 段）であり、メモリを追いかけない。外れたときは根まで辿って作り直すので、
/// **順序が乱れた入力でも結果は変わらない**（速さだけが落ちる）。
///
/// 効く根拠は構築側と同じ性質である——`PathStore::build` で親解決の照合表を「直前の親の
/// 使い回し」に替えたとき 152 → 23 ms になった。
pub(super) struct PathCursor {
    buf: String,
    /// いま `buf` に載っている鎖: `(index, その段まで書いた時点の buf の長さ)`。
    /// 先頭が根で、末尾が直前に組み立てたエントリ。
    stack: Vec<(u32, u32)>,
}

impl PathCursor {
    pub(super) const fn new() -> Self {
        Self {
            buf: String::new(),
            stack: Vec::new(),
        }
    }

    /// `i` の正規化キーを組み立てて貸す。鎖の状態に依らない素直な組み立て（`PathStore::normalized_into`・`cfg(test)`）と**必ず同じバイト列**を
    /// 返す（差し替えは最適化であって意味の変更ではない）。
    pub(super) fn normalized(&mut self, store: &PathStore, i: usize) -> &str {
        let parent = store.entries[i].parent;
        if parent == NO_PARENT {
            // 根そのもの。鎖を作り直して終わり（下の共通処理を通さない——通すと
            // 自分自身を 2 回書いてしまう）。
            self.buf.clear();
            self.stack.clear();
            push_segment(
                &mut self.buf,
                store.table[store.entries[i].aux as usize].trim(),
            );
            self.buf.make_ascii_lowercase();
            self.stack.push((i as u32, self.buf.len() as u32));
            return &self.buf;
        }

        match self.stack.iter().rposition(|&(idx, _)| idx == parent) {
            // 当たり: 親の段まで巻き戻す。共有している接頭辞は書き直さない。
            Some(pos) => {
                self.buf.truncate(self.stack[pos].1 as usize);
                self.stack.truncate(pos + 1);
            }
            // 外れ: 根まで辿って鎖ごと作り直す。`chain[0]` は `i` 自身なので下の共通処理へ譲る。
            None => {
                let (root, chain, depth) = store.walk_to_root(i);
                self.buf.clear();
                self.stack.clear();
                push_segment(
                    &mut self.buf,
                    store.table[store.entries[root].aux as usize].trim(),
                );
                self.buf.make_ascii_lowercase();
                self.stack.push((root as u32, self.buf.len() as u32));
                for d in (1..depth).rev() {
                    self.append(store, chain[d] as usize);
                }
            }
        }
        self.append(store, i);
        &self.buf
    }

    /// 1 段ぶんを書き足して鎖へ積む。**小文字化は書き足した範囲だけに当てる**——
    /// 全体へ当て直すと、巻き戻しで節約したぶんを取り戻してしまう。
    #[inline]
    fn append(&mut self, store: &PathStore, idx: usize) {
        store.push_separator(&mut self.buf);
        let start = self.buf.len();
        push_segment(&mut self.buf, &store.entries[idx].name);
        push_segment(&mut self.buf, &store.table[store.entries[idx].aux as usize]);
        self.buf[start..].make_ascii_lowercase();
        self.stack.push((idx as u32, self.buf.len() as u32));
    }
}

/// スレッドローカルのカーソルで `i` の正規化キーを組み立て、`&str` として貸す。
///
/// **正規化キーを得る経路はここ 1 本である**（`search/scoring.rs` の `with_normalized_key` が
/// 唯一の呼び出し元で、検索の `score_one_entry` と空クエリの `recent_history` が共有する）。
///
/// 借用は `f` の中に閉じる。**`f` の中からこの関数を再び呼んではならない**——`borrow_mut` の
/// 二重取得で panic する（誤りは沈黙せず落ちる）。
pub(super) fn with_cursor<R>(store: &PathStore, i: usize, f: impl FnOnce(&str) -> R) -> R {
    CURSOR.with(|cell| {
        let mut cursor = cell.borrow_mut();
        let key = cursor.normalized(store, i);
        f(key)
    })
}

thread_local! {
    /// 全件走査のカーソル。rayon の worker ごとに 1 本で、`MATCHER` と同じ形である。
    /// 走査は連続した index を順に舐めるので、worker ごとに鎖が暖まる。
    static CURSOR: std::cell::RefCell<PathCursor> =
        const { std::cell::RefCell::new(PathCursor::new()) };

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
        // 整列の判定は並列で 1 回だけ。全件走査の tie-break がこの 1 bit に載る。
        let sorted_by_path = entries
            .par_windows(2)
            .all(|w| w[0].target_path <= w[1].target_path);
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
        // 鎖の長さは**ここで**打ち止める。`parent < i` ゆえ親の深さは既に確定しており、
        // 1 引き算で数えられる（[`CHAIN_CAP`] の doc に理由）。
        let mut depths: Vec<u16> = Vec::with_capacity(n);
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
                    table.push(target_path.into_boxed_str());
                    (NO_PARENT, (table.len() - 1) as u32, 0)
                }
            };
            depths.push(depth);
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
            sorted_by_path,
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
