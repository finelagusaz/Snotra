//! 探索パスのキー正規化・拡張子正規化・重複マージ。

use super::*;

#[test]
fn default_scan_paths_have_lnk_extension() {
    let paths = Config::default_scan_paths();
    for sp in &paths {
        assert_eq!(sp.extensions, vec![".lnk"]);
        assert!(!sp.include_folders);
    }
}

#[test]
fn normalize_scan_path_key_case_insensitive() {
    assert_eq!(normalize_scan_path_key("C:\\Tools"), "c:\\tools");
}

#[test]
fn normalize_scan_path_key_slash_to_backslash() {
    assert_eq!(normalize_scan_path_key("C:/Tools"), "c:\\tools");
}

#[test]
fn normalize_scan_path_key_trims_whitespace() {
    assert_eq!(normalize_scan_path_key("  C:\\Tools  "), "c:\\tools");
}

#[test]
fn normalize_scan_path_key_strips_trailing_backslash() {
    assert_eq!(normalize_scan_path_key("C:\\Tools\\"), "c:\\tools");
}

#[test]
fn normalize_scan_path_key_preserves_drive_root() {
    assert_eq!(normalize_scan_path_key("C:\\"), "c:\\");
}

#[test]
fn normalize_scan_path_key_drive_root_forward_slash() {
    assert_eq!(normalize_scan_path_key("C:/"), "c:\\");
}

#[test]
fn normalize_scan_path_key_equivalence() {
    let a = normalize_scan_path_key("C:\\Tools");
    let b = normalize_scan_path_key("c:/tools");
    let c = normalize_scan_path_key("  C:\\Tools\\  ");
    assert_eq!(a, b);
    assert_eq!(b, c);
}

// ---- normalize_extension tests ----

#[test]
fn normalize_extension_adds_dot() {
    assert_eq!(normalize_extension("exe"), ".exe");
}

#[test]
fn normalize_extension_keeps_dot() {
    assert_eq!(normalize_extension(".exe"), ".exe");
}

#[test]
fn normalize_extension_lowercases() {
    assert_eq!(normalize_extension(".EXE"), ".exe");
}

#[test]
fn normalize_extension_empty() {
    assert_eq!(normalize_extension(""), "");
    assert_eq!(normalize_extension("."), "");
    assert_eq!(normalize_extension("  "), "");
}

// ---- dedup_scan_paths tests ----

#[test]
fn dedup_scan_paths_merges_case_variants() {
    let scan = vec![
        ScanPath {
            path: "C:\\Tools".to_string(),
            extensions: vec![".exe".to_string()],
            include_folders: false,
        },
        ScanPath {
            path: "c:\\TOOLS".to_string(),
            extensions: vec![".bat".to_string()],
            include_folders: true,
        },
    ];
    let result = dedup_scan_paths(&scan);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, "C:\\Tools");
    assert_eq!(result[0].extensions, vec![".bat", ".exe"]);
    assert!(result[0].include_folders);
}

#[test]
fn dedup_scan_paths_preserves_first_seen_order() {
    let scan = vec![
        ScanPath {
            path: "D:\\Apps".to_string(),
            extensions: vec![".exe".to_string()],
            include_folders: false,
        },
        ScanPath {
            path: "C:\\Tools".to_string(),
            extensions: vec![".lnk".to_string()],
            include_folders: false,
        },
        ScanPath {
            path: "d:/apps".to_string(),
            extensions: vec![".bat".to_string()],
            include_folders: false,
        },
    ];
    let result = dedup_scan_paths(&scan);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].path, "D:\\Apps");
    assert_eq!(result[1].path, "C:\\Tools");
}

#[test]
fn dedup_scan_paths_normalizes_extensions() {
    let scan = vec![ScanPath {
        path: "C:\\Tools".to_string(),
        extensions: vec!["EXE".to_string(), ".exe".to_string(), "bat".to_string()],
        include_folders: false,
    }];
    let result = dedup_scan_paths(&scan);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].extensions, vec![".bat", ".exe"]);
}

#[test]
fn dedup_scan_paths_skips_empty_paths() {
    let scan = vec![
        ScanPath {
            path: "".to_string(),
            extensions: vec![".exe".to_string()],
            include_folders: false,
        },
        ScanPath {
            path: "  ".to_string(),
            extensions: vec![".bat".to_string()],
            include_folders: false,
        },
    ];
    let result = dedup_scan_paths(&scan);
    assert!(result.is_empty());
}

#[test]
fn dedup_scan_paths_trailing_backslash_merge() {
    let scan = vec![
        ScanPath {
            path: "C:\\Tools".to_string(),
            extensions: vec![".exe".to_string()],
            include_folders: false,
        },
        ScanPath {
            path: "C:\\Tools\\".to_string(),
            extensions: vec![".bat".to_string()],
            include_folders: false,
        },
    ];
    let result = dedup_scan_paths(&scan);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, "C:\\Tools");
    assert_eq!(result[0].extensions, vec![".bat", ".exe"]);
}

#[test]
fn dedup_scan_paths_no_duplicates_unchanged() {
    let scan = vec![
        ScanPath {
            path: "C:\\Tools".to_string(),
            extensions: vec![".exe".to_string()],
            include_folders: false,
        },
        ScanPath {
            path: "D:\\Apps".to_string(),
            extensions: vec![".bat".to_string()],
            include_folders: false,
        },
    ];
    let result = dedup_scan_paths(&scan);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].path, "C:\\Tools");
    assert_eq!(result[1].path, "D:\\Apps");
}

// ---- PathsConfig::normalize_scan_paths tests ----

#[test]
fn normalize_scan_paths_returns_true_when_changed() {
    let mut config = Config::default();
    config.paths.scan = vec![
        ScanPath {
            path: "C:\\Tools".to_string(),
            extensions: vec![".exe".to_string()],
            include_folders: false,
        },
        ScanPath {
            path: "c:\\tools".to_string(),
            extensions: vec![".bat".to_string()],
            include_folders: false,
        },
    ];
    assert!(config.paths.normalize_scan_paths());
    assert_eq!(config.paths.scan.len(), 1);
}

#[test]
fn normalize_scan_paths_returns_false_when_no_change() {
    let mut config = Config::default();
    config.paths.scan = vec![ScanPath {
        path: "C:\\Tools".to_string(),
        extensions: vec![".exe".to_string()],
        include_folders: false,
    }];
    assert!(!config.paths.normalize_scan_paths());
}
