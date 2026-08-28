//! `Config` 全体の serde——欠損キー・欠損セクションの既定補完と、TOML の往復。

use super::*;

/// #646 PR1: 新キー欠落の旧 config は serde default(6/28)で読める(後方互換・移行不要)。
#[test]
fn visual_padding_defaults_for_missing_keys() {
    let toml = r#"
[hotkey]
modifier = "alt"
key = "q"
[appearance]
window_width = 600
[paths]
"#;
    let config: Config = toml::from_str(toml).expect("parse");
    assert_eq!(config.visual.row_padding, 6);
    assert_eq!(config.visual.bar_padding, 28);
    assert_eq!(VisualConfig::default().row_padding, 6);
    assert_eq!(VisualConfig::default().bar_padding, 28);
    assert_eq!(config.visual.window_gap, 4);
    assert_eq!(VisualConfig::default().window_gap, 4);
}

/// #646 PR1/PR2: `[visual]` セクション自体はあるが新キー(row_padding/bar_padding/
/// window_gap)が無い旧 config は、フィールド単位の serde default で読める。
/// `visual_padding_defaults_for_missing_keys` は `[visual]` セクションごと欠落する
/// ケースを検証しており、`Config.visual` の struct 級 `#[serde(default)]` 経由で
/// `VisualConfig::default()` に落ちるため、フィールド単位の `#[serde(default = "...")]`
/// 属性を検証していない(属性を外してもそちらのテストは通ってしまう)。
#[test]
fn visual_field_defaults_apply_when_section_present() {
    let toml = r#"
[hotkey]
modifier = "alt"
key = "q"
[appearance]
window_width = 600
[visual]
background_color = '#123456'
[paths]
"#;
    let config: Config = toml::from_str(toml).expect("parse");
    // 記載した既存キーが反映されている(struct 級 default に落ちていない証拠)。
    assert_eq!(config.visual.background_color, "#123456");
    // 記載していない新キーはフィールド単位の default が効く。
    assert_eq!(config.visual.row_padding, 6);
    assert_eq!(config.visual.bar_padding, 28);
    assert_eq!(config.visual.window_gap, 4);
}

/// #824: フィールド単位の `#[serde(default = "…")]` を測るテストは、**セクションを書き・
/// 対象キーだけを省き・既存キーを sentinel として置く**。上の
/// `visual_field_defaults_apply_when_section_present` と同じ理由で、セクションごと省くと
/// 親の struct 級 `#[serde(default)]` が丸ごと既定へ落とし、属性を外しても通ってしまう。
#[test]
fn appearance_window_width_default_applies_when_key_missing() {
    let toml = r#"
[hotkey]
modifier = "alt"
key = "q"
[appearance]
show_icons = false
[paths]
"#;
    let config: Config = toml::from_str(toml).expect("parse");
    // sentinel: 記載した既存キーが反映されている（struct 級 default に落ちていない証拠）。
    assert!(!config.appearance.show_icons);
    assert_eq!(config.appearance.window_width, 600);
}

#[test]
fn hotkey_key_default_applies_when_key_missing() {
    let toml = r#"
[hotkey]
modifier = "Ctrl"
[appearance]
[paths]
"#;
    let config: Config = toml::from_str(toml).expect("parse");
    assert_eq!(config.hotkey.modifier, "Ctrl"); // sentinel
    assert_eq!(config.hotkey.key, "Q");
}

#[test]
fn custom_theme_field_default_applies_when_key_missing() {
    let toml = r#"
[hotkey]
modifier = "alt"
key = "q"
[appearance]
[paths]
[visual.custom_theme]
background_color = '#123456'
"#;
    let config: Config = toml::from_str(toml).expect("parse");
    // `custom_theme` は `Option` なので、`None` のまま素通りしないことを先に確かめる。
    let theme = config
        .visual
        .custom_theme
        .expect("[visual.custom_theme] を書いたので Some である");
    assert_eq!(theme.background_color, "#123456"); // sentinel
    assert_eq!(theme.input_background_color, "#383838");
    assert_eq!(theme.text_color, "#E0E0E0");
    assert_eq!(theme.selected_row_color, "#505050");
    assert_eq!(theme.hint_text_color, "#808080");
}

