//! 保存前の整合性検証（検出のみ。補正しないことを含む）。

use super::*;
use crate::config::{InstantCommand, ScanPath};

#[test]
fn validate_default_config_returns_no_errors() {
    let config = Config::default();
    let errors = config.validate();
    assert!(
        errors.is_empty(),
        "default config should have no validation errors"
    );
}

#[test]
fn validate_empty_hotkey_modifier() {
    let mut config = Config::default();
    config.hotkey.modifier = "".to_string();
    let errors = config.validate();
    assert!(errors.contains(&ConfigError::HotkeyModifierEmpty));
}

#[test]
fn validate_whitespace_only_hotkey_modifier() {
    let mut config = Config::default();
    config.hotkey.modifier = "  ".to_string();
    let errors = config.validate();
    assert!(errors.contains(&ConfigError::HotkeyModifierEmpty));
}

#[test]
fn validate_empty_hotkey_key() {
    let mut config = Config::default();
    config.hotkey.key = "".to_string();
    let errors = config.validate();
    assert!(errors.contains(&ConfigError::HotkeyKeyEmpty));
}

#[test]
fn validate_whitespace_only_hotkey_key() {
    let mut config = Config::default();
    config.hotkey.key = " ".to_string();
    let errors = config.validate();
    assert!(errors.contains(&ConfigError::HotkeyKeyEmpty));
}

#[test]
fn validate_visible_rows_zero() {
    let mut config = Config::default();
    config.appearance.visible_rows = Some(0);
    let errors = config.validate();
    assert!(errors.contains(&ConfigError::VisibleRowsZero));
}

#[test]
fn validate_window_width_too_small() {
    let mut config = Config::default();
    config.appearance.window_width = 100;
    let errors = config.validate();
    assert!(errors.contains(&ConfigError::WindowWidthTooSmall(100)));
}

#[test]
fn validate_window_width_boundary_199() {
    let mut config = Config::default();
    config.appearance.window_width = 199;
    let errors = config.validate();
    assert!(errors.contains(&ConfigError::WindowWidthTooSmall(199)));
}

#[test]
fn validate_window_width_boundary_200_is_ok() {
    let mut config = Config::default();
    config.appearance.window_width = 200;
    let errors = config.validate();
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, ConfigError::WindowWidthTooSmall(_))),
        "window_width=200 should not produce an error"
    );
}

#[test]
fn validate_fuzzy_cap_ratio_out_of_range_above() {
    let mut config = Config::default();
    config.search.fuzzy_history_cap_ratio = 1.5;
    let errors = config.validate();
    assert!(errors.contains(&ConfigError::FuzzyCapRatioOutOfRange { value: 1.5 }));
}

#[test]
fn validate_fuzzy_cap_ratio_out_of_range_negative() {
    let mut config = Config::default();
    config.search.fuzzy_history_cap_ratio = -0.1;
    let errors = config.validate();
    assert!(errors.contains(&ConfigError::FuzzyCapRatioOutOfRange { value: -0.1 }));
}

#[test]
fn validate_fuzzy_cap_ratio_boundary_0_is_ok() {
    let mut config = Config::default();
    config.search.fuzzy_history_cap_ratio = 0.0;
    let errors = config.validate();
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, ConfigError::FuzzyCapRatioOutOfRange { .. })),
        "ratio=0.0 should not produce an error"
    );
}

#[test]
fn validate_fuzzy_cap_ratio_boundary_1_is_ok() {
    let mut config = Config::default();
    config.search.fuzzy_history_cap_ratio = 1.0;
    let errors = config.validate();
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, ConfigError::FuzzyCapRatioOutOfRange { .. })),
        "ratio=1.0 should not produce an error"
    );
}

#[test]
fn validate_empty_scan_path() {
    let mut config = Config::default();
    config.paths.scan = vec![
        ScanPath {
            path: "C:\\Valid".to_string(),
            extensions: vec![".exe".to_string()],
            include_folders: false,
        },
        ScanPath {
            path: "".to_string(),
            extensions: vec![".lnk".to_string()],
            include_folders: false,
        },
    ];
    let errors = config.validate();
    assert!(errors.contains(&ConfigError::ScanPathEmpty { index: 1 }));
    assert!(
        !errors.contains(&ConfigError::ScanPathEmpty { index: 0 }),
        "valid path at index 0 should not produce an error"
    );
}

