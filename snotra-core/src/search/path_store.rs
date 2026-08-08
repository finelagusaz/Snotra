//! `target_path` をフォルダ木の接頭辞共有で保持する索引側の表現（#961/#962 に続く反復 3）。
//!
//! **木そのもの（何を持ち、どう辿るか）の正本は [`crate::index_tree`] である。** ここが持つ
//! のは索引側の並べ方（1 件 32 B の構造体の列）と、全件走査に効く 2 つの最適化——[`PathCursor`]
//! と [`PathStore::cmp_paths`] の高速路——だけである。
//!
//! この組み替え自体の実測（実 `index.bin` 312,377 エントリ・`PERFORMANCE.md`
//! 「`target_path` のフォルダ木接頭辞共有」）: 文字列 35.56 → 0.22 MiB、`entries` の 1 要素
//! 56 → 32 B、1 エントリあたりの確保ブロック 1 → 0。常駐は 97.51 → 54.92 MiB になり、
//! 対価は構築の 2〜3 → 66〜78 ms だった。検索経路はどれも遅くなっていない
//! ——`recent_history` は 9.9 → 6.4 ms、パスクエリのフレームコストは全件で改善した。
//!
//! **上の数値はこの組み替え時点のものであり、索引の現在値ではない。** [`CompactEntry`] は
//! その後 `file_name_is_lower_name` を得て `lower_file_names` の共有を担っており、常駐は
//! 45.21 MiB へ下がっている。**現在値は `PERFORMANCE.md` を正本とする**——ここに絶対値を
//! 書き足すと、次の反復のたびに 2 か所を直すことになる。
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

