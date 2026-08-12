//! 索引の常駐ヒープの内訳を、**構築後の [`SearchEngine`] そのもの**から数える（計測専用）。
//!
//! 出力先は `tests/memory_footprint.rs`（計測値は `PERFORMANCE.md`）。あちらは計数アロケータで
//! **合計**を測り、こちらは同じ合計を項目へ割る。両者の差が「未帰属」として残る。
//!
//! # なぜ製品側に置くか
//!
//! 統合テストは別クレートゆえ [`SearchEngine`] の private フィールドへ届かない。反復 3 以前は
//! 構築**前**の `Vec<AppEntry>` を走査して代用していたが、`target_path` の `String` が
//! [`super::path_store::PathStore`] へ組み替えられて解放される今、その走査は
//! **もう存在しない物体を測っている**（実測: 内訳 122.52 MiB 対 常駐 54.92 MiB）。
//! 内訳が嘘になると候補の順位が付けられないので、走査をレイアウトの隣へ移した。
//!
//! # 網羅性はコンパイラに担保させる
//!
//! [`SearchEngine::footprint_rows`] は `self` を**網羅的に分解**する（`..` を書かない）。
//! 並列 Vec を足したときに、`CLAUDE.md` のチェックリストではなくコンパイラが漏れを捕まえる
//! ——文書の契約ではなく構造で担保する形である。**`..` を足してはならない。**
//!
//! # 数える勘定
//!
//! アロケータが数えるのは `layout.size()` ゆえ、`Vec` は `capacity × size_of`、`Box<str>` は
//! `len`（余剰容量を持たない）で数える。**確保ブロック数も別軸として持つ**——1 エントリあたり
//! 小さな確保をいくつ持つかがアロケータ由来のオーバーヘッドを決めるので、バイトだけでは
//! 候補を比べられない（このハーネス群が一貫して 2 軸で測っている理由は
//! `tests/memory_footprint.rs` の `//!`）。

use super::SearchEngine;

/// 常駐の内訳 1 行。
pub struct FootprintRow {
    /// 何を数えた行か。
    pub label: &'static str,
    /// ヒープ確保量。アロケータが数える `layout.size()` と同じ勘定。
    pub bytes: usize,
    /// 確保ブロック数。**長さ 0 の確保は数えない**——`Vec::new()` も `Box::from("")` も
    /// アロケータを呼ばずダングリングポインタを持つので、数えると実測と合わなくなる。
    pub blocks: usize,
    /// 数えた対象の個数（`Vec` 本体の行は容量、文字列群の行は本数）。
    /// 個数に意味を持たない行では 0。
    pub count: usize,
}

impl FootprintRow {
    pub(super) const fn empty(label: &'static str) -> Self {
        Self {
            label,
            bytes: 0,
            blocks: 0,
            count: 0,
        }
    }

    /// 文字列 1 本ぶんを足し込む。
    pub(super) fn add_str(&mut self, s: &str) {
        self.bytes += s.len();
        self.blocks += usize::from(!s.is_empty());
        self.count += 1;
    }
}

/// **1 つの確保**から成る行。ブロック数の導出（`bytes > 0`）はここ 1 か所に閉じる。
///
/// バイト数を呼び出し側が渡すのは、アリーナの部品（連結バイト列 / オフセット / 旗）が
/// 確保バイトを型の側（`footprint_bytes`）で知っているからである。
///
/// **旗 2 本のように 2 つの確保を 1 行へ束ねてはならない**——ブロックを 1 つ数え落とす
/// （**未帰属 +1 blocks として実測に出た**。バイトは合っていたので、バイトだけの検算では
/// 捕まらない）。
pub(super) fn arena_part(label: &'static str, bytes: usize, count: usize) -> FootprintRow {
    FootprintRow {
        label,
        bytes,
        blocks: usize::from(bytes > 0),
        count,
    }
}

/// `Vec` 本体（ヒープ上の連続領域）の行。中身とは別勘定である。
///
/// 掛け算だけがここの仕事で、行の組み立ては [`arena_part`] に委ねる。
pub(super) fn vec_body<T>(label: &'static str, capacity: usize) -> FootprintRow {
    arena_part(label, capacity * std::mem::size_of::<T>(), capacity)
}

/// `Box<str>` の並びの行（中身のバイトと本数）。
pub(super) fn boxed_strs<'a>(
    label: &'static str,
    it: impl Iterator<Item = &'a str>,
) -> FootprintRow {
    let mut row = FootprintRow::empty(label);
    for s in it {
        row.add_str(s);
    }
    row
}