/// 変わらないことの pin——`scan` は以前から `#[serde(default)]` を持つ。#824 で
/// `[paths]` セクションごとの欠落を既定へ落とすとき、**キー欠落側の値を動かさない**
/// （既に受理している入力の解釈を変えない）ことを固定する。
#[test]
fn paths_section_without_scan_key_stays_empty() {
    let toml = r#"
[hotkey]
modifier = "alt"
key = "q"
[appearance]
[paths]
additional = ['C:\Tools']
"#;
    let config: Config = toml::from_str(toml).expect("parse");
    assert_eq!(config.paths.additional, vec!["C:\\Tools".to_string()]); // sentinel
    assert!(config.paths.scan.is_empty());
}

#[test]
fn deserialize_full_config() {
    let toml_str = r#"
            [hotkey]
            modifier = "Ctrl"
            key = "Space"

            [appearance]
            max_results = 10
            window_width = 700
            top_n_history = 150
            max_history_display = 5

            [paths]
            additional = ["C:\\Tools"]

            [search]
            normal_mode = "prefix"
            folder_mode = "substring"
            show_hidden_system = true
            history_normalization = "fuzzy_relative_cap"
            fuzzy_history_cap_ratio = 0.25
        "#;
    // migration 適用前の raw デシリアライズ値を検証する。
    // 旧キー max_results / top_n_history / max_history_display は legacy slot（Some(v)）に入る。
    // SearchConfig / visible_rows への migration は migrate_oldest_appearance_legacy_to_new_keys で検証。
    let config: Config = toml::from_str(toml_str).expect("parse");
    assert_eq!(config.hotkey.modifier, "Ctrl");
    assert_eq!(config.hotkey.key, "Space");
    assert_eq!(config.appearance.max_results, Some(10));
    assert_eq!(config.appearance.window_width, 700);
    assert_eq!(config.appearance.top_n_history, Some(150));
    assert_eq!(config.appearance.max_history_display, Some(5));
    assert_eq!(config.paths.additional, vec!["C:\\Tools"]);
    assert_eq!(config.search.normal_mode, SearchModeConfig::Prefix);
    assert_eq!(config.search.folder_mode, SearchModeConfig::Substring);
    assert!(config.search.show_hidden_system);
    assert_eq!(
        config.search.history_normalization,
        SearchHistoryNormalizationConfig::FuzzyRelativeCap
    );
    assert!((config.search.fuzzy_history_cap_ratio - 0.25).abs() < f64::EPSILON);
    assert!(config.general.hotkey_toggle);
    assert!(!config.general.show_on_startup);
    assert!(config.general.auto_hide_on_focus_lost);
    assert!(config.general.show_tray_icon);
    assert!(!config.general.ime_off_on_show);
    assert_eq!(config.visual.preset, ThemePreset::Obsidian);
    assert_eq!(config.visual.background_color, "#282828");
    assert_eq!(config.visual.font_family, "Segoe UI");
    assert_eq!(config.visual.font_size, 15);
}

#[test]
fn deserialize_minimal_config_uses_defaults() {
    // Verify that omitting the `[search]` section entirely still yields correct defaults.
    // Each field uses `#[serde(default = "default_...")]`, so serde fills in the
    // function-level defaults (e.g. `default_result_limit` → 200) without needing
    // the section to be present in the TOML.
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
    let config: Config = toml::from_str(toml_str).expect("parse");
    // [search] セクションが省略されると None になり、accessor がデフォルト値を返す
    assert_eq!(config.search.effective_result_limit(), 200);
    assert_eq!(config.search.effective_recent_limit(), 8);
    assert_eq!(config.search.normal_mode, SearchModeConfig::Fuzzy);
    assert_eq!(config.search.folder_mode, SearchModeConfig::Fuzzy);
    assert!(!config.search.show_hidden_system);
    assert_eq!(
        config.search.history_normalization,
        SearchHistoryNormalizationConfig::Disabled
    );
    assert!((config.search.fuzzy_history_cap_ratio - 0.30).abs() < f64::EPSILON);
    assert!(config.general.hotkey_toggle);
    assert!(!config.general.show_on_startup);
    assert!(config.general.auto_hide_on_focus_lost);
    assert!(config.general.show_tray_icon);
    assert!(!config.general.ime_off_on_show);
    assert_eq!(config.visual.preset, ThemePreset::Obsidian);
    assert_eq!(config.visual.background_color, "#282828");
}

