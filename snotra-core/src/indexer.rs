use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fs::Metadata;
use std::hash::{Hash, Hasher};
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM};
#[cfg(windows)]
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
};

use crate::binfmt::{deserialize_with_header, serialize_with_header};
use crate::config::{Config, ScanPath};

const INDEX_MAGIC: [u8; 4] = *b"INDX";
const INDEX_CACHE_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppEntry {
    pub name: String,
    pub target_path: String,
    pub is_folder: bool,
}

pub fn scan_all(scan_paths: &[ScanPath], show_hidden_system: bool) -> Vec<AppEntry> {
    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for sp in scan_paths {
        let ext_set = build_extension_list(&sp.extensions);
        scan_directory_with_extensions(
            Path::new(&sp.path),
            &ext_set,
            sp.include_folders,
            show_hidden_system,
            &mut entries,
            &mut seen,
        );
    }

    entries
}

/// Recursively scan for files matching given extensions, optionally including folders
fn scan_directory_with_extensions(
    dir: &Path,
    extensions: &[String],
    include_folders: bool,
    show_hidden_system: bool,
    entries: &mut Vec<AppEntry>,
    seen: &mut std::collections::HashSet<String>,
) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };

        if !show_hidden_system && !is_visible_metadata(&meta) {
            continue;
        }

        if meta.is_dir() {
            if include_folders {
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                if !name.is_empty() {
                    let path_str = path.to_string_lossy();
                    let key = normalize_entry_key(path_str.as_ref());
                    if seen.insert(key) {
                        entries.push(AppEntry {
                            name,
                            target_path: path_str.into_owned(),
                            is_folder: true,
                        });
                    }
                }
            }
            scan_directory_with_extensions(
                &path,
                extensions,
                include_folders,
                show_hidden_system,
                entries,
                seen,
            );
        } else {
            let ext = path.extension().and_then(|e| e.to_str());
            let matches = ext.is_some_and(|e| matches_extension(extensions, e));
            if matches {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                let path_str = path.to_string_lossy();
                let key = normalize_entry_key(path_str.as_ref());
                if !name.is_empty() && seen.insert(key) {
                    entries.push(AppEntry {
                        name,
                        target_path: path_str.into_owned(),
                        is_folder: false,
                    });
                }
            }
        }
    }
}

fn is_visible_metadata(meta: &Metadata) -> bool {
    let attrs = meta.file_attributes();
    let hidden = (attrs & FILE_ATTRIBUTE_HIDDEN.0) != 0;
    let system = (attrs & FILE_ATTRIBUTE_SYSTEM.0) != 0;
    !hidden && !system
}

fn normalize_entry_key(path: &str) -> String {
    let trimmed = path.trim();
    let mut normalized = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch == '/' {
            normalized.push('\\');
        } else {
            normalized.extend(ch.to_lowercase());
        }
    }
    normalized
}

fn build_extension_list(extensions: &[String]) -> Vec<String> {
    let mut normalized: Vec<String> = extensions
        .iter()
        .map(|ext| ext.trim_start_matches('.'))
        .filter(|ext| !ext.is_empty())
        .map(|ext| ext.to_ascii_lowercase())
        .collect();
    normalized.sort_unstable();
    normalized.dedup();
    normalized
}

fn matches_extension(extensions: &[String], ext: &str) -> bool {
    extensions
        .binary_search_by(|candidate| compare_ascii_lower(candidate.as_str(), ext))
        .is_ok()
}

fn compare_ascii_lower(lower: &str, raw: &str) -> std::cmp::Ordering {
    for (a, b) in lower.bytes().zip(raw.bytes()) {
        let b_lower = b.to_ascii_lowercase();
        match a.cmp(&b_lower) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
    }
    lower.len().cmp(&raw.len())
}

#[derive(Debug, Clone, Copy)]
pub struct LoadOrScanStats {
    pub cache_hit: bool,
    pub hash_ms: u128,
    pub cache_load_ms: u128,
    pub scan_ms: u128,
    pub sort_ms: u128,
    pub cache_save_ms: u128,
    pub total_ms: u128,
}

#[derive(Serialize, Deserialize)]
struct IndexCache {
    built_at: u64,
    entries: Vec<AppEntry>,
    config_hash: u64,
}

