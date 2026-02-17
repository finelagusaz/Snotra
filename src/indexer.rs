use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{Config, ScanPath};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppEntry {
    pub name: String,
    pub target_path: String,
    pub is_folder: bool,
}

pub fn scan_all(additional_paths: &[String], scan_paths: &[ScanPath]) -> Vec<AppEntry> {
    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Scan standard Start Menu locations (.lnk only)
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let user_start = PathBuf::from(appdata).join("Microsoft\\Windows\\Start Menu\\Programs");
        scan_directory_lnk(&user_start, &mut entries, &mut seen);
    }
    if let Some(programdata) = std::env::var_os("ProgramData") {
        let common_start =
            PathBuf::from(programdata).join("Microsoft\\Windows\\Start Menu\\Programs");
        scan_directory_lnk(&common_start, &mut entries, &mut seen);
    }

    // Scan Desktop (.lnk only)
    if let Some(desktop) = dirs::desktop_dir() {
        scan_directory_lnk(&desktop, &mut entries, &mut seen);
    }

    // Scan legacy additional paths (.lnk only)
    for path in additional_paths {
        scan_directory_lnk(Path::new(path), &mut entries, &mut seen);
    }

    // Scan paths with per-path extension filtering
    for sp in scan_paths {
        let exts: Vec<String> = sp.extensions.iter().map(|e| e.to_lowercase()).collect();
        scan_directory_with_extensions(
            Path::new(&sp.path),
            &exts,
            sp.include_folders,
            &mut entries,
            &mut seen,
        );
    }

    entries
}

/// Recursively scan for .lnk shortcuts (original behavior)
fn scan_directory_lnk(
    dir: &Path,
    entries: &mut Vec<AppEntry>,
    seen: &mut std::collections::HashSet<String>,
) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_directory_lnk(&path, entries, seen);
        } else if path.extension().and_then(|e| e.to_str()) == Some("lnk") {
            if let Some(app) = parse_lnk(&path) {
                if seen.insert(app.name.to_lowercase()) {
                    entries.push(app);
                }
            }
        }
    }
}

/// Recursively scan for files matching given extensions, optionally including folders
fn scan_directory_with_extensions(
    dir: &Path,
    extensions: &[String],
    include_folders: bool,
    entries: &mut Vec<AppEntry>,
    seen: &mut std::collections::HashSet<String>,
) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if include_folders {
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                if !name.is_empty() {
                    let key = format!("folder:{}", name.to_lowercase());
                    if seen.insert(key) {
                        entries.push(AppEntry {
                            name,
                            target_path: path.to_string_lossy().to_string(),
                            is_folder: true,
                        });
                    }
                }
            }
            scan_directory_with_extensions(&path, extensions, include_folders, entries, seen);
        } else {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| format!(".{}", e.to_lowercase()));
            if let Some(ext) = ext {
                if extensions.contains(&ext) {
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();
                    if !name.is_empty() && seen.insert(name.to_lowercase()) {
                        entries.push(AppEntry {
                            name,
                            target_path: path.to_string_lossy().to_string(),
                            is_folder: false,
                        });
                    }
                }
            }
        }
    }
}

fn parse_lnk(path: &Path) -> Option<AppEntry> {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();

    let target = path.to_string_lossy().to_string();

    if target.is_empty() {
        return None;
    }

    Some(AppEntry {
        name,
        target_path: target,
        is_folder: false,
    })
}

#[derive(Serialize, Deserialize)]
struct IndexCache {
    version: u32,
    built_at: u64,
    entries: Vec<AppEntry>,
    config_hash: u64,
}

const INDEX_CACHE_VERSION: u32 = 1;

fn compute_config_hash(additional: &[String], scan: &[ScanPath]) -> u64 {
    let mut hasher = DefaultHasher::new();
    additional.hash(&mut hasher);
    for sp in scan {
        sp.path.hash(&mut hasher);
        sp.extensions.hash(&mut hasher);
        sp.include_folders.hash(&mut hasher);
    }
    hasher.finish()
}

fn cache_path() -> Option<PathBuf> {
    Config::config_dir().map(|p| p.join("index.bin"))
}

