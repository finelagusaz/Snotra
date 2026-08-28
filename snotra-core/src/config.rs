//! `%APPDATA%\Snotra\config.toml` の集約型 [`Config`] と、その扱いを責務ごとに分けた
//! 子モジュールの索引。
//!
//! **crate の外から見える名前はここの re-export が決める。** 子モジュールは private ゆえ、
//! `snotra_core::config::X` という呼び出しの形はこのファイルを通って保たれる。子モジュールの
//! 責務はそれぞれの `//!` が正本であり、ここでは数え上げない（下の `mod` 宣言が一覧である）。
//!
//! ここに置くのは集約型 [`Config`] とその [`Default`] だけである——セクションごとの型は
//! [`schema`]、それを読み書き・移行・検証する側はそれぞれの子モジュールが持つ。

use serde::{Deserialize, Serialize};

mod io;
mod location;
mod migrate;
mod paths;
mod schema;
mod validate;

#[cfg(test)]
mod tests;

pub use crate::error::ConfigError;
// ホットキーの永続型は `hotkey.rs` が文字列 parser・意味型とともに所有する。
// re-export で既存の `snotra_core::config::HotkeyConfig` パスを維持する。
pub use crate::hotkey::HotkeyConfig;
// opener マッチングエンジン・プリセット検出は `opener.rs` に分離済み（issue #435）。
// `OpenerRule`/`OpenerTool` は `Config.openers` として config.toml に紐づく serde 型のため、
// re-export で既存の呼び出し元（`snotra_core::config::...` パス、src-tauri/snotra-settings 含む）を壊さない。
pub use crate::opener::{
    OpenerPreset, OpenerRule, OpenerTool, detect_opener_presets, extract_ext_part,
    extract_path_condition, find_matching_tools, is_preset_already_added, normalize_openers,
    opener_specificity_order,
};
pub use io::LoadOutcome;
pub use paths::{PathsConfig, ScanPath, dedup_scan_paths};
// `pub(crate)`: opener ターゲットの正規化（`opener.rs::normalize_opener_target`）が共有する。
pub(crate) use paths::{normalize_extensions, normalize_scan_path_key};
pub use schema::{
    AppearanceConfig, AutoUpdateMode, CustomTheme, GeneralConfig, InstantAction, InstantCommand,
    Language, SearchConfig, SearchHistoryNormalizationConfig, SearchModeConfig, ThemePreset,
    VisualConfig,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub hotkey: HotkeyConfig,
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub appearance: AppearanceConfig,
    #[serde(default)]
    pub visual: VisualConfig,
    #[serde(default)]
    pub paths: PathsConfig,
    #[serde(default)]
    pub search: SearchConfig,
    #[serde(default)]
    pub openers: Vec<OpenerRule>,
    #[serde(default)]
    pub instant_commands: Vec<InstantCommand>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: HotkeyConfig::default(),
            general: GeneralConfig::default(),
            appearance: AppearanceConfig::default(),
            visual: VisualConfig::default(),
            // `scan` のシードを撒くのは**ここだけ**である（`PathsConfig::default()` は空）。
            // 「設定ファイルが無い / 読めない」ときに何も索引できないのを避けるためのシードで
            // あって、parse 経路の既定ではない——両者を揃えると `[paths]` を書いたか否かで
            // 未指定 `scan` の値が変わり、#795 が塞いだ乖離クラスが復活する（#824）。
            paths: PathsConfig {
                scan: Self::default_scan_paths(),
                ..Default::default()
            },
            search: SearchConfig::default(),
            openers: Vec::new(),
            instant_commands: vec![
                InstantCommand {
                    name: "g".to_string(),
                    description: "Google 検索".to_string(),
                    action: InstantAction::Url {
                        url: "https://www.google.com/search?q={query}".to_string(),
                    },
                },
                InstantCommand {
                    name: "gh".to_string(),
                    description: "GitHub 検索".to_string(),
                    action: InstantAction::Url {
                        url: "https://github.com/search?q={query}".to_string(),
                    },
                },
            ],
        }
    }
}
