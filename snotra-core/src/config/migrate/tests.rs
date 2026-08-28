//! レガシーキーの移行・正規化・不正値のフォールバック。

use super::*;
use crate::config::{InstantCommand, OpenerRule, OpenerTool};

#[test]
fn migrate_oldest_appearance_legacy_to_new_keys() {
    // 最古 [appearance] legacy のみに値があるケース: 新フィールドが None なので legacy 値で補完する。
    // max_results → visible_rows、appearance.top_n_history → result_limit、
    // appearance.max_history_display → recent_limit。
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            max_results = 8
            window_width = 600
            top_n_history = 300
            max_history_display = 12

            [paths]
            additional = []
        "#;
    let mut config: Config = toml::from_str(toml_str).expect("parse");
    assert!(config.apply_migrations());
    assert_eq!(config.search.result_limit, Some(300));
    assert_eq!(config.search.recent_limit, Some(12));
    assert_eq!(config.appearance.visible_rows, Some(8));
    // Legacy slots are cleared after migration
    assert_eq!(config.appearance.max_results, None);
    assert_eq!(config.appearance.top_n_history, None);
    assert_eq!(config.appearance.max_history_display, None);
}

#[test]
fn migrate_prefers_search_legacy_over_appearance_legacy() {
    // #388 改名後: [search].top_n_history（中間 legacy）と [appearance].top_n_history（最古 legacy）の
    // 両方に値があるケース。両 legacy を result_limit へ集約し、中間（search）を優先する。
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            max_results = 8
            window_width = 600
            top_n_history = 50
            max_history_display = 3

            [search]
            top_n_history = 400
            max_history_display = 15

            [paths]
            additional = []
        "#;
    let mut config: Config = toml::from_str(toml_str).expect("parse");
    assert!(config.apply_migrations());
    // 中間 legacy（search）の値が result_limit/recent_limit へ集約され、最古（appearance）に勝つ
    assert_eq!(config.search.result_limit, Some(400));
    assert_eq!(config.search.recent_limit, Some(15));
    // Legacy slots は全層クリーンアップ済み
    assert_eq!(config.search.top_n_history, None);
    assert_eq!(config.search.max_history_display, None);
    assert_eq!(config.appearance.top_n_history, None);
    assert_eq!(config.appearance.max_history_display, None);
}

#[test]
fn apply_migrations_always_resolves_none_to_some() {
    // [search] セクションも [appearance] legacy もない TOML では、
    // apply_migrations() 後に None → Some(default) が補完されることを確認する。
    // これにより設定画面の DragValue::get_or_insert が常に no-op になり、
    // has_changes() の誤発火（draft = Some vs saved = None）を防ぐ。
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            max_results = 8
            window_width = 600

            [paths]
            additional = []
        "#;
    let mut config: Config = toml::from_str(toml_str).expect("parse");
    config.apply_migrations();
    assert_eq!(config.search.result_limit, Some(200));
    assert_eq!(config.search.recent_limit, Some(8));
    assert_eq!(config.appearance.visible_rows, Some(8));
}

#[test]
fn normalized_default_resolves_all_migration_sentinels() {
    // #439: reset_to_default の apply_migrations() 手動呼び出し回避策をモデル層に閉じ込める。
    // normalized_default() は常に Some(v) を返し、legacy 二層フィールドは take() 済みで None。
    let config = Config::normalized_default();
    assert!(config.appearance.visible_rows.is_some());
    assert!(config.search.result_limit.is_some());
    assert!(config.search.recent_limit.is_some());
    assert_eq!(config.appearance.max_results, None);
    assert_eq!(config.appearance.top_n_history, None);
    assert_eq!(config.appearance.max_history_display, None);
    assert_eq!(config.search.top_n_history, None);
    assert_eq!(config.search.max_history_display, None);
}

#[test]
fn normalized_default_matches_default_plus_manual_migrations() {
    let mut expected = Config::default();
    expected.apply_migrations();
    assert_eq!(Config::normalized_default(), expected);
}