/// Scan filesystem every startup; compare with cache to detect changes.
/// Returns (entries, changed) where changed=true means the entry set differs from cache.
pub fn load_or_scan(additional: &[String], scan: &[ScanPath]) -> (Vec<AppEntry>, bool) {
    let current_hash = compute_config_hash(additional, scan);
    let entries = scan_all(additional, scan);

    // Compare with cached entries to determine if icon rebuild is needed
    let changed = if let Some(path) = cache_path() {
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(cache) = bincode::deserialize::<IndexCache>(&bytes) {
                cache.version != INDEX_CACHE_VERSION
                    || cache.config_hash != current_hash
                    || !entries_equal(&cache.entries, &entries)
            } else {
                true
            }
        } else {
            true
        }
    } else {
        true
    };

    if changed {
        save_cache(&entries, current_hash);
    }

    (entries, changed)
}

fn entries_equal(a: &[AppEntry], b: &[AppEntry]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).all(|(x, y)| {
        x.name == y.name && x.target_path == y.target_path && x.is_folder == y.is_folder
    })
}

fn save_cache(entries: &[AppEntry], config_hash: u64) {
    let Some(path) = cache_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }

    let cache = IndexCache {
        version: INDEX_CACHE_VERSION,
        built_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        entries: entries.to_vec(),
        config_hash,
    };

    let Ok(bytes) = bincode::serialize(&cache) else {
        return;
    };

    let tmp_path = path.with_extension("bin.tmp");
    if std::fs::write(&tmp_path, &bytes).is_ok() {
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::rename(&tmp_path, &path);
    }
}