#[test]
fn validate_whitespace_only_scan_path() {
    let mut config = Config::default();
    config.paths.scan = vec![ScanPath {
        path: "   ".to_string(),
        extensions: vec![".lnk".to_string()],
        include_folders: false,
    }];
    let errors = config.validate();
    assert!(errors.contains(&ConfigError::ScanPathEmpty { index: 0 }));
}

// ---- normalize_scan_path_key tests ----

#[test]
fn validate_instant_command_prefix_empty() {
    let mut config = Config::default();
    config.search.instant_command_prefix = "".to_string();
    let errors = config.validate();
    assert!(errors.contains(&ConfigError::InstantCommandPrefixEmpty));
}

#[test]
fn validate_instant_command_prefix_slash() {
    let mut config = Config::default();
    config.search.instant_command_prefix = "/".to_string();
    let errors = config.validate();
    assert!(errors.contains(&ConfigError::InstantCommandPrefixSlash));
}

#[test]
fn validate_instant_command_prefix_slash_multi_char() {
    let mut config = Config::default();
    config.search.instant_command_prefix = "//".to_string();
    let errors = config.validate();
    assert!(errors.contains(&ConfigError::InstantCommandPrefixSlash));
}

#[test]
fn validate_instant_command_prefix_at_is_ok() {
    let mut config = Config::default();
    config.search.instant_command_prefix = "@".to_string();
    let errors = config.validate();
    assert!(
        !errors.iter().any(|e| matches!(
            e,
            ConfigError::InstantCommandPrefixEmpty | ConfigError::InstantCommandPrefixSlash
        )),
        "@ should not produce prefix errors"
    );
}

#[test]
fn validate_instant_command_duplicate_name() {
    let config = Config {
        instant_commands: vec![
            InstantCommand {
                name: "google".to_string(),
                description: String::new(),
                action: InstantAction::Url {
                    url: "https://google.com".into(),
                },
            },
            InstantCommand {
                name: "google".to_string(),
                description: String::new(),
                action: InstantAction::Url {
                    url: "https://google.co.jp".into(),
                },
            },
        ],
        ..Default::default()
    };
    let errors = config.validate();
    assert!(errors.contains(&ConfigError::InstantCommandDuplicateName {
        name: "google".to_string(),
    }));
}

#[test]
fn validate_instant_command_unique_names_ok() {
    let config = Config {
        instant_commands: vec![
            InstantCommand {
                name: "google".to_string(),
                description: String::new(),
                action: InstantAction::Url {
                    url: "https://google.com".into(),
                },
            },
            InstantCommand {
                name: "bing".to_string(),
                description: String::new(),
                action: InstantAction::Url {
                    url: "https://bing.com".into(),
                },
            },
        ],
        ..Default::default()
    };
    let errors = config.validate();
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, ConfigError::InstantCommandDuplicateName { .. })),
    );
}

#[test]
fn validate_instant_command_unknown_modifier_url() {
    let config = Config {
        instant_commands: vec![InstantCommand {
            name: "g".to_string(),
            description: String::new(),
            action: InstantAction::Url {
                url: "https://x.com/?q={query | bogus}".into(),
            },
        }],
        ..Default::default()
    };
    let errors = config.validate();
    assert!(
        errors.contains(&ConfigError::InstantCommandUnknownModifier {
            name: "g".to_string(),
            modifier: "bogus".to_string(),
        })
    );
}

#[test]
fn validate_instant_command_unknown_modifier_in_args() {
    let config = Config {
        instant_commands: vec![InstantCommand {
            name: "ev".to_string(),
            description: String::new(),
            action: InstantAction::Exec {
                exe: "everything.exe".into(),
                args: "-s {query | nope}".into(),
            },
        }],
        ..Default::default()
    };
    let errors = config.validate();
    assert!(
        errors.contains(&ConfigError::InstantCommandUnknownModifier {
            name: "ev".to_string(),
            modifier: "nope".to_string(),
        })
    );
}