fn compute_config_hash(scan: &[ScanPath], show_hidden_system: bool) -> u64 {
    let mut hasher = DefaultHasher::new();
    for sp in scan {
        sp.path.hash(&mut hasher);
        sp.extensions.hash(&mut hasher);
        sp.include_folders.hash(&mut hasher);
    }
    show_hidden_system.hash(&mut hasher);
    hasher.finish()
}

fn cache_path() -> Option<PathBuf> {
    Config::config_dir().map(|p| p.join("index.bin"))
}

fn icon_cache_path() -> Option<PathBuf> {
    Config::config_dir().map(|p| p.join("icons.bin"))
}

fn invalidate_icon_cache_at(path: &Path) {
    let _ = std::fs::remove_file(path);
}

fn invalidate_icon_cache() {
    let Some(path) = icon_cache_path() else {
        return;
    };
    invalidate_icon_cache_at(&path);
}

/// Scan filesystem every startup; compare with cache to detect changes.
/// Returns (entries, changed) where changed=true means the entry set differs from cache.
pub fn load_or_scan(scan: &[ScanPath], show_hidden_system: bool) -> (Vec<AppEntry>, bool) {
    let (entries, changed, _) = load_or_scan_with_stats(scan, show_hidden_system);
    (entries, changed)
}

/// Same as `load_or_scan`, but also returns timing stats for startup profiling.
pub fn load_or_scan_with_stats(
    scan: &[ScanPath],
    show_hidden_system: bool,
) -> (Vec<AppEntry>, bool, LoadOrScanStats) {
    let total_started = Instant::now();

    let hash_started = Instant::now();
    let current_hash = compute_config_hash(scan, show_hidden_system);
    let hash_ms = hash_started.elapsed().as_millis();

    let cache_load_started = Instant::now();
    if let Some(cache) = load_cache(current_hash) {
        let cache_load_ms = cache_load_started.elapsed().as_millis();
        let return_entries = cache.entries;
        spawn_background_rescan(
            scan.to_vec(),
            show_hidden_system,
            current_hash,
            return_entries.clone(),
        );
        let stats = LoadOrScanStats {
            cache_hit: true,
            hash_ms,
            cache_load_ms,
            scan_ms: 0,
            sort_ms: 0,
            cache_save_ms: 0,
            total_ms: total_started.elapsed().as_millis(),
        };
        return (return_entries, false, stats);
    }
    let cache_load_ms = cache_load_started.elapsed().as_millis();

    let scan_started = Instant::now();
    let mut entries = scan_all(scan, show_hidden_system);
    let scan_ms = scan_started.elapsed().as_millis();

    let sort_started = Instant::now();
    sort_entries_canonical(&mut entries);
    let sort_ms = sort_started.elapsed().as_millis();

    let cache_save_started = Instant::now();
    save_cache_sorted(&entries, current_hash);
    let cache_save_ms = cache_save_started.elapsed().as_millis();

    let stats = LoadOrScanStats {
        cache_hit: false,
        hash_ms,
        cache_load_ms,
        scan_ms,
        sort_ms,
        cache_save_ms,
        total_ms: total_started.elapsed().as_millis(),
    };

    (entries, true, stats)
}

fn entries_equal(a: &[AppEntry], b: &[AppEntry]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).all(|(x, y)| {
        x.name == y.name && x.target_path == y.target_path && x.is_folder == y.is_folder
    })
}

fn sort_entries_canonical(entries: &mut [AppEntry]) {
    entries.sort_by(|a, b| {
        a.target_path
            .cmp(&b.target_path)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.is_folder.cmp(&b.is_folder))
    });
}

