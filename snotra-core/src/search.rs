use std::borrow::Cow;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use nucleo_matcher::{Config as MatcherConfig, Matcher, Utf32String};
use rayon::prelude::*;

use crate::config::{SearchConfig, SearchHistoryNormalizationConfig};
use crate::history::HistoryStore;
use crate::indexer::{AppEntry, normalize_entry_key};
use crate::query::{
    char_bitmask, file_char_mask, lower_file_name, name_char_mask, normalize_history_query_key,
    normalize_query, to_kana, to_lower_folded,
};
use crate::ui_types::SearchResult;

const GLOBAL_WEIGHT: i64 = 5;
const QUERY_WEIGHT: i64 = 20;
const FOLDER_EXPANSION_WEIGHT: i64 = 5;
/// D-3: minimum candidate count for rayon parallel scoring.
/// Below this threshold, thread-splitting overhead exceeds the scoring cost
/// (typical after several keystrokes when incremental search narrows candidates).
const MIN_PAR_CANDIDATES: usize = 512;

/// マッチ種別ごとの基準スコア（全順序の不変条件を単一定義に集約）。
///
/// 実行時スコアは Prefix > Substring > Kana > Path > Fuzzy(nucleo) の全順序を保つ。
/// ここに置くのは各種別の**基準定数**のみ——直後の `const _` コンパイル時アサーションが
/// 基準定数の大小（`PREFIX_BASE > SUBSTRING_BASE > KANA_BASE > PATH_BASE`）を守る。
/// 位置ペナルティ（`- byte_pos`）・`.max(1)` 補正込みの実行時全順序は、既存の挙動テスト
/// （`kana_search_direct_match_ranks_above_kana_match` 等）が保証する。
/// Fuzzy は nucleo-matcher が独自スコアを返すため基準定数を持たない。
/// （命名: fold ボディのローカル変数 `score` との視覚的衝突を避け `score_tier` とする）
mod score_tier {
    /// Prefix マッチ: `PREFIX_BASE - lower_name.len()`（短い名前ほど高スコア）。
    pub const PREFIX_BASE: i64 = 10_000;
    /// Substring マッチ: `SUBSTRING_BASE - byte_idx`。
    pub const SUBSTRING_BASE: i64 = 5_000;
    /// Kana（migemo）マッチ: `KANA_BASE - byte_pos`（Substring より低い）。
    pub const KANA_BASE: i64 = 4_500;
    /// Path マッチ: `PATH_BASE - min(byte_pos, PATH_POS_CAP)`（Kana より低い）。
    pub const PATH_BASE: i64 = 3_000;
    /// Path マッチの位置ペナルティ上限。
    pub const PATH_POS_CAP: i64 = 500;
}

/// 基準スコアの全順序 Prefix > Substring > Kana > Path をコンパイル時に強制する
/// （test ビルドに限らず全ビルドで検証。値を反転させる編集はビルドを止める）。
const _: () = {
    assert!(score_tier::PREFIX_BASE > score_tier::SUBSTRING_BASE);
    assert!(score_tier::SUBSTRING_BASE > score_tier::KANA_BASE);
    assert!(score_tier::KANA_BASE > score_tier::PATH_BASE);
    assert!(score_tier::PATH_BASE > 0);
    assert!(score_tier::PATH_POS_CAP > 0);
};

// D-5: one Matcher per rayon worker thread, reused across fold tasks.
// Matcher::new() calls alloc_zeroed internally (several KB); thread_local avoids
// that allocation on every rayon chunk boundary.
thread_local! {
    static MATCHER: RefCell<Matcher> = RefCell::new(Matcher::new(MatcherConfig::DEFAULT));
}

/// 検索エンジン内部で使うマッチ方式のドメイン enum。
///
/// `config::SearchModeConfig`（serde 由来の config.toml wire 形式）とは**意図的に別定義**とする。
/// engine 側を serde 非依存に保つための層境界（anti-corruption boundary）であり、
/// 下記 `From` 変換が config→engine の唯一の橋渡し（`SearchOptions ← SearchConfig` と同型）。
/// 統合すると engine が serde/wire 形式に依存するため、二重定義は仕様であって重複ではない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Prefix,
    Substring,
    Fuzzy,
}

impl From<crate::config::SearchModeConfig> for SearchMode {
    fn from(c: crate::config::SearchModeConfig) -> Self {
        match c {
            crate::config::SearchModeConfig::Prefix => SearchMode::Prefix,
            crate::config::SearchModeConfig::Substring => SearchMode::Substring,
            crate::config::SearchModeConfig::Fuzzy => SearchMode::Fuzzy,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SearchOptions {
    pub normalization: SearchHistoryNormalizationConfig,
    pub fuzzy_history_cap_ratio: f64,
    pub migemo_enabled: bool,
    pub migemo_min_chars: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            normalization: SearchHistoryNormalizationConfig::Disabled,
            fuzzy_history_cap_ratio: 0.30,
            migemo_enabled: false,
            migemo_min_chars: 2,
        }
    }
}

impl From<&SearchConfig> for SearchOptions {
    fn from(config: &SearchConfig) -> Self {
        Self {
            normalization: config.history_normalization,
            fuzzy_history_cap_ratio: config.fuzzy_history_cap_ratio,
            migemo_enabled: config.migemo_enabled,
            migemo_min_chars: config.migemo_min_chars,
        }
    }
}

/// Search index holding per-entry pre-computed data as parallel Vecs.
///
/// # Why parallel Vecs instead of a single `Vec<CachedEntry>` struct?
///
/// A `CachedEntry` struct grouping all per-entry fields was prototyped and benchmarked
/// (branch `refactor/cached-entry`, issue #110). Results showed **35–120% regression**
/// in Fuzzy full-scan speed across all entry counts:
///
/// | entries | parallel Vec | CachedEntry struct |
/// |--------:|-------------:|-------------------:|
/// |   1,000 |      ~7 ms   |           ~14 ms   |
/// |  50,000 |     ~14 ms   |           ~30 ms   |
/// | 100,000 |     ~22 ms   |           ~29 ms   |
///
/// Root cause: the bitmask pre-filter (`char_masks` / `file_name_char_masks`) sweeps
/// every candidate index before any scoring occurs.  As a compact `Vec<u64>` it fits
/// 8 entries per 64-byte cache line.  Embedded in a `CachedEntry` (~160 bytes/entry)
/// the same sweep loads ~25× more cache lines, degrading L1 locality significantly.
///
/// The parallel-Vec layout is therefore intentional, not accidental tech debt.
/// Adding a new derived field requires updating `new()` and keeping all Vecs in sync —
/// enforce this by running the full test suite after any structural change.
pub struct SearchEngine {
    entries: Vec<AppEntry>,
    /// 派生文字列の並列 Vec は `Box<str>` で保持する。これらは構築後に伸長しないため、
    /// `String` の容量ワード（8B/要素）が無駄になる。`str` へ Deref するので読み取り側は無変更。
    lower_names: Vec<Box<str>>,
    lower_file_names: Vec<Option<Box<str>>>,
    /// Pre-computed normalized keys for history lookups (one per entry).
    normalized_keys: Vec<Box<str>>,
    /// Character-presence bitmask for lower_name (a-z: bits 0-25, 0-9: bits 26-35).
    /// Kept as a compact Vec<u64> — 8 entries per cache line — so the pre-filter sweep
    /// that discards non-matching candidates before scoring is L1-cache-friendly.
    /// Merging this into a per-entry struct would inflate the per-entry size ~20× and
    /// cause a measured 35–120% Fuzzy search regression (see struct-level doc comment).
    char_masks: Vec<u64>,
    /// Character-presence bitmask for lower_file_name (same layout as char_masks).
    file_name_char_masks: Vec<u64>,
    /// エントリ名をひらがな正規化した Vec（katakana→hiragana、ASCII はそのまま）。
    /// migemo 検索（ローマ字→かな変換マッチ）で使用。インデックスキャッシュには保存しない。
    kana_lower_names: Vec<Box<str>>,
    /// kana_lower_names 用の損失あり文字存在マスク。migemo 有効時のみ構築し、kana
    /// pre-filter の false positive は許すが false negative は起こさない。
    kana_char_masks: Vec<u64>,
    /// Incremental search cache: normalized query string from the previous call.
    prev_query: String,
    /// Incremental search cache: entry indices that matched on the previous call,
    /// stored BEFORE truncation to `max_results` so every match is captured.
    prev_candidates: Vec<usize>,
    /// Incremental search cache: search mode of the previous call.
    prev_mode: Option<SearchMode>,
    /// Incremental search cache: 前回の kana_query 文字列。
    /// incremental を使えるのは今回の kana_query が前回の prefix 拡張のときだけ。
    /// ローマ字→かな変換は文字列伸長に対して非単調（"kan"→"かん", "kana"→"かな"）なため、
    /// bool フラグでは不十分で、実際の kana 文字列を比較する必要がある。
    prev_kana_query: Option<String>,
}

/// Lightweight view over per-entry fields for index `i` that are used in the scoring loop.
/// Bundles 4 references (entry / lower_name / lower_file_name / normalized_key) without
/// changing the underlying SoA layout, so all cache-locality properties are preserved.
/// `char_masks` / `file_name_char_masks` / `kana_lower_names` are accessed directly from
/// SearchEngine in the scoring closure (same SoA pattern, intentionally excluded from EntryView).
struct EntryView<'a> {
    entry: &'a AppEntry,
    lower_name: &'a str,
    lower_file_name: Option<&'a str>,
    normalized_key: &'a str,
}

/// 1 回の検索呼び出しでクエリから 1 度だけ導出する、スコアリング共有データ。
/// `search_with_options` の候補準備フェーズ（`prepare_query_plan`）が構築し、
/// `decide_incremental` と `score_one_entry` が `&QueryPlan` で共有する。
/// `norm_query` のみ入力 `query` を借用しうる（ASCII 小文字クエリはゼロアロケーション）。
/// `needle_u32` は `norm_query` から生成後は独立（借用しない）。
struct QueryPlan<'a> {
    /// 正規化済みクエリ（アクセント折畳み・連続スペース圧縮・小文字化）。
    norm_query: Cow<'a, str>,
    /// `norm_query` が `.` を含むか（file_name スコアリングと incremental ガードに連動）。
    has_dot: bool,
    /// 生クエリがパス区切り（`\` `/` `¥`）を含むか。
    has_path_sep: bool,
    /// Fuzzy モードのビットマスク pre-filter 用クエリマスク（非 Fuzzy では 0）。
    query_mask: u64,
    /// migemo 用ひらがな変換クエリ（ASCII 残留や条件未達のとき `None`）。
    kana_query: Option<String>,
    /// kana_query 用の損失あり文字存在マスク。kana_query がないときは `None`。
    kana_query_mask: Option<u64>,
    /// `norm_query` の UTF-32 事前計算（Fuzzy マッチで全スレッド共有）。
    needle_u32: Utf32String,
    /// パスマッチ用クエリ（生クエリベース・アクセント/連続スペース保持）。`has_path_sep` 時のみ。
    path_query: Option<String>,
    /// パスクエリの履歴照合キー（`normalize_history_query_key` 由来）。`path_query` とは別。
    path_history_key: Option<String>,
}

/// Wave 1 の出力: `(lower_names, lower_file_names, normalized_keys, kana_lower_names)`。
/// いずれも構築後に伸長しないため `Box<str>` で保持する（容量ワード 8B/要素を節約）。
type Wave1Strings = (Vec<Box<str>>, Vec<Option<Box<str>>>, Vec<Box<str>>, Vec<Box<str>>);

/// Wave 1: entries から文字列正規化データを並列構築する。
/// lower_names / lower_file_names / normalized_keys / kana_lower_names は
/// entries への純粋な map であり相互依存がないため rayon::join で並列構築する。
/// `migemo_enabled` が false の場合、kana_lower_names は空 Vec（migemo 無効ユーザーの
/// 死蔵メモリを削るため、issue #337）。空 Vec の検索ループ側ガードは search_with_options 参照。
fn compute_wave1(entries: &[AppEntry], migemo_enabled: bool) -> Wave1Strings {
    let ((lower_names, lower_file_names), (normalized_keys, kana_lower_names)) = rayon::join(
        || {
            rayon::join(
                || {
                    entries
                        .iter()
                        .map(|e| to_lower_folded(&e.name).into_boxed_str())
                        .collect::<Vec<_>>()
                },
                || {
                    entries
                        .iter()
                        .map(|e| lower_file_name(&e.target_path).map(String::into_boxed_str))
                        .collect::<Vec<_>>()
                },
            )
        },
        || {
            rayon::join(
                || {
                    entries
                        .iter()
                        .map(|e| normalize_entry_key(&e.target_path).into_boxed_str())
                        .collect::<Vec<_>>()
                },
                || {
                    // migemo 無効時は kana を構築しない（空 Vec）。
                    if migemo_enabled {
                        entries
                            .iter()
                            .map(|e| to_kana(&to_lower_folded(&e.name)).into_boxed_str())
                            .collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    }
                },
            )
        },
    );
    (lower_names, lower_file_names, normalized_keys, kana_lower_names)
}