/// serde は既定を **2 経路**で解決する——「セクションごと欠落 → `Section::default()`」と
/// 「キーだけ欠落 → `#[serde(default = "default_X")]`」。この 2 つが食い違うと、**同じ既定が
/// TOML の書き方によって変わる**（#795）。空セクションのデシリアライズ結果と `Default` 実装を
/// 突き合わせて、フィールド単位でその乖離クラスを塞ぐ。
///
/// `deserialize_minimal_config_uses_defaults` はセクションを**丸ごと省略**する形なので
/// キー欠落の経路を通らず、この乖離は見えない。
///
/// 各 struct について「空セクションのデシリアライズ結果 == `Default` 実装」を固定し、
/// 必須フィールドの混入をその struct の範囲で捕まえる（#824）。**新しい struct を
/// `Config` へ足したときにこの群へ 1 本足すことは機構では強制されない**——`Config` 全体を
/// 見るのは `config_parses_with_all_sections_omitted` の側である。
#[test]
fn empty_section_deserializes_to_default_general() {
    let parsed: GeneralConfig = toml::from_str("").expect("空の [general] は既定で埋まる");
    assert_eq!(parsed, GeneralConfig::default());
}

#[test]
fn empty_section_deserializes_to_default_search() {
    let parsed: SearchConfig = toml::from_str("").expect("空の [search] は既定で埋まる");
    assert_eq!(parsed, SearchConfig::default());
}

#[test]
fn empty_section_deserializes_to_default_visual() {
    let parsed: VisualConfig = toml::from_str("").expect("空の [visual] は既定で埋まる");
    assert_eq!(parsed, VisualConfig::default());
}

#[test]
fn empty_section_deserializes_to_default_appearance() {
    let parsed: AppearanceConfig = toml::from_str("").expect("空の [appearance] は既定で埋まる");
    assert_eq!(parsed, AppearanceConfig::default());
}

#[test]
fn empty_section_deserializes_to_default_hotkey() {
    let parsed: HotkeyConfig = toml::from_str("").expect("空の [hotkey] は既定で埋まる");
    assert_eq!(parsed, HotkeyConfig::default());
}

/// 空 `Vec` リテラルではなく `PathsConfig::default()` と比較する——`derive(Default)` と
/// parse 経路を互いに固定し、既定の探索パス（`Config::default()` 専用のシード）が
/// どちらかへ紛れ込んだら落ちるようにする。
#[test]
fn empty_section_deserializes_to_default_paths() {
    let parsed: PathsConfig = toml::from_str("").expect("空の [paths] は既定で埋まる");
    assert_eq!(parsed, PathsConfig::default());
}

#[test]
fn empty_section_deserializes_to_default_custom_theme() {
    let parsed: CustomTheme =
        toml::from_str("").expect("空の [visual.custom_theme] は既定で埋まる");
    assert_eq!(parsed, CustomTheme::default());
}

/// TOML 空文字列が `Config` として parse できること（#824）。**`Config::default()` との
/// 全体比較はしない**——`paths.scan` は意図的に食い違い（`Config::default()` は既定の
/// 探索パスを撒くシード、parse 経路は空）、`general.language` は OS ロケール依存である。
#[test]
fn config_parses_with_all_sections_omitted() {
    let config: Config = toml::from_str("").expect("全セクション欠落でも既定で埋まる");
    assert_eq!(config.hotkey, HotkeyConfig::default());
    assert_eq!(config.appearance, AppearanceConfig::default());
    assert_eq!(config.general, GeneralConfig::default());
    assert_eq!(config.visual, VisualConfig::default());
    assert_eq!(config.search, SearchConfig::default());
    assert_eq!(config.paths, PathsConfig::default());
    assert!(
        config.paths.scan.is_empty(),
        "parse 経路の既定に探索パスのシードを混ぜない（シードは Config::default() だけ）"
    );
    // `instant_commands` も `Config::default()`（g / gh の 2 件）と食い違う。これは
    // #824 より前からある非対称で、3 セクションを書いた最小 config では今日も空になる
    // ——parse 経路の既定は `Vec::new()` のままにする（シードを撒くのは default だけ）。
    assert!(config.openers.is_empty());
    assert!(config.instant_commands.is_empty());
}