fn save_cache_sorted(entries: &[AppEntry], config_hash: u64) {
    let Some(path) = cache_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let cache = IndexCache {
        built_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        entries: entries.to_vec(),
        config_hash,
    };

    let Some(bytes) = serialize_with_header(INDEX_MAGIC, INDEX_CACHE_VERSION, &cache) else {
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
pub fn rebuild_and_save(scan: &[ScanPath], show_hidden_system: bool) -> Vec<AppEntry> {
    let mut entries = scan_all(scan, show_hidden_system);
    sort_entries_canonical(&mut entries);
    let config_hash = compute_config_hash(scan, show_hidden_system);
    save_cache_sorted(&entries, config_hash);
    entries
}

fn load_cache(config_hash: u64) -> Option<IndexCache> {
    let path = cache_path()?;
    let bytes = std::fs::read(path).ok()?;
    let cache: IndexCache = deserialize_with_header(&bytes, INDEX_MAGIC, INDEX_CACHE_VERSION)?;
    if cache.config_hash != config_hash {
        return None;
    }
    Some(cache)
}

fn spawn_background_rescan(
    scan: Vec<ScanPath>,
    show_hidden_system: bool,
    config_hash: u64,
    cached_entries: Vec<AppEntry>,
) {
    let _ = thread::Builder::new()
        .name("snotra-index-rescan".to_string())
        .spawn(move || {
            lower_current_thread_priority();
            let mut scanned = scan_all(&scan, show_hidden_system);
            sort_entries_canonical(&mut scanned);
            if !entries_equal(&cached_entries, &scanned) {
                save_cache_sorted(&scanned, config_hash);
                invalidate_icon_cache();
            }
        });
}

#[cfg(windows)]
fn lower_current_thread_priority() {
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

#[cfg(not(windows))]
fn lower_current_thread_priority() {}

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
        let exts = build_extension_list(&["exe".to_string(), "bat".to_string()]);
        scan_directory_with_extensions(&dir, &exts, false, true, &mut entries, &mut seen);

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
        let exts = build_extension_list(&["exe".to_string()]);
        scan_directory_with_extensions(&dir, &exts, true, true, &mut entries, &mut seen);

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
        let exts = build_extension_list(&["exe".to_string()]);
        scan_directory_with_extensions(&dir, &exts, false, true, &mut entries, &mut seen);

        assert!(entries.iter().all(|e| !e.is_folder));
        assert_eq!(entries.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_keeps_same_name_different_paths() {
        let dir = temp_dir("ext_dedup");
        let sub1 = dir.join("a");
        let sub2 = dir.join("b");
        fs::create_dir_all(&sub1).unwrap();
        fs::create_dir_all(&sub2).unwrap();
        fs::write(sub1.join("tool.exe"), "").unwrap();
        fs::write(sub2.join("tool.exe"), "").unwrap();

        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let exts = build_extension_list(&["exe".to_string()]);
        scan_directory_with_extensions(&dir, &exts, false, true, &mut entries, &mut seen);

        let tools: Vec<&AppEntry> = entries.iter().filter(|e| e.name == "tool").collect();
        assert_eq!(tools.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_extensions_case_insensitive() {
        let dir = temp_dir("ext_case");
        fs::write(dir.join("app.EXE"), "").unwrap();

        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let exts = build_extension_list(&["exe".to_string()]);
        scan_directory_with_extensions(&dir, &exts, false, true, &mut entries, &mut seen);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "app");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn index_cache_binary_roundtrip() {
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
            built_at: 1700000000,
            entries: entries.clone(),
            config_hash: 12345,
        };

        let bytes =
            serialize_with_header(INDEX_MAGIC, INDEX_CACHE_VERSION, &cache).expect("serialize");
        let restored: IndexCache =
            deserialize_with_header(&bytes, INDEX_MAGIC, INDEX_CACHE_VERSION).expect("deserialize");

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
        let scan1 = vec![ScanPath {
            path: "C:\\A".to_string(),
            extensions: vec![".lnk".to_string()],
            include_folders: false,
        }];
        let scan2 = vec![ScanPath {
            path: "C:\\B".to_string(),
            extensions: vec![".lnk".to_string()],
            include_folders: false,
        }];
        let hash1 = compute_config_hash(&scan1, false);
        let hash2 = compute_config_hash(&scan2, false);
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn entries_equal_identical() {
        let a = vec![
            AppEntry {
                name: "A".into(),
                target_path: "C:\\a.exe".into(),
                is_folder: false,
            },
            AppEntry {
                name: "B".into(),
                target_path: "C:\\b".into(),
                is_folder: true,
            },
        ];
        let b = a.clone();
        assert!(entries_equal(&a, &b));
    }

    #[test]
    fn entries_equal_different_length() {
        let a = vec![AppEntry {
            name: "A".into(),
            target_path: "C:\\a.exe".into(),
            is_folder: false,
        }];
        let b = vec![
            AppEntry {
                name: "A".into(),
                target_path: "C:\\a.exe".into(),
                is_folder: false,
            },
            AppEntry {
                name: "B".into(),
                target_path: "C:\\b.exe".into(),
                is_folder: false,
            },
        ];
        assert!(!entries_equal(&a, &b));
    }

    #[test]
    fn entries_equal_different_name() {
        let a = vec![AppEntry {
            name: "A".into(),
            target_path: "C:\\a.exe".into(),
            is_folder: false,
        }];
        let b = vec![AppEntry {
            name: "B".into(),
            target_path: "C:\\a.exe".into(),
            is_folder: false,
        }];
        assert!(!entries_equal(&a, &b));
    }

    #[test]
    fn entries_equal_different_target() {
        let a = vec![AppEntry {
            name: "A".into(),
            target_path: "C:\\a.exe".into(),
            is_folder: false,
        }];
        let b = vec![AppEntry {
            name: "A".into(),
            target_path: "C:\\b.exe".into(),
            is_folder: false,
        }];
        assert!(!entries_equal(&a, &b));
    }

    #[test]
    fn entries_equal_different_is_folder() {
        let a = vec![AppEntry {
            name: "A".into(),
            target_path: "C:\\a".into(),
            is_folder: false,
        }];
        let b = vec![AppEntry {
            name: "A".into(),
            target_path: "C:\\a".into(),
            is_folder: true,
        }];
        assert!(!entries_equal(&a, &b));
    }

    #[test]
    fn entries_equal_both_empty() {
        assert!(entries_equal(&[], &[]));
    }

    #[test]
    fn sorted_comparison_ignores_enumeration_order() {
        let mut a = vec![
            AppEntry {
                name: "B".into(),
                target_path: "C:\\b.exe".into(),
                is_folder: false,
            },
            AppEntry {
                name: "A".into(),
                target_path: "C:\\a.exe".into(),
                is_folder: false,
            },
        ];
        let mut b = vec![
            AppEntry {
                name: "A".into(),
                target_path: "C:\\a.exe".into(),
                is_folder: false,
            },
            AppEntry {
                name: "B".into(),
                target_path: "C:\\b.exe".into(),
                is_folder: false,
            },
        ];

        sort_entries_canonical(&mut a);
        sort_entries_canonical(&mut b);
        assert!(entries_equal(&a, &b));
    }

    #[test]
    fn canonical_sort_orders_by_target_then_name_then_is_folder() {
        let mut entries = vec![
            AppEntry {
                name: "B".into(),
                target_path: "C:\\a.exe".into(),
                is_folder: true,
            },
            AppEntry {
                name: "A".into(),
                target_path: "C:\\b.exe".into(),
                is_folder: false,
            },
            AppEntry {
                name: "A".into(),
                target_path: "C:\\a.exe".into(),
                is_folder: false,
            },
            AppEntry {
                name: "A".into(),
                target_path: "C:\\a.exe".into(),
                is_folder: true,
            },
        ];

        sort_entries_canonical(&mut entries);

        assert_eq!(entries[0].target_path, "C:\\a.exe");
        assert_eq!(entries[0].name, "A");
        assert!(!entries[0].is_folder);

        assert_eq!(entries[1].target_path, "C:\\a.exe");
        assert_eq!(entries[1].name, "A");
        assert!(entries[1].is_folder);

        assert_eq!(entries[2].target_path, "C:\\a.exe");
        assert_eq!(entries[2].name, "B");
        assert!(entries[2].is_folder);

        assert_eq!(entries[3].target_path, "C:\\b.exe");
        assert_eq!(entries[3].name, "A");
        assert!(!entries[3].is_folder);
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
        let hash1 = compute_config_hash(&scan1, false);
        let hash2 = compute_config_hash(&scan2, false);
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn invalidate_icon_cache_removes_icons_bin_if_present() {
        let dir = temp_dir("icons_cache_remove");
        let icon_path = dir.join("icons.bin");
        fs::write(&icon_path, b"dummy").unwrap();
        assert!(icon_path.exists());

        invalidate_icon_cache_at(&icon_path);
        assert!(!icon_path.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalidate_icon_cache_is_noop_when_missing() {
        let dir = temp_dir("icons_cache_missing");
        let icon_path = dir.join("icons.bin");
        assert!(!icon_path.exists());

        invalidate_icon_cache_at(&icon_path);
        assert!(!icon_path.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_all_empty_when_no_paths() {
        let entries = scan_all(&[], false);
        assert!(
            entries.is_empty(),
            "scan_all with no paths should return empty"
        );
    }
}
