//! 読み書きの統合経路——`.bak` 退避・読み込み失敗の扱い分け・TOML 文字列との相互変換。

use super::*;
use crate::config::{AppearanceConfig, HotkeyConfig, PathsConfig};

#[test]
fn invalid_toml_falls_back_to_default() {
    // Garbage text that isn't valid TOML → should parse as default
    let config: Config = toml::from_str("{{{{not valid toml!!!!").unwrap_or_default();
    let default = Config::default();
    assert_eq!(config.hotkey.modifier, default.hotkey.modifier);
    assert_eq!(config.hotkey.key, default.hotkey.key);
    assert_eq!(
        config.appearance.effective_visible_rows(),
        default.appearance.effective_visible_rows()
    );
}

#[test]
fn valid_toml_invalid_values_caught_by_validate() {
    // Config with all required sections but invalid field values
    let toml_str = r#"
            [hotkey]
            modifier = ""
            key = ""

            [appearance]
            visible_rows = 0
            window_width = 50

            [paths]
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    let errors = config.validate();
    // Should have errors for empty hotkey and invalid visible_rows/window_width
    assert!(
        errors.len() >= 2,
        "Expected at least 2 errors, got: {:?}",
        errors
    );
}

/// #824 で契約が反転した——欠落セクションは parse 失敗ではなく既定補完になる。
/// **書かれた値は既定で上書きされない**ことも同時に固定する。
#[test]
fn partial_toml_fills_missing_sections_with_defaults() {
    let toml_str = r#"
            [hotkey]
            modifier = "Ctrl"
            key = "Space"
        "#;
    let config: Config = toml::from_str(toml_str).expect("欠落セクションは既定で埋まる");
    // 書かれた値は保たれる
    assert_eq!(config.hotkey.modifier, "Ctrl");
    assert_eq!(config.hotkey.key, "Space");
    // 欠落セクションは対応する `Default` へ落ちる
    assert_eq!(config.appearance, AppearanceConfig::default());
    assert_eq!(config.paths, PathsConfig::default());
}

// -- backup_invalid: parse 失敗時の .bak 退避（issue #338） --

