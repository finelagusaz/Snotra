use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub use crate::error::ConfigError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenerTool {
    pub name: String,
    pub exe: String,
    #[serde(default)]
    pub args: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenerRule {
    pub target: String,
    pub tools: Vec<OpenerTool>,
}

/// パスとフォルダフラグに対してマッチするツール一覧を返す。
/// マッチするルールがなければ空スライスを返す（呼び出し側でフォールバック処理）。
pub fn find_matching_tools<'a>(
    path: &str,
    is_folder: bool,
    rules: &'a [OpenerRule],
) -> &'a [OpenerTool] {
    let path_lower = path.to_lowercase();
    let path_ext = path_lower
        .rfind('.')
        .map(|i| &path_lower[i..])
        .unwrap_or("");

    for rule in rules {
        if is_folder && rule.target == "folder" {
            return &rule.tools;
        }
        if !is_folder && rule.target.starts_with("ext:") {
            let ext_part = &rule.target["ext:".len()..];
            for raw_ext in ext_part.split(',') {
                let ext = raw_ext.trim().to_lowercase();
                let ext_with_dot = if ext.starts_with('.') {
                    ext
                } else {
                    format!(".{ext}")
                };
                if path_ext == ext_with_dot {
                    return &rule.tools;
                }
            }
        }
    }
    &[]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub hotkey: HotkeyConfig,
    #[serde(default)]
    pub general: GeneralConfig,
    pub appearance: AppearanceConfig,
    #[serde(default)]
    pub visual: VisualConfig,
    pub paths: PathsConfig,
    #[serde(default)]
    pub search: SearchConfig,
    #[serde(default)]
    pub openers: Vec<OpenerRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub modifier: String,
    pub key: String,
}

fn default_hotkey_toggle() -> bool {
    true
}

fn default_show_on_startup() -> bool {
    false
}

fn default_auto_hide_on_focus_lost() -> bool {
    true
}

fn default_show_tray_icon() -> bool {
    true
}

fn default_ime_off_on_show() -> bool {
    false
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_hotkey_toggle")]
    pub hotkey_toggle: bool,
    #[serde(default = "default_show_on_startup")]
    pub show_on_startup: bool,
    #[serde(default = "default_auto_hide_on_focus_lost")]
    pub auto_hide_on_focus_lost: bool,
    #[serde(default = "default_show_tray_icon")]
    pub show_tray_icon: bool,
    #[serde(default = "default_ime_off_on_show")]
    pub ime_off_on_show: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            hotkey_toggle: true,
            show_on_startup: false,
            auto_hide_on_focus_lost: true,
            show_tray_icon: true,
            ime_off_on_show: false,
        }
    }
}

fn default_top_n_history() -> usize {
    200
}

fn default_max_history_display() -> usize {
    8
}

fn default_show_icons() -> bool {
    true
}

fn default_search_mode() -> SearchModeConfig {
    SearchModeConfig::Fuzzy
}

fn default_show_hidden_system() -> bool {
    false
}

fn default_history_normalization() -> SearchHistoryNormalizationConfig {
    SearchHistoryNormalizationConfig::Disabled
}