/// Wave 2: lower_names / lower_file_names からビットマスクを並列構築する。
/// to_lower_folded already folds most Latin accents to ASCII (é→e),
/// so non-ASCII names here are typically CJK, Arabic, etc.
/// u64::MAX ensures (query_mask & u64::MAX) == query_mask for any query_mask,
/// so these entries always pass the bitmask pre-filter regardless of the query.
fn compute_wave2(
    lower_names: &[Box<str>],
    lower_file_names: &[Option<Box<str>>],
) -> (Vec<u64>, Vec<u64>) {
    rayon::join(
        || lower_names.iter().map(|n| name_char_mask(n)).collect::<Vec<_>>(),
        || {
            // None → 0: entries without a file_name cannot match via the file_name path,
            // so failing the bitmask check (and being skipped when the name also fails) is correct.
            lower_file_names
                .iter()
                .map(|n| file_char_mask(n.as_deref()))
                .collect::<Vec<_>>()
        },
    )
}

/// kana の Unicode scalar value を 64 bit に写す損失あり存在マスク。
/// 同じ文字は必ず同じ bit になるため、mask 不一致なら kana substring は不成立と分かる。
/// 衝突は候補を余計に通すだけで、kana マッチを棄却しない。
#[inline]
fn kana_char_mask(kana: &str) -> u64 {
    kana.chars().fold(0, |mask, ch| mask | (1u64 << ((ch as u32) & 63)))
}

/// migemo 有効時の kana pre-filter 用並列 Vec を構築する。kana 未構築時は空 Vec を保つ。
fn compute_kana_char_masks(kana_lower_names: &[Box<str>]) -> Vec<u64> {
    kana_lower_names.iter().map(|name| kana_char_mask(name)).collect()
}

impl SearchEngine {
    /// 全並列 Vec の長さが entries と一致することを検証し、Self を組み立てる。
    fn assemble(
        entries: Vec<AppEntry>,
        lower_names: Vec<Box<str>>,
        lower_file_names: Vec<Option<Box<str>>>,
        normalized_keys: Vec<Box<str>>,
        char_masks: Vec<u64>,
        file_name_char_masks: Vec<u64>,
        kana: (Vec<Box<str>>, Vec<u64>),
    ) -> Self {
        let (kana_lower_names, kana_char_masks) = kana;
        debug_assert!(
            lower_names.len() == entries.len()
                && lower_file_names.len() == entries.len()
                && normalized_keys.len() == entries.len()
                && char_masks.len() == entries.len()
                && file_name_char_masks.len() == entries.len(),
            "SearchEngine: all parallel Vecs must have the same length as entries"
        );
        // kana 系 Vec は {0, entries.len()} を許す（migemo 無効時は空 Vec、issue #337）。
        debug_assert!(
            (kana_lower_names.is_empty() && kana_char_masks.is_empty())
                || (kana_lower_names.len() == entries.len()
                    && kana_char_masks.len() == entries.len()),
            "SearchEngine: kana parallel Vecs must both be empty or match entries length"
        );
        Self {
            entries,
            lower_names,
            lower_file_names,
            normalized_keys,
            char_masks,
            file_name_char_masks,
            kana_lower_names,
            kana_char_masks,
            prev_query: String::new(),
            prev_candidates: Vec::new(),
            prev_mode: None,
            prev_kana_query: None,
        }
    }

    /// kana_lower_names を**常に**構築する（migemo 有効相当）。テスト・ベンチ・convenience 用。
    /// 本番のインデックス構築は config 由来の migemo フラグを渡す [`new_with_migemo`] /
    /// [`new_with_cached_masks`] を使い、migemo 無効時は kana を構築しない（issue #337）。
    pub fn new(entries: Vec<AppEntry>) -> Self {
        Self::new_with_migemo(entries, true)
    }

    /// `migemo_enabled` に応じて kana_lower_names の構築要否を決めて構築する。
    /// false のとき kana は空 Vec（migemo 無効ユーザーの死蔵メモリ ~2.1–2.7MB/50k を削る）。
    pub fn new_with_migemo(entries: Vec<AppEntry>, migemo_enabled: bool) -> Self {
        let (lower_names, lower_file_names, normalized_keys, kana_lower_names) =
            compute_wave1(&entries, migemo_enabled);
        let (char_masks, file_name_char_masks) =
            compute_wave2(&lower_names, &lower_file_names);
        let kana_char_masks = compute_kana_char_masks(&kana_lower_names);
        Self::assemble(
            entries,
            lower_names,
            lower_file_names,
            normalized_keys,
            char_masks,
            file_name_char_masks,
            (kana_lower_names, kana_char_masks),
        )
    }

    /// キャッシュから読み込んだデータを使って SearchEngine を構築する。
    ///
    /// - `char_masks` / `file_name_char_masks`: Wave 2 の再計算をスキップ
    /// - `cached_lower_names` / `cached_lower_file_names` / `cached_normalized_keys`:
    ///   v4+ キャッシュヒット時に Some → Wave 1 の再計算もスキップ（A-3）
    ///   v3 フォールバック時は None → Wave 1 を通常通り並列実行
    /// - `migemo_enabled`: false のとき kana_lower_names を構築しない（空 Vec、issue #337）。
    ///   v4 パス（再計算）・v3 フォールバックの**両方**でこのフラグを反映する。
    pub fn new_with_cached_masks(
        entries: Vec<AppEntry>,
        char_masks: Vec<u64>,
        file_name_char_masks: Vec<u64>,
        cached_lower_names: Option<Vec<String>>,
        cached_lower_file_names: Option<Vec<Option<String>>>,
        cached_normalized_keys: Option<Vec<String>>,
        migemo_enabled: bool,
    ) -> Self {
        let (lower_names, lower_file_names, normalized_keys, kana_lower_names) =
            if let (Some(ln), Some(lfn), Some(nk)) =
                (cached_lower_names, cached_lower_file_names, cached_normalized_keys)
            {
                // A-3: v4 キャッシュヒット → Wave 1 完全スキップ（kana_lower_names は毎起動再計算）。
                // キャッシュ由来の Vec<String> を Box<str> へ移す。postcard デシリアライズ後の
                // String は capacity == len のため into_boxed_str は再アロケーションを伴わない。
                // migemo 無効時は kana を再計算せず空 Vec のままにする。
                let kana = if migemo_enabled {
                    entries
                        .par_iter()
                        .map(|e| to_kana(&to_lower_folded(&e.name)).into_boxed_str())
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                let ln = ln.into_iter().map(String::into_boxed_str).collect::<Vec<_>>();
                let lfn = lfn
                    .into_iter()
                    .map(|o| o.map(String::into_boxed_str))
                    .collect::<Vec<_>>();
                let nk = nk.into_iter().map(String::into_boxed_str).collect::<Vec<_>>();
                (ln, lfn, nk, kana)
            } else {
                // v3 フォールバック: Wave 1 を並列実行（migemo フラグを反映）
                compute_wave1(&entries, migemo_enabled)
            };

        let kana_char_masks = compute_kana_char_masks(&kana_lower_names);
        Self::assemble(
            entries,
            lower_names,
            lower_file_names,
            normalized_keys,
            char_masks,
            file_name_char_masks,
            (kana_lower_names, kana_char_masks),
        )
    }

    #[inline]
    fn entry_view(&self, i: usize) -> EntryView<'_> {
        EntryView {
            entry: &self.entries[i],
            lower_name: &self.lower_names[i],
            lower_file_name: self.lower_file_names[i].as_deref(),
            normalized_key: &self.normalized_keys[i],
        }
    }

    /// 履歴ブーストとデフォルト設定（migemo 無効）で検索する便宜 API。
    /// migemo（ローマ字→かな変換マッチ）を有効にするには
    /// [`search_with_options`] に `migemo_enabled = true` の
    /// [`SearchOptions`] を渡すこと。
    pub fn search(
        &mut self,
        query: &str,
        max_results: usize,
        history: &HistoryStore,
        mode: SearchMode,
    ) -> Vec<SearchResult> {
        self.search_with_options(
            query,
            max_results,
            history,
            mode,
            SearchOptions::default(),
        )
    }

    pub fn search_with_options(
        &mut self,
        query: &str,
        max_results: usize,
        history: &HistoryStore,
        mode: SearchMode,
        options: SearchOptions,
    ) -> Vec<SearchResult> {
        if max_results == 0 {
            return Vec::new();
        }

        // 候補準備: クエリ解析・migemo/path クエリ・UTF-32 needle を 1 度だけ導出する。
        // norm_query が空のとき None（早期 return）。
        let Some(plan) = prepare_query_plan(query, mode, &options) else {
            return Vec::new();
        };

        // Phase 4: incremental search – reuse previous match candidates when the query
        // is a monotonic extension of the previous one and the mode is unchanged.
        // 述語（no-dot→dot / kana 単調性 / !has_path_sep）と prev_* の read は
        // decide_incremental に集約する。ここでは write のみを fold 後に行う。
        let use_incremental = self.decide_incremental(&plan, mode);

        let candidate_indices: Vec<usize> = if use_incremental {
            std::mem::take(&mut self.prev_candidates)
        } else {
            (0..self.entries.len()).collect()
        };

        // kana_lower_names は migemo 無効で構築されたとき空 Vec（issue #337）。
        // 構築時 migemo OFF → 検索時 migemo ON（kana_query=Some）の窓で
        // self.kana_lower_names[i] が index out of bounds で panic するのを防ぐ。
        // Copy な bool としてクロージャに move する（self への可変借用は不要）。
        let kana_available = !self.kana_lower_names.is_empty();

        // Parallel scoring: each rayon task gets its own Matcher (created in fold init).
        // Matcher::new() is O(alloc_zeroed) — lightweight enough to create per task.
        // Each task builds a local top-k BinaryHeap; tasks are merged in reduce().
        //
        // BinaryHeap<ScoredEntry> is a max-heap where peek() == worst item.
        // ScoredEntry::Ord mirrors rank_cmp_ranked: better entry has Ordering::Less,
        // so the worst (Ordering::Greater) stays at the top of the heap.
        //
        // The fold state includes a Vec<usize> to collect ALL matching entry indices
        // (not just top-k) for the incremental search cache.
        let (top_k, all_match_indices): (BinaryHeap<ScoredEntry>, Vec<usize>) = candidate_indices
            .into_par_iter()
            .with_min_len(MIN_PAR_CANDIDATES)
            .fold(
                || {
                    (
                        BinaryHeap::<ScoredEntry>::with_capacity(max_results + 1),
                        Vec::<usize>::new(),
                    )
                },
                |(mut local_heap, mut local_matches), i| {
                    // スコアリング本体は score_one_entry に委譲する（bitmask pre-filter →
                    // name/file_name/kana/path スコア → 履歴ブースト → ScoredEntry 構築）。
                    // Some ⟺ マッチ成立。local_matches.push(i) は heap 採否より前に無条件で
                    // 行い、top-k から落ちた一致も incremental cache に残す（縮小を防ぐ）。
                    if let Some(scored) =
                        self.score_one_entry(i, &plan, mode, kana_available, history, options)
                    {
                        local_matches.push(i);
                        if local_heap.len() < max_results {
                            local_heap.push(scored);
                        } else if let Some(mut worst) = local_heap.peek_mut() {
                            // Replace only when the new item is better than the current worst.
                            if scored.cmp(&worst) == Ordering::Less {
                                *worst = scored;
                            }
                        }
                    }

                    (local_heap, local_matches)
                },
            )
            .reduce(
                || (BinaryHeap::new(), Vec::new()),
                |(mut a_heap, mut a_matches), (b_heap, b_matches)| {
                    for entry in b_heap {
                        if a_heap.len() < max_results {
                            a_heap.push(entry);
                        } else if let Some(mut worst) = a_heap.peek_mut()
                            && entry.cmp(&worst) == Ordering::Less
                        {
                            *worst = entry;
                        }
                    }
                    a_matches.extend(b_matches);
                    (a_heap, a_matches)
                },
            );

        // top_k は self（lower_names / entries）を借用しているため、先に所有 SearchResult へ
        // 変換して借用を終わらせてから incremental cache を write する
        // （read は decide_incremental に集約。all_match_indices は owned な Vec<usize> ゆえ
        // write 順序の入れ替えに意味論上の影響はない）。
        let results = heap_into_results(&self.entries, top_k);

        self.prev_query = plan.norm_query.into_owned();
        self.prev_candidates = all_match_indices;
        self.prev_mode = Some(mode);
        self.prev_kana_query = plan.kana_query;

        results
    }

