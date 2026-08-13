//! incremental search キャッシュ（prefix 拡張・backspace・mode 変更・dot 遷移）のテスト。

use super::common::{empty_history, make_entries};
use crate::indexer::AppEntry;
use crate::search::*;

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
    let mut engine = SearchEngine::new(make_entries(&["Firefox", "Final Cut", "Chrome", "Firm"]));
    let h = empty_history();

    let _ = engine.search("fire", 8, &h, SearchMode::Fuzzy);
    // "fir" は "fire" の拡張ではない → full scan にフォールバック
    let results = engine.search("fir", 8, &h, SearchMode::Fuzzy);

    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"Firefox"),
        "full scan で Firefox が返る必要がある"
    );
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
    assert!(
        names.contains(&"Firefox"),
        "Prefix 'fi' で Firefox が返る必要がある"
    );
    assert!(
        names.contains(&"Final Cut"),
        "Prefix 'fi' で Final Cut が返る必要がある"
    );
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
        inc_names, fresh_names,
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

/// パスクエリの収集停止（#1070）を突くための 2 件 fixture。
///
/// - `Chrome` — クエリ `"c"` に**名前で**マッチする。`target_path` は `D:` 配下ゆえ
///   `"c:\"` には**マッチしない**
/// - `Zephyr` — 名前にも file_name にも `c` を含まず、`target_path` だけが `c:\` 配下にある
///
/// この非対称が、read 側（`can_reuse`）だけ述語を落とす変異を観測可能にする
/// ——`"c"` の候補（`Chrome` のみ）を `"c:\"` で再利用すると `Zephyr` が欠ける。
fn path_drift_entries() -> Vec<AppEntry> {
    vec![
        AppEntry {
            name: "Chrome".to_string(),
            target_path: "D:\\apps\\Chrome.lnk".to_string(),
            is_folder: false,
        },
        AppEntry {
            name: "Zephyr".to_string(),
            target_path: "C:\\tools\\zephyr.lnk".to_string(),
            is_folder: false,
        },
    ]
}

#[test]
fn path_query_leaves_the_incremental_candidates_empty() {
    // #1070 の新しい不変条件そのもの: パス区切りを含むクエリでは全一致 index を集めない。
    let mut engine = SearchEngine::new(path_drift_entries());
    let h = empty_history();

    let results = engine.search("c:\\", 8, &h, SearchMode::Fuzzy);

    // マッチ 0 件では「空」が自明に成立して述語反転の変異を検出できない。
    // 収集が起きたなら非空になる状態であることを先に固定する。
    assert_eq!(
        results.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        vec!["Zephyr"],
        "パスクエリは Zephyr にマッチする（収集が起きれば候補は非空になる状態）"
    );
    assert!(
        engine.incremental_cache.prev_candidates.is_empty(),
        "パスクエリでは candidate index を収集しない（読み手 can_reuse が !has_path_sep を要求するため）"
    );
}

#[test]
fn non_path_query_still_populates_the_incremental_candidates() {
    // 逆向き: 述語の反転・収集の無条件停止を捕まえる。
    let mut engine = SearchEngine::new(path_drift_entries());
    let h = empty_history();

    let results = engine.search("c", 8, &h, SearchMode::Fuzzy);

    assert_eq!(
        results.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        vec!["Chrome"],
        "区切りを含まないクエリは名前で Chrome にマッチする"
    );
    assert!(
        !engine.incremental_cache.prev_candidates.is_empty(),
        "区切りを含まないクエリでは今までどおり全一致 index を収集する（incremental の再利用元）"
    );
}

#[test]
fn path_query_results_are_identical_to_a_fresh_engine() {
    // read 側（can_reuse）だけ述語を落とすドリフトを捕まえる唯一の形。
    // 打鍵列 "c" → "c:\" は can_reuse の他条件（prev_mode 一致 / prev_candidates 非空 /
    // starts_with / dot 単調性 / kana 単調性）を**全部満たす**ため、has_path_sep だけが
    // 分岐点になる。既存の path_match_incremental_cache_monotonic は 2 打鍵ともパスクエリで、
    // 2 回目は自分自身の has_path_sep で必ず落ちるのでこの差を見ない。
    let mut engine = SearchEngine::new(path_drift_entries());
    let h = empty_history();

    let first = engine.search("c", 8, &h, SearchMode::Fuzzy);
    assert!(!first.is_empty(), "1 打鍵目が候補を残す前提を固定する");
    let incremental = engine.search("c:\\", 8, &h, SearchMode::Fuzzy);

    let mut fresh = SearchEngine::new(path_drift_entries());
    let fresh_result = fresh.search("c:\\", 8, &h, SearchMode::Fuzzy);

    let inc: Vec<(&str, &str)> = incremental
        .iter()
        .map(|r| (r.name.as_str(), r.path.as_str()))
        .collect();
    let expected: Vec<(&str, &str)> = fresh_result
        .iter()
        .map(|r| (r.name.as_str(), r.path.as_str()))
        .collect();
    assert_eq!(
        inc, expected,
        "非パス → パスの遷移でも、結果は新品 engine と順序込みで一致する"
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
