//! 検索順位計算エンジン（`SearchEngine`）。
//!
//! Prefix / Substring / Kana / Fuzzy / Path のマッチと履歴ブースト、incremental search
//! キャッシュ、空クエリ時の履歴候補を担う。スコア階層は Prefix > Substring > Kana > Path >
//! Fuzzy（基準は `mod score_tier`）。cache locality のためエントリ属性は並列 Vec で保持する
//! （struct 化はベンチ劣化を確認済み——根拠は `SearchEngine` の struct doc を参照）。

use std::collections::HashMap;

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
// スコアリング・順位計算（score_tier・ScoredEntry・score_one_entry・heap_into_results 等）は
// 子モジュールへ分離（#600）。候補選択・rayon fold/reduce・incremental cache 更新は本ファイル。
mod scoring;
use scoring::TopK;

const GLOBAL_WEIGHT: i64 = 5;
const QUERY_WEIGHT: i64 = 20;
const FOLDER_EXPANSION_WEIGHT: i64 = 5;
/// D-3: minimum candidate count for rayon parallel scoring.
/// Below this threshold, thread-splitting overhead exceeds the scoring cost
/// (typical after several keystrokes when incremental search narrows candidates).
const MIN_PAR_CANDIDATES: usize = 512;

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
    /// `SearchConfig` の既定から導く——4 値を再手打ちすると `config.rs` と乖離しうる（#795）。
    /// 本番経路は `SearchOptions::from(&config.search)` を直接使い、この `Default` はテストが使う。
    fn default() -> Self {
        Self::from(&SearchConfig::default())
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
    /// incremental search の再利用状態（前回クエリ・候補・mode・kana query）。
    /// read（再利用判定）と write（検索後更新）を [`IncrementalCache`] のメソッドに閉じ、
    /// 述語や状態を足したときの read/write 対称更新漏れを防ぐ（#601）。
    incremental_cache: IncrementalCache,
}

/// incremental search（前回候補の再利用）の状態を集約する private 型（#601）。
/// ホットパスの並列 `Vec` とは別の小さな状態であり、専用型に閉じても cache locality は不変。
/// 再利用判定（read）と検索完了後の状態更新（write）を対にしてメソッド化し、述語追加時の
/// 対称更新漏れを型で防ぐ。全 4 フィールドは `Default`（空 query / 空候補 / mode 未設定 /
/// kana なし）で初期化し、初回検索は必ず full scan になる。
#[derive(Default)]
struct IncrementalCache {
    /// 正規化済み前回クエリ。
    prev_query: String,
    /// 前回一致した entry index 群。`max_results` truncation の**前**に全件保存する
    /// （top-k から落ちた一致も次回の再利用候補に残し、候補集合の縮小を防ぐ）。
    prev_candidates: Vec<usize>,
    /// 前回の SearchMode。
    prev_mode: Option<SearchMode>,
    /// 前回の kana_query 文字列。ローマ字→かな変換は非単調（"kan"→"かん", "kana"→"かな"）
    /// なため bool では不十分で、実値の prefix 比較が要る。
    prev_kana_query: Option<String>,
}

impl IncrementalCache {
    /// incremental search（前回候補の再利用）の可否を判定する（read のみ）。
    ///
    /// 全述語は「今回の候補集合 ⊆ 前回の候補集合」を保証する単調条件。
    ///
    /// - `!has_dot || prev_query.contains('.')`: "no-dot → dot" 遷移は full scan にフォールバック。
    ///   prev_candidates が file_name スコアリングなしで構築されており、file_name のみで
    ///   マッチするエントリが欠落して false negative になるのを防ぐ。
    /// - kana_monotonic: kana_query の単調性。(None,_)→OK / (Some curr, Some prev) は
    ///   `curr.starts_with(prev)` のとき候補が狭まる / (Some,None) は新規出現ゆえ full scan。
    ///   ローマ字→かな変換は非単調（"kan"→"かん", "kana"→"かな"）ゆえ実値比較が必要。
    /// - `!has_path_sep`: パスクエリは norm_query と path_query で正規化が異なり単調性を
    ///   保証できないため無条件で無効化する（稀ゆえ性能影響は無視できる）。
    fn can_reuse(&self, plan: &QueryPlan, mode: SearchMode) -> bool {
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

    /// 再利用時に前回候補の所有権を取り出す（`std::mem::take` で `self` から移す）。
    fn take_candidates(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.prev_candidates)
    }

