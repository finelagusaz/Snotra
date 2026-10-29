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

/// `empty_history()` は名前どおり空でなければならない（#963）。
/// **CI では常に緑で開発機でだけ落ちる**性質は `engine.rs` の同名テストの doc を参照。
///
/// 直下の `recent_history_empty_when_no_launches` はこの前提に乗っている——fixture が
/// 実履歴を持つと「起動していないから空」ではなく「実履歴のパスが合成 entry と
/// たまたま一致しないから空」を測ることになり、意味が入れ替わる。
#[test]
fn empty_history_fixture_is_actually_empty() {
    assert!(
        empty_history().recent_launches(usize::MAX).is_empty(),
        "empty_history() が空でない — 実 history.bin を読んでいる"
    );
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
/// 期待値は本テストが記録したパス（`c:\fake\`）へ絞ってある。**#963 で `empty_history()` が
/// 真に空になった今この絞り込みは不要だが、期待値を「自分が記録したもの」に限る形は
/// fixture の実装に依存しないぶん強い**ので残す。
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

/// 索引 `i` のスコアを、製品と同じ `QueryPlan` を通して `score_one_entry` から直接読む
/// （`SearchResult` はスコアを運ばない）。履歴は空なので、返るのは基準スコアそのものである。
fn score_at(
    engine: &SearchEngine,
    i: usize,
    query: &str,
    mode: SearchMode,
    options: SearchOptions,
) -> Option<i64> {
    let plan =
        prepare_query_plan(query, mode, &options).expect("非空クエリは QueryPlan を生成する");
    let kana_available = !engine.kana_lower_names.is_empty();
    engine
        .score_one_entry(i, &plan, mode, kana_available, &empty_history(), options)
        .map(|s| s.score)
}

/// file name が name と同一のエントリ（フラグが立つ）を `.` のクエリで引いても、名前で
/// 照合したときと同じスコアが出る（#1281: 同じ文字列の 2 度目の照合を飛ばしても結果は変わらない）。
///
/// 期待値は**照合の部品から独立に**組む——Substring は std の `find`、Fuzzy は
/// `fuzzy_match_score` に名前を 1 度だけ通した値。
#[test]
fn has_dot_on_shared_file_name_scores_like_the_name() {
    let mut engine = SearchEngine::new(vec![AppEntry {
        name: "Proj.Tools".to_string(),
        target_path: "C:\\work\\Proj.Tools".to_string(),
        is_folder: true,
    }]);
    assert!(
        engine.entry_view(0).entry.file_name_is_lower_name,
        "前提: name と file name が同一のフォルダにはフラグが立つ"
    );
    let lower = engine.entry_view(0).lower_name.to_owned();
    let options = SearchOptions::default();

    for query in ["proj.t", "j.to", "s.x"] {
        let want = lower.find(query).map(|idx| 5_000 - idx as i64);
        assert_eq!(
            score_at(&engine, 0, query, SearchMode::Substring, options),
            want,
            "Substring {query:?}"
        );
        let needle = nucleo_matcher::Utf32String::from(query);
        let want =
            crate::search::scoring::fuzzy_match_score(&lower, needle.slice(..)).map(i64::from);
        assert_eq!(
            score_at(&engine, 0, query, SearchMode::Fuzzy, options),
            want,
            "Fuzzy {query:?}"
        );
    }
    // 結果の列でも引ける（スコアの直読みだけでなく、走査の経路を通しても落ちない）。
    for mode in [SearchMode::Substring, SearchMode::Fuzzy] {
        let results = engine.search("proj.t", 8, &empty_history(), mode);
        assert_eq!(results.len(), 1, "{mode:?}");
    }
}

/// フラグが立たないエントリ（name ≠ file name）は、`.` のクエリで file name 側に一致する
/// ——Fuzzy の版（Substring は `has_dot_uses_cached_lower_file_name`）。フラグの条件を
/// 取り違えて「立たないほうを飛ばす」と、ここが落ちる。
#[test]
fn has_dot_fuzzy_matches_distinct_file_name() {
    let mut engine = SearchEngine::new(vec![AppEntry {
        name: "Dummy".to_string(),
        target_path: "C:\\fake\\Tool.EXE".to_string(),
        is_folder: false,
    }]);
    assert!(!engine.entry_view(0).entry.file_name_is_lower_name);
    let results = engine.search("tool.exe", 8, &empty_history(), SearchMode::Fuzzy);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Dummy");
}

/// Substring・kana・パスの位置ペナルティは、非 ASCII を含む haystack でも std の `find` が返す
/// バイト位置で決まる（#1281: 照合の手段を替えても位置が変わらない）。
///
/// **期待値の位置は std の `find` から計算する**——照合器の側から取ると不動点になる。
#[test]
fn substring_kana_and_path_positions_agree_with_std_find_on_non_ascii() {
    let options = SearchOptions::default();

    // Substring: 名前の途中の非 ASCII に一致する。
    let engine = SearchEngine::new(make_entries(&["日本語ツール設定"]));
    let lower = engine.entry_view(0).lower_name.to_owned();
    let idx = lower.find("設定").expect("前提: 名前が クエリを含む") as i64;
    assert!(idx > 0, "前提: 先頭以外で一致する");
    assert_eq!(
        score_at(&engine, 0, "設定", SearchMode::Substring, options),
        Some(5_000 - idx)
    );

    // kana: 名前の途中のかなに一致する（名前・file name 側は一致しない）。
    let migemo = SearchOptions {
        migemo_enabled: true,
        migemo_min_chars: 2,
        ..SearchOptions::default()
    };
    let engine = SearchEngine::new_with_migemo(make_entries(&["マイドキュメント"]), true);
    let plan = prepare_query_plan("dokyu", SearchMode::Substring, &migemo).unwrap();
    let kana_query = plan
        .kana_query
        .clone()
        .expect("前提: dokyu は kana へ変換できる");
    let pos = engine
        .kana_lower_names
        .get(0)
        .find(kana_query.as_str())
        .expect("前提: かな名がクエリを含む") as i64;
    assert!(pos > 0, "前提: 先頭以外で一致する");
    assert_eq!(
        score_at(&engine, 0, "dokyu", SearchMode::Substring, migemo),
        Some((4_500 - pos).max(1))
    );

    // パス: 非 ASCII の段を含むパスの途中に一致する（名前は一致しない）。
    let engine = SearchEngine::new(vec![AppEntry {
        name: "Alpha".to_string(),
        target_path: "C:\\日本語\\tool\\alpha.exe".to_string(),
        is_folder: false,
    }]);
    let key = crate::indexer::normalize_entry_key("C:\\日本語\\tool\\alpha.exe");
    let pos = key.find("tool\\alpha").expect("前提: キーがクエリを含む") as i64;
    for mode in [SearchMode::Substring, SearchMode::Fuzzy] {
        assert_eq!(
            score_at(&engine, 0, "tool\\alpha", mode, options),
            Some((3_000 - pos.min(500)).max(1)),
            "{mode:?}"
        );
    }
}

/// `prepare_query_plan` は Substring のときだけ名前の照合器を作る——`match_score_single_cached` の
/// Substring の腕はこれを `expect` で取り出すので、片側だけ変わると panic になり、Substring
/// 以外で作ると使われない確保が打鍵ごとに乗る。kana / パスの照合器は needle の有無に一致する。
#[test]
fn query_plan_builds_finders_exactly_for_the_needles_in_use() {
    let migemo = SearchOptions {
        migemo_enabled: true,
        migemo_min_chars: 2,
        ..SearchOptions::default()
    };
    for mode in [SearchMode::Prefix, SearchMode::Substring, SearchMode::Fuzzy] {
        for query in ["dokyu", "tool\\alpha", "app.exe"] {
            let plan = prepare_query_plan(query, mode, &migemo).unwrap();
            assert_eq!(
                plan.substring_finder.is_some(),
                mode == SearchMode::Substring,
                "{mode:?} {query:?}"
            );
            assert_eq!(
                plan.kana_finder.is_some(),
                plan.kana_query.is_some(),
                "{query:?}"
            );
            assert_eq!(
                plan.path_finder.is_some(),
                plan.path_query.is_some(),
                "{query:?}"
            );
        }
    }
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

/// 照合器へ渡す haystack を候補ごとには確保せずに組む形が、置き換え前の
/// `Utf32String::from(..).slice(..)` と同じ値を返す。
///
/// 回帰テスト: 確保をやめる変更の安全性はこの一致だけに依る——照合器への入力が 1 文字でも
/// 違えばスコアが静かに動き、順位だけが変わる（クラッシュしない）。**`nucleo-matcher` 0.3.1 では
/// 結合アクセントの行が要石である**: `Utf32Str::new` は非 ASCII でも書記素の先頭がすべて ASCII の
/// 文字列に `Ascii` を返し、`Utf32String::from` が返す `Unicode` とずれる。その誤った置き換えを
/// 弾くのはこの行である（0.3.1 では、残りの行はどちらの形でも一致する）。**版を上げてこれが赤に
/// なったら、まず上流の側が変わったかを見る**（`utf32_haystack` は 0.3.1 の `From<&str>` の写し）。
#[test]
fn utf32_haystack_matches_utf32_string_from() {
    let mut buf = Vec::new();
    for s in [
        "",
        "visual studio code",
        "c:\\program files\\app.exe",
        "ドキュメント",
        "日本語入力テスト 00001",
        "re\u{301}sume\u{301}",
        "\u{30ab}\u{3099}",
        "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467} family",
        "a\r\nb",
        "mixed ascii と かな",
    ] {
        let want = nucleo_matcher::Utf32String::from(s);
        assert_eq!(
            crate::search::scoring::utf32_haystack(s, &mut buf),
            want.slice(..),
            "{s:?} で照合器への入力がずれた"
        );
    }
}

/// 同じ一致を、実インデックスで照合器へ渡りうる全 haystack（`entry_view` が返す照合用の文字列）で
/// 確かめる。実インデックスが無ければ自動スキップする corpus であり、機構としての保証は上の
/// 合成のほうが持つ（`real_index_entries` の doc）。
#[test]
fn utf32_haystack_matches_utf32_string_from_over_real_index() {
    let Some(entries) = super::common::real_index_entries() else {
        println!("実インデックスが無いためスキップします。");
        return;
    };
    let engine = SearchEngine::new_with_migemo(entries, false);
    let mut buf = Vec::new();
    let (mut checked, mut non_ascii) = (0usize, 0usize);
    for i in 0..engine.entry_count() {
        let view = engine.entry_view(i);
        for s in std::iter::once(view.lower_name).chain(view.lower_file_name) {
            let want = nucleo_matcher::Utf32String::from(s);
            assert_eq!(
                crate::search::scoring::utf32_haystack(s, &mut buf),
                want.slice(..),
                "index {i} の {s:?} で照合器への入力がずれた"
            );
            checked += 1;
            non_ascii += usize::from(!s.is_ascii());
        }
    }
    println!("{checked} 件の haystack で一致しました（うち非 ASCII {non_ascii} 件）。");
}
