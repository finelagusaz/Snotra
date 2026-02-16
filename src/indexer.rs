use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppEntry {
    pub name: String,
    pub target_path: String,
}

pub fn scan_all(additional_paths: &[String]) -> Vec<AppEntry> {
    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Scan standard Start Menu locations
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let user_start = PathBuf::from(appdata).join("Microsoft\\Windows\\Start Menu\\Programs");
        scan_directory(&user_start, &mut entries, &mut seen);
    }
    if let Some(programdata) = std::env::var_os("ProgramData") {
        let common_start =
            PathBuf::from(programdata).join("Microsoft\\Windows\\Start Menu\\Programs");
        scan_directory(&common_start, &mut entries, &mut seen);
    }

    // Scan Desktop
    if let Some(desktop) = dirs::desktop_dir() {
        scan_directory(&desktop, &mut entries, &mut seen);
    }

    // Scan additional paths from config
    for path in additional_paths {
        scan_directory(Path::new(path), &mut entries, &mut seen);
    }

    entries
}

fn scan_directory(dir: &Path, entries: &mut Vec<AppEntry>, seen: &mut std::collections::HashSet<String>) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_directory(&path, entries, seen);
        } else if path.extension().and_then(|e| e.to_str()) == Some("lnk") {
            if let Some(app) = parse_lnk(&path) {
                if seen.insert(app.name.to_lowercase()) {
                    entries.push(app);
                }
            }
        }
    }
}

fn parse_lnk(path: &Path) -> Option<AppEntry> {
    // Get display name from filename (without .lnk extension)
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();

    // Launching the .lnk itself avoids parsing malformed shortcut metadata.
    let target = path.to_string_lossy().to_string();

    // Skip entries without a valid target
    if target.is_empty() {
        return None;
    }

    Some(AppEntry {
        name,
        target_path: target,
    })
}