    /// 検索完了後に状態を更新する（write）。`candidates` は truncation 前の全一致 index。
    /// `can_reuse` が read する全フィールドをここで対にして書き、対称更新漏れを構造で防ぐ。
    fn update(
        &mut self,
        norm_query: String,
        candidates: Vec<usize>,
        mode: SearchMode,
        kana_query: Option<String>,
    ) {
        self.prev_query = norm_query;
        self.prev_candidates = candidates;
        self.prev_mode = Some(mode);
        self.prev_kana_query = kana_query;
    }
}

/// kana の Unicode scalar value を 64 bit に写す損失あり存在マスク。
/// 同じ文字は必ず同じ bit になるため、mask 不一致なら kana substring は不成立と分かる。
/// 衝突は候補を余計に通すだけで、kana マッチを棄却しない。
#[inline]
fn kana_char_mask(kana: &str) -> u64 {
    kana.chars().fold(0, |mask, ch| mask | (1u64 << ((ch as u32) & 63)))
}

impl SearchEngine {
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
        // 述語（no-dot→dot / kana 単調性 / !has_path_sep）と前回状態の read は
        // IncrementalCache::can_reuse に集約する。ここでは write のみを fold 後に行う。
        let use_incremental = self.incremental_cache.can_reuse(&plan, mode);

        let candidate_indices: Vec<usize> = if use_incremental {
            self.incremental_cache.take_candidates()
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
        // top-k 更新規則は TopK 型に一元化する（fold の単一候補挿入・reduce の task 統合が
        // 同じ push 規則を通す・#602）。fold state の Vec<usize> は top-k とは別に全一致 index を
        // 集める（incremental search cache 用。top-k から落ちた一致も残す）。
        let (top_k, all_match_indices): (TopK, Vec<usize>) = candidate_indices
            .into_par_iter()
            .with_min_len(MIN_PAR_CANDIDATES)
            .fold(
                || (TopK::new(max_results), Vec::<usize>::new()),
                |(mut top_k, mut local_matches), i| {
                    // スコアリング本体は score_one_entry に委譲する（bitmask pre-filter →
                    // name/file_name/kana/path スコア → 履歴ブースト → ScoredEntry 構築）。
                    // Some ⟺ マッチ成立。local_matches.push(i) は top-k 採否より前に無条件で
                    // 行い、top-k から落ちた一致も incremental cache に残す（縮小を防ぐ）。
                    if let Some(scored) =
                        self.score_one_entry(i, &plan, mode, kana_available, history, options)
                    {
                        local_matches.push(i);
                        top_k.push(scored);
                    }

                    (top_k, local_matches)
                },
            )
            .reduce(
                || (TopK::new(max_results), Vec::new()),
                |(mut a_top, mut a_matches), (b_top, b_matches)| {
                    a_top.merge(b_top);
                    a_matches.extend(b_matches);
                    (a_top, a_matches)
                },
            );

        // top_k は self（lower_names / entries）を借用しているため、先に所有 SearchResult へ
        // 変換して借用を終わらせてから incremental cache を write する
        // （all_match_indices は owned な Vec<usize> ゆえ write 順序の入れ替えは意味論に影響しない）。
        let results = top_k.into_results(&self.entries);

        // write: can_reuse が read する全状態を IncrementalCache::update で対にして更新する。
        // all_match_indices は top-k から落ちた一致も含む全件。
        self.incremental_cache.update(
            plan.norm_query.into_owned(),
            all_match_indices,
            mode,
            plan.kana_query,
        );

        results
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

#[cfg(test)]
mod tests;
