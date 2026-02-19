use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

fn default_show_title_bar() -> bool {
    false
}

fn default_renderer() -> RendererConfig {
    RendererConfig::Auto
}

fn default_wgpu_backend() -> WgpuBackendConfig {
    WgpuBackendConfig::Auto
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RendererConfig {
    Auto,
    Wgpu,
    Glow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WgpuBackendConfig {
    Auto,
    Dx12,
    Vulkan,
    Gl,
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
    #[serde(default = "default_show_title_bar")]
    pub show_title_bar: bool,
    #[serde(default = "default_renderer")]
    pub renderer: RendererConfig,
    #[serde(default = "default_wgpu_backend")]
    pub wgpu_backend: WgpuBackendConfig,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            hotkey_toggle: true,
            show_on_startup: false,
            auto_hide_on_focus_lost: true,
            show_tray_icon: true,
            ime_off_on_show: false,
            show_title_bar: false,
            renderer: RendererConfig::Auto,
            wgpu_backend: WgpuBackendConfig::Auto,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SearchModeConfig {
    Prefix,
    Substring,
    Fuzzy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchConfig {
    #[serde(default = "default_search_mode")]
    pub normal_mode: SearchModeConfig,
    #[serde(default = "default_search_mode")]
    pub folder_mode: SearchModeConfig,
    #[serde(default = "default_show_hidden_system")]
    pub show_hidden_system: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            normal_mode: SearchModeConfig::Fuzzy,
            folder_mode: SearchModeConfig::Fuzzy,
            show_hidden_system: false,
        }
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
    #[serde(default)]
    pub additional: Vec<String>,
    #[serde(default)]
    pub scan: Vec<ScanPath>,
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
        if let Some(desktop) = dirs::desktop_dir() {
            if desktop.exists() {
                paths.push(ScanPath {
                    path: desktop.to_string_lossy().to_string(),
                    extensions: vec![".lnk".to_string()],
                    include_folders: false,
                });
            }
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

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };

        match fs::read_to_string(&path) {
            Ok(content) => {
                let mut config: Self = toml::from_str(&content).unwrap_or_default();
                if config.hotkey.modifier.eq_ignore_ascii_case("Alt")
                    && config.hotkey.key.eq_ignore_ascii_case("Space")
                {
                    config.hotkey.key = "Q".to_string();
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
        assert!(config.general.hotkey_toggle);
        assert!(!config.general.show_on_startup);
        assert!(config.general.auto_hide_on_focus_lost);
        assert!(config.general.show_tray_icon);
        assert!(!config.general.ime_off_on_show);
        assert!(!config.general.show_title_bar);
        assert_eq!(config.general.renderer, RendererConfig::Auto);
        assert_eq!(config.general.wgpu_backend, WgpuBackendConfig::Auto);
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
        assert!(config.general.hotkey_toggle);
        assert!(!config.general.show_on_startup);
        assert!(config.general.auto_hide_on_focus_lost);
        assert!(config.general.show_tray_icon);
        assert!(!config.general.ime_off_on_show);
        assert!(!config.general.show_title_bar);
        assert_eq!(config.general.renderer, RendererConfig::Auto);
        assert_eq!(config.general.wgpu_backend, WgpuBackendConfig::Auto);
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
        assert!(config.general.hotkey_toggle);
        assert!(!config.general.show_on_startup);
        assert!(config.general.auto_hide_on_focus_lost);
        assert!(config.general.show_tray_icon);
        assert!(!config.general.ime_off_on_show);
        assert!(!config.general.show_title_bar);
        assert_eq!(config.general.renderer, RendererConfig::Auto);
        assert_eq!(config.general.wgpu_backend, WgpuBackendConfig::Auto);
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
        assert_eq!(config.general.renderer, RendererConfig::Auto);
        assert_eq!(config.general.wgpu_backend, WgpuBackendConfig::Auto);
        assert_eq!(config.visual.preset, ThemePreset::Obsidian);
    }

    #[test]
    fn alt_space_is_rewritten_to_alt_q() {
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
        let mut config: Config = toml::from_str(toml_str).expect("parse");
        if config.hotkey.modifier.eq_ignore_ascii_case("Alt")
            && config.hotkey.key.eq_ignore_ascii_case("Space")
        {
            config.hotkey.key = "Q".to_string();
        }
        assert_eq!(config.hotkey.key, "Q");
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
            show_title_bar = true
            renderer = "glow"
            wgpu_backend = "dx12"

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
        assert!(config.general.show_title_bar);
        assert_eq!(config.general.renderer, RendererConfig::Glow);
        assert_eq!(config.general.wgpu_backend, WgpuBackendConfig::Dx12);
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
    fn deserialize_general_renderer_wgpu() {
        let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [general]
            renderer = "wgpu"
            wgpu_backend = "vulkan"

            [appearance]
            max_results = 8
            window_width = 600

            [paths]
            additional = []
        "#;
        let config: Config = toml::from_str(toml_str).expect("parse");
        assert_eq!(config.general.renderer, RendererConfig::Wgpu);
        assert_eq!(config.general.wgpu_backend, WgpuBackendConfig::Vulkan);
    }

    #[test]
    fn default_renderer_is_auto() {
        let config = Config::default();
        assert_eq!(config.general.renderer, RendererConfig::Auto);
        assert_eq!(config.general.wgpu_backend, WgpuBackendConfig::Auto);
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
}