fn default_fuzzy_history_cap_ratio() -> f64 {
    0.30
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SearchModeConfig {
    Prefix,
    Substring,
    Fuzzy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SearchHistoryNormalizationConfig {
    Disabled,
    FuzzyRelativeCap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchConfig {
    #[serde(default = "default_search_mode")]
    pub normal_mode: SearchModeConfig,
    #[serde(default = "default_search_mode")]
    pub folder_mode: SearchModeConfig,
    #[serde(default = "default_show_hidden_system")]
    pub show_hidden_system: bool,
    #[serde(default = "default_history_normalization")]
    pub history_normalization: SearchHistoryNormalizationConfig,
    #[serde(default = "default_fuzzy_history_cap_ratio")]
    pub fuzzy_history_cap_ratio: f64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            normal_mode: SearchModeConfig::Fuzzy,
            folder_mode: SearchModeConfig::Fuzzy,
            show_hidden_system: false,
            history_normalization: SearchHistoryNormalizationConfig::Disabled,
            fuzzy_history_cap_ratio: default_fuzzy_history_cap_ratio(),
        }
    }
}

impl SearchConfig {
    #[deprecated(note = "use Config::validate() to detect issues instead")]
    pub fn sanitize(&mut self) -> bool {
        if self.fuzzy_history_cap_ratio.is_finite()
            && (0.0..=1.0).contains(&self.fuzzy_history_cap_ratio)
        {
            return false;
        }

        self.fuzzy_history_cap_ratio = default_fuzzy_history_cap_ratio();
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppearanceConfig {
    pub max_results: usize,
    pub window_width: u32,
    #[serde(default = "default_top_n_history")]
    pub top_n_history: usize,
    #[serde(default = "default_max_history_display")]
    pub max_history_display: usize,
    #[serde(default = "default_show_icons")]
    pub show_icons: bool,
}

fn default_theme_preset() -> ThemePreset {
    ThemePreset::Obsidian
}

fn default_background_color() -> String {
    "#282828".to_string()
}

fn default_input_background_color() -> String {
    "#383838".to_string()
}

fn default_text_color() -> String {
    "#E0E0E0".to_string()
}

fn default_selected_row_color() -> String {
    "#505050".to_string()
}

fn default_hint_text_color() -> String {
    "#808080".to_string()
}

fn default_font_family() -> String {
    "Segoe UI".to_string()
}

fn default_font_size() -> u32 {
    15
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreset {
    Obsidian,
    Paper,
    Solarized,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualConfig {
    #[serde(default = "default_theme_preset")]
    pub preset: ThemePreset,
    #[serde(default = "default_background_color")]
    pub background_color: String,
    #[serde(default = "default_input_background_color")]
    pub input_background_color: String,
    #[serde(default = "default_text_color")]
    pub text_color: String,
    #[serde(default = "default_selected_row_color")]
    pub selected_row_color: String,
    #[serde(default = "default_hint_text_color")]
    pub hint_text_color: String,
    #[serde(default = "default_font_family")]
    pub font_family: String,
    #[serde(default = "default_font_size")]
    pub font_size: u32,
}

impl Default for VisualConfig {
    fn default() -> Self {
        Self {
            preset: ThemePreset::Obsidian,
            background_color: default_background_color(),
            input_background_color: default_input_background_color(),
            text_color: default_text_color(),
            selected_row_color: default_selected_row_color(),
            hint_text_color: default_hint_text_color(),
            font_family: default_font_family(),
            font_size: default_font_size(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanPath {
    pub path: String,
    pub extensions: Vec<String>,
    #[serde(default)]
    pub include_folders: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

fn normalize_scan_path_key(path: &str) -> String {
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

fn normalize_extensions(exts: &[String]) -> Vec<String> {
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

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: HotkeyConfig {
                modifier: "Alt".to_string(),
                key: "Q".to_string(),
            },
            general: GeneralConfig::default(),
            appearance: AppearanceConfig {
                max_results: 8,
                window_width: 600,
                top_n_history: 200,
                max_history_display: 8,
                show_icons: true,
            },
            visual: VisualConfig::default(),
            paths: PathsConfig {
                additional: Vec::new(),
                scan: Self::default_scan_paths(),
            },
            search: SearchConfig::default(),
            openers: Vec::new(),
        }
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

    /// Returns true if this is the first run (no config file exists yet).
    /// Must be called before `Config::load()` since load() creates the file.
    pub fn is_first_run() -> bool {
        match Self::config_path() {
            Some(path) => !path.exists(),
            None => true,
        }
    }

    pub fn config_dir() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("Snotra"))
    }

    pub fn config_path() -> Option<PathBuf> {
        Self::config_dir().map(|p| p.join("config.toml"))
    }

    /// Migrate legacy `paths.additional` entries into `paths.scan` with `.lnk` extension.
    fn migrate_additional_to_scan(&mut self) {
        if self.paths.additional.is_empty() {
            return;
        }
        let lnk = ".lnk".to_string();
        for path in self.paths.additional.drain(..) {
            let key = path.to_lowercase();
            if let Some(existing) = self
                .paths
                .scan
                .iter_mut()
                .find(|sp| sp.path.to_lowercase() == key)
            {
                // Same directory already in scan — merge .lnk into its extensions
                if !existing
                    .extensions
                    .iter()
                    .any(|e| e.eq_ignore_ascii_case(&lnk))
                {
                    existing.extensions.push(lnk.clone());
                }
            } else {
                self.paths.scan.push(ScanPath {
                    path,
                    extensions: vec![lnk.clone()],
                    include_folders: false,
                });
            }
        }
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };

        match fs::read_to_string(&path) {
            Ok(content) => {
                let mut config: Self = toml::from_str(&content).unwrap_or_default();
                let mut needs_save = false;
                if !config.paths.additional.is_empty() {
                    config.migrate_additional_to_scan();
                    needs_save = true;
                }
                #[allow(deprecated)]
                if config.search.sanitize() {
                    needs_save = true;
                }
                if config.paths.normalize_scan_paths() {
                    needs_save = true;
                }
                if needs_save {
                    config.save();
                }
                config
            }
            Err(_) => {
                let config = Self::default();
                config.save();
                config
            }
        }
    }

    pub fn save(&self) {
        let Some(dir) = Self::config_dir() else {
            return;
        };
        let _ = fs::create_dir_all(&dir);

        let Some(path) = Self::config_path() else {
            return;
        };
        if let Ok(content) = toml::to_string_pretty(self) {
            let _ = fs::write(path, content);
        }
    }

    /// Validates config consistency. Call before save.
    pub fn validate(&self) -> Vec<ConfigError> {
        let mut errors = Vec::new();

        // Hotkey validation
        if self.hotkey.modifier.trim().is_empty() {
            errors.push(ConfigError::HotkeyModifierEmpty);
        }
        if self.hotkey.key.trim().is_empty() {
            errors.push(ConfigError::HotkeyKeyEmpty);
        }

        // Appearance validation
        if self.appearance.max_results == 0 {
            errors.push(ConfigError::MaxResultsZero);
        }
        if self.appearance.window_width < 200 {
            errors.push(ConfigError::WindowWidthTooSmall(self.appearance.window_width));
        }

        // Search validation
        let ratio = self.search.fuzzy_history_cap_ratio;
        if !(0.0..=1.0).contains(&ratio) {
            errors.push(ConfigError::FuzzyCapRatioOutOfRange { value: ratio });
        }

        // Paths validation
        for (i, scan_path) in self.paths.scan.iter().enumerate() {
            if scan_path.path.trim().is_empty() {
                errors.push(ConfigError::ScanPathEmpty { index: i });
            }
        }

        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let config: Config = toml::from_str(toml_str).expect("parse");
        assert_eq!(config.hotkey.modifier, "Ctrl");
        assert_eq!(config.hotkey.key, "Space");
        assert_eq!(config.appearance.max_results, 10);
        assert_eq!(config.appearance.window_width, 700);
        assert_eq!(config.appearance.top_n_history, 150);
        assert_eq!(config.appearance.max_history_display, 5);
        assert_eq!(config.paths.additional, vec!["C:\\Tools"]);
        assert!(config.paths.scan.is_empty());
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
        assert_eq!(config.appearance.top_n_history, 200);
        assert_eq!(config.appearance.max_history_display, 8);
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

    #[test]
    fn default_config_has_expected_values() {
        let config = Config::default();
        assert_eq!(config.hotkey.modifier, "Alt");
        assert_eq!(config.hotkey.key, "Q");
        assert_eq!(config.appearance.max_results, 8);
        assert_eq!(config.appearance.window_width, 600);
        assert_eq!(config.appearance.top_n_history, 200);
        assert_eq!(config.appearance.max_history_display, 8);
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
    fn default_scan_paths_have_lnk_extension() {
        let paths = Config::default_scan_paths();
        for sp in &paths {
            assert_eq!(sp.extensions, vec![".lnk"]);
            assert!(!sp.include_folders);
        }
    }

    #[test]
    fn is_first_run_returns_true_when_no_config() {
        // This test relies on Config::config_path() returning a valid path
        // We can't easily test is_first_run without side effects,
        // but we can verify the method exists and returns a bool
        let _result: bool = Config::is_first_run();
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
    #[allow(deprecated)]
    fn sanitize_invalid_fuzzy_history_cap_ratio() {
        let mut config = SearchConfig {
            normal_mode: SearchModeConfig::Fuzzy,
            folder_mode: SearchModeConfig::Fuzzy,
            show_hidden_system: false,
            history_normalization: SearchHistoryNormalizationConfig::FuzzyRelativeCap,
            fuzzy_history_cap_ratio: 1.5,
        };

        assert!(config.sanitize());
        assert!((config.fuzzy_history_cap_ratio - 0.30).abs() < f64::EPSILON);
    }

    #[test]
    fn validate_default_config_returns_no_errors() {
        let config = Config::default();
        let errors = config.validate();
        assert!(errors.is_empty(), "default config should have no validation errors");
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
    fn validate_max_results_zero() {
        let mut config = Config::default();
        config.appearance.max_results = 0;
        let errors = config.validate();
        assert!(errors.contains(&ConfigError::MaxResultsZero));
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
            !errors.iter().any(|e| matches!(e, ConfigError::WindowWidthTooSmall(_))),
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
            !errors.iter().any(|e| matches!(e, ConfigError::FuzzyCapRatioOutOfRange { .. })),
            "ratio=0.0 should not produce an error"
        );
    }

    #[test]
    fn validate_fuzzy_cap_ratio_boundary_1_is_ok() {
        let mut config = Config::default();
        config.search.fuzzy_history_cap_ratio = 1.0;
        let errors = config.validate();
        assert!(
            !errors.iter().any(|e| matches!(e, ConfigError::FuzzyCapRatioOutOfRange { .. })),
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

    // ---- find_matching_tools tests ----

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
    fn find_matching_tools_folder_target() {
        let rules = vec![
            make_rule("folder", &[("TC", "TOTALCMD64.EXE", "/O /T")]),
            make_rule("ext:png,jpg", &[("IrfanView", "i_view64.exe", "")]),
        ];
        let tools = find_matching_tools("C:\\Projects", true, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "TC");
    }

    #[test]
    fn find_matching_tools_ext_target_with_dot() {
        let rules = vec![make_rule("ext:.png,jpg", &[("IrfanView", "i_view64.exe", "")])];
        let tools = find_matching_tools("C:\\image.PNG", false, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "IrfanView");
    }

    #[test]
    fn find_matching_tools_ext_target_without_dot() {
        let rules = vec![make_rule("ext:png,jpg,gif", &[("IrfanView", "i_view64.exe", "")])];
        let tools = find_matching_tools("C:\\photo.jpg", false, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "IrfanView");
    }

    #[test]
    fn find_matching_tools_no_match_returns_empty() {
        let rules = vec![
            make_rule("folder", &[("TC", "TOTALCMD64.EXE", "")]),
            make_rule("ext:png,jpg", &[("IrfanView", "i_view64.exe", "")]),
        ];
        let tools = find_matching_tools("C:\\doc.pdf", false, &rules);
        assert!(tools.is_empty());
    }

    #[test]
    fn find_matching_tools_file_does_not_match_folder_rule() {
        let rules = vec![make_rule("folder", &[("TC", "TOTALCMD64.EXE", "")])];
        let tools = find_matching_tools("C:\\file.exe", false, &rules);
        assert!(tools.is_empty());
    }

    #[test]
    fn find_matching_tools_folder_does_not_match_ext_rule() {
        let rules = vec![make_rule("ext:png", &[("IrfanView", "i_view64.exe", "")])];
        let tools = find_matching_tools("C:\\MyFolder", true, &rules);
        assert!(tools.is_empty());
    }

    #[test]
    fn find_matching_tools_multiple_rules_first_wins() {
        let rules = vec![
            make_rule("ext:png", &[("Tool1", "tool1.exe", "")]),
            make_rule("ext:png,jpg", &[("Tool2", "tool2.exe", "")]),
        ];
        let tools = find_matching_tools("C:\\image.png", false, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "Tool1");
    }

    #[test]
    fn find_matching_tools_multiple_tools_in_rule() {
        let rules = vec![make_rule(
            "folder",
            &[("TC", "TOTALCMD64.EXE", ""), ("Explorer", "explorer.exe", "")],
        )];
        let tools = find_matching_tools("C:\\Projects", true, &rules);
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "TC");
        assert_eq!(tools[1].name, "Explorer");
    }

    #[test]
    fn find_matching_tools_case_insensitive_ext() {
        let rules = vec![make_rule("ext:PNG,JPG", &[("IrfanView", "i_view64.exe", "")])];
        let tools = find_matching_tools("C:\\Photo.png", false, &rules);
        assert_eq!(tools.len(), 1);
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
    }

    #[test]
    fn validate_multiple_errors_all_reported() {
        let mut config = Config::default();
        config.hotkey.modifier = "".to_string();
        config.hotkey.key = "".to_string();
        config.appearance.max_results = 0;
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
        assert!(errors.contains(&ConfigError::MaxResultsZero));
        assert!(errors.contains(&ConfigError::WindowWidthTooSmall(50)));
        assert!(errors.contains(&ConfigError::FuzzyCapRatioOutOfRange { value: 2.0 }));
        assert!(errors.contains(&ConfigError::ScanPathEmpty { index: 0 }));
        assert_eq!(errors.len(), 6, "all 6 errors should be reported");
    }
}