/// Force rebuild: scan and save cache, regardless of existing cache.
/// Called from settings dialog (Phase 5).
#[allow(dead_code)]
pub fn rebuild_and_save(additional: &[String], scan: &[ScanPath]) -> Vec<AppEntry> {
    let entries = scan_all(additional, scan);
    let config_hash = compute_config_hash(additional, scan);
    save_cache(&entries, config_hash);
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("snotra_idx_test_{}", tag));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn scan_with_extensions_filters_by_ext() {
        let dir = temp_dir("ext_filter");
        fs::write(dir.join("app.exe"), "").unwrap();
        fs::write(dir.join("script.bat"), "").unwrap();
        fs::write(dir.join("readme.txt"), "").unwrap();

        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let exts = vec![".exe".to_string(), ".bat".to_string()];
        scan_directory_with_extensions(&dir, &exts, false, &mut entries, &mut seen);

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"app"));
        assert!(names.contains(&"script"));
        assert!(!names.contains(&"readme"));
        assert!(entries.iter().all(|e| !e.is_folder));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_with_extensions_includes_folders() {
        let dir = temp_dir("ext_folders");
        fs::write(dir.join("app.exe"), "").unwrap();
        fs::create_dir(dir.join("subdir")).unwrap();

        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let exts = vec![".exe".to_string()];
        scan_directory_with_extensions(&dir, &exts, true, &mut entries, &mut seen);

        let folder_entries: Vec<&AppEntry> = entries.iter().filter(|e| e.is_folder).collect();
        assert_eq!(folder_entries.len(), 1);
        assert_eq!(folder_entries[0].name, "subdir");
        assert!(folder_entries[0].is_folder);

        let file_entries: Vec<&AppEntry> = entries.iter().filter(|e| !e.is_folder).collect();
        assert_eq!(file_entries.len(), 1);
        assert_eq!(file_entries[0].name, "app");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_with_extensions_no_folders_when_disabled() {
        let dir = temp_dir("ext_no_folders");
        fs::write(dir.join("app.exe"), "").unwrap();
        fs::create_dir(dir.join("subdir")).unwrap();

        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let exts = vec![".exe".to_string()];
        scan_directory_with_extensions(&dir, &exts, false, &mut entries, &mut seen);

        assert!(entries.iter().all(|e| !e.is_folder));
        assert_eq!(entries.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_deduplicates_by_name() {
        let dir = temp_dir("ext_dedup");
        let sub1 = dir.join("a");
        let sub2 = dir.join("b");
        fs::create_dir_all(&sub1).unwrap();
        fs::create_dir_all(&sub2).unwrap();
        fs::write(sub1.join("tool.exe"), "").unwrap();
        fs::write(sub2.join("tool.exe"), "").unwrap();

        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let exts = vec![".exe".to_string()];
        scan_directory_with_extensions(&dir, &exts, false, &mut entries, &mut seen);

        let tools: Vec<&AppEntry> = entries.iter().filter(|e| e.name == "tool").collect();
        assert_eq!(tools.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_extensions_case_insensitive() {
        let dir = temp_dir("ext_case");
        fs::write(dir.join("app.EXE"), "").unwrap();

        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let exts = vec![".exe".to_string()];
        scan_directory_with_extensions(&dir, &exts, false, &mut entries, &mut seen);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "app");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_lnk_sets_is_folder_false() {
        let dir = temp_dir("lnk_flag");
        let lnk_path = dir.join("Test App.lnk");
        fs::write(&lnk_path, "").unwrap();

        let entry = parse_lnk(&lnk_path).unwrap();
        assert_eq!(entry.name, "Test App");
        assert!(!entry.is_folder);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn index_cache_bincode_roundtrip() {
        let entries = vec![
            AppEntry {
                name: "Firefox".to_string(),
                target_path: "C:\\apps\\firefox.lnk".to_string(),
                is_folder: false,
            },
            AppEntry {
                name: "Projects".to_string(),
                target_path: "C:\\Projects".to_string(),
                is_folder: true,
            },
        ];

        let cache = IndexCache {
            version: INDEX_CACHE_VERSION,
            built_at: 1700000000,
            entries: entries.clone(),
            config_hash: 12345,
        };

        let bytes = bincode::serialize(&cache).expect("serialize");
        let restored: IndexCache = bincode::deserialize(&bytes).expect("deserialize");

        assert_eq!(restored.version, INDEX_CACHE_VERSION);
        assert_eq!(restored.built_at, 1700000000);
        assert_eq!(restored.entries.len(), 2);
        assert_eq!(restored.entries[0].name, "Firefox");
        assert!(!restored.entries[0].is_folder);
        assert_eq!(restored.entries[1].name, "Projects");
        assert!(restored.entries[1].is_folder);
        assert_eq!(restored.config_hash, 12345);
    }

    #[test]
    fn config_hash_changes_with_different_paths() {
        let hash1 = compute_config_hash(&["C:\\A".to_string()], &[]);
        let hash2 = compute_config_hash(&["C:\\B".to_string()], &[]);
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn entries_equal_identical() {
        let a = vec![
            AppEntry { name: "A".into(), target_path: "C:\\a.exe".into(), is_folder: false },
            AppEntry { name: "B".into(), target_path: "C:\\b".into(), is_folder: true },
        ];
        let b = a.clone();
        assert!(entries_equal(&a, &b));
    }

    #[test]
    fn entries_equal_different_length() {
        let a = vec![
            AppEntry { name: "A".into(), target_path: "C:\\a.exe".into(), is_folder: false },
        ];
        let b = vec![
            AppEntry { name: "A".into(), target_path: "C:\\a.exe".into(), is_folder: false },
            AppEntry { name: "B".into(), target_path: "C:\\b.exe".into(), is_folder: false },
        ];
        assert!(!entries_equal(&a, &b));
    }

    #[test]
    fn entries_equal_different_name() {
        let a = vec![AppEntry { name: "A".into(), target_path: "C:\\a.exe".into(), is_folder: false }];
        let b = vec![AppEntry { name: "B".into(), target_path: "C:\\a.exe".into(), is_folder: false }];
        assert!(!entries_equal(&a, &b));
    }

    #[test]
    fn entries_equal_different_target() {
        let a = vec![AppEntry { name: "A".into(), target_path: "C:\\a.exe".into(), is_folder: false }];
        let b = vec![AppEntry { name: "A".into(), target_path: "C:\\b.exe".into(), is_folder: false }];
        assert!(!entries_equal(&a, &b));
    }

    #[test]
    fn entries_equal_different_is_folder() {
        let a = vec![AppEntry { name: "A".into(), target_path: "C:\\a".into(), is_folder: false }];
        let b = vec![AppEntry { name: "A".into(), target_path: "C:\\a".into(), is_folder: true }];
        assert!(!entries_equal(&a, &b));
    }

    #[test]
    fn entries_equal_both_empty() {
        assert!(entries_equal(&[], &[]));
    }

    #[test]
    fn config_hash_changes_with_different_scan() {
        let scan1 = vec![ScanPath {
            path: "C:\\Tools".to_string(),
            extensions: vec![".exe".to_string()],
            include_folders: false,
        }];
        let scan2 = vec![ScanPath {
            path: "C:\\Tools".to_string(),
            extensions: vec![".exe".to_string(), ".bat".to_string()],
            include_folders: false,
        }];
        let hash1 = compute_config_hash(&[], &scan1);
        let hash2 = compute_config_hash(&[], &scan2);
        assert_ne!(hash1, hash2);
    }
}