impl SearchEngine {
    /// 常駐ヒープの内訳を、構築後の自分自身を走査して数える（**計測専用**）。
    ///
    /// 製品は呼ばない。`tests/memory_footprint.rs` が計数アロケータの実測と突き合わせるための
    /// 唯一の観測口であり、private フィールドを別クレートから測る手段が他に無いために `pub`
    /// にしてある（モジュールの `//!`）。
    ///
    /// 返す行の合計は**実測を超えてはならない**——超えたなら丸めではなく二重計上である。
    #[doc(hidden)]
    pub fn footprint_rows(&self) -> Vec<FootprintRow> {
        // **`..` を書かない。** フィールドを足したらここが compile error になるのが要点である。
        let Self {
            entries,
            lower_names,
            lower_file_names,
            char_masks,
            file_name_char_masks,
            kana_lower_names,
            kana_char_masks,
            // ヒープ確保を持たない `bool` ゆえ行は出さない（この表が数えるのは確保である）。
            // **「パディングへ収まる」とは書かない**——`SearchEngine` の他メンバは全て align 8 で
            // 隙間が無く、実体は 8 B の純増である（導出。実測はしていない）。
            any_name_has_path_sep: _,
            incremental_cache,
        } = self;

        let mut rows = Vec::new();
        entries.footprint_rows(&mut rows);

        // **共有漏れは「件数だけ」の行で出す。** かつては一致する分・しない分をバイトで割って
        // いたが、実体がアリーナの連結バイト列に入った今、ここでバイトを足すと下のアリーナ行と
        // **二重計上**になる（帰属 100% の検算が壊れる）。
        //
        // **それでも行を消さない。** 潰しが効いていれば `count` は 0 になるが、消すと
        // 「測って 0 だった」と「測っていない」が区別できなくなる——この行が 0 であること
        // 自体が、`assemble` の共有判定が効いている証拠である。**0 でなくなったら、
        // 潰す判定か潰す位置が壊れている**（結果は正しいまま削減だけが減るので、挙動テストは
        // 緑のまま通る種類の誤りである）。
        let mut leaked_names = FootprintRow::empty("lower_names（= 表示名なのに実体を持つ・件数）");
        for i in 0..lower_names.len() {
            leaked_names.count += usize::from(lower_names.get(i) == Some(entries.name_at(i)));
        }
        rows.push(leaked_names);

        let (blob, offsets, present) = lower_names.footprint_bytes();
        rows.push(arena_part(
            "lower_names（アリーナの連結バイト列）",
            blob,
            lower_names.len(),
        ));
        rows.push(arena_part(
            "lower_names（アリーナのオフセット）",
            offsets,
            lower_names.len() + 1,
        ));
        rows.push(arena_part(
            "lower_names（present 旗）",
            present,
            lower_names.len(),
        ));

        // 比較相手は**解決後**の `lower_name`（鎖の上段が潰れていれば表示名）。旗が立って
        // いる分は列に実体を持たないので、ここへは現れない。
        let mut leaked_files =
            FootprintRow::empty("lower_file_names（= lower_names なのに実体を持つ・件数）");
        for i in 0..lower_file_names.len() {
            let Some(s) = lower_file_names.text_at(i) else {
                continue;
            };
            let lower_name = lower_names.get(i).unwrap_or_else(|| entries.name_at(i));
            leaked_files.count += usize::from(lower_name == s);
        }
        rows.push(leaked_files);

        // **旗は 2 行に分ける。** `arena_part` は「1 行 = 1 確保」でブロックを数えるので、
        // 2 本を束ねると 1 つ数え落とす（**未帰属 +1 blocks として実測に出た**）。
        let (blob, offsets, present, same_as_lower) = lower_file_names.footprint_bytes();
        rows.push(arena_part(
            "lower_file_names（アリーナの連結バイト列）",
            blob,
            lower_file_names.len(),
        ));
        rows.push(arena_part(
            "lower_file_names（アリーナのオフセット）",
            offsets,
            lower_file_names.len() + 1,
        ));
        rows.push(arena_part(
            "lower_file_names（present 旗）",
            present,
            lower_file_names.len(),
        ));
        rows.push(arena_part(
            "lower_file_names（same_as_lower 旗）",
            same_as_lower,
            lower_file_names.len(),
        ));

        rows.push(vec_body::<u64>("char_masks", char_masks.capacity()));
        rows.push(vec_body::<u64>(
            "file_name_char_masks",
            file_name_char_masks.capacity(),
        ));

        // migemo 無効なら 2 本とも空で、行は 0 のまま残る（**消さない**——「測って 0 だった」と
        // 「測っていない」は別物であり、消すと後者に見える）。
        rows.push(boxed_strs(
            "kana_lower_names",
            kana_lower_names.iter().map(|s| &**s),
        ));
        rows.push(vec_body::<Box<str>>(
            "kana_lower_names（Vec 本体）",
            kana_lower_names.capacity(),
        ));
        rows.push(vec_body::<u64>(
            "kana_char_masks",
            kana_char_masks.capacity(),
        ));

        rows.push(incremental_cache.footprint_row());
        rows
    }
}

impl super::IncrementalCache {
    /// incremental search の前回状態。構築直後は `Default`（全部空）ゆえ 0 になるが、
    /// **行は出す**——網羅的な分解の相手として、0 であることを示すのが役目である。
    fn footprint_row(&self) -> FootprintRow {
        let Self {
            prev_query,
            prev_candidates,
            // `Option<SearchMode>` はヒープを持たない。
            prev_mode: _,
            prev_kana_query,
        } = self;

        let mut row = FootprintRow::empty("incremental_cache（前回クエリ・候補）");
        // `String` / `Vec` は伸長しうるので **容量** で数える（`Box<str>` と違い余剰を持つ）。
        for bytes in [
            prev_query.capacity(),
            prev_candidates.capacity() * std::mem::size_of::<usize>(),
            prev_kana_query.as_ref().map_or(0, String::capacity),
        ] {
            row.bytes += bytes;
            row.blocks += usize::from(bytes > 0);
        }
        row
    }
}