/// `Config::default()` だけが `scan` へシードを撒く。parse 経路の既定（空）と食い違うのは
/// 意図であり、`config_parses_with_all_sections_omitted` と対でその意図を両側から固定する
/// （片側だけだと、シードを `PathsConfig::default()` へ寄せる変更が無検知で通る）。
/// 既定の探索パスは環境依存で空にもなりうるため、件数ではなく導出元との一致で測る。
#[test]
fn config_default_seeds_scan_paths_unlike_parse_path() {
    assert_eq!(Config::default().paths.scan, Config::default_scan_paths());
    assert!(PathsConfig::default().scan.is_empty());
}

/// 後方互換の証明はこの向き——**今日受理されている完全形**を新コードで読み、全値が
/// そのまま残ること（`snotra-core/CLAUDE.md`「データ永続化の注意」）。#824 の変更は
/// 受理集合を広げるだけで、既に parse できる入力の解釈を動かさない。
#[test]
fn full_config_parse_is_unchanged() {
    let toml = r#"
[hotkey]
modifier = "Ctrl"
key = "Space"
[appearance]
window_width = 900
show_icons = false
[visual.custom_theme]
background_color = '#111111'
input_background_color = '#222222'
text_color = '#333333'
selected_row_color = '#444444'
hint_text_color = '#555555'
[[paths.scan]]
path = 'C:\Tools'
extensions = ['.exe']
"#;
    let config: Config = toml::from_str(toml).expect("parse");
    assert_eq!(config.hotkey.modifier, "Ctrl");
    assert_eq!(config.hotkey.key, "Space");
    assert_eq!(config.appearance.window_width, 900);
    assert!(!config.appearance.show_icons);
    let theme = config.visual.custom_theme.expect("custom_theme");
    assert_eq!(theme.background_color, "#111111");
    assert_eq!(theme.input_background_color, "#222222");
    assert_eq!(theme.text_color, "#333333");
    assert_eq!(theme.selected_row_color, "#444444");
    assert_eq!(theme.hint_text_color, "#555555");
    assert_eq!(config.paths.scan.len(), 1);
    assert_eq!(config.paths.scan[0].path, "C:\\Tools");
    assert_eq!(config.paths.scan[0].extensions, vec![".exe".to_string()]);
}

