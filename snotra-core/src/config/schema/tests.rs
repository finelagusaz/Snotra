//! セクション型・要素型の serde と、件数パラメータから派生する上限。

use super::*;

#[test]
fn visual_config_without_custom_theme_deserializes() {
    let toml_str = r##"
            preset = "obsidian"
            background_color = "#282828"
            input_background_color = "#383838"
            text_color = "#E0E0E0"
            selected_row_color = "#505050"
            hint_text_color = "#808080"
            font_family = "Segoe UI"
            font_size = 15
        "##;
    let vc: VisualConfig = toml::from_str(toml_str).expect("parse");
    assert_eq!(vc.preset, ThemePreset::Obsidian);
    assert!(vc.custom_theme.is_none());
}

#[test]
fn visual_config_with_custom_theme_round_trip() {
    let toml_str = r##"
            preset = "custom"
            background_color = "#1a1a2a"
            input_background_color = "#2a2a3a"
            text_color = "#d0d0ff"
            selected_row_color = "#3a3a5a"
            hint_text_color = "#7070a0"
            font_family = "Segoe UI"
            font_size = 15

            [custom_theme]
            background_color = "#1a1a2a"
            input_background_color = "#2a2a3a"
            text_color = "#d0d0ff"
            selected_row_color = "#3a3a5a"
            hint_text_color = "#7070a0"
        "##;
    let vc: VisualConfig = toml::from_str(toml_str).expect("parse");
    assert_eq!(vc.preset, ThemePreset::Custom);
    let ct = vc.custom_theme.as_ref().expect("custom_theme should exist");
    assert_eq!(ct.background_color, "#1a1a2a");
    assert_eq!(ct.text_color, "#d0d0ff");

    // round-trip: serialize then deserialize
    let serialized = toml::to_string_pretty(&vc).expect("serialize");
    let vc2: VisualConfig = toml::from_str(&serialized).expect("re-parse");
    assert_eq!(vc, vc2);
}

#[test]
fn visual_config_custom_theme_omitted_when_none() {
    let vc = VisualConfig::default();
    let serialized = toml::to_string_pretty(&vc).expect("serialize");
    assert!(
        !serialized.contains("custom_theme"),
        "custom_theme should not appear when None"
    );
}

#[test]
fn theme_preset_custom_deserializes() {
    let toml_str = r#"preset = "custom""#;
    #[derive(Deserialize)]
    struct Wrapper {
        preset: ThemePreset,
    }
    let w: Wrapper = toml::from_str(toml_str).expect("parse");
    assert_eq!(w.preset, ThemePreset::Custom);
}

// -- Config defense tests (external process writes) --

#[test]
fn language_serialize_deserialize() {
    // TOML doesn't support bare enum values; test via a wrapper struct
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Wrapper {
        lang: Language,
    }
    let ja = Wrapper { lang: Language::Ja };
    let serialized = toml::to_string(&ja).unwrap();
    assert!(serialized.contains("lang = \"ja\""));
    let deserialized: Wrapper = toml::from_str(&serialized).unwrap();
    assert_eq!(deserialized, ja);

    let en = Wrapper { lang: Language::En };
    let serialized = toml::to_string(&en).unwrap();
    assert!(serialized.contains("lang = \"en\""));
    let deserialized: Wrapper = toml::from_str(&serialized).unwrap();
    assert_eq!(deserialized, en);
}

#[test]
fn language_default_in_general_config() {
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [general]

            [appearance]
            max_results = 8
            window_width = 600

            [paths]
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    // Without explicit language, should use OS-detected default
    let expected = default_language();
    assert_eq!(config.general.language, expected);
}

#[test]
fn language_explicit_in_config() {
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [general]
            language = "en"

            [appearance]
            max_results = 8
            window_width = 600

            [paths]
        "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.general.language, Language::En);
}

#[test]
fn icon_cache_cap_derives_from_working_set_times_retention() {
    // 既定: visible_rows=8, result_limit=200, recent_limit=8 → working_set=200
    // → cap = 200 × 5 = 1000（旧 default と一致＝既定挙動は不変）。
    let config = Config::default();
    assert_eq!(config.search.effective_result_limit(), 200);
    assert_eq!(config.icon_cache_cap(), 1000);
}