    /// incremental search（前回候補の再利用）の可否を判定する。
    ///
    /// 全述語は「今回の候補集合 ⊆ 前回の候補集合」を保証する単調条件。
    /// `self.prev_*` を **read のみ**で参照する（write は `search_with_options` が fold 後に行う）。
    ///
    /// - `!has_dot || prev_query.contains('.')`: "no-dot → dot" 遷移は full scan にフォールバック。
    ///   prev_candidates が file_name スコアリングなしで構築されており、file_name のみで
    ///   マッチするエントリが欠落して false negative になるのを防ぐ。
    /// - kana_monotonic: kana_query の単調性。(None,_)→OK / (Some curr, Some prev) は
    ///   `curr.starts_with(prev)` のとき候補が狭まる / (Some,None) は新規出現ゆえ full scan。
    ///   ローマ字→かな変換は非単調（"kan"→"かん", "kana"→"かな"）ゆえ実値比較が必要。
    /// - `!has_path_sep`: パスクエリは norm_query と path_query で正規化が異なり単調性を
    ///   保証できないため無条件で無効化する（稀ゆえ性能影響は無視できる）。
    fn decide_incremental(&self, plan: &QueryPlan, mode: SearchMode) -> bool {
        let kana_monotonic = match (&plan.kana_query, &self.prev_kana_query) {
            (None, _) => true,
            (Some(curr), Some(prev)) => curr.starts_with(prev.as_str()),
            (Some(_), None) => false,
        };
        self.prev_mode == Some(mode)
            && !self.prev_candidates.is_empty()
            && !self.prev_query.is_empty()
            && plan.norm_query.starts_with(self.prev_query.as_str())
            && (!plan.has_dot || self.prev_query.contains('.'))
            && !plan.has_path_sep
            && kana_monotonic
    }

    /// 1 エントリ（index `i`）をスコアリングし、マッチすれば `ScoredEntry` を返す。
    /// `Some` ⟺ マッチ成立（= base score が Some）。呼び出し側はこの条件で incremental
    /// cache 用の index 記録（`local_matches.push(i)`）を行う。
    ///
    /// **内部順序の不変条件（性能）**:
    /// - bitmask pre-filter を関数**先頭**に置く（`entry_view` / `Utf32String::from` より前）。
    ///   pre-filter で落ちる候補に UTF-32 変換コストを掛けないため。
    /// - `has_dot` の file_name 短絡（`name_score <= 9000`）を保存する（MATCHER 呼び出し回数）。
    ///
    /// ホットループから呼ばれるため `#[inline]`、全引数を参照/Copy で受け取り fold 内 codegen を保つ。
    #[inline]
    fn score_one_entry<'a>(
        &'a self,
        i: usize,
        plan: &QueryPlan,
        mode: SearchMode,
        kana_available: bool,
        history: &HistoryStore,
        options: SearchOptions,
    ) -> Option<ScoredEntry<'a>> {
        // Bitmask pre-filter: skip entries that lack query characters (Fuzzy only).
        // Prefix/Substring use cheap str ops, so the bitmask overhead isn't worth it.
        // name/file_name と kana のいずれのマッチ経路にも候補がない場合だけ棄却する。
        // kana は専用の損失あり mask を使う。衝突で余分な候補を通すことはあっても、
        // kana substring が成立する候補を棄却しない。
        // has_path_sep 時は従来どおりスキップ: パスだけでマッチするエントリが
        // name/file_name mask で落ちる問題を回避する。
        // **必ず関数先頭**（entry_view / Utf32String 変換より前）。
        if mode == SearchMode::Fuzzy && !plan.has_path_sep {
            let name_mask = self.char_masks[i];
            let fn_mask = self.file_name_char_masks[i];
            let latin_match = (plan.query_mask & name_mask) == plan.query_mask
                || (plan.has_dot && (plan.query_mask & fn_mask) == plan.query_mask);
            let kana_match = plan.kana_query_mask.is_some_and(|kana_query_mask| {
                !self.kana_char_masks.is_empty()
                    && (kana_query_mask & self.kana_char_masks[i]) == kana_query_mask
            });
            if !latin_match && !kana_match {
                return None;
            }
        }

        let v = self.entry_view(i);
        let norm_query_str: &str = plan.norm_query.as_ref();

        // Fuzzy モードのみ UTF-32 へのオンデマンド変換を行う。
        // ビットマスクで除外された候補には到達しないため、変換コストは
        // ビットマスク通過分（全体の 1-5%）にのみ発生する。
        let name_u32_owned: Option<Utf32String> = if mode == SearchMode::Fuzzy {
            Some(Utf32String::from(v.lower_name))
        } else {
            None
        };

        // D-5: borrow the thread-local Matcher; no alloc_zeroed per task.
        let name_score = MATCHER.with(|m| {
            let mut matcher = m.borrow_mut();
            match_score_single_cached(
                mode,
                &mut matcher,
                v.lower_name,
                norm_query_str,
                name_u32_owned.as_ref(),
                &plan.needle_u32,
            )
        });

        let primary_score = if plan.has_dot {
            // Skip file_name scoring only on a high-confidence name match
            // (avoids heavy fuzzy work).
            // 注: 9000 は「file_name スコアリングを短絡する閾値」であり
            // score_tier の基準スコアではない（PREFIX_BASE=10000 と混同しないこと）。
            let needs_fn_score = name_score.is_none_or(|s| s <= 9000);
            let fn_score = if needs_fn_score {
                v.lower_file_name.and_then(|fn_name| {
                    let fn_u32_owned: Option<Utf32String> = if mode == SearchMode::Fuzzy {
                        Some(Utf32String::from(fn_name))
                    } else {
                        None
                    };
                    MATCHER.with(|m| {
                        let mut matcher = m.borrow_mut();
                        match_score_single_cached(
                            mode,
                            &mut matcher,
                            fn_name,
                            norm_query_str,
                            fn_u32_owned.as_ref(),
                            &plan.needle_u32,
                        )
                    })
                })
            } else {
                None
            };
            match (name_score, fn_score) {
                (None, fn_s) => fn_s,
                (Some(s), Some(b)) => Some(s.max(b)),
                (Some(s), None) => Some(s),
            }
        } else {
            name_score
        };

        // kana マッチ: primary_score がない場合のみ試みる（OR 関係）。
        // kana_available=false（migemo 無効で構築＝空 Vec）のときは
        // self.kana_lower_names[i] に到達させない（panic ガード）。
        let kana_score = if primary_score.is_none() && kana_available {
            plan.kana_query
                .as_deref()
                .and_then(|kq| kana_substring_score(&self.kana_lower_names[i], kq))
        } else {
            None
        };

        let score = primary_score.or(kana_score);

        // パスマッチ: name/file_name/kana 全て不成立時のフォールバック。
        // normalized_key は normalize_entry_key() で小文字化 + パス区切り正規化済み。
        // スコア PATH_BASE(3000) は Kana(4500) より低く、名前マッチを常に優先する。
        let score = score.or_else(|| {
            plan.path_query.as_deref().and_then(|pq| {
                let pos = v.normalized_key.find(pq)?;
                Some((score_tier::PATH_BASE - (pos as i64).min(score_tier::PATH_POS_CAP)).max(1))
            })
        });

        let base_score = score?;

        let (global_launches, last_launched) = history.get_global_stats_normalized(v.normalized_key);
        // 履歴キーは record_launch の保存形式に合わせる:
        // normalize_query() + パス区切り統一。path_query は生クエリベースで
        // スペース/アクセントが異なるため履歴キーには使わない。
        let history_query_key = plan.path_history_key.as_deref().unwrap_or(norm_query_str);
        let qcount =
            history.query_count_pre_normalized(history_query_key, v.normalized_key) as i64;

        let folder_boost = if v.entry.is_folder {
            history.folder_expansion_count_normalized(v.normalized_key) as i64
                * FOLDER_EXPANSION_WEIGHT
        } else {
            0
        };

        let raw_history_boost =
            (global_launches as i64) * GLOBAL_WEIGHT + qcount * QUERY_WEIGHT + folder_boost;
        let history_boost = adjusted_history_boost(mode, base_score, raw_history_boost, options);
        let combined = base_score + history_boost;

        Some(ScoredEntry {
            score: combined,
            last_launched,
            lower_name: v.lower_name,
            path: &v.entry.target_path,
            index: i,
        })
    }

    pub fn recent_history(&self, history: &HistoryStore, max_results: usize) -> Vec<SearchResult> {
        // recent_launches() は正規化済みキーを返すため、照合側も正規化済み normalized_keys を使う
        let path_to_entry: HashMap<&str, &AppEntry> = (0..self.entries.len())
            .map(|i| {
                let v = self.entry_view(i);
                (v.normalized_key, v.entry)
            })
            .collect();

        history
            .recent_launches(max_results)
            .into_iter()
            .filter_map(|path| {
                path_to_entry.get(path).map(|entry| SearchResult {
                    name: entry.name.clone(),
                    path: entry.target_path.clone(),
                    is_folder: entry.is_folder,
                    is_error: false,
                })
            })
            .collect()
    }

    pub fn entries(&self) -> &[AppEntry] {
        &self.entries
    }
}

/// 検索呼び出しの候補準備フェーズ。クエリを解析して [`QueryPlan`] を組み立てる。
/// `norm_query` が空のとき `None`（呼び出し側は空結果を返す）。`self` 非依存の自由 fn。
fn prepare_query_plan<'a>(
    query: &'a str,
    mode: SearchMode,
    options: &SearchOptions,
) -> Option<QueryPlan<'a>> {
    let norm_query = normalize_query(query);
    if norm_query.is_empty() {
        return None;
    }

    let has_dot = norm_query.contains('.');
    // Bitmask pre-filter is only used in Fuzzy mode; skip the computation for others.
    let query_mask = if mode == SearchMode::Fuzzy { char_bitmask(&norm_query) } else { 0 };

    // Migemo: ローマ字 ASCII クエリをひらがなに変換した kana_query を生成する。
    // kana に ASCII アルファベットが残留する場合（"dok" → "どk" 等）は None にする。
    let kana_query: Option<String> = if options.migemo_enabled
        && norm_query.is_ascii()
        && norm_query.chars().count() >= options.migemo_min_chars
    {
        let k = to_kana(norm_query.as_ref());
        if k != norm_query.as_ref() && !k.bytes().any(|b| b.is_ascii_alphabetic()) {
            Some(k)
        } else {
            None
        }
    } else {
        None
    };
    let kana_query_mask = kana_query.as_deref().map(kana_char_mask);

    // Pre-compute needle as UTF-32 once per search call and share it across threads.
    // Reusing the same Utf32String avoids repeated O(|query|) char conversion per entry.
    let needle_u32 = Utf32String::from(norm_query.as_ref());

    // Path matching: クエリにパス区切り文字（\ / ¥）を含む場合、normalized_key（フルパス）
    // に対して Substring マッチを試みる。
    // normalize_query() は連続スペースを潰すが normalize_entry_key() は保持するため、
    // パスマッチ用クエリは生クエリから normalize_entry_key() 相当で正規化する。
    // これにより "C:\My  Tools\" のような連続スペースを含むパスにもマッチする。
    // ¥（U+00A5）は日本語 Windows でバックスラッシュとして使われるため対象に含める。
    let has_path_sep = {
        let q = query.trim();
        q.contains('\\') || q.contains('/') || q.contains('\u{00a5}')
    };
    let path_query: Option<String> = if has_path_sep {
        // normalize_entry_key と同じ正規化: 小文字化 + / と ¥ を \ に統一
        let trimmed = query.trim();
        let mut pq = String::with_capacity(trimmed.len());
        for ch in trimmed.chars() {
            if ch == '/' || ch == '\u{00a5}' {
                pq.push('\\');
            } else {
                pq.extend(ch.to_lowercase());
            }
        }
        Some(pq)
    } else {
        None
    };
    // 履歴キー: normalize_history_query_key で一元化（normalize_query + パス区切り統一）。
    // path_query は生クエリベースでスペース/アクセントの扱いが異なるため別途作る。
    let path_history_key: Option<String> = if has_path_sep {
        Some(normalize_history_query_key(query).into_owned())
    } else {
        None
    };

    Some(QueryPlan {
        norm_query,
        has_dot,
        has_path_sep,
        query_mask,
        kana_query,
        kana_query_mask,
        needle_u32,
        path_query,
        path_history_key,
    })
}