/// テスト用の作業ディレクトリを作り直して返す。
///
/// 名前に `std::process::id()` を含める理由は `indexer.rs` の `temp_dir` の doc を正本とする（#978 / #985）。
fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("snotra_config_test_{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn temp_dir_name_contains_process_id() {
    let dir = temp_dir("process_unique");
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .expect("temp dir name");
    assert_eq!(
        name,
        format!("snotra_config_test_process_unique-{}", std::process::id()),
        "作業ディレクトリ名に自プロセスの pid が入っていない（#985）"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn backup_invalid_renames_to_bak_preserving_content() {
    let dir = temp_dir("backup_rename");
    let path = dir.join("config.toml");
    let bad = "{{{{not valid toml!!!!";
    fs::write(&path, bad).unwrap();

    Config::backup_invalid(&path);

    let bak = path.with_extension("toml.bak");
    // 元ファイルは退避され存在しない（= default で上書きされ得ない）
    assert!(
        !path.exists(),
        "config.toml must be moved away on parse failure (not left for default overwrite)"
    );
    // .bak が元の不正内容を保全している（手動復旧可能）
    assert_eq!(
        fs::read_to_string(&bak).unwrap(),
        bad,
        ".bak must preserve the original (unparseable) content"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn backup_invalid_overwrites_existing_bak() {
    let dir = temp_dir("backup_overwrite");
    let path = dir.join("config.toml");
    let bak = path.with_extension("toml.bak");
    fs::write(&bak, "OLD BAK CONTENT").unwrap();
    let newer_bad = "also = invalid = toml";
    fs::write(&path, newer_bad).unwrap();

    Config::backup_invalid(&path);

    // 単一 .bak を最新の不正内容で上書きする（KISS）
    assert_eq!(fs::read_to_string(&bak).unwrap(), newer_bad);
    assert!(!path.exists());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn backup_invalid_missing_source_is_noop_no_panic() {
    let dir = temp_dir("backup_missing");
    let path = dir.join("config.toml"); // 作成しない
    // rename は Err になるが panic せず、.bak も作られない
    Config::backup_invalid(&path);
    let bak = path.with_extension("toml.bak");
    assert!(
        !bak.exists(),
        "no .bak should be created when source is absent"
    );

    let _ = fs::remove_dir_all(&dir);
}

// -- load_from_dir: load() 統合経路（config_dir 注入、issue #338） --

#[test]
fn load_from_dir_parse_failure_backs_up_and_does_not_save() {
    let dir = temp_dir("load_parse_fail");
    let path = dir.join("config.toml");
    let bad = "{{{ not valid toml";
    fs::write(&path, bad).unwrap();

    let (config, outcome) = Config::load_from_dir_reporting(&dir);

    assert_eq!(outcome, LoadOutcome::RecoveredFromCorrupt);
    // default 値で起動する
    assert_eq!(config.hotkey.modifier, Config::default().hotkey.modifier);
    // parse 失敗時は default で再保存しない（config.toml は .bak へ退避され不在）
    assert!(
        !path.exists(),
        "config.toml must NOT be recreated/overwritten on parse failure"
    );
    let bak = path.with_extension("toml.bak");
    assert_eq!(
        fs::read_to_string(&bak).unwrap(),
        bad,
        ".bak must hold the original broken content"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn load_from_dir_missing_file_is_first_run_and_saves_default() {
    let dir = temp_dir("load_missing");
    let path = dir.join("config.toml"); // 作らない（NotFound = first-run）

    let (config, outcome) = Config::load_from_dir_reporting(&dir);

    assert_eq!(outcome, LoadOutcome::FirstRun);
    assert_eq!(config.hotkey.modifier, Config::default().hotkey.modifier);
    // first-run は default を保存する
    assert!(path.exists(), "first-run must create config.toml");
    let reparsed: Config = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(reparsed.hotkey.modifier, config.hotkey.modifier);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn load_from_dir_valid_config_is_parsed() {
    let dir = temp_dir("load_valid");
    let path = dir.join("config.toml");
    let mut written = Config::default();
    written.appearance.window_width = 777;
    fs::write(&path, toml::to_string_pretty(&written).unwrap()).unwrap();

    let (loaded, outcome) = Config::load_from_dir_reporting(&dir);

    assert_eq!(outcome, LoadOutcome::Loaded);
    assert_eq!(loaded.appearance.window_width, 777);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn load_from_dir_repairs_and_saves_invalid_hotkey() {
    for (case, modifier, key) in [
        ("unknown_modifier", "Hyper", "Q"),
        ("unsupported_key", "Alt", "!"),
        ("semantic_conflict", "Alt+Alt", "F4"),
    ] {
        let dir = temp_dir(case);
        let path = dir.join("config.toml");
        let mut written = Config::default();
        written.hotkey.modifier = modifier.to_string();
        written.hotkey.key = key.to_string();
        fs::write(&path, toml::to_string_pretty(&written).unwrap()).unwrap();

        let (loaded, outcome) = Config::load_from_dir_reporting(&dir);
        assert_eq!(outcome, LoadOutcome::Loaded, "case={case}");
        assert_eq!(loaded.hotkey, HotkeyConfig::default(), "case={case}");
        let saved: Config = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved.hotkey, HotkeyConfig::default(), "case={case}");

        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn load_from_dir_invalid_utf8_is_backed_up() {
    let dir = temp_dir("load_invalid_utf8");
    let path = dir.join("config.toml");
    // 不正な UTF-8 → read_to_string が InvalidData で失敗する。
    // 壊れた永続データなので parse 失敗と同じく .bak へ byte-preserving 退避する。
    let invalid_utf8: &[u8] = &[0xFF, 0xFE, 0x00, 0x80];
    fs::write(&path, invalid_utf8).unwrap();

    let (config, outcome) = Config::load_from_dir_reporting(&dir);

    assert_eq!(outcome, LoadOutcome::RecoveredFromCorrupt);
    assert_eq!(config.hotkey.modifier, Config::default().hotkey.modifier);
    // canonical path には残さず（後続 save() で破損元を失わないため）.bak へ退避
    assert!(
        !path.exists(),
        "corrupt (non-UTF-8) config.toml must be moved to .bak, not left at canonical path"
    );
    let bak = path.with_extension("toml.bak");
    assert_eq!(
        fs::read(&bak).unwrap(),
        invalid_utf8,
        ".bak must byte-preserve the corrupt content"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// #824: 構文は正しいがセクションを欠く config.toml は「破損」ではない——既定で埋めて
/// `Loaded` を返し、`.bak` を作らない（`SPEC.md`「13.1 設定データ」）。1 キーの打ち間違いで
/// 設定ファイル全体が退避される経路を塞ぐ、この変更の直接の回帰テストである。
#[test]
fn load_from_dir_missing_section_is_loaded_not_recovered() {
    let dir = temp_dir("load_missing_section");
    let path = dir.join("config.toml");
    fs::write(&path, "[hotkey]\nmodifier = \"Ctrl\"\nkey = \"Space\"\n").unwrap();

    let (config, outcome) = Config::load_from_dir_reporting(&dir);

    assert_eq!(outcome, LoadOutcome::Loaded);
    assert_eq!(config.hotkey.modifier, "Ctrl");
    assert_eq!(config.appearance.window_width, 600);
    assert!(
        !path.with_extension("toml.bak").exists(),
        "構文の正しい config を .bak へ退避してはならない"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn load_from_dir_transient_read_error_leaves_file_intact() {
    let dir = temp_dir("load_transient");
    let path = dir.join("config.toml");
    // config.toml をディレクトリにして read_to_string を NotFound/InvalidData 以外で
    // 失敗させる（permission/lock 等の一時的失敗の代理）。読めないファイルは安全に
    // 退避できないため、退避も上書きもせず据え置く。
    fs::create_dir(&path).unwrap();

    let (config, outcome) = Config::load_from_dir_reporting(&dir);

    assert_eq!(outcome, LoadOutcome::ReadFailed);
    assert_eq!(config.hotkey.modifier, Config::default().hotkey.modifier);
    assert!(
        path.is_dir(),
        "transient read error must NOT move or overwrite the existing path"
    );
    let bak = path.with_extension("toml.bak");
    assert!(
        !bak.exists(),
        "transient read error must NOT create a .bak (the file may be intact, just unreadable)"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn from_toml_str_fills_defaults() {
    // 全セクションが `#[serde(default)]` を持つ（#824）。ここでは書いたセクションの値が
    // 保たれ、書かなかった openers / instant_commands が既定になることを見る。
    let toml = r#"
[hotkey]
modifier = "Ctrl"
key = "Space"
[appearance]
max_results = 10
window_width = 700
[paths]
scan = []
"#;
    let config = Config::from_toml_str(toml).expect("parse");
    assert_eq!(config.hotkey.modifier, "Ctrl");
    assert_eq!(config.hotkey.key, "Space");
    // 旧キー max_results は legacy slot に入る（from_toml_str は migration を実行しない）
    assert_eq!(config.appearance.max_results, Some(10));
    // Defaults filled for missing optional sections
    assert!(config.openers.is_empty());
    assert!(config.instant_commands.is_empty());
}

#[test]
fn from_toml_str_rejects_invalid() {
    let result = Config::from_toml_str("this is not valid toml {{{}}}");
    assert!(result.is_err());
}

#[test]
fn from_toml_str_fills_missing_sections() {
    // #824: `[appearance]` と `[paths]` が無くても既定で埋まる（インポート経路も同じ）
    let config = Config::from_toml_str("[hotkey]\nmodifier = \"Alt\"\nkey = \"Q\"\n")
        .expect("欠落セクションは既定で埋まる");
    assert_eq!(config.appearance, AppearanceConfig::default());
    assert_eq!(config.paths, PathsConfig::default());
}

#[test]
fn from_toml_str_ignores_unknown_keys() {
    let toml = r#"
[hotkey]
modifier = "Alt"
key = "Q"
[appearance]
max_results = 8
window_width = 600
[paths]
scan = []
unknown_field = "hello"
[unknown_section]
foo = 42
"#;
    let config = Config::from_toml_str(toml).expect("parse");
    assert_eq!(config.hotkey.key, "Q");
}

#[test]
fn export_filename_format() {
    let name = Config::export_filename(2026, 3, 11, 14, 30);
    assert_eq!(name, "config_202603111430.toml");
}

#[test]
fn export_filename_zero_pads() {
    let name = Config::export_filename(2026, 1, 5, 9, 3);
    assert_eq!(name, "config_202601050903.toml");
}

#[test]
fn dedup_load_does_not_rewrite_config_file() {
    // 決定 2 の直接検証: 重複入り TOML を load してもメモリ上のみ dedup され、
    // config.toml のバイト列は不変(ユーザーの手編集行を消さない)。
    let dir = temp_dir("dedup");
    let path = dir.join("config.toml");
    let toml_str = r#"
[hotkey]
modifier = "Alt"
key = "Q"

[appearance]
window_width = 600
visible_rows = 10

[paths]
additional = []

[[instant_commands]]
name = "gh"
url = "https://github.com/{q}"

[[instant_commands]]
name = "gh"
url = "https://example.com/{q}"
"#;
    std::fs::write(&path, toml_str).unwrap();
    let (config, _) = Config::load_from_dir_reporting(&dir);
    assert_eq!(config.instant_commands.len(), 1);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        toml_str,
        "load が config.toml を書き戻していない"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
