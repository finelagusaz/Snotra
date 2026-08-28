//! 探索パス（`[paths]`）の型と、そのキー正規化・重複マージ。
//!
//! 正規化の 2 つ（[`normalize_scan_path_key`] / [`normalize_extensions`]）は
//! `crate::opener::normalize_opener_target` が opener ターゲットの正規化として共有するため
//! `pub(crate)` で出す（依存の向きの取り決めは `snotra-core/CLAUDE.md` の `opener.rs` 節）。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::Config;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanPath {
    pub path: String,
    pub extensions: Vec<String>,
    #[serde(default)]
    pub include_folders: bool,
}

/// `PathsConfig::default()` は **`scan` が空**である。既定の探索パス
/// （`Config::default_scan_paths()`）を撒くのは `Config::default()` だけで、
/// これは「設定ファイルが無い / 読めない」ときのシードだからである。
/// `[paths]` セクションを省いた TOML と `scan` キーを省いた TOML はどちらも
/// この既定へ落ちる——2 経路の既定を一致させるための非対称である（#824）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathsConfig {
    #[serde(default, skip_serializing)]
    pub additional: Vec<String>,
    #[serde(default)]
    pub scan: Vec<ScanPath>,
}

fn is_drive_root(path: &str) -> bool {
    let b = path.as_bytes();
    b.len() == 3 && b[1] == b':' && b[2] == b'\\'
}

// `pub(crate)`: opener ターゲットのパス正規化（`opener.rs::normalize_opener_target`）が共有する。
pub(crate) fn normalize_scan_path_key(path: &str) -> String {
    let mut key = path.trim().replace('/', "\\").to_lowercase();
    if key.ends_with('\\') && !is_drive_root(&key) {
        let trimmed_len = key.trim_end_matches('\\').len();
        key.truncate(trimmed_len);
    }
    key
}

fn normalize_extension(ext: &str) -> String {
    let trimmed = ext.trim().trim_start_matches('.');
    if trimmed.is_empty() {
        return String::new();
    }
    format!(".{}", trimmed.to_lowercase())
}

// `pub(crate)`: opener ターゲットの拡張子リスト正規化（`opener.rs::normalize_opener_target`）が共有する。
pub(crate) fn normalize_extensions(exts: &[String]) -> Vec<String> {
    let mut result: Vec<String> = exts
        .iter()
        .map(|e| normalize_extension(e))
        .filter(|e| !e.is_empty())
        .collect();
    result.sort();
    result.dedup();
    result
}

pub fn dedup_scan_paths(scan: &[ScanPath]) -> Vec<ScanPath> {
    let mut result: Vec<ScanPath> = Vec::new();
    let mut keys: Vec<String> = Vec::new();

    for sp in scan {
        let trimmed = sp.path.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = normalize_scan_path_key(trimmed);
        let exts = normalize_extensions(&sp.extensions);

        if let Some(pos) = keys.iter().position(|k| k == &key) {
            let existing = &mut result[pos];
            for ext in &exts {
                if !existing.extensions.iter().any(|e| e == ext) {
                    existing.extensions.push(ext.clone());
                }
            }
            existing.extensions.sort();
            existing.include_folders |= sp.include_folders;
        } else {
            keys.push(key);
            result.push(ScanPath {
                path: trimmed.to_string(),
                extensions: exts,
                include_folders: sp.include_folders,
            });
        }
    }

    result
}

impl PathsConfig {
    pub fn normalize_scan_paths(&mut self) -> bool {
        let normalized = dedup_scan_paths(&self.scan);
        if normalized != self.scan {
            self.scan = normalized;
            return true;
        }
        false
    }
}

impl Config {
    /// Returns the default scan paths (common Start Menu + Desktop).
    /// User Start Menu is intentionally excluded.
    pub fn default_scan_paths() -> Vec<ScanPath> {
        let mut paths = Vec::new();

        // Common Start Menu Programs (.lnk)
        if let Some(programdata) = std::env::var_os("ProgramData") {
            let common_start =
                PathBuf::from(programdata).join("Microsoft\\Windows\\Start Menu\\Programs");
            if common_start.exists() {
                paths.push(ScanPath {
                    path: common_start.to_string_lossy().to_string(),
                    extensions: vec![".lnk".to_string()],
                    include_folders: false,
                });
            }
        }

        // Desktop (.lnk)
        if let Some(desktop) = dirs::desktop_dir()
            && desktop.exists()
        {
            paths.push(ScanPath {
                path: desktop.to_string_lossy().to_string(),
                extensions: vec![".lnk".to_string()],
                include_folders: false,
            });
        }

        paths
    }
}

#[cfg(test)]
mod tests;