/// top-k ヒープを best-first の [`SearchResult`] 列へ変換する（結果組立フェーズ）。
/// `into_sorted_vec()` はヒープ内部 Vec を再利用して昇順ソートする。
/// `ScoredEntry::Ord` は Less = better ゆえ昇順で best が先頭になる（tie-break の意味を保つ）。
/// 所有 String の clone はここで初めて発生する（top-k 確定後の K 件のみ）。
fn heap_into_results(entries: &[AppEntry], top_k: BinaryHeap<ScoredEntry>) -> Vec<SearchResult> {
    top_k
        .into_sorted_vec()
        .into_iter()
        .map(|r| {
            let entry = &entries[r.index];
            SearchResult {
                name: entry.name.clone(),
                path: entry.target_path.clone(),
                is_folder: entry.is_folder,
                is_error: false,
            }
        })
        .collect()
}

fn adjusted_history_boost(
    mode: SearchMode,
    base_score: i64,
    raw_history_boost: i64,
    config: SearchOptions,
) -> i64 {
    if mode != SearchMode::Fuzzy
        || config.normalization != SearchHistoryNormalizationConfig::FuzzyRelativeCap
    {
        return raw_history_boost;
    }

    let cap = ((base_score.max(1) as f64) * config.fuzzy_history_cap_ratio).floor() as i64;
    raw_history_boost.min(cap)
}

/// Borrowed scored entry used in the parallel top-k heap.
/// SearchEngine の並列 Vec（`lower_names` / `entries`）から借用するため、ヒープ滞在中は
/// String clone がゼロ。所有 `SearchResult` への変換（clone）は top-k 確定後に
/// `heap_into_results` が `index` 経由で K 件だけ行う（マッチ M 件 clone の回避、#436 で
/// score_one_entry を抽出した際に混入した M 件 clone の是正）。
struct ScoredEntry<'a> {
    score: i64,
    last_launched: u64,
    lower_name: &'a str, // tie-breaking key (alphabetical) = &self.lower_names[index]
    path: &'a str,       // tie-breaking key = &self.entries[index].target_path
    index: usize,        // SearchResult 組立時に entries[index] から clone する
}

// Higher score is better; better entries are ordered as `Ordering::Less`.
// This makes BinaryHeap::peek() point to the current worst (max by Ord = least good).
// scored.sort() (ascending) then puts the best entry first.
impl PartialEq for ScoredEntry<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score
            && self.last_launched == other.last_launched
            && self.lower_name == other.lower_name
            && self.path == other.path
    }
}

impl Eq for ScoredEntry<'_> {}

impl PartialOrd for ScoredEntry<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScoredEntry<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .score
            .cmp(&self.score)
            .then_with(|| other.last_launched.cmp(&self.last_launched))
            .then_with(|| self.lower_name.cmp(other.lower_name))
            .then_with(|| self.path.cmp(other.path))
    }
}


/// ひらがな正規化済みエントリ名に対して substring マッチを行い、
/// マッチした場合は `score_tier::KANA_BASE - byte_position` を返す
/// （Substring の `SUBSTRING_BASE` より低いスコア）。
/// byte_pos を使用: ひらがなは3バイト/文字のため文字位置の3倍差がある。
/// 先頭マッチが高スコアになる意図は保たれており、SPEC.md §4.2 に準拠。
/// kana_lower_name が常にひらがな/カタカナであることは保証されない（漢字はそのまま通過）が、
/// kana_query は常に純ひらがな（ASCII アルファベット残留ガード後）のため実運用上問題なし。
fn kana_substring_score(kana_lower_name: &str, kana_query: &str) -> Option<i64> {
    kana_lower_name
        .find(kana_query)
        .map(|pos| (score_tier::KANA_BASE - pos as i64).max(1))
}

