//! 基本検索・拡張子照合・アクセント正規化・空クエリ履歴候補のテスト。

use super::common::{empty_history, make_entries};
use crate::indexer::AppEntry;
use crate::search::*;

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

/// `recent_history` の出力順は `recent_launches` の順に従い、対応する entry を持たない
/// 履歴パスは落ちる。
///
/// **`recent_launches` の返り値そのものを期待値にするのが要点である**——同一秒に記録すると
/// `last_launched` が並び、順序はパス昇順のタイブレークで決まる。期待値を固定文字列で
/// 書くと、計測の秒境界をまたいだ回だけ落ちる不安定なテストになる。
///
/// **`empty_history()` は空ではない**——実体は `HistoryStore::load()` で、開発機の実
/// `history.bin` を読む。ゆえに期待値は本テストが記録したパス（`c:\fake\`）へ絞る。
/// 絞らないと開発機の実起動履歴が期待値に混入する（実際に踏んだ）。
#[test]
fn recent_history_follows_recent_launches_order_and_drops_unmatched() {
    let entries = make_entries(&["alpha", "beta", "gamma"]);
    let engine = SearchEngine::new(entries);
    let mut history = empty_history();
    history.record_launch("C:\\fake\\gamma.lnk", "g");
    history.record_launch("C:\\fake\\alpha.lnk", "a");
    history.record_launch("C:\\fake\\beta.lnk", "b");
    // 索引に対応する entry を持たない履歴（アンインストール後などに実際に起きる）。
    history.record_launch("C:\\fake\\vanished.lnk", "v");

    let results = engine.recent_history(&history, 8);
    assert_eq!(results.len(), 3, "対応する entry の無い履歴は落ちる");

    let expected: Vec<String> = history
        .recent_launches(usize::MAX)
        .into_iter()
        .filter(|p| p.starts_with("c:\\fake\\") && !p.contains("vanished"))
        .map(str::to_string)
        .collect();
    let actual: Vec<String> = results
        .iter()
        .map(|r| crate::indexer::normalize_entry_key(&r.path))
        .collect();
    assert_eq!(actual, expected, "出力順は recent_launches の順に従う");
}

/// 正規化キーが衝突する entry が複数あるとき、**索引の後ろにあるほうを返す**。
///
/// 旧実装は全 entry から `HashMap` を組んでおり、後から挿入したものが上書きしていた。
/// 走査 1 パスへ変えたときに前勝ちへ転ぶと、同じ入力で違う結果を返すようになる
/// ——挙動として現れにくい（どちらも「それらしい」結果に見える）ので検査で固定する。
#[test]
fn recent_history_keeps_last_entry_when_paths_collapse_to_same_key() {
    let entries = vec![
        AppEntry {
            name: "First".to_string(),
            target_path: "C:\\Fake\\Dup.lnk".to_string(),
            is_folder: false,
        },
        AppEntry {
            name: "Second".to_string(),
            target_path: "C:/fake/dup.lnk".to_string(),
            is_folder: false,
        },
    ];
    let engine = SearchEngine::new(entries);
    let mut history = empty_history();
    history.record_launch("C:\\fake\\dup.lnk", "dup");

    let results = engine.recent_history(&history, 8);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Second", "衝突時は索引の後ろが勝つ");
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