use super::footprint::{FootprintRow, boxed_strs, vec_body};
use crate::index_tree::{IndexTree, NO_PARENT, TreeNodes, raw_path_into, walk_to_root};
use crate::indexer::AppEntry;

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
    /// 真なら `lower_file_names[i]` の内容が `lower_names[i]` と同一であり、索引は前者の
    /// `Box<str>` を持たない（`None` に潰してある）。読み替えは `SearchEngine::entry_view`
    /// の 1 点だけで行う。
    ///
    /// **`is_folder` から推論してはならない。** 実データでは folder の 100% がこれに当たるが、
    /// それは indexer の名前導出規則の帰結であって `SearchEngine::new` が受け取る
    /// `AppEntry` の性質ではない。旗は構築時に**測った**結果であり（`build.rs` の `assemble`）、
    /// 外れた入力は文字列を持ち続けるだけで結果は変わらない（[`PathStore::sorted_by_path`]
    /// と同じ形である）。
    ///
    /// **この bool は無料である**——`Box<str>`(16) + `u32`×2(8) + `bool`(1) の後ろに 7 バイトの
    /// パディングがあり、2 つめの `bool` を置いても `CompactEntry` は 32 B のままである（実測）。
    /// 3 変種の `enum` で `lower_file_names` 側に持たせる案は `Box<str>` の niche が 1 つしか
    /// 無いため 16 → 24 B になり、削減 9.71 MiB のうち 2.38 MiB を食う。
    pub(super) file_name_is_lower_name: bool,
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
    /// 真なら index の順序がフルパスのバイト順と**厳密に**一致するので、[`Self::cmp_paths`] は
    /// 組み立てずに index を比べるだけでよい（`sort_entries_canonical` は第 1 キーが
    /// `target_path` であり、dedup により `target_path` は一意ゆえ第 1 キーだけで全順序が
    /// 決まる）。偽なら組み立てて比べる経路へ落ち、結果は変わらない。
    ///
    /// **狭義の単調増加でなければならない**——重複を許すと同じパスの 2 件へ index 比較が
    /// `Less`/`Greater` を返し、`Equal` が返らなくなる（判定は `build` のコメント）。
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

    /// `i` の [`CompactEntry::file_name_is_lower_name`] を立てる。
    ///
    /// **判定はここでしない**——旗の意味は派生文字列（`lower_names` / `lower_file_names`）に
    /// あり、`PathStore` はそれらを知らない。記憶域だけをここが持ち、何を立てるかは
    /// `build.rs` の `assemble` が決める。
    pub(super) fn mark_file_name_is_lower_name(&mut self, i: usize) {
        self.entries[i].file_name_is_lower_name = true;
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

    /// 常駐ヒープの内訳を数えて `rows` へ積む（計測専用・勘定の規約は `search/footprint.rs`
    /// の `//!`）。**自分の内側は自分で数える**——フィールドは private ゆえ、外から数えると
    /// 表現を変えるたびに数え方も外で直すことになる。
    pub(super) fn footprint_rows(&self, rows: &mut Vec<FootprintRow>) {
        // **`..` を書かない**（理由は `SearchEngine::footprint_rows`）。`sorted_by_path` は
        // `bool` ゆえヒープを持たないが、束縛して捨てる形にすることで網羅性は保たれる。
        let Self {
            entries,
            table,
            sorted_by_path: _,
        } = self;
        rows.push(boxed_strs(
            "PathStore: entries[].name",
            entries.iter().map(|e| &*e.name),
        ));
        rows.push(vec_body::<CompactEntry>(
            "PathStore: entries（Vec 本体）",
            entries.capacity(),
        ));
        rows.push(boxed_strs(
            "PathStore: table（親不在のフルパス + 拡張子）",
            table.iter().map(|s| &**s),
        ));
        rows.push(vec_body::<Box<str>>(
            "PathStore: table（Vec 本体）",
            table.capacity(),
        ));
    }

    /// `i` のフルパスを**原文のまま**組み立てる（小文字化も `/` → `\` 変換も trim もしない）。
    ///
    /// 結果は元の `AppEntry.target_path` とバイト一致する。合成 fixture での保証は
    /// `search/tests/path.rs` の `path_store_cursor_matches_full_rebuild` が常に持ち、実 `index.bin`
    /// の全件（実測 312,377 件）での照合は同ファイルの
    /// `path_store_raw_matches_target_path_over_real_index` が受け持つ——**後者は実インデックスの
    /// 無い環境（CI）では自動スキップする**ので、開発機で走る corpus であって保証ではない。
    ///
    /// **規則は [`raw_path_into`] が唯一持つ。** ディスク側の並べ方（[`IndexTree`]）も同じ
    /// 実装を通るので、両者がずれることは表現できない。
    pub(super) fn raw_into(&self, buf: &mut String, i: usize) {
        raw_path_into(self, buf, i);
    }

    /// `i` の正規化キー（`normalize_entry_key` と同じ規則）を組み立てる。
    ///
    /// **組み立ててから正規化する二段払いにはしない**——正規化バッファへ直接書き出す 1 パスで
    /// あり、増分は段数ぶんの読みだけである。規則の正本は
    /// [`crate::indexer::normalize_entry_key_into`] である。
    ///
    /// **製品はこれを呼ばない**——全件走査は [`PathCursor`] が祖先の鎖を持ち回る形で組み立てる。
    /// ここに残すのは**カーソルの正しさを固定する参照実装**としてであり、鎖の状態に依らない
    /// この素直な形と 1 バイトも違わないことを `path_store_cursor_matches_full_rebuild` が
    /// 順・逆順・乱順の 3 通りで検証する（カーソルは最適化であって意味の変更ではない）。
    #[cfg(test)]
    pub(super) fn normalized_into(&self, buf: &mut String, i: usize) {
        let (root, chain, depth) = walk_to_root(self, i);
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

    #[inline]
    fn push_separator(&self, buf: &mut String) {
        // 親がドライブ直下（`C:\`）なら既に区切りで終わっている。
        if buf.as_bytes().last() != Some(&b'\\') {
            buf.push('\\');
        }
    }
}

/// 索引側の並べ方（1 件 32 B の構造体の列）を [`raw_path_into`] へ差し出す。
///
/// **単型化されるので、辿る段はこれまでどおり [`CompactEntry`] のフィールドを直接読む形へ
/// コンパイルされる**——`parent` と `name` が同じキャッシュラインに載る利得は失われない
/// （[`CompactEntry`] の doc がその測定を持つ）。
impl TreeNodes for PathStore {
    #[inline]
    fn parent_of(&self, i: usize) -> u32 {
        self.entries[i].parent
    }
    #[inline]
    fn aux_of(&self, i: usize) -> u32 {
        self.entries[i].aux
    }
    #[inline]
    fn name_of(&self, i: usize) -> &str {
        &self.entries[i].name
    }
    #[inline]
    fn table_str(&self, id: u32) -> &str {
        &self.table[id as usize]
    }
}

/// 全件走査のあいだ、直前に組み立てた祖先の鎖を持ち回るカーソル。
///
/// **索引はソート順に並んでいるので、隣り合うエントリは祖先をほぼ共有する。** 毎回根まで
/// 辿って全部書き直すと、共有している部分を何度も書き直すことになる。鎖を持ち回れば、
/// 大半のエントリは**バッファを巻き戻して 1 段だけ書き足す**だけで済む。
///
/// 親が鎖に載っているかの判定は `u32` の線形走査（高々 [`crate::index_tree::CHAIN_CAP`] 個・実測の深さは平均
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
    ///
    /// **記録側（`normalize_entry_key`）と 1 バイトも違わないことがここの契約である**——ずれると
    /// 履歴照合が沈黙で外れる（クラッシュせず検索結果も返り、ブーストだけが消える）。合成 fixture
    /// での検証は `search/tests/path.rs` の `path_store_cursor_matches_full_rebuild`（順・逆順・
    /// 乱順）、実 `index.bin` の全件での照合は同ファイルの
    /// `path_store_cursor_matches_normalize_entry_key_over_real_index`（**実インデックスが無ければ
    /// 自動スキップ**）。
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
                let (root, chain, depth) = walk_to_root(store, i);
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

impl PathStore {
    /// [`IndexTree`] を索引側の並べ方（1 件 32 B の構造体の列）へ組み替えて受け取る。
    ///
    /// **並べ方を変えるだけで、木そのものは作り直さない。** 親と拡張子の解決は
    /// [`IndexTree::build`] が済ませてあり、`index.bin` から読んだ木では**保存した側が**
    /// 済ませてある——ここが「ディスクの木表現」がロードと構築の両方に効く理由である。
    ///
    /// 名前は `String` から `Box<str>` へ移す。[`CompactEntry`] を 32 B に保つための 8 B で
    /// あり、全件で 2.4 MiB に相当する（容量が長さと等しければ確保は起きない）。
    ///
    /// **長さの一致は [`IndexTree::from_parts`] が確かめている。** ここで `zip` が短い側で
    /// 黙って止まらないのはそのためであり、`zip` 自身が保証するのではない。
    pub(super) fn adopt(tree: IndexTree) -> Self {
        let IndexTree {
            names,
            is_folder,
            parent,
            aux,
            table,
            sorted_by_path,
        } = tree;
        debug_assert_eq!(names.len(), is_folder.len());
        debug_assert_eq!(names.len(), parent.len());
        debug_assert_eq!(names.len(), aux.len());
        let entries = names
            .into_iter()
            .zip(is_folder)
            .zip(parent)
            .zip(aux)
            .map(|(((name, is_folder), parent), aux)| CompactEntry {
                name: name.into_boxed_str(),
                parent,
                aux,
                is_folder,
                // 旗は `build.rs` の `assemble` が派生文字列を測ってから立てる。
                // 木は `target_path` の形しか知らないので判定できない。
                file_name_is_lower_name: false,
            })
            .collect();
        Self {
            entries,
            table: table.into_iter().map(String::into_boxed_str).collect(),
            sorted_by_path,
        }
    }

    /// `Vec<AppEntry>` から木を建てて受け取る（テストの fixture 用）。
    ///
    /// **製品は通らない**——`SearchEngine` の全経路は `IndexTree` を受け取る形へ移った。
    #[cfg(test)]
    ///
    /// 木を建てる規則そのものは [`IndexTree::build`] が持つ——`index.bin` へ書く側と
    /// ここが**同じ 1 つを通る**ことが、ディスクと索引で木が一致する根拠である。
    pub(super) fn build(entries: Vec<AppEntry>) -> Self {
        Self::adopt(IndexTree::build(entries))
    }
}