#[test]
fn default_config_has_expected_values() {
    let config = Config::default();
    assert_eq!(config.hotkey.modifier, "Alt");
    assert_eq!(config.hotkey.key, "Q");
    assert_eq!(config.appearance.effective_visible_rows(), 8);
    assert_eq!(config.appearance.window_width, 600);
    assert_eq!(config.search.effective_result_limit(), 200);
    assert_eq!(config.search.effective_recent_limit(), 8);
    assert!(config.appearance.show_icons);
    assert!(config.paths.additional.is_empty());
    // default scan paths are populated from environment (common Start Menu + Desktop)
    // so they may or may not be empty depending on the test environment
    assert_eq!(config.search.normal_mode, SearchModeConfig::Fuzzy);
    assert_eq!(config.search.folder_mode, SearchModeConfig::Fuzzy);
    assert!(!config.search.show_hidden_system);
    assert_eq!(
        config.search.history_normalization,
        SearchHistoryNormalizationConfig::Disabled
    );
    assert!((config.search.fuzzy_history_cap_ratio - 0.30).abs() < f64::EPSILON);
    assert!(config.general.hotkey_toggle);
    assert!(!config.general.show_on_startup);
    assert!(config.general.auto_hide_on_focus_lost);
    assert!(config.general.show_tray_icon);
    assert!(!config.general.ime_off_on_show);
    assert_eq!(config.visual.preset, ThemePreset::Obsidian);
    assert_eq!(config.visual.background_color, "#282828");
    assert_eq!(config.visual.input_background_color, "#383838");
    assert_eq!(config.visual.text_color, "#E0E0E0");
    assert_eq!(config.visual.selected_row_color, "#505050");
    assert_eq!(config.visual.hint_text_color, "#808080");
    assert_eq!(config.visual.font_family, "Segoe UI");
    assert_eq!(config.visual.font_size, 15);
    // default instant commands
    assert_eq!(config.instant_commands.len(), 2);
    assert_eq!(config.instant_commands[0].name, "g");
    assert_eq!(
        config.instant_commands[0].action,
        InstantAction::Url {
            url: "https://www.google.com/search?q={query}".to_string()
        }
    );
    assert_eq!(config.instant_commands[1].name, "gh");
    assert_eq!(
        config.instant_commands[1].action,
        InstantAction::Url {
            url: "https://github.com/search?q={query}".to_string()
        }
    );
}

#[test]
fn deserialize_scan_paths() {
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            max_results = 8
            window_width = 600

            [paths]
            additional = []

            [[paths.scan]]
            path = "C:\\Tools"
            extensions = [".exe", ".bat"]
            include_folders = true

            [[paths.scan]]
            path = "D:\\Docs"
            extensions = [".pdf", ".xlsx"]
        "#;
    let config: Config = toml::from_str(toml_str).expect("parse");
    assert_eq!(config.paths.scan.len(), 2);
    assert_eq!(config.paths.scan[0].path, "C:\\Tools");
    assert_eq!(config.paths.scan[0].extensions, vec![".exe", ".bat"]);
    assert!(config.paths.scan[0].include_folders);
    assert_eq!(config.paths.scan[1].path, "D:\\Docs");
    assert_eq!(config.paths.scan[1].extensions, vec![".pdf", ".xlsx"]);
    assert!(!config.paths.scan[1].include_folders);
}

#[test]
fn backward_compat_no_scan_field() {
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            max_results = 8
            window_width = 600

            [paths]
            additional = ["C:\\Old"]
        "#;
    let config: Config = toml::from_str(toml_str).expect("parse");
    assert_eq!(config.paths.additional, vec!["C:\\Old"]);
    assert!(config.paths.scan.is_empty());
    assert!(config.appearance.show_icons);
    assert!(config.general.hotkey_toggle);
    assert_eq!(config.visual.preset, ThemePreset::Obsidian);
}

#[test]
fn alt_space_is_preserved() {
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Space"

            [appearance]
            max_results = 8
            window_width = 600

            [paths]
            additional = []
        "#;
    let config: Config = toml::from_str(toml_str).expect("parse");
    assert!(config.hotkey.modifier.eq_ignore_ascii_case("Alt"));
    assert!(config.hotkey.key.eq_ignore_ascii_case("Space"));
}

#[test]
fn deserialize_general_and_visual_config() {
    let toml_str = r##"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [general]
            hotkey_toggle = false
            show_on_startup = true
            auto_hide_on_focus_lost = false
            show_tray_icon = false
            ime_off_on_show = true

            [appearance]
            max_results = 8
            window_width = 600

            [visual]
            preset = "paper"
            background_color = "#ffffff"
            input_background_color = "#f2f2f2"
            text_color = "#111111"
            selected_row_color = "#d0d0d0"
            hint_text_color = "#666666"
            font_family = "Yu Gothic UI"
            font_size = 18

            [paths]
            additional = []
        "##;
    let config: Config = toml::from_str(toml_str).expect("parse");
    assert!(!config.general.hotkey_toggle);
    assert!(config.general.show_on_startup);
    assert!(!config.general.auto_hide_on_focus_lost);
    assert!(!config.general.show_tray_icon);
    assert!(config.general.ime_off_on_show);
    assert_eq!(config.visual.preset, ThemePreset::Paper);
    assert_eq!(config.visual.background_color, "#ffffff");
    assert_eq!(config.visual.input_background_color, "#f2f2f2");
    assert_eq!(config.visual.text_color, "#111111");
    assert_eq!(config.visual.selected_row_color, "#d0d0d0");
    assert_eq!(config.visual.hint_text_color, "#666666");
    assert_eq!(config.visual.font_family, "Yu Gothic UI");
    assert_eq!(config.visual.font_size, 18);
}