#[test]
fn normalized_default_is_idempotent_under_reapplied_migrations() {
    // モデル層で正規化済みなら、タブ遷移順序の違いを模した再適用（DragValue の
    // get_or_insert 等）で値が変化しない。変化すれば draft/saved の PartialEq が
    // 遷移順序に依存する既知のバグが再発している。
    let mut config = Config::normalized_default();
    let before = config.clone();
    let changed = config.apply_migrations();
    assert!(
        !changed,
        "normalized_default() should already be migration-stable"
    );
    assert_eq!(config, before);
}

#[test]
fn migrate_legacy_max_results_to_visible_rows() {
    // 旧キー [appearance].max_results のみ → visible_rows へ移行（#388）。
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            max_results = 15
            window_width = 600

            [paths]
            additional = []
        "#;
    let mut config: Config = toml::from_str(toml_str).expect("parse");
    assert!(config.apply_migrations());
    assert_eq!(config.appearance.effective_visible_rows(), 15);
    assert_eq!(config.appearance.max_results, None); // legacy cleared
}

#[test]
fn migrate_search_intermediate_legacy_to_result_limit() {
    // 中間 legacy [search].top_n_history のみ（appearance 最古なし）→ result_limit へ移行（#388）。
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            window_width = 600

            [search]
            top_n_history = 333
            max_history_display = 7

            [paths]
            additional = []
        "#;
    let mut config: Config = toml::from_str(toml_str).expect("parse");
    assert!(config.apply_migrations());
    assert_eq!(config.search.result_limit, Some(333));
    assert_eq!(config.search.recent_limit, Some(7));
    assert_eq!(config.search.top_n_history, None); // intermediate legacy cleared
    assert_eq!(config.search.max_history_display, None);
}

#[test]
fn migrate_new_key_wins_over_legacy() {
    // 核心不変条件: 新キー（明示）と legacy が両在 → 新キーが勝つ（get_or_insert が no-op、新優先）。
    // この回帰ガードがないと get_or_insert を `= Some(v)`（無条件上書き）に誤改変しても検知できない。
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            visible_rows = 12
            max_results = 99
            window_width = 600

            [search]
            result_limit = 250
            top_n_history = 999
            recent_limit = 6
            max_history_display = 88

            [paths]
            additional = []
        "#;
    let mut config: Config = toml::from_str(toml_str).expect("parse");
    config.apply_migrations();
    // 新キーが legacy で上書きされない
    assert_eq!(config.appearance.visible_rows, Some(12));
    assert_eq!(config.search.result_limit, Some(250));
    assert_eq!(config.search.recent_limit, Some(6));
    // legacy は全層クリア済み
    assert_eq!(config.appearance.max_results, None);
    assert_eq!(config.search.top_n_history, None);
    assert_eq!(config.search.max_history_display, None);
}

#[test]
fn new_keys_round_trip_and_legacy_not_serialized() {
    // 新キーで読み込み → serialize → 旧キーが出力されず（skip_serializing）新キーで再読み込みできる。
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            visible_rows = 20
            window_width = 600

            [search]
            result_limit = 250
            recent_limit = 6

            [paths]
            additional = []
        "#;
    let mut config: Config = toml::from_str(toml_str).expect("parse");
    config.apply_migrations();
    assert_eq!(config.appearance.effective_visible_rows(), 20);
    assert_eq!(config.search.effective_result_limit(), 250);
    assert_eq!(config.search.effective_recent_limit(), 6);

    let serialized = toml::to_string(&config).expect("serialize");
    // 旧キーは skip_serializing で出力されない
    assert!(
        !serialized.contains("max_results"),
        "old key leaked: {serialized}"
    );
    assert!(
        !serialized.contains("top_n_history"),
        "old key leaked: {serialized}"
    );
    assert!(
        !serialized.contains("max_history_display"),
        "old key leaked: {serialized}"
    );
    // 新キーは出力される
    assert!(serialized.contains("visible_rows"));
    assert!(serialized.contains("result_limit"));
    assert!(serialized.contains("recent_limit"));

    // 再読み込みで同値
    let reloaded: Config = toml::from_str(&serialized).expect("reparse");
    assert_eq!(reloaded.appearance.effective_visible_rows(), 20);
    assert_eq!(reloaded.search.effective_result_limit(), 250);
    assert_eq!(reloaded.search.effective_recent_limit(), 6);
}

