//! 検索スコアリングと順位計算（マッチ方式・bitmask pre-filter・履歴加点・tie-break・
//! `BinaryHeap` 順序・所有 `SearchResult` 変換）を search.rs から分離（#600）。
//!
//! スコア階層 Prefix > Substring > Kana > Path > Fuzzy の基準は `mod score_tier`（直後の
//! `const _` がコンパイル時に全順序を強制）。`ScoredEntry::Ord` は better = `Ordering::Less`
//! で `BinaryHeap::peek()` = worst。候補選択・rayon fold/reduce・incremental cache 更新は
//! 親 `search.rs` に残す。`SearchEngine` の private 並列 Vec を子モジュールとして直接読む。

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

use nucleo_matcher::{Config as MatcherConfig, Matcher, Utf32String};

use crate::config::SearchHistoryNormalizationConfig;
use crate::history::HistoryStore;
use crate::ui_types::SearchResult;

use super::path_store::CompactEntry;
use super::{
    FOLDER_EXPANSION_WEIGHT, GLOBAL_WEIGHT, PathStore, QUERY_WEIGHT, QueryPlan, SearchEngine,
    SearchMode, SearchOptions,
};

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

    /// 正規化キーの詰め直し先。索引は `normalized_keys` を持たず（実測 35.56 MiB を削った）、
    /// 必要な候補についてだけ `target_path` からここへ導出する。
    /// **容量を再利用するので暖まったあとの確保は起きない。** rayon の worker ごとに 1 本ゆえ
    /// 常駐への寄与は worker 数ぶんで、`MATCHER` と同じ形である。
    static KEY_BUF: RefCell<String> = const { RefCell::new(String::new()) };
}

/// スレッドローカルのバッファへ索引 `i` の正規化キーを詰め、`&str` として貸す。
///
/// **索引の `normalized_keys` を廃した後、正規化キーを得る経路はここ 1 本である**
/// （検索の `score_one_entry` と空クエリの `recent_history` が共有する）。
/// 組み立ての実体は [`PathStore::normalized_into`]——索引は `target_path` すら持たず、
/// フォルダ木の接頭辞共有から正規化バッファへ直接書き出す。規則の正本は
/// [`crate::indexer::normalize_entry_key_into`] で、記録側（`indexer` / `history`）と同じ
/// 規則を通ることが実インデックス全件のバイト一致テストで固定されている。
///
/// 借用は `f` の中に閉じる。`f` の戻り値へキーの参照を持ち出すことはできない
/// （持ち出せてしまうと、次のエントリの詰め直しで内容が入れ替わる）。
///
/// **`f` の中からこの関数を再び呼んではならない**——`borrow_mut` の二重取得で panic する。
/// 現在の 2 つの呼び出し点は入れ子にならないが、`f` は `history` の照合を含む長さがあるので、
/// 中へ正規化を要する処理を足すときは外へ出すこと（誤りは沈黙せず panic として出る）。
pub(super) fn with_normalized_key<R>(paths: &PathStore, i: usize, f: impl FnOnce(&str) -> R) -> R {
    KEY_BUF.with(|cell| {
        let mut key = cell.borrow_mut();
        paths.normalized_into(&mut key, i);
        f(&key)
    })
}

/// Lightweight view over per-entry fields for index `i` that are used in the scoring loop.
/// Bundles 3 references (entry / lower_name / lower_file_name) without
/// changing the underlying SoA layout, so all cache-locality properties are preserved.
/// `char_masks` / `file_name_char_masks` / `kana_lower_names` / `kana_char_masks` are accessed
/// directly from SearchEngine in the scoring closure (same SoA pattern, intentionally excluded
/// from EntryView).
///
/// `normalized_key` はここに無い——索引から外し `KEY_BUF` へ導出する形へ移した
/// （`PERFORMANCE.md`「パスクエリ全走査のコスト — `normalized_keys` を保持するか導出するか」）。
pub(super) struct EntryView<'a> {
    pub(super) entry: &'a CompactEntry,
    pub(super) lower_name: &'a str,
    pub(super) lower_file_name: Option<&'a str>,
}