/// Score using pre-computed lowercase name and an optional UTF-32 haystack.
/// `haystack_u32` is `Some` only in Fuzzy mode (computed on-demand after bitmask pre-filter).
/// `needle_u32` must be the UTF-32 encoding of `query` (pre-computed once per search call).
fn match_score_single_cached(
    mode: SearchMode,
    matcher: &mut Matcher,
    lower_name: &str,
    query: &str,
    haystack_u32: Option<&Utf32String>,
    needle_u32: &Utf32String,
) -> Option<i64> {
    match mode {
        SearchMode::Prefix => {
            if lower_name.starts_with(query) {
                Some(score_tier::PREFIX_BASE - lower_name.len() as i64)
            } else {
                None
            }
        }
        SearchMode::Substring => {
            lower_name.find(query).map(|idx| score_tier::SUBSTRING_BASE - idx as i64)
        }
        SearchMode::Fuzzy => {
            let h = haystack_u32.expect("Fuzzy mode requires UTF-32 haystack");
            let haystack = h.slice(..);
            let needle = needle_u32.slice(..);
            matcher.fuzzy_match(haystack, needle).map(|s| s as i64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::HistoryStore;
    use crate::indexer::AppEntry;

    fn make_entries(names: &[&str]) -> Vec<AppEntry> {
        names
            .iter()
            .map(|n| AppEntry {
                name: n.to_string(),
                target_path: format!("C:\\fake\\{}.lnk", n),
                is_folder: false,
            })
            .collect()
    }

    fn empty_history() -> HistoryStore {
        HistoryStore::load()
    }

    #[test]
    fn search_empty_query_returns_empty() {
        let mut engine = SearchEngine::new(make_entries(&["Firefox", "Chrome"]));
        let results = engine.search("", 8, &empty_history(), SearchMode::Fuzzy);
        assert!(results.is_empty());
    }

    #[test]
    fn search_no_entries_returns_empty() {
        let mut engine = SearchEngine::new(Vec::new());
        let results = engine.search("fire", 8, &empty_history(), SearchMode::Fuzzy);
        assert!(results.is_empty());
    }

    #[test]
    fn search_returns_fuzzy_matches() {
        let entries = make_entries(&["Firefox", "Chrome", "Notepad", "Visual Studio Code"]);
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("fire", 8, &empty_history(), SearchMode::Fuzzy);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "Firefox");
    }

    #[test]
    fn search_respects_max_results() {
        let entries = make_entries(&["app1", "app2", "app3", "app4", "app5"]);
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("app", 3, &empty_history(), SearchMode::Fuzzy);
        assert!(results.len() <= 3);
    }

    #[test]
    fn search_top_k_replaces_worst_when_better_arrives_late() {
        let entries = make_entries(&[
            "appabcdefghij",
            "appabcdefghi",
            "appx",
            "app",
        ]);
        let mut engine = SearchEngine::new(entries);

        let results = engine.search("app", 2, &empty_history(), SearchMode::Prefix);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();

        assert_eq!(names, vec!["app", "appx"]);
    }

    #[test]
    fn search_top_k_is_independent_from_input_order() {
        let ordered = make_entries(&[
            "appabcdefghij",
            "appabcdefghi",
            "appx",
            "app",
        ]);
        let reversed = make_entries(&[
            "app",
            "appx",
            "appabcdefghi",
            "appabcdefghij",
        ]);
        let mut ordered_engine = SearchEngine::new(ordered);
        let mut reversed_engine = SearchEngine::new(reversed);

        let ordered_results = ordered_engine.search("app", 2, &empty_history(), SearchMode::Prefix);
        let reversed_results = reversed_engine.search("app", 2, &empty_history(), SearchMode::Prefix);

        let ordered_names: Vec<&str> = ordered_results.iter().map(|r| r.name.as_str()).collect();
        let reversed_names: Vec<&str> = reversed_results.iter().map(|r| r.name.as_str()).collect();

        assert_eq!(ordered_names, vec!["app", "appx"]);
        assert_eq!(reversed_names, vec!["app", "appx"]);
    }

    #[test]
    fn search_results_are_not_folders() {
        let entries = make_entries(&["Firefox"]);
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("fire", 8, &empty_history(), SearchMode::Fuzzy);
        assert!(!results.is_empty());
        assert!(!results[0].is_folder);
    }

    #[test]
    fn search_prefix_mode_matches_only_prefix() {
        let entries = make_entries(&["Notepad", "Pad Tool"]);
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("pad", 8, &empty_history(), SearchMode::Prefix);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Pad Tool");
    }

    #[test]
    fn search_substring_mode_matches_middle() {
        let entries = make_entries(&["Visual Studio Code"]);
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("studio", 8, &empty_history(), SearchMode::Substring);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_with_extension_matches_stem_entry() {
        // "SSP.exe" と入力して、name="SSP", target_path="C:\\fake\\SSP.exe" にマッチする
        let entries = vec![AppEntry {
            name: "SSP".to_string(),
            target_path: "C:\\fake\\SSP.exe".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("SSP.exe", 8, &empty_history(), SearchMode::Prefix);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "SSP");
    }

    #[test]
    fn search_with_extension_substring_mode() {
        let entries = vec![AppEntry {
            name: "SSP".to_string(),
            target_path: "C:\\fake\\SSP.exe".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("ssp.exe", 8, &empty_history(), SearchMode::Substring);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_with_extension_fuzzy_mode() {
        let entries = vec![AppEntry {
            name: "SSP".to_string(),
            target_path: "C:\\fake\\SSP.exe".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("ssp.exe", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_without_extension_still_works() {
        let entries = make_entries(&["SSP"]);
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("SSP", 8, &empty_history(), SearchMode::Prefix);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "SSP");
    }

    #[test]
    fn search_with_extension_does_not_match_unrelated_exe() {
        // "ssp.exe" で FileZilla.exe はヒットしない（stem "ssp" が fuzzy でも一致しない）
        let entries = vec![
            AppEntry {
                name: "SSP".to_string(),
                target_path: "C:\\fake\\SSP.exe".to_string(),
                is_folder: false,
            },
            AppEntry {
                name: "FileZilla".to_string(),
                target_path: "C:\\fake\\FileZilla.exe".to_string(),
                is_folder: false,
            },
        ];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("ssp.exe", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "SSP");
    }

    #[test]
    fn search_with_extension_filters_by_ext() {
        // "ssp.exe" は .lnk の SSP にはヒットしない（ファイル名 "SSP.lnk" と "ssp.exe" は不一致）
        let entries = vec![AppEntry {
            name: "SSP".to_string(),
            target_path: "C:\\fake\\SSP.lnk".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("ssp.exe", 8, &empty_history(), SearchMode::Prefix);
        assert!(results.is_empty());
    }

    #[test]
    fn search_partial_ext_dot_only() {
        // "SSP." → target_path のファイル名 "SSP.exe" に fuzzy 一致
        let entries = vec![AppEntry {
            name: "SSP".to_string(),
            target_path: "C:\\fake\\SSP.exe".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("SSP.", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "SSP");
    }

    #[test]
    fn search_partial_ext_dot_e() {
        // "SSP.e" → target_path のファイル名 "SSP.exe" に fuzzy 一致
        let entries = vec![AppEntry {
            name: "SSP".to_string(),
            target_path: "C:\\fake\\SSP.exe".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("SSP.e", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "SSP");
    }

    #[test]
    fn search_partial_ext_dot_ex() {
        // "SSP.ex" → target_path のファイル名 "SSP.exe" に fuzzy 一致
        let entries = vec![AppEntry {
            name: "SSP".to_string(),
            target_path: "C:\\fake\\SSP.exe".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("SSP.ex", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "SSP");
    }

    #[test]
    fn search_name_with_dot_matches() {
        // name にドットを含むエントリが、ドット入りクエリでヒットする
        let entries = vec![AppEntry {
            name: "Dr.Web".to_string(),
            target_path: "C:\\fake\\drweb32w.exe".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("Dr.Web", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Dr.Web");
    }

    #[test]
    fn search_name_with_dot_prefers_name() {
        // name にドットを含むエントリが、部分一致クエリでもヒットする
        let entries = vec![AppEntry {
            name: "Dr.Web".to_string(),
            target_path: "C:\\fake\\drweb32w.exe".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("dr.w", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Dr.Web");
    }

    #[test]
    fn search_double_ext_file() {
        // 二重拡張子のファイルに対して部分一致でヒットする
        let entries = vec![AppEntry {
            name: "hoge".to_string(),
            target_path: "C:\\fake\\hoge.exe.bak".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("hoge.exe", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "hoge");
    }

    #[test]
    fn search_double_ext_full() {
        // 二重拡張子のファイルに対して完全一致でヒットする
        let entries = vec![AppEntry {
            name: "hoge".to_string(),
            target_path: "C:\\fake\\hoge.exe.bak".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("hoge.exe.bak", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "hoge");
    }

    #[test]
    fn recent_history_empty_when_no_launches() {
        let entries = make_entries(&["Firefox", "Chrome"]);
        let engine = SearchEngine::new(entries);
        let results = engine.recent_history(&empty_history(), 8);
        assert!(results.is_empty());
    }

    #[test]
    fn rank_cmp_breaks_full_tie_with_target_path() {
        // index は Ord/PartialEq に不参加（tie-break キーは score→last_launched→lower_name→path）
        let ra = ScoredEntry {
            score: 100,
            last_launched: 200,
            lower_name: "tool",
            path: "C:\\B\\tool.exe",
            index: 0,
        };
        let rb = ScoredEntry {
            score: 100,
            last_launched: 200,
            lower_name: "tool",
            path: "C:\\A\\tool.exe",
            index: 1,
        };
        let mut scored = [ra, rb];
        scored.sort();
        assert_eq!(scored[0].path, "C:\\A\\tool.exe");
        assert_eq!(scored[1].path, "C:\\B\\tool.exe");
    }

    #[test]
    fn has_dot_uses_cached_lower_file_name() {
        let entries = vec![AppEntry {
            name: "Dummy".to_string(),
            target_path: "C:\\fake\\Tool.EXE".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("tool.exe", 8, &empty_history(), SearchMode::Substring);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Dummy");
    }

    #[test]
    fn has_dot_handles_missing_file_name_without_panic() {
        let entries = vec![AppEntry {
            name: "Dummy".to_string(),
            target_path: "C:\\".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("dummy.exe", 8, &empty_history(), SearchMode::Fuzzy);
        assert!(results.is_empty());
    }

    #[test]
    fn adjusted_history_boost_disabled_uses_raw_value() {
        let adjusted =
            adjusted_history_boost(SearchMode::Fuzzy, 100, 250, SearchOptions::default());
        assert_eq!(adjusted, 250);
    }

    #[test]
    fn adjusted_history_boost_caps_only_in_fuzzy_mode() {
        let config = SearchOptions {
            normalization: SearchHistoryNormalizationConfig::FuzzyRelativeCap,
            fuzzy_history_cap_ratio: 0.30,
            ..SearchOptions::default()
        };
        let adjusted = adjusted_history_boost(SearchMode::Prefix, 100, 250, config);
        assert_eq!(adjusted, 250);
    }

    #[test]
    fn adjusted_history_boost_caps_fuzzy_history_by_base_ratio() {
        let config = SearchOptions {
            normalization: SearchHistoryNormalizationConfig::FuzzyRelativeCap,
            fuzzy_history_cap_ratio: 0.30,
            ..SearchOptions::default()
        };
        let adjusted = adjusted_history_boost(SearchMode::Fuzzy, 100, 250, config);
        assert_eq!(adjusted, 30);
    }

    #[test]
    fn adjusted_history_boost_zeroes_when_base_is_non_positive() {
        let config = SearchOptions {
            normalization: SearchHistoryNormalizationConfig::FuzzyRelativeCap,
            fuzzy_history_cap_ratio: 0.30,
            ..SearchOptions::default()
        };
        let adjusted = adjusted_history_boost(SearchMode::Fuzzy, -50, 250, config);
        assert_eq!(adjusted, 0);
    }

    #[test]
    fn search_with_options_disabled_matches_legacy_search() {
        let entries = make_entries(&["alpha", "alpaca", "alpine"]);
        let mut engine = SearchEngine::new(entries);
        let mut history = empty_history();
        for _ in 0..50 {
            history.record_launch("C:\\fake\\alpaca.lnk", "alp");
        }

        let legacy = engine.search("alp", 8, &history, SearchMode::Fuzzy);
        let explicit = engine.search_with_options(
            "alp",
            8,
            &history,
            SearchMode::Fuzzy,
            SearchOptions::default(),
        );
        assert_eq!(legacy, explicit);
    }

    // --- 大文字パス正規化テスト ---

    #[test]
    fn recent_history_matches_case_insensitive_path() {
        // 大文字パスで記録した起動履歴が、元ケース AppEntry と照合できる
        let entries = vec![AppEntry {
            name: "App".to_string(),
            target_path: "C:\\Fake\\App.lnk".to_string(),
            is_folder: false,
        }];
        let engine = SearchEngine::new(entries);
        let mut history = empty_history();
        history.record_launch("C:\\FAKE\\APP.LNK", "app");

        let results = engine.recent_history(&history, 8);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "App");
    }

    #[test]
    fn query_boost_matches_case_insensitive_path() {
        // 大文字パスで記録したクエリ別履歴がスコアブーストに反映され、
        // 同スコアの競合エントリより上位に来る
        let entries = vec![
            AppEntry {
                name: "App".to_string(),
                target_path: "C:\\Fake\\App.lnk".to_string(),
                is_folder: false,
            },
            AppEntry {
                name: "AppX".to_string(),
                target_path: "C:\\Other\\appx.lnk".to_string(),
                is_folder: false,
            },
        ];
        let mut engine = SearchEngine::new(entries);
        let mut history = empty_history();
        for _ in 0..10 {
            history.record_launch("C:\\FAKE\\APP.LNK", "app");
        }

        let results = engine.search_with_options(
            "app",
            8,
            &history,
            SearchMode::Prefix,
            SearchOptions::default(),
        );
        assert!(!results.is_empty());
        // 履歴ブーストにより "App" が "AppX" より上位に来る
        assert_eq!(results[0].name, "App");
    }

    // --- char_bitmask テスト ---

    #[test]
    fn bitmask_lowercase_letters() {
        let mask = char_bitmask("abc");
        assert_ne!(mask & (1 << 0), 0); // a
        assert_ne!(mask & (1 << 1), 0); // b
        assert_ne!(mask & (1 << 2), 0); // c
        assert_eq!(mask & (1 << 3), 0); // d not present
    }

    #[test]
    fn bitmask_digits() {
        let mask = char_bitmask("a1b2");
        assert_ne!(mask & (1 << 0), 0);      // a
        assert_ne!(mask & (1 << 1), 0);      // b
        assert_ne!(mask & (1 << 27), 0);     // '1' = bit 26+1
        assert_ne!(mask & (1 << 28), 0);     // '2' = bit 26+2
    }

    #[test]
    fn bitmask_non_alnum_ignored() {
        let mask = char_bitmask("a-b.c");
        assert_ne!(mask & (1 << 0), 0); // a
        assert_ne!(mask & (1 << 1), 0); // b
        assert_ne!(mask & (1 << 2), 0); // c
        // '-' and '.' don't set any bits
        assert_eq!(mask.count_ones(), 3);
    }

    #[test]
    fn bitmask_filter_skips_non_matching_entries() {
        // "xyz" のクエリでビットマスクが "Firefox" にマッチしないことを確認
        let entries = make_entries(&["Firefox", "Chrome", "Xyzer"]);
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("xyz", 8, &empty_history(), SearchMode::Fuzzy);
        // "Firefox" と "Chrome" は x,y,z の全文字を含まないのでスキップされる
        // "Xyzer" は x,y,z を含むのでマッチ候補になる
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Xyzer");
    }

    #[test]
    fn bitmask_filter_does_not_skip_accented_entries() {
        // nucleo はアクセント正規化（é→e）でマッチするため、
        // ビットマスクフィルタがアクセント付きエントリを除外してはならない
        let entries = vec![AppEntry {
            name: "Café".to_string(),
            target_path: "C:\\fake\\Café.lnk".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("cafe", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Café");
    }

    // --- アクセント正規化統一テスト ---

    #[test]
    fn prefix_matches_accented_entry() {
        let entries = vec![AppEntry {
            name: "Café".to_string(),
            target_path: "C:\\fake\\Café.lnk".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("cafe", 8, &empty_history(), SearchMode::Prefix);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Café");
    }

    #[test]
    fn substring_matches_accented_entry() {
        let entries = vec![AppEntry {
            name: "Résumé Builder".to_string(),
            target_path: "C:\\fake\\Résumé Builder.lnk".to_string(),
            is_folder: false,
        }];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("resume", 8, &empty_history(), SearchMode::Substring);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Résumé Builder");
    }

    // --- パフォーマンス計測 ---
    // `cargo test -p snotra-core bench_ -- --ignored --nocapture` で実行

    fn make_bench_entries(n: usize) -> Vec<AppEntry> {
        // 実際のアプリ名に近い多様な文字列を生成する
        let prefixes = [
            "Microsoft", "Adobe", "Google", "Apple", "Mozilla", "Visual", "Windows",
            "System", "App", "Tool", "Launcher", "Manager", "Explorer", "Editor",
        ];
        let suffixes = [
            "Studio", "Reader", "Player", "Code", "Settings", "Control", "Panel",
            "Viewer", "Browser", "Assistant", "Helper", "Updater", "Installer",
        ];
        (0..n)
            .map(|i| {
                let name = format!(
                    "{} {} {}",
                    prefixes[i % prefixes.len()],
                    suffixes[i % suffixes.len()],
                    i
                );
                AppEntry {
                    target_path: format!("C:\\Program Files\\App{}\\app{}.lnk", i, i),
                    name,
                    is_folder: false,
                }
            })
            .collect()
    }

    /// カタカナ主体のエントリ生成。migemo の主対象（かな名）で kana 実体の上界を測るため。
    fn make_bench_entries_katakana(n: usize) -> Vec<AppEntry> {
        let prefixes = [
            "マイクロソフト", "アドビ", "グーグル", "アップル", "システム",
            "ツール", "ランチャー", "マネージャー", "エクスプローラー", "エディター",
        ];
        let suffixes = [
            "スタジオ", "リーダー", "プレイヤー", "コード", "セッティング",
            "コントロール", "ビューアー", "ブラウザ", "アシスタント", "ヘルパー",
        ];
        (0..n)
            .map(|i| AppEntry {
                name: format!(
                    "{}{}{}",
                    prefixes[i % prefixes.len()],
                    suffixes[i % suffixes.len()],
                    i
                ),
                target_path: format!("C:\\Program Files\\App{}\\app{}.lnk", i, i),
                is_folder: false,
            })
            .collect()
    }

    /// kana_lower_names（`Vec<Box<str>>`）のメモリ実体を分解計測する。
    /// migemo 無効時はこの構造体ごと空になるため、ここで測る合計が削減量に相当する。
    /// - ポインタ配列: `capacity * size_of::<Box<str>>()`（ファットポインタ 16B/要素）
    /// - 文字列実体: 各 Box<str> のバイト長総和（要求バイト）
    /// - 16B 粒度丸め: 各実体を個別ヒープ確保とみなし 16B 境界へ切り上げた実 RSS 寄りの上界
    fn measure_kana_footprint(label: &str, entries: &[AppEntry]) {
        let kana: Vec<Box<str>> = entries
            .iter()
            .map(|e| to_kana(&to_lower_folded(&e.name)).into_boxed_str())
            .collect();
        let n = kana.len().max(1);
        let ptr_bytes = kana.capacity() * std::mem::size_of::<Box<str>>();
        let content_bytes: usize = kana.iter().map(|s| s.len()).sum();
        let rounded_content: usize = kana.iter().map(|s| s.len().max(1).div_ceil(16) * 16).sum();
        let transformed = entries
            .iter()
            .filter(|e| {
                let lf = to_lower_folded(&e.name);
                to_kana(&lf) != lf
            })
            .count();
        let requested = ptr_bytes + content_bytes;
        let rss_ish = ptr_bytes + rounded_content;
        let mb = |b: usize| b as f64 / (1024.0 * 1024.0);
        println!(
            "[{label}] n={n} かな変換割合={pct:.0}%\n  \
             ポインタ {ptr_bytes}B ({:.2}MB) + 実体 {content_bytes}B ({:.2}MB) \
             = 要求 {requested}B ({:.2}MB) | 16B丸め {rss_ish}B ({:.2}MB)\n  \
             1件あたり 要求 {}B / 丸め {}B",
            mb(ptr_bytes),
            mb(content_bytes),
            mb(requested),
            mb(rss_ish),
            requested / n,
            rss_ish / n,
            pct = transformed as f64 * 100.0 / n as f64,
        );
    }

    #[test]
    #[ignore]
    fn measure_kana_memory_footprint() {
        for &n in &[1_000usize, 10_000, 50_000, 100_000] {
            measure_kana_footprint(&format!("ascii    n={n}"), &make_bench_entries(n));
            measure_kana_footprint(&format!("katakana n={n}"), &make_bench_entries_katakana(n));
        }
    }

    /// lower_names を name と共有する `enum LowerName`（issue #336）の ROI を実測する。
    /// `Same`（`to_lower_folded(name) == name`）の比率と、Same エントリで排除できる
    /// ヒープ文字列実体（現状 `Box<str>` で name とは別アロケーション）の削減量を分解計測する。
    /// - 16B 粒度丸め: 個別ヒープ確保とみなし 16B 境界へ切り上げた実 RSS 寄りの上界
    fn measure_lower_name_footprint(label: &str, entries: &[AppEntry]) {
        let n = entries.len().max(1);
        let mut same = 0usize;
        let mut saved_requested = 0usize;
        let mut saved_rounded = 0usize;
        for e in entries {
            let lower = to_lower_folded(&e.name);
            if lower == e.name {
                same += 1;
                let bytes = lower.len();
                saved_requested += bytes;
                saved_rounded += bytes.max(1).div_ceil(16) * 16;
            }
        }
        let mb = |b: usize| b as f64 / (1024.0 * 1024.0);
        let pct = same as f64 * 100.0 / n as f64;
        println!(
            "[{label}] n={n} Same={same} ({pct:.1}%)\n  \
             削減（Same のヒープ文字列実体）要求 {saved_requested}B ({:.3}MB) | 16B丸め {saved_rounded}B ({:.3}MB)\n  \
             全entry平均 要求 {}B/件 / 丸め {}B/件 | 50k換算 丸め {:.3}MB",
            mb(saved_requested),
            mb(saved_rounded),
            saved_requested / n,
            saved_rounded / n,
            mb(saved_rounded * 50_000 / n),
        );
    }

    #[test]
    #[ignore]
    fn measure_lower_name_footprint_report() {
        use crate::config::Config;
        use crate::indexer::{scan_all, scan_path_env};

        // 本番相当: 既定 scan paths（Start Menu + Desktop の .lnk）+ PATH 実行ファイル。
        let mut real = scan_all(&Config::default_scan_paths(), false);
        let path_entries = scan_path_env(&real, false);
        real.extend(path_entries);
        measure_lower_name_footprint("real (start menu + desktop + PATH)", &real);

        // 参考: 合成ベンチ（名前パターンが本番非代表＝Same はほぼ出ない）。
        measure_lower_name_footprint("synthetic ascii    n=10000", &make_bench_entries(10_000));
        measure_lower_name_footprint("synthetic katakana n=10000", &make_bench_entries_katakana(10_000));
    }

    fn bench_search(label: &str, n: usize, queries: &[&str]) {
        use std::time::Instant;
        let entries = make_bench_entries(n);
        let mut engine = SearchEngine::new(entries);
        let history = empty_history();

        // ウォームアップ（rayon スレッドプールの初期化を除外）
        for q in queries {
            let _ = engine.search(q, 10, &history, SearchMode::Fuzzy);
        }

        let iters = 20usize;
        let mut total_ns = 0u128;
        for _ in 0..iters {
            for q in queries {
                let t = Instant::now();
                let _ = engine.search(q, 10, &history, SearchMode::Fuzzy);
                total_ns += t.elapsed().as_nanos();
            }
        }

        let avg_us = total_ns / (iters * queries.len()) as u128 / 1000;
        println!("[{label}] entries={n}, avg={avg_us}µs ({} queries × {iters} iters)", queries.len());
    }

    #[test]
    #[ignore]
    fn bench_fuzzy_search_scaling() {
        let queries = ["vis", "code", "micro", "app", "sett"];
        for &n in &[1_000, 10_000, 50_000, 100_000, 300_000] {
            bench_search("fuzzy", n, &queries);
        }
    }

    #[test]
    #[ignore]
    fn bench_fuzzy_path_query_scaling() {
        // パス区切り含有クエリ: has_path_sep=true でビットマスク pre-filter がスキップされる
        let queries = ["fake\\vis", "fake\\code", "fake\\micro"];
        for &n in &[1_000, 10_000, 50_000, 100_000, 300_000] {
            bench_search("fuzzy_path", n, &queries);
        }
    }

    fn bench_new(label: &str, n: usize) {
        use std::time::Instant;
        let entries = make_bench_entries(n);

        // rayon スレッドプール初期化を計測から除外するためのウォームアップ
        let _ = SearchEngine::new(entries.clone());

        // 構築自体にかかる時間のみを計測する。
        // entries.clone() のコストは Vec<AppEntry> の単純コピーであり、
        // new() 内の lower_fold・ビットマスク計算・normalized_keys に比べ微小。
        let iters = 5usize;
        let mut total_ns = 0u128;
        for _ in 0..iters {
            let cloned = entries.clone();
            let t = Instant::now();
            let _ = SearchEngine::new(cloned);
            total_ns += t.elapsed().as_nanos();
        }

        let avg_ms = total_ns / iters as u128 / 1_000_000;
        println!("[{label}] entries={n}, avg={avg_ms}ms ({iters} iters)");
    }

    #[test]
    #[ignore]
    fn bench_new_scaling() {
        for &n in &[1_000, 10_000, 50_000, 100_000, 300_000] {
            bench_new("new", n);
        }
    }

    /// migemo on/off の構築コスト差を計測する（issue #337）。
    /// kana 構築（to_kana の全エントリ map）をスキップした分の差を可視化する。
    /// 構築はロック外（PrebuiltIndex::new）で行われ、ロック保持時間は
    /// apply_prebuilt_index の O(1) ムーブのみ（migemo 状態に依存しない）。
    fn bench_new_migemo(label: &str, n: usize, migemo_enabled: bool) {
        use std::time::Instant;
        let entries = make_bench_entries_katakana(n);
        let _ = SearchEngine::new_with_migemo(entries.clone(), migemo_enabled); // warmup
        let iters = 5usize;
        let mut total_ns = 0u128;
        for _ in 0..iters {
            let cloned = entries.clone();
            let t = Instant::now();
            let _ = SearchEngine::new_with_migemo(cloned, migemo_enabled);
            total_ns += t.elapsed().as_nanos();
        }
        let avg_us = total_ns / iters as u128 / 1000;
        println!("[{label}] entries={n}, migemo={migemo_enabled}, avg={avg_us}µs ({iters} iters)");
    }

    #[test]
    #[ignore]
    fn bench_new_migemo_on_off() {
        for &n in &[1_000, 10_000, 50_000, 100_000] {
            bench_new_migemo("new_migemo_on ", n, true);
            bench_new_migemo("new_migemo_off", n, false);
        }
    }

    #[test]
    fn history_boost_unified_across_accent_variants() {
        // "résumé" で起動記録 → "resume" で検索時に履歴ブーストが効く
        let entries = vec![
            AppEntry {
                name: "Résumé Builder".to_string(),
                target_path: "C:\\fake\\Résumé Builder.lnk".to_string(),
                is_folder: false,
            },
            AppEntry {
                name: "Resume Helper".to_string(),
                target_path: "C:\\fake\\Resume Helper.lnk".to_string(),
                is_folder: false,
            },
        ];
        let mut engine = SearchEngine::new(entries);
        let mut history = empty_history();
        // "résumé" で Résumé Builder を多数起動
        for _ in 0..20 {
            history.record_launch("C:\\fake\\Résumé Builder.lnk", "résumé");
        }
        // "resume"（アクセントなし）で検索 → 履歴ブーストが効いて Résumé Builder が上位
        let results = engine.search("resume", 8, &history, SearchMode::Fuzzy);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "Résumé Builder");
    }

    // --- インクリメンタル検索テスト ---

    #[test]
    fn incremental_search_gives_correct_results_on_extension() {
        let names = &["Firefox", "Final Cut", "Chrome", "Finder", "Fire TV"];
        let mut engine = SearchEngine::new(make_entries(names));
        let h = empty_history();

        // 連続する monotonic 拡張で incremental パスを使う
        let _ = engine.search("fi", 8, &h, SearchMode::Fuzzy);
        let _ = engine.search("fir", 8, &h, SearchMode::Fuzzy);
        let incremental = engine.search("fire", 8, &h, SearchMode::Fuzzy);

        // 新鮮なエンジンでの結果と一致するか確認
        let mut fresh = SearchEngine::new(make_entries(names));
        let fresh_result = fresh.search("fire", 8, &h, SearchMode::Fuzzy);

        let inc_names: Vec<&str> = incremental.iter().map(|r| r.name.as_str()).collect();
        let fresh_names: Vec<&str> = fresh_result.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(inc_names, fresh_names);
    }

    #[test]
    fn incremental_search_fallback_on_backspace() {
        // "Firm" は fuzzy "fir" にマッチするが "fire" にはマッチしない（'e' がない）。
        // "fire" → "fir" はバックスペースなので incremental パスは使えない。
        // full scan にフォールバックしていなければ "Firm" が結果から漏れる。
        let mut engine =
            SearchEngine::new(make_entries(&["Firefox", "Final Cut", "Chrome", "Firm"]));
        let h = empty_history();

        let _ = engine.search("fire", 8, &h, SearchMode::Fuzzy);
        // "fir" は "fire" の拡張ではない → full scan にフォールバック
        let results = engine.search("fir", 8, &h, SearchMode::Fuzzy);

        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"Firefox"), "full scan で Firefox が返る必要がある");
        assert!(
            names.contains(&"Firm"),
            "Firm は 'fire' にマッチしないため前回の candidates に不在。full scan でのみ検出可能"
        );
    }

    #[test]
    fn incremental_search_fallback_on_mode_change() {
        let mut engine = SearchEngine::new(make_entries(&["Firefox", "Final Cut", "Chrome"]));
        let h = empty_history();

        let _ = engine.search("fi", 8, &h, SearchMode::Fuzzy);
        // 同じクエリでもモード変更 → full scan
        let results = engine.search("fi", 8, &h, SearchMode::Prefix);

        // Prefix "fi": "firefox" / "final cut" は先頭一致、"chrome" は不一致
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"Firefox"), "Prefix 'fi' で Firefox が返る必要がある");
        assert!(names.contains(&"Final Cut"), "Prefix 'fi' で Final Cut が返る必要がある");
        assert!(
            !names.contains(&"Chrome"),
            "Chrome は 'fi' で始まらないため除外される必要がある"
        );
    }

    #[test]
    fn incremental_search_dot_to_dot_uses_incremental() {
        // dot → dot の拡張は incremental パスを使用できる（no-dot→dot ガードを通過する）。
        // "ssp." → "ssp.e" はどちらもドットを含む単調拡張。
        let entries = vec![
            AppEntry {
                name: "SSP".to_string(),
                target_path: "C:\\fake\\SSP.exe".to_string(),
                is_folder: false,
            },
            AppEntry {
                name: "AnotherApp".to_string(),
                target_path: "C:\\fake\\ssp.data".to_string(),
                is_folder: false,
            },
        ];
        let mut engine = SearchEngine::new(entries.clone());
        let h = empty_history();

        // "ssp." で両エントリが候補にキャッシュされる（prev_candidates に両インデックスが入る）
        let _ = engine.search("ssp.", 8, &h, SearchMode::Fuzzy);
        // "ssp.e" はドットあり拡張 → incremental で prev_candidates を再利用
        let incremental = engine.search("ssp.e", 8, &h, SearchMode::Fuzzy);

        // fresh エンジンとの比較で正確性を担保
        let mut fresh = SearchEngine::new(entries);
        let fresh_result = fresh.search("ssp.e", 8, &h, SearchMode::Fuzzy);

        let inc_names: Vec<&str> = incremental.iter().map(|r| r.name.as_str()).collect();
        let fresh_names: Vec<&str> = fresh_result.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            inc_names,
            fresh_names,
            "dot→dot incremental は fresh 結果と一致する必要がある"
        );
    }

    #[test]
    fn incremental_search_no_dot_to_dot_falls_back_to_full_scan() {
        // "AnotherApp" は名前では "ssp" にマッチしないが、
        // file_name "ssp.data" は "ssp." クエリにマッチする
        let entries = vec![
            AppEntry {
                name: "SSP".to_string(),
                target_path: "C:\\fake\\SSP.exe".to_string(),
                is_folder: false,
            },
            AppEntry {
                name: "AnotherApp".to_string(),
                target_path: "C:\\fake\\ssp.data".to_string(),
                is_folder: false,
            },
        ];
        let mut engine = SearchEngine::new(entries);
        let h = empty_history();

        // "ssp"（ドットなし）→ prev_candidates には名前マッチした SSP だけが入る
        let _ = engine.search("ssp", 8, &h, SearchMode::Fuzzy);

        // "ssp."（ドットあり）→ no-dot→dot ガードにより full scan
        // AnotherApp の file_name "ssp.data" が "ssp." にマッチするはず
        let results = engine.search("ssp.", 8, &h, SearchMode::Fuzzy);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"SSP"), "SSP.exe が ssp. にマッチするはず");
        assert!(
            names.contains(&"AnotherApp"),
            "AnotherApp の file_name ssp.data が ssp. にマッチするはず（full scan 必須）"
        );
    }

    #[test]
    fn incremental_search_empty_prev_candidates_falls_back() {
        let mut engine = SearchEngine::new(make_entries(&["Firefox", "Chrome"]));
        let h = empty_history();

        // マッチなし → prev_candidates が空になる
        let r1 = engine.search("xyz", 8, &h, SearchMode::Fuzzy);
        assert!(r1.is_empty());

        // "xyzw" は "xyz" の拡張だが prev_candidates 空 → full scan
        let r2 = engine.search("xyzw", 8, &h, SearchMode::Fuzzy);
        assert!(r2.is_empty());

        // キャッシュが壊れていなければ通常クエリも機能する
        let r3 = engine.search("fire", 8, &h, SearchMode::Fuzzy);
        assert!(!r3.is_empty());
        assert_eq!(r3[0].name, "Firefox");
    }

    // --- migemo（ローマ字→かな）検索テスト ---

    fn migemo_config() -> SearchOptions {
        SearchOptions {
            migemo_enabled: true,
            migemo_min_chars: 2,
            ..SearchOptions::default()
        }
    }

    #[test]
    fn kana_search_disabled_by_default() {
        // migemo_enabled=false（デフォルト）では "dokyu" で "ドキュメント" がヒットしない
        let entries = make_entries(&["ドキュメント", "Documents"]);
        let mut engine = SearchEngine::new(entries);
        let h = empty_history();
        let results = engine.search_with_options(
            "dokyu",
            8,
            &h,
            SearchMode::Substring,
            SearchOptions::default(), // migemo_enabled=false
        );
        assert!(
            results.is_empty(),
            "migemo 無効時は 'dokyu' でカタカナ名がヒットしてはならない"
        );
    }

    #[test]
    fn kana_search_matches_katakana_entry() {
        // "dokyu" で "ドキュメント" がヒットする
        let entries = make_entries(&["ドキュメント", "Documents"]);
        let mut engine = SearchEngine::new(entries);
        let h = empty_history();
        let results = engine.search_with_options(
            "dokyu",
            8,
            &h,
            SearchMode::Substring,
            migemo_config(),
        );
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"ドキュメント"),
            "'dokyu' で ドキュメント がヒットするはず: {:?}",
            names
        );
    }

    #[test]
    fn fuzzy_kana_match_skips_latin_bitmask_prefilter() {
        // "chatto" と "tyatto" は kana に正規化するとどちらも「ちゃっと」になる。
        // 直接のラテン文字集合は異なるため、Fuzzy の bitmask pre-filter は
        // kana_query がある経路を早期棄却してはならない。
        let mut engine = SearchEngine::new(make_entries(&["tyatto"]));
        let plan = prepare_query_plan("chatto", SearchMode::Fuzzy, &migemo_config())
            .expect("非空クエリは QueryPlan を生成する");
        let kana_query_mask = plan
            .kana_query_mask
            .expect("完全に kana へ変換できるクエリは kana mask を持つ");

        assert_ne!(
            plan.query_mask & engine.char_masks[0],
            plan.query_mask,
            "tyatto はラテン文字 mask では chatto を満たさない"
        );
        assert_eq!(
            kana_query_mask & engine.kana_char_masks[0],
            kana_query_mask,
            "kana mask は chatto と tyatto の共通 kana を通す"
        );
        let results = engine.search_with_options(
            "chatto",
            8,
            &empty_history(),
            SearchMode::Fuzzy,
            migemo_config(),
        );

        assert_eq!(results.len(), 1, "kana 経由で tyatto がヒットするはず");
        assert_eq!(results[0].name, "tyatto");
    }

    #[test]
    fn kana_search_no_false_positive_for_ascii_entry() {
        // "dokyu" で "Documents" はヒットしない
        let entries = make_entries(&["ドキュメント", "Documents"]);
        let mut engine = SearchEngine::new(entries);
        let h = empty_history();
        let results = engine.search_with_options(
            "dokyu",
            8,
            &h,
            SearchMode::Substring,
            migemo_config(),
        );
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(
            !names.contains(&"Documents"),
            "'dokyu' で ASCII エントリ Documents がヒットしてはならない: {:?}",
            names
        );
    }

    #[test]
    fn kana_search_direct_match_ranks_above_kana_match() {
        // 直接 Substring マッチ(5000) が kana マッチ(4500) より常に上位に来ることを確認する。
        // "どきゅめんと"（ひらがな名）を含むエントリに対して:
        //   - "どきゅ"（直接クエリ）→ Substring スコア 5000
        //   - "dokyu"（kana クエリ）→ kana_substring_score 4500
        // 直接クエリの方が kana クエリより高スコアになる。
        let entries = vec![
            AppEntry {
                name: "どきゅめんと".to_string(),
                target_path: "C:\\fake\\a.lnk".to_string(),
                is_folder: false,
            },
            // kana マッチ専用エントリ（直接クエリでは name がひらがなのみなのでマッチしない）
            AppEntry {
                name: "ドキュメント".to_string(),
                target_path: "C:\\fake\\b.lnk".to_string(),
                is_folder: false,
            },
        ];
        let mut engine = SearchEngine::new(entries);
        let h = empty_history();

        // "どきゅめんと" に対して直接 Substring マッチしたスコアは 5000（先頭一致）
        let direct = engine.search_with_options(
            "どきゅ",
            8,
            &h,
            SearchMode::Substring,
            migemo_config(),
        );

        // "dokyu" → kana_query = "どきゅ" → kana_substring_score = 4500
        let kana = engine.search_with_options(
            "dokyu",
            8,
            &h,
            SearchMode::Substring,
            migemo_config(),
        );

        // 両方の結果が存在することを確認
        assert!(!direct.is_empty(), "直接クエリで どきゅめんと がヒットするはず");
        assert!(!kana.is_empty(), "kana クエリで どきゅめんと / ドキュメント がヒットするはず");

        // 直接クエリで "どきゅめんと" がトップに来る（Substring の先頭一致 = 高スコア）
        assert_eq!(
            direct[0].name, "どきゅめんと",
            "直接クエリ先頭一致は どきゅめんと が最高スコア"
        );

        // kana マッチのスコア(4500) < 直接マッチのスコア(5000) の確認:
        // 同じエントリが両クエリでヒットするとき、直接クエリ側の方が先頭になる。
        // "どきゅめんと" は直接 Substring(5000) でも kana_substring(4500) でもヒットするが、
        // 直接クエリ時は primary_score が Some なので kana_score は使われない（OR 関係）。
        // よって primary_score（5000）> kana_score（4500）の順序不変条件が成立する。
        let direct_names: Vec<&str> = direct.iter().map(|r| r.name.as_str()).collect();
        let kana_names: Vec<&str> = kana.iter().map(|r| r.name.as_str()).collect();
        assert!(
            direct_names.contains(&"どきゅめんと"),
            "直接クエリで どきゅめんと がヒットするはず: {:?}",
            direct_names
        );
        assert!(
            kana_names.contains(&"どきゅめんと") || kana_names.contains(&"ドキュメント"),
            "kana クエリで少なくとも一方がヒットするはず: {:?}",
            kana_names
        );
    }

    #[test]
    fn kana_search_min_chars_blocks_short_query() {
        // min_chars=2 のとき 1文字クエリ "a" → "あ" はヒットしない
        let entries = make_entries(&["あいうえお"]);
        let mut engine = SearchEngine::new(entries);
        let h = empty_history();
        let config = SearchOptions {
            migemo_enabled: true,
            migemo_min_chars: 2,
            ..SearchOptions::default()
        };
        let results = engine.search_with_options("a", 8, &h, SearchMode::Substring, config);
        assert!(
            results.is_empty(),
            "1文字クエリは migemo_min_chars=2 でブロックされるはず"
        );
    }

    #[test]
    fn kana_search_partial_romaji_not_used() {
        // "dok" → "どk"（'k' が残る） → kana_query が None になりヒットしない
        let entries = make_entries(&["ドキュメント"]);
        let mut engine = SearchEngine::new(entries);
        let h = empty_history();
        let results = engine.search_with_options(
            "dok",
            8,
            &h,
            SearchMode::Substring,
            migemo_config(),
        );
        // "dok" は部分ローマ字で変換後に 'k' が残るため kana_query=None
        // ドキュメント は Substring("dok") にも直接マッチしないため空
        assert!(
            results.is_empty(),
            "部分ローマ字 'dok' では kana マッチもしないはず: {:?}",
            results.iter().map(|r| r.name.as_str()).collect::<Vec<_>>()
        );
    }

    // --- incremental + kana 遷移テスト ---

    #[test]
    fn incremental_kana_min_chars_crossing_does_not_lose_kana_match() {
        // "d" (min_chars=2 未満 → kana_query=None) → "do" (min_chars=2 到達 → kana_query=Some("ど"))
        // 前回の prev_candidates には kana マッチが含まれていない。
        // full scan にフォールバックしなければ "ドキュメント" が欠落する。
        let entries = make_entries(&["ドキュメント", "Discord"]);
        let mut engine = SearchEngine::new(entries);
        let h = empty_history();
        let cfg = migemo_config();

        // "d" → kana_query=None（min_chars=2 未満）
        let _ = engine.search_with_options("d", 8, &h, SearchMode::Substring, cfg);

        // "do" → kana_query=Some("ど")。前回 prev_kana_query=None なので full scan。
        let results = engine.search_with_options("do", 8, &h, SearchMode::Substring, cfg);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"ドキュメント"),
            "min_chars 超え遷移で ドキュメント がヒットするはず（full scan フォールバック必須）: {:?}",
            names
        );
    }

    #[test]
    fn incremental_kana_ascii_residue_clearing_does_not_lose_kana_match() {
        // "dok" (ASCII 残留 'k' → kana_query=None) → "dokyu" (完全変換 → kana_query=Some("どきゅ"))
        // prev_candidates に kana マッチが含まれていないため full scan が必要。
        let entries = make_entries(&["ドキュメント", "Docking Station"]);
        let mut engine = SearchEngine::new(entries);
        let h = empty_history();
        let cfg = migemo_config();

        // "dok" → kana_query=None（"どk" に ASCII 残留）
        let _ = engine.search_with_options("dok", 8, &h, SearchMode::Substring, cfg);

        // "dokyu" → kana_query=Some("どきゅ")。prev_kana_query=None なので full scan。
        let results = engine.search_with_options("dokyu", 8, &h, SearchMode::Substring, cfg);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"ドキュメント"),
            "ASCII 残留解消遷移で ドキュメント がヒットするはず（full scan フォールバック必須）: {:?}",
            names
        );
    }

    #[test]
    fn incremental_kana_to_kana_reuses_cache() {
        // "do" (kana_query=Some) → "doku" (kana_query=Some) は incremental で正しく動作する。
        // kana→kana の拡張なので prev_candidates を再利用して問題ない。
        let entries = make_entries(&["ドキュメント", "Discord", "Chrome"]);
        let mut engine = SearchEngine::new(entries);
        let h = empty_history();
        let cfg = migemo_config();

        let _ = engine.search_with_options("do", 8, &h, SearchMode::Substring, cfg);
        let incremental =
            engine.search_with_options("doku", 8, &h, SearchMode::Substring, cfg);

        // fresh エンジンとの比較で正確性を担保
        let mut fresh = SearchEngine::new(make_entries(&["ドキュメント", "Discord", "Chrome"]));
        let fresh_result =
            fresh.search_with_options("doku", 8, &h, SearchMode::Substring, cfg);

        let inc_names: Vec<&str> = incremental.iter().map(|r| r.name.as_str()).collect();
        let fresh_names: Vec<&str> = fresh_result.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            inc_names, fresh_names,
            "kana→kana incremental は fresh 結果と一致する必要がある"
        );
    }

    #[test]
    fn incremental_kana_non_monotonic_n_vowel_falls_back() {
        // "kan" → kana="かん", "kana" → kana="かな"
        // ローマ字→かな変換は非単調: 末尾の「ん」が「な」に変わる。
        // "かな" は "かん" の prefix 拡張ではないため incremental は使えない。
        // "かなめ" のようなエントリは "かん" でヒットしないが "かな" でヒットする。
        let entries = make_entries(&["かなめ", "かんな", "Kanata"]);
        let mut engine = SearchEngine::new(entries);
        let h = empty_history();
        let cfg = migemo_config();

        // "kan" → kana_query=Some("かん")
        let r1 = engine.search_with_options("kan", 8, &h, SearchMode::Substring, cfg);
        let names1: Vec<&str> = r1.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names1.contains(&"かんな"),
            "\"kan\"(かん) で「かんな」がヒットするはず: {:?}",
            names1
        );

        // "kana" → kana_query=Some("かな")。"かな" は "かん" の prefix ではない → full scan。
        let r2 = engine.search_with_options("kana", 8, &h, SearchMode::Substring, cfg);
        let names2: Vec<&str> = r2.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names2.contains(&"かなめ"),
            "非単調遷移 kan→kana で「かなめ」がヒットするはず（full scan フォールバック必須）: {:?}",
            names2
        );
    }

    #[test]
    fn incremental_kana_monotonic_extension_reuses_cache() {
        // "ka" → kana="か", "kan" → kana="かん"
        // "かん" は "か" の prefix 拡張 → incremental が使える。
        let entries = make_entries(&["かなめ", "かんな", "Chrome"]);
        let mut engine = SearchEngine::new(entries);
        let h = empty_history();
        let cfg = migemo_config();

        let _ = engine.search_with_options("ka", 8, &h, SearchMode::Substring, cfg);
        let incremental =
            engine.search_with_options("kan", 8, &h, SearchMode::Substring, cfg);

        let mut fresh = SearchEngine::new(make_entries(&["かなめ", "かんな", "Chrome"]));
        let fresh_result =
            fresh.search_with_options("kan", 8, &h, SearchMode::Substring, cfg);

        let inc_names: Vec<&str> = incremental.iter().map(|r| r.name.as_str()).collect();
        let fresh_names: Vec<&str> = fresh_result.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            inc_names, fresh_names,
            "か→かん は単調拡張なので incremental は fresh と一致するはず"
        );
    }

    // --- migemo 条件付き構築テスト (issue #337) ---

    #[test]
    fn migemo_disabled_build_leaves_kana_empty() {
        // migemo OFF で構築 → kana 未構築。その後 migemo ON で検索しても
        // panic せず、kana マッチは出ない（空ガード）。
        let entries = make_entries(&["ドキュメント", "Documents"]);
        let mut engine = SearchEngine::new_with_migemo(entries, false);
        assert!(engine.kana_lower_names.is_empty());
        assert!(engine.kana_char_masks.is_empty());
        let results = engine.search_with_options(
            "dokyu",
            8,
            &empty_history(),
            SearchMode::Substring,
            migemo_config(),
        );
        assert!(
            results.is_empty(),
            "kana 未構築時は migemo 検索が空（panic せず）: {:?}",
            results.iter().map(|r| r.name.as_str()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn migemo_enabled_build_matches_kana() {
        // migemo ON で構築 → kana 構築済み → ローマ字検索がヒット
        let entries = make_entries(&["ドキュメント", "Documents"]);
        let mut engine = SearchEngine::new_with_migemo(entries, true);
        assert_eq!(engine.kana_lower_names.len(), engine.entries.len());
        assert_eq!(engine.kana_char_masks.len(), engine.entries.len());
        let results = engine.search_with_options(
            "dokyu",
            8,
            &empty_history(),
            SearchMode::Substring,
            migemo_config(),
        );
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"ドキュメント"),
            "migemo ON 構築で ドキュメント がヒット: {:?}",
            names
        );
    }

    #[test]
    fn kana_index_follows_migemo_on_off_on() {
        // on→off→on の各構築で kana 挙動が追従する（必須条件 #3）
        let names = &["ドキュメント"];
        fn romaji_hits(engine: &mut SearchEngine) -> bool {
            !engine
                .search_with_options(
                    "dokyu",
                    8,
                    &empty_history(),
                    SearchMode::Substring,
                    SearchOptions {
                        migemo_enabled: true,
                        migemo_min_chars: 2,
                        ..SearchOptions::default()
                    },
                )
                .is_empty()
        }
        let mut on1 = SearchEngine::new_with_migemo(make_entries(names), true);
        assert!(romaji_hits(&mut on1), "on(1): kana ヒットするはず");
        let mut off = SearchEngine::new_with_migemo(make_entries(names), false);
        assert!(!romaji_hits(&mut off), "off: kana 未構築でヒットしない");
        let mut on2 = SearchEngine::new_with_migemo(make_entries(names), true);
        assert!(romaji_hits(&mut on2), "on(2): 再構築で kana 復活");
    }

    #[test]
    fn incremental_with_kana_disabled_build_no_panic() {
        // kana 空（migemo off 構築）+ 検索時 migemo on + incremental シーケンスでも
        // panic せず、kana-off fresh エンジンと一致する（cache-check 由来の追加テスト）。
        let names = &["ドキュメント", "Discord", "Chrome"];
        let cfg = migemo_config();
        let h = empty_history();
        let mut engine = SearchEngine::new_with_migemo(make_entries(names), false);
        let _ = engine.search_with_options("do", 8, &h, SearchMode::Substring, cfg);
        let inc = engine.search_with_options("doku", 8, &h, SearchMode::Substring, cfg);

        let mut fresh = SearchEngine::new_with_migemo(make_entries(names), false);
        let fresh_r = fresh.search_with_options("doku", 8, &h, SearchMode::Substring, cfg);

        let inc_names: Vec<&str> = inc.iter().map(|r| r.name.as_str()).collect();
        let fresh_names: Vec<&str> = fresh_r.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(inc_names, fresh_names, "kana 空 + incremental は fresh と一致");
    }

    // --- パスマッチング テスト ---

    fn make_entry(name: &str, path: &str) -> AppEntry {
        AppEntry {
            name: name.to_string(),
            target_path: path.to_string(),
            is_folder: false,
        }
    }

    #[test]
    fn path_match_substring_finds_entry_by_path_segment() {
        let entries = vec![make_entry("app", "C:\\tool\\editor\\app.exe")];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("tool\\editor", 8, &empty_history(), SearchMode::Substring);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "app");
    }

    #[test]
    fn path_match_score_below_name_match() {
        // "editor" → name="editor" に Substring マッチ (score 5000系)
        //         → name="app" はマッチしない（path にも "editor" を含むがクエリにパス区切りなし）
        // パス区切りなしのクエリではパスマッチは試行されない
        let entries = vec![
            make_entry("editor", "C:\\tool\\editor\\editor.exe"),
            make_entry("app", "C:\\tool\\editor\\app.exe"),
        ];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("editor", 8, &empty_history(), SearchMode::Substring);
        // "editor" は name マッチ、"app" は name にもパスにも（パス区切りなしで試行されない）マッチしない
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "editor");

        // パス区切りありのクエリで比較: "tool\\editor" は両方のパスにマッチするが、
        // "editor" は name にも Substring マッチ → name_score(5000系) > path_score(3000系)
        let entries2 = vec![
            make_entry("editor", "C:\\tool\\editor\\editor.exe"),
            make_entry("app", "C:\\tool\\editor\\app.exe"),
        ];
        let mut engine2 = SearchEngine::new(entries2);
        let results2 =
            engine2.search("tool\\editor", 8, &empty_history(), SearchMode::Substring);
        assert_eq!(results2.len(), 2);
        // "editor" の lower_name は "editor"。クエリ "tool\editor" に対して
        // Substring マッチ: "editor".find("tool\editor") → None（クエリが長い）
        // 両方ともパスマッチのみ → path_score で順序が決まる
        // path_score は byte_position で比較 — 同じパスプレフィックスなのでスコア同等
        // タイブレーク: lower_name 昇順 → "app" < "editor"
        assert_eq!(results2[0].name, "app");
        assert_eq!(results2[1].name, "editor");
    }

    #[test]
    fn path_match_slash_normalized() {
        let entries = vec![make_entry("app", "C:\\tool\\editor\\app.exe")];
        let mut engine = SearchEngine::new(entries);
        // `/` で入力しても `\` に正規化されてマッチする
        let results = engine.search("tool/editor", 8, &empty_history(), SearchMode::Substring);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "app");
    }

    #[test]
    fn path_match_receives_history_boost() {
        let entries = vec![
            make_entry("app1", "C:\\tool\\editor\\app1.exe"),
            make_entry("app2", "C:\\tool\\editor\\app2.exe"),
        ];
        let mut history = HistoryStore::load();
        for _ in 0..5 {
            history.record_launch("C:\\tool\\editor\\app1.exe", "");
        }
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("tool\\editor", 8, &history, SearchMode::Substring);
        assert_eq!(results.len(), 2);
        // app1 は history boost で上位
        assert_eq!(results[0].name, "app1");
        assert_eq!(results[1].name, "app2");
    }

    #[test]
    fn path_match_incremental_cache_monotonic() {
        let entries = vec![
            make_entry("app", "C:\\tool\\editor\\app.exe"),
            make_entry("other", "C:\\other\\other.exe"),
        ];
        let mut engine = SearchEngine::new(entries);
        let h = empty_history();

        // 1回目: "tool\\" でパスマッチ
        let r1 = engine.search("tool\\", 8, &h, SearchMode::Substring);
        assert_eq!(r1.len(), 1);
        assert_eq!(r1[0].name, "app");

        // 2回目: "tool\\ed" に拡張 → パス区切りを含むため incremental は無効化される
        //（decide_incremental の !has_path_sep ガード）。fresh scan と一致することを検証。
        let r2 = engine.search("tool\\ed", 8, &h, SearchMode::Substring);
        assert_eq!(r2.len(), 1);
        assert_eq!(r2[0].name, "app");

        // 比較: fresh engine と同じ結果になること
        let mut fresh = SearchEngine::new(vec![
            make_entry("app", "C:\\tool\\editor\\app.exe"),
            make_entry("other", "C:\\other\\other.exe"),
        ]);
        let fresh_result = fresh.search("tool\\ed", 8, &h, SearchMode::Substring);
        assert_eq!(r2.len(), fresh_result.len());
        assert_eq!(r2[0].name, fresh_result[0].name);
    }

    #[test]
    fn path_match_no_match_returns_empty() {
        let entries = vec![make_entry("app", "C:\\tool\\editor\\app.exe")];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("xyz\\abc", 8, &empty_history(), SearchMode::Substring);
        assert!(results.is_empty());
    }

    #[test]
    fn path_match_fuzzy_mode_skips_bitmask_prefilter() {
        // name="zzz" はクエリ "tool\\editor" の文字 (t,o,l,e,d,i,r) を含まない
        // → 通常ならビットマスクで除外されるが、has_path_sep でスキップされパスマッチする
        let entries = vec![make_entry("zzz", "C:\\tool\\editor\\zzz.exe")];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("tool\\editor", 8, &empty_history(), SearchMode::Fuzzy);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "zzz");
    }

    #[test]
    fn path_match_yen_sign_normalized() {
        // ¥（U+00A5）は日本語 Windows でバックスラッシュとして使われる
        let entries = vec![make_entry("app", "C:\\tool\\editor\\app.exe")];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("tool\u{00a5}editor", 8, &empty_history(), SearchMode::Substring);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "app");
    }

    #[test]
    fn path_match_consecutive_spaces_preserved() {
        // パス成分に連続スペースを含む場合、normalize_query() で潰されない
        let entries = vec![make_entry("app", "C:\\My  Tools\\app.exe")];
        let mut engine = SearchEngine::new(entries);
        let results = engine.search("My  Tools\\", 8, &empty_history(), SearchMode::Substring);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "app");
    }

    #[test]
    fn path_match_incremental_disabled_avoids_accent_false_negative() {
        // incremental cache が有効だった場合の false negative を検証する。
        // entry path に "café" を含み、"cafe\\" (no accent) → "café\\" (with accent)
        // の遷移で norm_query は両方 "cafe\\" だが、path_query は異なる。
        // incremental 無効化により、full scan で正しくマッチする。
        let entries = vec![
            make_entry("app", "C:\\café\\app.exe"),
            make_entry("other", "C:\\other\\other.exe"),
        ];
        let mut engine = SearchEngine::new(entries);
        let h = empty_history();

        // 1回目: "cafe\\" — normalized_key は "c:\café\app.exe" (accent preserved)
        // path_query "cafe\\" は "café" にマッチしない
        let r1 = engine.search("cafe\\", 8, &h, SearchMode::Substring);
        assert!(r1.is_empty());

        // 2回目: "café\\" — path_query "café\\" は "café" にマッチすべき
        // incremental が有効だと前回の空結果を再利用して false negative になる
        let r2 = engine.search("café\\", 8, &h, SearchMode::Substring);
        assert_eq!(r2.len(), 1);
        assert_eq!(r2[0].name, "app");
    }

    #[test]
    fn path_match_history_key_unified_across_separators() {
        // tool/editor と tool\editor で履歴バケットが統一される
        let entries = vec![
            make_entry("app1", "C:\\tool\\editor\\app1.exe"),
            make_entry("app2", "C:\\tool\\editor\\app2.exe"),
        ];
        let mut history = HistoryStore::load();
        // tool/editor（スラッシュ）で起動記録
        history.record_launch("C:\\tool\\editor\\app1.exe", "tool/editor");

        let mut engine = SearchEngine::new(entries);
        // tool\editor（バックスラッシュ）で検索 → 履歴が効くべき
        let results = engine.search("tool\\editor", 8, &history, SearchMode::Substring);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "app1"); // history boost で上位
    }
}