#[test]
fn all_legacy_keys_behavior_unchanged() {
    // 全旧キーを持つ config が改名前と同じ実効値・icon_cache_cap に解決する（挙動不変）。
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            max_results = 8
            window_width = 600
            top_n_history = 200
            max_history_display = 8

            [paths]
            additional = []
        "#;
    let mut config: Config = toml::from_str(toml_str).expect("parse");
    config.apply_migrations();
    assert_eq!(config.appearance.effective_visible_rows(), 8);
    assert_eq!(config.search.effective_result_limit(), 200);
    assert_eq!(config.search.effective_recent_limit(), 8);
    assert_eq!(config.icon_cache_cap(), 1000); // 改名前と同値
}

#[test]
fn migrate_additional_to_scan_converts_paths() {
    let mut config = Config::default();
    config.paths.additional = vec!["C:\\Tools".to_string(), "D:\\Apps".to_string()];
    config.paths.scan.clear();

    config.migrate_additional_to_scan();

    assert!(config.paths.additional.is_empty());
    assert_eq!(config.paths.scan.len(), 2);
    assert_eq!(config.paths.scan[0].path, "C:\\Tools");
    assert_eq!(config.paths.scan[0].extensions, vec![".lnk"]);
    assert!(!config.paths.scan[0].include_folders);
    assert_eq!(config.paths.scan[1].path, "D:\\Apps");
}

#[test]
fn migrate_additional_to_scan_merges_lnk_into_existing() {
    let mut config = Config::default();
    config.paths.scan = vec![ScanPath {
        path: "C:\\Tools".to_string(),
        extensions: vec![".exe".to_string()],
        include_folders: false,
    }];
    config.paths.additional = vec!["C:\\Tools".to_string(), "D:\\New".to_string()];

    config.migrate_additional_to_scan();

    assert!(config.paths.additional.is_empty());
    assert_eq!(config.paths.scan.len(), 2);
    // .lnk merged into existing scan entry
    assert_eq!(config.paths.scan[0].extensions, vec![".exe", ".lnk"]);
    // New path added separately
    assert_eq!(config.paths.scan[1].path, "D:\\New");
    assert_eq!(config.paths.scan[1].extensions, vec![".lnk"]);
}

#[test]
fn migrate_additional_to_scan_case_insensitive_merge() {
    let mut config = Config::default();
    config.paths.scan = vec![ScanPath {
        path: "C:\\TOOLS".to_string(),
        extensions: vec![".exe".to_string()],
        include_folders: false,
    }];
    config.paths.additional = vec!["c:\\tools".to_string()];

    config.migrate_additional_to_scan();

    assert!(config.paths.additional.is_empty());
    assert_eq!(config.paths.scan.len(), 1);
    assert_eq!(config.paths.scan[0].extensions, vec![".exe", ".lnk"]);
}

#[test]
fn migrate_additional_no_duplicate_lnk_when_already_present() {
    let mut config = Config::default();
    config.paths.scan = vec![ScanPath {
        path: "C:\\Links".to_string(),
        extensions: vec![".lnk".to_string()],
        include_folders: false,
    }];
    config.paths.additional = vec!["C:\\Links".to_string()];

    config.migrate_additional_to_scan();

    assert!(config.paths.additional.is_empty());
    assert_eq!(config.paths.scan.len(), 1);
    assert_eq!(
        config.paths.scan[0].extensions,
        vec![".lnk"],
        ".lnk should not be duplicated"
    );
}

#[test]
fn migrate_additional_noop_when_empty() {
    let mut config = Config::default();
    let scan_before = config.paths.scan.clone();

    config.migrate_additional_to_scan();

    assert_eq!(config.paths.scan, scan_before);
}

#[test]
fn skip_serializing_additional() {
    let mut config = Config::default();
    config.paths.additional = vec!["C:\\Old".to_string()];
    let toml_str = toml::to_string_pretty(&config).expect("serialize");
    assert!(
        !toml_str.contains("additional"),
        "additional should not appear in serialized output"
    );
}

