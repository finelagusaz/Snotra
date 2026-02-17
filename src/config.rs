use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub hotkey: HotkeyConfig,
    pub appearance: AppearanceConfig,
    pub paths: PathsConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub modifier: String,
    pub key: String,
}

fn default_top_n_history() -> usize {
    200
}

fn default_max_history_display() -> usize {
    8
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppearanceConfig {
    pub max_results: usize,
    pub window_width: u32,
    #[serde(default = "default_top_n_history")]
    pub top_n_history: usize,
    #[serde(default = "default_max_history_display")]
    pub max_history_display: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PathsConfig {
    pub additional: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: HotkeyConfig {
                modifier: "Alt".to_string(),
                key: "Q".to_string(),
            },
            appearance: AppearanceConfig {
                max_results: 8,
                window_width: 600,
                top_n_history: 200,
                max_history_display: 8,
            },
            paths: PathsConfig {
                additional: Vec::new(),
            },
        }
    }
}

impl Config {
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