#[test]
fn validate_instant_command_known_modifiers_ok() {
    let config = Config {
        instant_commands: vec![InstantCommand {
            name: "g".to_string(),
            description: String::new(),
            action: InstantAction::Url {
                url: "https://x.com/?q={query | lower | trim | default:x | raw}".into(),
            },
        }],
        ..Default::default()
    };
    let errors = config.validate();
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, ConfigError::InstantCommandUnknownModifier { .. })),
    );
}

// ---- instant command TOML round-trip ----

#[test]
fn validate_multiple_errors_all_reported() {
    let mut config = Config::default();
    config.hotkey.modifier = "".to_string();
    config.hotkey.key = "".to_string();
    config.appearance.visible_rows = Some(0);
    config.appearance.window_width = 50;
    config.search.fuzzy_history_cap_ratio = 2.0;
    config.paths.scan = vec![ScanPath {
        path: "".to_string(),
        extensions: vec![],
        include_folders: false,
    }];

    let errors = config.validate();
    assert!(errors.contains(&ConfigError::HotkeyModifierEmpty));
    assert!(errors.contains(&ConfigError::HotkeyKeyEmpty));
    assert!(errors.contains(&ConfigError::VisibleRowsZero));
    assert!(errors.contains(&ConfigError::WindowWidthTooSmall(50)));
    assert!(errors.contains(&ConfigError::FuzzyCapRatioOutOfRange { value: 2.0 }));
    assert!(errors.contains(&ConfigError::ScanPathEmpty { index: 0 }));
    assert_eq!(errors.len(), 6, "all 6 errors should be reported");
}

#[test]
fn validate_system_shortcut_produces_conflict_error() {
    let mut config = Config::default();
    config.hotkey.modifier = "Alt".to_string();
    config.hotkey.key = "F4".to_string();
    let errors = config.validate();
    assert!(errors.contains(&ConfigError::HotkeySystemConflict {
        modifier: "Alt".to_string(),
        key: "F4".to_string(),
    }));
}

#[test]
fn validate_duplicate_modifier_cannot_bypass_system_conflict() {
    let mut config = Config::default();
    config.hotkey.modifier = "Alt+Alt".to_string();
    config.hotkey.key = "F4".to_string();
    assert!(
        config
            .validate()
            .contains(&ConfigError::HotkeySystemConflict {
                modifier: "Alt+Alt".to_string(),
                key: "F4".to_string(),
            })
    );
}

#[test]
fn validate_reports_unknown_modifier_and_unsupported_key() {
    let mut config = Config::default();
    config.hotkey.modifier = "Ctrl+Hyper".to_string();
    assert_eq!(
        config.validate_hotkey(),
        vec![ConfigError::HotkeyUnknownModifier {
            modifier: "Hyper".to_string(),
        }]
    );

    config.hotkey.modifier = "Alt".to_string();
    config.hotkey.key = "!".to_string();
    assert_eq!(
        config.validate_hotkey(),
        vec![ConfigError::HotkeyUnsupportedKey {
            key: "!".to_string(),
        }]
    );
}

#[test]
fn validate_preserves_both_empty_hotkey_errors() {
    let mut config = Config::default();
    config.hotkey.modifier.clear();
    config.hotkey.key.clear();
    assert_eq!(
        config.validate_hotkey(),
        vec![
            ConfigError::HotkeyModifierEmpty,
            ConfigError::HotkeyKeyEmpty
        ]
    );

    config.hotkey.modifier = "++".to_string();
    assert_eq!(
        config.validate_hotkey(),
        vec![
            ConfigError::HotkeyModifierEmpty,
            ConfigError::HotkeyKeyEmpty
        ]
    );
}

#[test]
fn validate_allowed_hotkey_no_conflict_error() {
    let mut config = Config::default();
    config.hotkey.modifier = "Alt".to_string();
    config.hotkey.key = "Q".to_string();
    let errors = config.validate();
    let has_conflict = errors
        .iter()
        .any(|e| matches!(e, ConfigError::HotkeySystemConflict { .. }));
    assert!(
        !has_conflict,
        "Alt+Q should not produce a system conflict error"
    );
}

// ---- CustomTheme / ThemePreset::Custom tests ----