#[test]
fn opener_round_trip_toml() {
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            max_results = 8
            window_width = 600

            [paths]
            additional = []

            [[openers]]
            target = "folder"

            [[openers.tools]]
            name = "Total Commander"
            exe = "C:\\totalcmd\\TOTALCMD64.EXE"
            args = "/O /T"

            [[openers.tools]]
            name = "Explorer"
            exe = "explorer.exe"

            [[openers]]
            target = "ext:png,jpg,gif"

            [[openers.tools]]
            name = "IrfanView"
            exe = "C:\\irfan\\i_view64.exe"
        "#;
    let config: Config = toml::from_str(toml_str).expect("parse");
    assert_eq!(config.openers.len(), 2);
    assert_eq!(config.openers[0].target, "folder");
    assert_eq!(config.openers[0].tools.len(), 2);
    assert_eq!(config.openers[0].tools[0].name, "Total Commander");
    assert_eq!(config.openers[0].tools[0].args, "/O /T");
    assert_eq!(config.openers[0].tools[1].args, "");
    assert_eq!(config.openers[1].target, "ext:png,jpg,gif");
    assert_eq!(config.openers[1].tools[0].name, "IrfanView");
}

#[test]
fn config_without_openers_defaults_to_empty() {
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
    let config: Config = toml::from_str(toml_str).expect("parse");
    assert!(config.openers.is_empty());
    assert!(config.instant_commands.is_empty());
}

// ---- instant command prefix validation tests ----

#[test]
fn instant_command_round_trip_toml() {
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"
            [appearance]
            window_width = 600
            [paths]
            additional = []
            [[instant_commands]]
            name = "g"
            command = "https://google.com/search?q={query}"
            [[instant_commands]]
            name = "memo"
            command = "C:\\tools\\editor.exe"
        "#;
    let mut config: Config = toml::from_str(toml_str).expect("parse");
    config.apply_migrations();
    assert_eq!(config.instant_commands.len(), 2);
    assert_eq!(config.instant_commands[0].name, "g");
    assert!(matches!(
        config.instant_commands[0].action,
        InstantAction::Url { .. }
    ));
    assert_eq!(config.instant_commands[1].name, "memo");
    assert!(matches!(
        config.instant_commands[1].action,
        InstantAction::Url { .. }
    ));
}

#[test]
fn instant_mixed_variants_roundtrip() {
    // Url 1件 + Exec 1件（description 省略）の Vec が serialize → deserialize で両変種と name を保つ。
    let config = Config {
        instant_commands: vec![
            InstantCommand {
                name: "g".to_string(),
                description: "Google".to_string(),
                action: InstantAction::Url {
                    url: "https://www.google.com/search?q={query}".to_string(),
                },
            },
            InstantCommand {
                name: "ev".to_string(),
                description: String::new(),
                action: InstantAction::Exec {
                    exe: "everything.exe".to_string(),
                    args: "-s {query}".to_string(),
                },
            },
        ],
        ..Default::default()
    };
    let serialized = toml::to_string_pretty(&config).expect("serialize");
    let parsed: Config = toml::from_str(&serialized).expect("parse");
    assert_eq!(parsed.instant_commands.len(), 2);
    assert_eq!(parsed.instant_commands[0].name, "g");
    assert!(
        matches!(&parsed.instant_commands[0].action, InstantAction::Url { url } if url.contains("google"))
    );
    assert_eq!(parsed.instant_commands[1].name, "ev");
    assert!(
        matches!(&parsed.instant_commands[1].action, InstantAction::Exec { exe, .. } if exe == "everything.exe")
    );
}
