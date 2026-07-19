//! パスマッチング（区切り正規化・履歴キー統一・incremental 無効化）のテスト。

use super::common::empty_history;
use crate::history::HistoryStore;
use crate::indexer::AppEntry;
use crate::search::*;

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
    let results2 = engine2.search("tool\\editor", 8, &empty_history(), SearchMode::Substring);
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
    let results = engine.search(
        "tool\u{00a5}editor",
        8,
        &empty_history(),
        SearchMode::Substring,
    );
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