/// 並列 top-k ヒープで使う、借用ベースのスコア済みエントリ。
/// SearchEngine の並列 Vec（`lower_names` / `entries`）から借用するため、ヒープ滞在中は
/// String clone がゼロ。所有 `SearchResult` への変換（clone）は top-k 確定後に
/// `heap_into_results` が `index` 経由で K 件だけ行う（マッチ M 件 clone の回避、#436 で
/// score_one_entry を抽出した際に混入した M 件 clone の是正）。
pub(super) struct ScoredEntry<'a> {
    pub(super) score: i64,
    pub(super) last_launched: u64,
    pub(super) lower_name: &'a str, // tie-breaking key (alphabetical) = &self.lower_names[index]
    /// tie-break の最終キー（`index` のフルパス）を組み立てるための参照。
    ///
    /// **フルパスの `&str` は借用できない**——索引はフルパスを連続したバイト列として
    /// 持っておらず、組み立て先はスレッドローカルの一時バッファだからである。参照 1 本を
    /// 持ち、比較のときだけ [`PathStore::cmp_paths`] へ `index` を渡す。
    pub(super) paths: &'a PathStore,
    pub(super) index: usize, // SearchResult 組立時に entries[index] から clone する
}

// Higher score is better; better entries are ordered as `Ordering::Less`.
// This makes BinaryHeap::peek() point to the current worst (max by Ord = least good).
// scored.sort() (ascending) then puts the best entry first.
impl PartialEq for ScoredEntry<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score
            && self.last_launched == other.last_launched
            && self.lower_name == other.lower_name
            // `cmp` と同じ経路で判定する（フルパスは組み立てないと比べられない）。
            && self.paths.cmp_paths(self.index, other.index) == Ordering::Equal
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
            // フルパスの**原文のバイト列**で比べる。組み立てはここまで落ちたときだけ走る。
            .then_with(|| self.paths.cmp_paths(self.index, other.index))
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
        SearchMode::Substring => lower_name
            .find(query)
            .map(|idx| score_tier::SUBSTRING_BASE - idx as i64),
        SearchMode::Fuzzy => {
            let h = haystack_u32.expect("Fuzzy mode requires UTF-32 haystack");
            let haystack = h.slice(..);
            let needle = needle_u32.slice(..);
            matcher.fuzzy_match(haystack, needle).map(|s| s as i64)
        }
    }
}

pub(super) fn adjusted_history_boost(
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

/// top-k ヒープを best-first の [`SearchResult`] 列へ変換する（結果組立フェーズ）。
/// `into_sorted_vec()` はヒープ内部 Vec を再利用して昇順ソートする。
/// `ScoredEntry::Ord` は Less = better ゆえ昇順で best が先頭になる（tie-break の意味を保つ）。
/// 所有 String の生成はここで初めて発生する（top-k 確定後の K 件のみ）。
/// **フルパスの組み立てもここに閉じる**——索引はフルパスを持たないため、`clone` ではなく
/// [`PathStore::to_path`] で組み立てる。K 件ぶんゆえホットパスへの寄与はない。
fn heap_into_results(paths: &PathStore, top_k: BinaryHeap<ScoredEntry>) -> Vec<SearchResult> {
    top_k
        .into_sorted_vec()
        .into_iter()
        .map(|r| {
            let entry = paths.get(r.index);
            SearchResult {
                name: entry.name.to_string(),
                path: paths.to_path(r.index),
                is_folder: entry.is_folder,
                is_error: false,
            }
        })
        .collect()
}

/// 並列 top-k 更新規則を一元化する型（#602）。rayon の fold（単一候補）と reduce
/// （task 統合）が同じ挿入規則を共有し、片方だけが変更されるドリフトを防ぐ。
///
/// 順序契約: `ScoredEntry::Ord` は better = `Ordering::Less`、`BinaryHeap` は max-heap ゆえ
/// `peek()` = current worst。満杯時は `candidate.cmp(worst) == Ordering::Less` のときだけ置換。
/// 最終結果は best-first（`into_results`）。tie-break（score / last_launched / lower_name / path）
/// は `ScoredEntry::Ord` が担う。incremental cache 用の全一致 index は TopK に含めず独立保持する。
pub(super) struct TopK<'a> {
    limit: usize,
    heap: BinaryHeap<ScoredEntry<'a>>,
}

