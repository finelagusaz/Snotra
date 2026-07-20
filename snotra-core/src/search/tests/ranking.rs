//! top-k 選択・タイブレーク・履歴ブースト正規化・文字ビットマスクのテスト。

use super::common::{empty_history, make_entries};
use crate::config::SearchHistoryNormalizationConfig;
use crate::indexer::AppEntry;
use crate::query::char_bitmask;
use crate::search::scoring::{ScoredEntry, TopK, adjusted_history_boost};
use crate::search::*;

#[test]
fn search_top_k_replaces_worst_when_better_arrives_late() {
    let entries = make_entries(&["appabcdefghij", "appabcdefghi", "appx", "app"]);
    let mut engine = SearchEngine::new(entries);

    let results = engine.search("app", 2, &empty_history(), SearchMode::Prefix);
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();

    assert_eq!(names, vec!["app", "appx"]);
}

#[test]
fn search_top_k_is_independent_from_input_order() {
    let ordered = make_entries(&["appabcdefghij", "appabcdefghi", "appx", "app"]);
    let reversed = make_entries(&["app", "appx", "appabcdefghi", "appabcdefghij"]);
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
fn adjusted_history_boost_disabled_uses_raw_value() {
    let adjusted = adjusted_history_boost(SearchMode::Fuzzy, 100, 250, SearchOptions::default());
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
    assert_ne!(mask & (1 << 0), 0); // a
    assert_ne!(mask & (1 << 1), 0); // b
    assert_ne!(mask & (1 << 27), 0); // '1' = bit 26+1
    assert_ne!(mask & (1 << 28), 0); // '2' = bit 26+2
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

// --- TopK 単体テスト（#602: top-k 更新規則の一元化） ---

/// index i を name "e{i}" に対応させる entries（`TopK::into_results` の照合用）。
fn topk_entries(n: usize) -> Vec<AppEntry> {
    (0..n)
        .map(|i| AppEntry {
            name: format!("e{i}"),
            target_path: format!("C:\\fake\\e{i}.lnk"),
            is_folder: false,
        })
        .collect()
}

/// score と index を指定した `ScoredEntry`。lower_name/path は `entries[index]` を借用するため、
/// entries は TopK より長生きする必要がある。
fn se(score: i64, index: usize, entries: &[AppEntry]) -> ScoredEntry<'_> {
    ScoredEntry {
        score,
        last_launched: 0,
        lower_name: &entries[index].name,
        path: &entries[index].target_path,
        index,
    }
}

/// `(score, index)` 列を limit の TopK に push し、best-first の name 列を返す。
fn topk_names(limit: usize, scores: &[(i64, usize)], entries: &[AppEntry]) -> Vec<String> {
    let mut t = TopK::new(limit);
    for &(score, index) in scores {
        t.push(se(score, index, entries));
    }
    t.into_results(entries).into_iter().map(|r| r.name).collect()
}

#[test]
fn topk_replaces_worst_when_better_arrives_late() {
    // limit 2 が満杯（10,20）後に、worst(10) より良い 30 が来たら置換する。
    let entries = topk_entries(3);
    let names = topk_names(2, &[(10, 0), (20, 1), (30, 2)], &entries);
    assert_eq!(names, vec!["e2", "e1"]); // best-first: 30, 20
}

#[test]
fn topk_limit_zero_stays_empty() {
    let entries = topk_entries(3);
    let names = topk_names(0, &[(10, 0), (20, 1), (30, 2)], &entries);
    assert!(names.is_empty());
}

#[test]
fn topk_limit_one_keeps_best() {
    let entries = topk_entries(3);
    let names = topk_names(1, &[(10, 0), (30, 1), (20, 2)], &entries);
    assert_eq!(names, vec!["e1"]); // 最高スコア 30 のみ
}

#[test]
fn topk_under_capacity_keeps_all_best_first() {
    // 候補が limit 未満なら全件を best-first で返す。
    let entries = topk_entries(3);
    let names = topk_names(5, &[(10, 0), (30, 1), (20, 2)], &entries);
    assert_eq!(names, vec!["e1", "e2", "e0"]); // 30, 20, 10
}

#[test]
fn topk_over_capacity_keeps_top_k() {
    // 候補が limit 超過なら上位 K 件を best-first で返す。
    let entries = topk_entries(4);
    let names = topk_names(2, &[(10, 0), (40, 1), (20, 2), (30, 3)], &entries);
    assert_eq!(names, vec!["e1", "e3"]); // 40, 30
}

#[test]
fn topk_full_tie_keeps_first_inserted_and_does_not_thrash() {
    // 完全 tie（score/last_launched/lower_name/path すべて同一）では cmp == Equal ゆえ
    // 満杯後の候補は置換しない（Ordering::Less のときだけ置換）。
    let entries = topk_entries(1);
    let names = topk_names(2, &[(100, 0), (100, 0), (100, 0)], &entries);
    assert_eq!(names.len(), 2); // limit 分だけ保持、3 件目は Equal で不採用
    assert!(names.iter().all(|n| n == "e0"));
}

#[test]
fn topk_merge_is_order_independent() {
    // task 統合（merge）の順序を変えても最終集合は同一。
    let entries = topk_entries(4);
    let build = |seed: &[(i64, usize)]| {
        let mut t = TopK::new(2);
        for &(s, i) in seed {
            t.push(se(s, i, &entries));
        }
        t
    };

    let mut a1 = build(&[(10, 0), (40, 1)]);
    a1.merge(build(&[(20, 2), (30, 3)]));
    let names_ab: Vec<String> = a1.into_results(&entries).into_iter().map(|r| r.name).collect();

    let mut b1 = build(&[(20, 2), (30, 3)]);
    b1.merge(build(&[(10, 0), (40, 1)]));
    let names_ba: Vec<String> = b1.into_results(&entries).into_iter().map(|r| r.name).collect();

    assert_eq!(names_ab, vec!["e1", "e3"]); // top2 of {10,40,20,30} = 40,30
    assert_eq!(names_ab, names_ba); // merge 順に依存しない
}