#[test]
fn apply_migrations_sanitizes_invalid_fuzzy_history_cap_ratio() {
    // 旧 `SearchConfig::sanitize()` の直接処理を `apply_migrations()` へ移設した
    // 補正ロジックを検証する（issue #437）。範囲外（> 1.0）の値は既定値へ補正される。
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            window_width = 600

            [search]
            fuzzy_history_cap_ratio = 1.5

            [paths]
            additional = []
        "#;
    let mut config: Config = toml::from_str(toml_str).expect("parse");
    assert!(config.apply_migrations());
    assert!((config.search.fuzzy_history_cap_ratio - 0.30).abs() < f64::EPSILON);
}

#[test]
fn apply_migrations_leaves_valid_fuzzy_history_cap_ratio_unchanged() {
    // 有効範囲内の値は補正されず、apply_migrations() の changed フラグにも寄与しない
    // （他の移行項目が無い最小 TOML では false を返す）ことを確認する。
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            window_width = 600
            visible_rows = 10

            [search]
            fuzzy_history_cap_ratio = 0.5
            result_limit = 200
            recent_limit = 10

            [paths]
            additional = []
        "#;
    let mut config: Config = toml::from_str(toml_str).expect("parse");
    assert!(!config.apply_migrations());
    assert!((config.search.fuzzy_history_cap_ratio - 0.5).abs() < f64::EPSILON);
}

// opener ロジック本体のテストは `opener.rs` に移設済み（issue #435）。以下 3 件は
// `Config::normalize_openers()`（config.rs 側メソッド）のテストのため、この test mod
// ローカルの make_rule ヘルパーを使う（opener.rs 側テストの重複だがテスト専用の小ヘルパーであり
// モジュール間の pub(crate) 露出を避けるため意図的に複製する）。
fn make_rule(target: &str, tools: &[(&str, &str, &str)]) -> OpenerRule {
    OpenerRule {
        target: target.to_string(),
        tools: tools
            .iter()
            .map(|(name, exe, args)| OpenerTool {
                name: name.to_string(),
                exe: exe.to_string(),
                args: args.to_string(),
            })
            .collect(),
    }
}

#[test]
fn normalize_openers_returns_true_when_changed() {
    let mut config = Config {
        openers: vec![make_rule("ext:png,jpg", &[("Viewer", "viewer.exe", "")])],
        ..Default::default()
    };

    assert!(config.normalize_openers());
    assert_eq!(config.openers[0].target, "ext:.jpg,.png");
}

#[test]
fn normalize_openers_returns_false_when_no_change() {
    let mut config = Config {
        openers: vec![make_rule("ext:.jpg,.png", &[("Viewer", "viewer.exe", "")])],
        ..Default::default()
    };

    assert!(!config.normalize_openers());
}

#[test]
fn normalize_openers_returns_false_when_multiple_already_sorted() {
    let mut config = Config {
        openers: vec![
            make_rule("folder:c:\\workspace", &[("Terminal", "wt.exe", "")]),
            make_rule("folder", &[("Explorer", "explorer.exe", "")]),
            make_rule("ext:.md", &[("Typora", "typora.exe", "")]),
        ],
        ..Default::default()
    };
    assert!(!config.normalize_openers());
}

#[test]
fn valid_hotkey_aliases_are_not_rewritten_and_migration_is_idempotent() {
    let mut config = Config::normalized_default();
    config.hotkey.modifier = " Control++Control ".to_string();
    config.hotkey.key = "return".to_string();

    assert!(!config.apply_migrations());
    assert_eq!(config.hotkey.modifier, " Control++Control ");
    assert_eq!(config.hotkey.key, "return");
    assert!(!config.apply_migrations());
}

#[test]
fn apply_migrations_normalizes_additional() {
    let mut config = Config::default();
    #[allow(deprecated)]
    config.paths.additional.push("C:\\Legacy".to_string());
    assert!(config.apply_migrations());
    assert!(config.paths.additional.is_empty());
    assert!(!config.paths.scan.is_empty());
}

