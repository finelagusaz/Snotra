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