impl<'a> TopK<'a> {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            limit,
            heap: BinaryHeap::with_capacity(limit + 1),
        }
    }

    /// 単一候補を top-k 規則で挿入する。limit 未満なら push、満杯なら現在の worst
    /// （`peek_mut` = max by Ord = 最悪）より良い（`Ordering::Less`）ときだけ置換する。
    pub(super) fn push(&mut self, scored: ScoredEntry<'a>) {
        if self.heap.len() < self.limit {
            self.heap.push(scored);
        } else if let Some(mut worst) = self.heap.peek_mut()
            && scored.cmp(&worst) == Ordering::Less
        {
            *worst = scored;
        }
    }

    /// 別 rayon task の TopK を統合する。各候補を同じ `push` 規則へ通すため、
    /// merge 順を変えても最終集合は同一（task 分割の非決定性に依存しない）。
    pub(super) fn merge(&mut self, other: TopK<'a>) {
        for scored in other.heap {
            self.push(scored);
        }
    }

    /// best-first の `SearchResult` 列へ変換する終端操作。所有 String の clone は
    /// top-k 確定後の K 件だけで `index` 経由で行う。
    pub(super) fn into_results(self, paths: &PathStore) -> Vec<SearchResult> {
        heap_into_results(paths, self.heap)
    }
}

impl SearchEngine {
    #[inline]
    pub(super) fn entry_view(&self, i: usize) -> EntryView<'_> {
        EntryView {
            entry: self.entries.get(i),
            lower_name: &self.lower_names[i],
            lower_file_name: self.lower_file_names[i].as_deref(),
        }
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
    pub(super) fn score_one_entry<'a>(
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

        // **正規化キーが要る候補だけを先に切り分ける。** 索引は `normalized_keys` を持たず
        // `target_path` から導出するため、ここを素通りさせると bitmask を通っただけの
        // 不一致候補にまで導出コストが乗る（旧実装が `normalized_key` を読まずに
        // `score?` で抜けていた経路と同じ形を保つ）。キーが要るのは 2 通りだけ:
        // パスマッチ（name/file/kana 全滅かつパスクエリあり）と、マッチ成立後の履歴照合。
        if score.is_none() && plan.path_query.is_none() {
            return None;
        }

        // 1 エントリにつき 1 回だけ詰める——下のパスマッチと履歴照合 3 種が同じ結果を見る。
        with_normalized_key(&self.entries, i, |key| {
            // パスマッチ: name/file_name/kana 全て不成立時のフォールバック。
            // スコア PATH_BASE(3000) は Kana(4500) より低く、名前マッチを常に優先する。
            let score = score.or_else(|| {
                plan.path_query.as_deref().and_then(|pq| {
                    let pos = key.find(pq)?;
                    Some(
                        (score_tier::PATH_BASE - (pos as i64).min(score_tier::PATH_POS_CAP)).max(1),
                    )
                })
            });

            let base_score = score?;

            let (global_launches, last_launched) = history.get_global_stats_normalized(key);
            // 履歴キーは record_launch の保存形式に合わせる:
            // normalize_query() + パス区切り統一。path_query は生クエリベースで
            // スペース/アクセントが異なるため履歴キーには使わない。
            let history_query_key = plan.path_history_key.as_deref().unwrap_or(norm_query_str);
            let qcount = history.query_count_pre_normalized(history_query_key, key) as i64;

            let folder_boost = if v.entry.is_folder {
                history.folder_expansion_count_normalized(key) as i64 * FOLDER_EXPANSION_WEIGHT
            } else {
                0
            };

            let raw_history_boost =
                (global_launches as i64) * GLOBAL_WEIGHT + qcount * QUERY_WEIGHT + folder_boost;
            let history_boost =
                adjusted_history_boost(mode, base_score, raw_history_boost, options);
            let combined = base_score + history_boost;

            Some(ScoredEntry {
                score: combined,
                last_launched,
                lower_name: v.lower_name,
                paths: &self.entries,
                index: i,
            })
        })
    }
}