/// `apply_migrations` の並びのうち**唯一の真の順序依存**を pin する——(1) `additional` → `scan`
/// の追加は、(5) `scan` の正規化・dedup より前に来なければならない。
///
/// 入れ替えると `C:/Tools/` が正規化されないまま push され、`migrate_additional_to_scan` の
/// 照合は生の小文字比較なので既存の `C:\Tools` と一致せず、重複が残る。**この形でしか落ちない**
/// ——`migrate_additional_to_scan_*` の 4 本は private fn を直接呼ぶので並びを通らず、並びを
/// 通る `apply_migrations_normalizes_additional` は件数を見ないため入れ替えても真のままである。
#[test]
fn legacy_additional_moves_into_scan_before_scan_paths_are_normalized() {
    let mut config = Config::default();
    config.paths.scan = vec![ScanPath {
        path: "C:\\Tools".to_string(),
        extensions: vec![".lnk".to_string()],
        include_folders: false,
    }];
    // 同じディレクトリを、正規化しないと一致しない綴りで legacy 側へ置く。
    config.paths.additional.push("C:/Tools/".to_string());

    assert!(config.apply_migrations());

    assert_eq!(
        config.paths.scan.len(),
        1,
        "legacy エントリが正規化より後に push され、重複が残った"
    );
}

#[test] // T15 + T17: legacy → Url 移行（自動分割しない）・冪等
fn instant_legacy_migrates_to_url_idempotently() {
    let mut cfg = Config {
        instant_commands: vec![InstantCommand {
            name: "ev".into(),
            description: String::new(),
            action: InstantAction::Legacy {
                command: "C:\\tools\\editor.exe".into(),
            },
        }],
        ..Default::default()
    };
    assert!(cfg.apply_migrations());
    assert_eq!(
        cfg.instant_commands[0].action,
        InstantAction::Url {
            url: "C:\\tools\\editor.exe".into()
        }
    ); // Exec にしない
    // 冪等: 2回目は Legacy が残っていないので action は Url のまま
    cfg.apply_migrations();
    assert_eq!(
        cfg.instant_commands[0].action,
        InstantAction::Url {
            url: "C:\\tools\\editor.exe".into()
        }
    );
}

#[test]
fn dedup_instant_commands_first_wins_and_keeps_order() {
    // #638: 重複名は先勝ち（実行時 first-match と同じ行が残る＝挙動変化なし）。
    let mut config = Config::normalized_default();
    config.instant_commands = vec![
        InstantCommand {
            name: "gh".into(),
            description: String::new(),
            action: InstantAction::Url {
                url: "https://github.com/{q}".into(),
            },
        },
        InstantCommand {
            name: "gh".into(),
            description: String::new(),
            action: InstantAction::Url {
                url: "https://example.com/{q}".into(),
            },
        },
        InstantCommand {
            name: "g".into(),
            description: String::new(),
            action: InstantAction::Url {
                url: "https://google.com/{q}".into(),
            },
        },
    ];
    config.apply_migrations();
    assert_eq!(config.instant_commands.len(), 2);
    assert_eq!(config.instant_commands[0].name, "gh");
    assert!(
        matches!(&config.instant_commands[0].action, InstantAction::Url { url } if url.contains("github")),
        "先頭定義（github）が残る"
    );
    assert_eq!(config.instant_commands[1].name, "g");
}

#[test]
fn dedup_instant_commands_does_not_flag_changed() {
    // 決定 2: dedup は changed に寄与しない（true だと load が config.toml へ書き戻し、
    // ユーザーの手編集行を消す・config.rs load_from_dir_reporting）。
    let mut config = Config::normalized_default(); // migration 済み＝以後の changed は新規要因のみ
    config.instant_commands = vec![
        InstantCommand {
            name: "gh".into(),
            description: String::new(),
            action: InstantAction::Url {
                url: "https://github.com/{q}".into(),
            },
        },
        InstantCommand {
            name: "gh".into(),
            description: String::new(),
            action: InstantAction::Url {
                url: "https://example.com/{q}".into(),
            },
        },
    ];
    assert!(!config.apply_migrations());
    assert_eq!(config.instant_commands.len(), 1);
}