#[test]
fn icon_cache_cap_scales_with_result_limit() {
    // result_limit を上げると上限も自動追従する（検証不要・drift なし）。
    let mut config = Config::default();
    config.search.result_limit = Some(500);
    assert_eq!(config.icon_cache_cap(), 2500); // 500 × 5
}

#[test]
fn icon_cache_cap_uses_max_of_all_list_limits() {
    // working_set = max(visible_rows, result_limit, recent_limit)。
    // ここでは recent_limit が支配的になるケースを検証する。
    let mut config = Config::default();
    config.appearance.visible_rows = Some(8);
    config.search.result_limit = Some(50);
    config.search.recent_limit = Some(120);
    assert_eq!(config.icon_cache_cap(), 600); // max(8,50,120)=120 × 5
}

#[test]
fn icon_cache_cap_visible_rows_dominates_when_list_limits_small() {
    let mut config = Config::default();
    config.appearance.visible_rows = Some(12);
    config.search.result_limit = Some(5);
    config.search.recent_limit = Some(3);
    assert_eq!(config.icon_cache_cap(), 60); // max(12,5,3)=12 × 5
}

#[test]
fn icon_cache_cap_never_collapses_to_zero() {
    // 退行防御: 全リスト上限が 0 でも working_set は 1 で floor され、cap は RETENTION 以上。
    // （visible_rows=0 は validate の VisibleRowsZero 対象だが、cap 導出は panic/0 にならない）。
    let mut config = Config::default();
    config.appearance.visible_rows = Some(0);
    config.search.result_limit = Some(0);
    config.search.recent_limit = Some(0);
    assert_eq!(config.icon_cache_cap(), ICON_CACHE_RETENTION_FACTOR);
    assert!(config.icon_cache_cap() >= 1);
}

// ---- InstantAction serde gate (release gate: 失敗は全設定リセットを意味する) ----
fn cfg_with_instant(cmds: Vec<InstantCommand>) -> Config {
    Config {
        instant_commands: cmds,
        ..Default::default()
    }
}

#[test] // T2: legacy 行が deserialize できる（最重要・データ損失検出器）
fn instant_legacy_command_deserializes() {
    let legacy = cfg_with_instant(vec![InstantCommand {
        name: "g".into(),
        description: String::new(),
        action: InstantAction::Legacy {
            command: "https://x/?q={query}".into(),
        },
    }]);
    let s = toml::to_string(&legacy).expect("serialize legacy");
    // Legacy は `command = "..."` 形（=旧オンディスク形式）で出力される
    assert!(s.contains("command ="));
    let parsed: Config = toml::from_str(&s).expect("legacy deserialize must succeed");
    assert!(matches!(
        parsed.instant_commands[0].action,
        InstantAction::Legacy { .. }
    ));
}

#[test] // T1: Config 全体の serialize 往復で変種が保たれる
fn instant_exec_roundtrip_preserves_variant() {
    let cfg = cfg_with_instant(vec![InstantCommand {
        name: "ev".into(),
        description: "Everything".into(),
        action: InstantAction::Exec {
            exe: "everything.exe".into(),
            args: "-s {query}".into(),
        },
    }]);
    let s = toml::to_string_pretty(&cfg).expect("serialize");
    let parsed: Config = toml::from_str(&s).expect("deserialize");
    assert_eq!(
        parsed.instant_commands[0].action,
        InstantAction::Exec {
            exe: "everything.exe".into(),
            args: "-s {query}".into()
        }
    );
}

#[test] // T3: url と exe を両方書いた行は Url 先勝ち（untagged 宣言順）
fn instant_both_url_and_exe_prefers_url() {
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"
            [appearance]
            window_width = 600
            [paths]
            additional = []
            [[instant_commands]]
            name = "x"
            url = "https://x"
            exe = "y.exe"
        "#;
    let cfg: Config = toml::from_str(toml_str).expect("parse");
    assert!(matches!(
        cfg.instant_commands[0].action,
        InstantAction::Url { .. }
    ));
}

#[test] // T4: Exec で args 省略 → 空文字
fn instant_exec_args_defaults_empty() {
    let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"
            [appearance]
            window_width = 600
            [paths]
            additional = []
            [[instant_commands]]
            name = "n"
            exe = "notepad.exe"
        "#;
    let cfg: Config = toml::from_str(toml_str).expect("parse");
    assert_eq!(
        cfg.instant_commands[0].action,
        InstantAction::Exec {
            exe: "notepad.exe".into(),
            args: String::new()
        }
    );
}
