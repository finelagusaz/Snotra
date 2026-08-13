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
use crate::index_tree::NameArena;
use crate::str_arena::{LowerFileColumn, LowerNameColumn};
use crate::ui_types::SearchResult;

// 構築処理（Wave 1/2・kana マスク・IndexCache 復元・全コンストラクタ）は子モジュールへ分離（#598）。
mod build;
// 常駐ヒープの内訳を数える計測専用の走査。**製品のレイアウトの隣に置く**——別クレートの
// 統合テストからは private フィールドへ届かず、代用すると測る対象が製品からずれる。
mod footprint;
pub use footprint::FootprintRow;
// `target_path` の圧縮表現（フォルダ木の接頭辞共有）は子モジュールへ分離。
mod path_store;
use path_store::PathStore;
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
    /// エントリ本体（`name` / `is_folder`）と、`target_path` をフォルダ木の接頭辞共有で
    /// 保持する表現。フルパスは持たず、必要な時点で組み立てる（`search/path_store.rs` の `//!`）。
    entries: PathStore,
    /// 派生文字列の並列 Vec は `Box<str>` で保持する。これらは構築後に伸長しないため、
    /// `String` の容量ワード（8B/要素）が無駄になる。`str` へ Deref するので読み取り側は無変更。
    ///
    /// **`None` は「`entries[i].name` と同一」を意味する**（`assemble` が構築時に測って潰す）。
    /// `to_lower_folded` が恒等写像になる名前——既に小文字の ASCII と、変換対象を持たない
    /// CJK——がそれに当たり、実データでは 86.6% を占める。**`Option` で足りるのは
    /// `lower_names` に「無い」という状態が元から無いからである**（`lower_file_names` の
    /// `None` は「file name 成分が無い」を先に意味しており、そちらは旗を別に要した）。
    /// **型は `Vec<Option<Box<str>>>` ではなく [`LowerNameColumn`]（アリーナ）である**——
    /// 1 エントリ 1 確保をやめ、spine（16 B × 件数）ごと消すための表現で、線上のバイト列は
    /// 変わっていない（正本は `crate::str_arena` の doc）。
    lower_names: LowerNameColumn,
    /// `None` は「file name 成分が無い」。内容が `lower_names[i]` と同一のときは
    /// `CompactEntry::file_name_is_lower_name` が立ち、ここは `None` に潰れている。
    ///
    /// **ディスク側の 3 状態のうち `SameAsLowerName` はここに残らない**（`assemble` が旗へ
    /// 移す）ので、列としては [`LowerNameColumn`] と同じ 2 状態で足りる。
    lower_file_names: LowerFileColumn,
    /// Character-presence bitmask for lower_name (a-z: bits 0-25, 0-9: bits 26-35).
    /// Kept as a compact `Vec<u64>` — 8 entries per cache line — so the pre-filter sweep
    /// that discards non-matching candidates before scoring is L1-cache-friendly.
    /// Merging this into a per-entry struct would inflate the per-entry size ~20× and
    /// cause a measured 35–120% Fuzzy search regression (see struct-level doc comment).
    char_masks: Vec<u64>,
    /// Character-presence bitmask for lower_file_name (same layout as char_masks).
    file_name_char_masks: Vec<u64>,
    /// エントリ名をひらがな正規化したアリーナ（katakana→hiragana、ASCII はそのまま）。
    /// migemo 検索（ローマ字→かな変換マッチ）で使用。**インデックスキャッシュには保存しない**
    /// ——ゆえに `crate::index_tree::NameArena` と同じ物体でありながら、線上表現の制約は
    /// この列にかからない（`index_tree` の `//!`）。
    kana_lower_names: NameArena,
    /// kana_lower_names 用の損失あり文字存在マスク。migemo 有効時のみ構築し、kana
    /// pre-filter の false positive は許すが false negative は起こさない。
    kana_char_masks: Vec<u64>,
    /// 照合に使う文字列（`entry_view` が返す `lower_name` / `lower_file_name`）のいずれかが
    /// パス区切り（`\` `/` `¥`）を含むか。**契約ではなく構築時に測った結果である**
    /// （`PathStore::sorted_by_path` と同じ形——`SearchEngine::new` は任意の `AppEntry` を
    /// 受け取れるので、「名前に区切りは入らない」を前提にしない）。
    ///
    /// **false のとき、区切りを含む needle は名前に部分列として存在しえない**——ゆえに
    /// パスクエリでは name/file_name の Fuzzy スコアリングを**証明として**飛ばせる（#1057）。
    /// 実運用点の実測は 0 件 / 312,108 件だが、外れた入力（区切りを含む表示名）は
    /// 従来どおりスコアリングを通るだけで**結果は変わらない**。
    ///
    /// 判定述語は [`crate::query::contains_path_sep`] 1 本で、クエリ側
    /// （`QueryPlan::norm_query_has_path_sep`）と**必ず同じ関数を通る**。
    any_name_has_path_sep: bool,
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
    /// このクエリの全一致 index を `prev_candidates` として扱うか——**収集（write）と
    /// 再利用（read）の両方が通る唯一の述語**（#1070）。
    ///
    /// **同じ関数が 1 回の検索で 2 つの時制で呼ばれる**: 検索前は「前回集めたものを**今回**
    /// 再利用してよいか」（[`Self::can_reuse`] 経由）、fold では「**次回**のために今回の
    /// 一致を集めるか」。片側だけ変わるドリフト（読むのに集めない／集めるのに読まない）は
    /// 呼び出し点を 1 本の関数へ寄せることでしか防げない
    /// （`normalize_entry_key_into` / `measure_derived_sharing` と同じ理屈）。
    ///
    /// **`plan.has_path_sep` と `plan.norm_query_has_path_sep` の取り違えについて。** 2 つは
    /// 同じ型の隣接フィールドで、**現在の `nucleo-matcher` の下では外延的に一致する**ため、
    /// ここを書き換えても挙動テストは緑のまま通る（前提と射程は `search/scoring.rs` の
    /// `skip_name` のコメントが条件つきで記録している）。**その前提が崩れた版では結果が変わる**
    /// ——`has_path_sep` が真で `norm_query_has_path_sep` が偽のクエリに対し、
    /// [`Self::can_reuse`] が単調性の無いまま真を返しうるからである。
    ///
    /// 構造が禁じたのは**2 か所の食い違い**であって、この 1 か所での取り違えではない。
    /// 述語を 1 本にしたことで read と write が別々のフィールドを見る形は書けなくなったが、
    /// **どちらのフィールドを見るかの誤りはここに残る**。
    ///
    /// **安全性は正規化の詳細に依存しない。** 集めなければ `prev_candidates` が空になり、
    /// [`Self::can_reuse`] の `!self.prev_candidates.is_empty()` が落ちて**全件走査へ倒れる
    /// だけ**である。全件走査は候補集合の上位集合ではなく**母集団そのもの**で、
    /// `score_one_entry` は呼び出し間状態を読まない純粋な述語ゆえ、結果は 1 件も変わらない。
    /// 述語が動く向きも一方通行である（`can_reuse` を真 → 偽にしか動かせない）。
    fn caches_candidates(plan: &QueryPlan) -> bool {
        // パスクエリは norm_query と path_query で正規化が異なり単調性を保証できないため、
        // 再利用できない（下の can_reuse の doc）。読み手が居ない候補を集めるのは、
        // 実運用点で毎打鍵 312,208 件ぶんの push を裾に積むだけである（額は `PERFORMANCE.md`）。
        !plan.has_path_sep
    }

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
    /// - [`Self::caches_candidates`]: パスクエリは norm_query と path_query で正規化が異なり
    ///   単調性を保証できないため無条件で無効化する。**この項は落とせない**——述語は収集
    ///   （write）側と共有するが、**評価するクエリが違う**（write は前回の検索の `plan`、
    ///   read は今回の検索の `plan`）。ゆえに「非パス → パス」の遷移では `prev_candidates` が
    ///   **非空のまま**この項だけが偽になり、下の `!is_empty()` は助けにならない。ここを落とすと
    ///   `path_query_results_are_identical_to_a_fresh_engine` が落ちる（変異注入で実測）。
    ///   **正しさを担うのはこの read 側であり、write 側の停止は最適化である。**
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
            && Self::caches_candidates(plan)
            && kana_monotonic
    }

    /// 再利用時に前回候補の所有権を取り出す（`std::mem::take` で `self` から移す）。
    fn take_candidates(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.prev_candidates)
    }

    /// 検索完了後に状態を更新する（write）。`candidates` は truncation 前の全一致 index
    /// ——ただし [`Self::caches_candidates`] が偽のクエリでは**空である**（#1070）。
    /// `can_reuse` が read する全フィールドをここで対にして書き、対称更新漏れを構造で防ぐ。
    ///
    /// **残り 3 フィールド（`prev_query` / `prev_mode` / `prev_kana_query`）は条件づけない。**
    /// 止めると「`can_reuse` が read する全フィールドを `update` が書く」という上の対称
    /// 不変条件のほうが壊れる。候補が空になれば `!is_empty()` で再利用は落ちるので、
    /// 条件づける必要もない。
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
    kana.chars()
        .fold(0, |mask, ch| mask | (1u64 << ((ch as u32) & 63)))
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
        self.search_with_options(query, max_results, history, mode, SearchOptions::default())
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
        // 述語（no-dot→dot / kana 単調性 / caches_candidates）と前回状態の read は
        // IncrementalCache::can_reuse に集約する。
        let use_incremental = self.incremental_cache.can_reuse(&plan, mode);

        // 次回のために全一致 index を集めるか（write 側の時制）。read 側と同じ述語を通す
        // ——読み手が居ないクエリで集めない根拠は IncrementalCache::caches_candidates の doc。
        // Copy な bool としてクロージャに move する（kana_available と同じイディオム）。
        let collect_candidates = IncrementalCache::caches_candidates(&plan);

        let candidate_indices: Vec<usize> = if use_incremental {
            self.incremental_cache.take_candidates()
        } else {
            (0..self.entries.len()).collect()
        };

        // kana_lower_names は migemo 無効で構築されたとき空（issue #337）。
        // 構築時 migemo OFF → 検索時 migemo ON（kana_query=Some）の窓で
        // self.kana_lower_names.get(i) が範囲外の添字で panic するのを防ぐ。
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
                    // Some ⟺ マッチ成立。local_matches.push(i) は top-k 採否とは独立に
                    // 行い、top-k から落ちた一致も incremental cache に残す（縮小を防ぐ）。
                    // **収集そのものは collect_candidates が条件づける**（#1070）——
                    // 再利用されえないクエリで集めないだけであり、top_k.push は影響を受けない
                    // ので結果は 1 件も変わらない（IncrementalCache::caches_candidates の doc）。
                    if let Some(scored) =
                        self.score_one_entry(i, &plan, mode, kana_available, history, options)
                    {
                        if collect_candidates {
                            local_matches.push(i);
                        }
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
        // all_match_indices は top-k から落ちた一致も含む全件（ただし collect_candidates が
        // 偽のクエリでは空。#1070）。
        self.incremental_cache.update(
            plan.norm_query.into_owned(),
            all_match_indices,
            mode,
            plan.kana_query,
        );

        results
    }

    /// 「最近起動した」候補。**呼び出し元はいずれも明示の操作である**——`/r` スラッシュコマンド（`src-tauri` の `launcher_controller`）とトレイの履歴メニュー。
    ///
    /// **窓を開くたび・クエリを消すたびには走らない。** [`Self::search_with_options`] は
    /// 空クエリに対して `Vec::new()` を返すだけで、ここを呼ばない。
    /// この一行が「全件走査が毎回の窓表示に乗る」という誤読を 2 度招いたので、頻度を
    /// 推測させない形にしてある——**頻度を書くなら呼び出し元を名指しする。**
    ///
    /// **照合表は探す側（履歴・高々 `max_results` 件）で組む。** 探される側（全エントリ）で
    /// 組むと索引の規模に比例した確保が毎回走り、312,377 エントリで 65.4 ms を実測した
    /// （`recent_launches` が返すのは既定 8 件である）。走査は 1 パスで、キーは
    /// [`scoring::with_normalized_key`] が導出する。
    pub fn recent_history(&self, history: &HistoryStore, max_results: usize) -> Vec<SearchResult> {
        // recent_launches() は正規化済みキーを返すため、照合側も同じ正規化を通した値で引く。
        let recent = history.recent_launches(max_results);
        if recent.is_empty() {
            return Vec::new();
        }
        let wanted: HashMap<&str, usize> = recent
            .iter()
            .enumerate()
            .map(|(rank, path)| (*path, rank))
            .collect();

        // rayon の collect は入力順を保つ。後勝ちで詰めることで、同じキーへ潰れる
        // エントリが複数あるときの取り分けを旧実装（HashMap への後勝ち collect）と揃える。
        // 借用ではなく index で拾う——索引はフルパスを連続したバイト列として持たず、
        // 正規化キーはスレッドローカルの一時バッファにしか無い（`path_store` の `//!`）。
        let hits: Vec<(usize, usize)> = (0..self.entries.len())
            .into_par_iter()
            .filter_map(|i| {
                scoring::with_normalized_key(&self.entries, i, |key| {
                    wanted.get(key).map(|&rank| (rank, i))
                })
            })
            .collect();
        let mut found: Vec<Option<usize>> = vec![None; recent.len()];
        for (rank, i) in hits {
            found[rank] = Some(i);
        }

        found
            .into_iter()
            .flatten()
            .map(|i| {
                let entry = self.entries.get(i);
                SearchResult {
                    name: self.entries.name_at(i).to_string(),
                    // フルパスの組み立ては高々 `max_results` 件に閉じる。
                    path: self.entries.to_path(i),
                    is_folder: entry.is_folder,
                    is_error: false,
                }
            })
            .collect()
    }

    /// 索引の件数。
    ///
    /// **`&[AppEntry]` を貸す `entries()` は無い**——索引は `target_path` を圧縮して持ち、
    /// `AppEntry` の形では存在しないからである（`search/path_store.rs` の `//!`）。
    /// フルパスが要る呼び出し元は `search` / `recent_history` が返す `SearchResult` を使う。
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// 索引が `target_path` のバイト順に並んでいるか（**計測ハーネス専用の観測口**）。
    ///
    /// **製品はこれを読んで分岐してはならない**——分岐は `PathStore::cmp_paths` の内側に
    /// 閉じており、そこへ戻すためにこの値を外へ出していない。要るのは 1 つの理由による:
    /// **パスクエリのフレームコストは、この旗で tie-break の経路が変わる**（真なら index の
    /// 比較、偽なら両辺のフルパスを組み立てる）。旗は構築時の実測であって契約ではないので、
    /// **計器が「どちらの経路を測ったか」を出力に添えられなければ、その計測値は読めない。**
    ///
    /// PATH エントリのマージ（`IndexTree::extend_with_roots`）は**非空のときに限り**旗を
    /// 下ろす。ゆえに「`include_path_env` が真か」は代理指標にならない。
    /// **`pub` にしない。** 唯一の crate 外の読み手は計測ハーネスであり、そちらは
    /// `Engine` の passthrough を通る（そこは `src-tauri/clippy.toml` が製品 crate で禁じている）。
    /// ここを `pub` にすると、**禁止の掛からない 2 つ目の綴り**が生まれる。
    pub(crate) fn sorted_by_path(&self) -> bool {
        self.entries.sorted_by_path()
    }

    /// 先頭から何件までがフルパスのバイト順に並んでいるか（**計測ハーネス専用の観測口**）。
    ///
    /// **`sorted_by_path` が偽でも 0 とは限らない。** PATH マージは末尾へ足すだけなので、
    /// マージ前の範囲は今もバイト順のままである——`cmp_paths` はその範囲の中でだけ
    /// index 比較の高速路へ入れる（契約は `PathStore::cmp_paths` の doc）。
    /// **読み手は crate 内の検知器だけなので `#[cfg(test)]` で閉じる。**
    /// 計測ハーネス（別 crate）が読むのは `Engine::sorted_by_path` であってこちらではない
    /// ——製品から届く綴りを増やさないほうが、禁止を 1 つ足すより強い。
    #[cfg(test)]
    pub(crate) fn sorted_prefix_len(&self) -> usize {
        self.entries.sorted_prefix_len()
    }

    /// 整列している範囲を 0 へ潰す（**A/B 検知器専用**・契約は `PathStore` の同名メソッド）。
    #[cfg(test)]
    pub(crate) fn force_unsorted_for_test(&mut self) {
        self.entries.force_unsorted_for_test();
    }

    /// 索引 `i` の表示名。
    pub fn entry_name(&self, i: usize) -> &str {
        self.entries.name_at(i)
    }
}

#[cfg(test)]
mod tests;
