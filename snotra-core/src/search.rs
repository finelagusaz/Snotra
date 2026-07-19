//! 検索順位計算エンジン（`SearchEngine`）。
//!
//! Prefix / Substring / Kana / Fuzzy / Path のマッチと履歴ブースト、incremental search
//! キャッシュ、空クエリ時の履歴候補を担う。スコア階層は Prefix > Substring > Kana > Path >
//! Fuzzy（基準は `mod score_tier`）。cache locality のためエントリ属性は並列 Vec で保持する
//! （struct 化はベンチ劣化を確認済み——根拠は `SearchEngine` の struct doc を参照）。

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use nucleo_matcher::{Config as MatcherConfig, Matcher, Utf32String};
use rayon::prelude::*;

use crate::config::{SearchConfig, SearchHistoryNormalizationConfig};
use crate::history::HistoryStore;
use crate::indexer::AppEntry;
use crate::ui_types::SearchResult;

// 構築処理（Wave 1/2・kana マスク・IndexCache 復元・全コンストラクタ）は子モジュールへ分離（#598）。
mod build;
// クエリ計画（QueryPlan・prepare_query_plan の純粋導出）は子モジュールへ分離（#599）。
mod query_plan;
use query_plan::{QueryPlan, prepare_query_plan};

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
    /// Kept as a compact `Vec<u64>` — 8 entries per cache line — so the pre-filter sweep
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
/// `char_masks` / `file_name_char_masks` / `kana_lower_names` / `kana_char_masks` are accessed
/// directly from SearchEngine in the scoring closure (same SoA pattern, intentionally excluded
/// from EntryView).
struct EntryView<'a> {
    entry: &'a AppEntry,
    lower_name: &'a str,
    lower_file_name: Option<&'a str>,
    normalized_key: &'a str,
}

/// kana の Unicode scalar value を 64 bit に写す損失あり存在マスク。
/// 同じ文字は必ず同じ bit になるため、mask 不一致なら kana substring は不成立と分かる。
/// 衝突は候補を余計に通すだけで、kana マッチを棄却しない。
#[inline]
fn kana_char_mask(kana: &str) -> u64 {
    kana.chars().fold(0, |mask, ch| mask | (1u64 << ((ch as u32) & 63)))
}

impl SearchEngine {
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
    /// [`Self::search_with_options`] に `migemo_enabled = true` の
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

        // Phase 4: incremental search — クエリが前回クエリの単調拡張でモードが不変の
        // とき、前回のマッチ候補を再利用する。
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
        // Bitmask pre-filter: クエリ文字を含まないエントリを棄却する（Fuzzy のみ。
        // Prefix/Substring は安価な str 操作で足り、bitmask のオーバーヘッドが見合わない）。
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
            // name マッチが高確度のときだけ file_name スコアリングを短絡する
            // （重い fuzzy 処理の回避）。
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

/// 並列 top-k ヒープで使う、借用ベースのスコア済みエントリ。
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
mod tests;
