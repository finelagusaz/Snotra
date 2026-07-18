//! `%APPDATA%\Snotra\config.toml` の読込・保存と既定値補完。
//!
//! `Config` 型と、`Language` enum（`Ja` / `En`）・OS 言語自動判定 `default_language()` を定義。
//! デシリアライズ後の後処理（レガシーキー移行・正規化）は `apply_migrations()` に集約し、
//! `Config` を作る全経路が通す前提で書く。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub use crate::error::ConfigError;
// opener マッチングエンジン・プリセット検出は `opener.rs` に分離済み（issue #435）。
// `OpenerRule`/`OpenerTool` は `Config.openers` として config.toml に紐づく serde 型のため、
// re-export で既存の呼び出し元（`snotra_core::config::...` パス、src-tauri/snotra-settings 含む）を壊さない。
pub use crate::opener::{
    detect_opener_presets, extract_ext_part, extract_path_condition, find_matching_tools,
    is_preset_already_added, normalize_openers, opener_specificity_order, OpenerPreset,
    OpenerRule, OpenerTool,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Ja,
    En,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutoUpdateMode {
    #[default]
    Full,       // チェック + インストール（インストーラー版向け）
    CheckOnly,  // チェックのみ・通知する（ポータブル版向け）
    Disabled,   // チェックしない
}

/// `Config::load_reporting()` の結果区分。UI 文字列を持たない（表示・通知は呼び出し側の責務）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadOutcome {
    /// 正常に parse できた。
    Loaded,
    /// 設定ファイルが存在せず（first-run）、既定値を生成・保存した。
    FirstRun,
    /// 内容が壊れていた（TOML parse 失敗 or 非 UTF-8）。`config.toml.bak` へ退避し既定値で起動。
    RecoveredFromCorrupt,
    /// 一時的・環境的な read 失敗（権限/ロック等）。既存ファイルを退避も上書きもせず既定値で起動。
    ReadFailed,
}

/// OS ロケール（`sys-locale`）から既定言語を判定する。
/// `ja` で始まるロケールは日本語、それ以外は英語にフォールバックする。
/// ロケールを取得できないときも英語にフォールバックする。
fn default_language() -> Language {
    sys_locale::get_locale()
        .map(|l| {
            if l.starts_with("ja") {
                Language::Ja
            } else {
                Language::En
            }
        })
        .unwrap_or(Language::En)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstantCommand {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(flatten)]
    pub action: InstantAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InstantAction {
    Url { url: String },
    Exec {
        exe: String,
        #[serde(default)]
        args: String,
    },
    Legacy { command: String },
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
    #[serde(default)]
    pub instant_commands: Vec<InstantCommand>,
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

fn default_follow_cursor_monitor() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_language")]
    pub language: Language,
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
    #[serde(default = "default_follow_cursor_monitor")]
    pub follow_cursor_monitor: bool,
    #[serde(default)]
    pub auto_update: AutoUpdateMode,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            language: default_language(),
            hotkey_toggle: true,
            show_on_startup: false,
            auto_hide_on_focus_lost: true,
            show_tray_icon: true,
            ime_off_on_show: false,
            follow_cursor_monitor: true,
            auto_update: AutoUpdateMode::Full,
        }
    }
}

fn default_result_limit() -> usize {
    200
}

fn default_recent_limit() -> usize {
    8
}

fn default_visible_rows() -> usize {
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

fn default_instant_command_prefix() -> String {
    "@".to_string()
}

fn default_migemo_min_chars() -> usize {
    2
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
    #[serde(default = "default_instant_command_prefix")]
    pub instant_command_prefix: String,
    /// ローマ字入力でかな名ファイルを検索する（migemo 風検索）。
    /// デフォルト off: "a" → "あ" のような意図しないマッチを防ぐため、
    /// ユーザーが明示的に有効化する設計。
    #[serde(default)]
    pub migemo_enabled: bool,
    /// migemo 検索を有効にするクエリの最小文字数。
    /// 1文字（"a"→"あ"）の意図しないマッチを防ぐため、デフォルト 2。
    #[serde(default = "default_migemo_min_chars")]
    pub migemo_min_chars: usize,
    /// 検索・フォルダの結果リスト最大長（materialize 件数・スクロール対象。fetch_limit）。
    /// `None` = 未設定（実効値は `effective_result_limit()` で取得）。
    /// `Option` にすることで「TOML に明示されたか否か」を区別し、
    /// レガシー（`top_n_history` / `[appearance]`）からの移行時に明示値を上書きしない。
    #[serde(default)]
    pub result_limit: Option<usize>,
    /// 空クエリ時の recent リスト件数（UI 表示件数）。
    /// `None` = 未設定（実効値は `effective_recent_limit()` で取得）。
    #[serde(default)]
    pub recent_limit: Option<usize>,
    /// Legacy: 旧キー `[search].top_n_history`。`apply_migrations()` で `result_limit` へ移行し、
    /// 書き戻さない（`skip_serializing`）。新コードは `result_limit` を読む。
    #[serde(default, skip_serializing)]
    pub top_n_history: Option<usize>,
    /// Legacy: 旧キー `[search].max_history_display`。`apply_migrations()` で `recent_limit` へ移行。
    /// 書き戻さない（`skip_serializing`）。新コードは `recent_limit` を読む。
    #[serde(default, skip_serializing)]
    pub max_history_display: Option<usize>,
    /// ユーザー PATH 環境変数の実行ファイルを検索対象に含めるか。
    #[serde(default)]
    pub include_path_env: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            normal_mode: SearchModeConfig::Fuzzy,
            folder_mode: SearchModeConfig::Fuzzy,
            show_hidden_system: false,
            history_normalization: SearchHistoryNormalizationConfig::Disabled,
            fuzzy_history_cap_ratio: default_fuzzy_history_cap_ratio(),
            instant_command_prefix: default_instant_command_prefix(),
            migemo_enabled: false,
            migemo_min_chars: default_migemo_min_chars(),
            result_limit: None,
            recent_limit: None,
            top_n_history: None,
            max_history_display: None,
            include_path_env: false,
        }
    }
}

impl SearchConfig {
    /// 検索・フォルダの結果リスト最大長。`result_limit` が未設定ならデフォルト値を返す。
    pub fn effective_result_limit(&self) -> usize {
        self.result_limit.unwrap_or_else(default_result_limit)
    }

    /// 空クエリ時の recent リスト件数。`recent_limit` が未設定ならデフォルト値を返す。
    pub fn effective_recent_limit(&self) -> usize {
        self.recent_limit.unwrap_or_else(default_recent_limit)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppearanceConfig {
    /// ウィンドウ可視行数。`None` = 未設定（実効値は `effective_visible_rows()` で取得）。
    /// `Option` にすることで「TOML に明示されたか否か」を区別し、レガシー `max_results` からの
    /// 移行時に明示値を上書きしない。
    #[serde(default)]
    pub visible_rows: Option<usize>,
    pub window_width: u32,
    #[serde(default = "default_show_icons")]
    pub show_icons: bool,
    /// Legacy: 旧キー `[appearance].max_results`。`apply_migrations()` で `visible_rows` へ移行し、
    /// 書き戻さない（`skip_serializing`）。新コードは `visible_rows` を読む。
    #[serde(default, skip_serializing)]
    pub max_results: Option<usize>,
    /// Legacy: 最古キー `[appearance].top_n_history`。`apply_migrations()` で `SearchConfig.result_limit`
    /// へ移行し、書き戻さない（`skip_serializing`）。新コードは `SearchConfig.result_limit` を読む。
    #[serde(default, skip_serializing)]
    pub top_n_history: Option<usize>,
    /// Legacy: 最古キー `[appearance].max_history_display`。`apply_migrations()` で
    /// `SearchConfig.recent_limit` へ移行し、書き戻さない（`skip_serializing`）。
    #[serde(default, skip_serializing)]
    pub max_history_display: Option<usize>,
}

impl AppearanceConfig {
    /// ウィンドウ可視行数。`visible_rows` が未設定ならデフォルト値を返す。
    pub fn effective_visible_rows(&self) -> usize {
        self.visible_rows.unwrap_or_else(default_visible_rows)
    }
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
    Monokai,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomTheme {
    pub background_color: String,
    pub input_background_color: String,
    pub text_color: String,
    pub selected_row_color: String,
    pub hint_text_color: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_theme: Option<CustomTheme>,
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
            custom_theme: None,
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

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: HotkeyConfig {
                modifier: "Alt".to_string(),
                key: "Q".to_string(),
            },
            general: GeneralConfig::default(),
            appearance: AppearanceConfig {
                visible_rows: None,
                window_width: 600,
                show_icons: true,
                max_results: None,
                top_n_history: None,
                max_history_display: None,
            },
            visual: VisualConfig::default(),
            paths: PathsConfig {
                additional: Vec::new(),
                scan: Self::default_scan_paths(),
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

/// アイコンキャッシュが保持する「結果リスト何本分か」。表示ワーキングセットへの倍率。
/// 1 より大きくすることで、検索を切り替えても直近数本分のアイコンを残し再抽出を抑える。
const ICON_CACHE_RETENTION_FACTOR: usize = 5;

impl Config {
    /// アイコンキャッシュ（常駐メモリ + `icons.bin`）の最大保持件数（派生値）。
    ///
    /// 表示ワーキングセット = アイコンを要求しうる結果リストの最大長
    /// `max(visible_rows, result_limit, recent_limit)`
    /// （検索・フォルダ = `result_limit`、空クエリ recent = `recent_limit`、
    /// 可視行 = `visible_rows`。フロント `LruIconCache` サイズも `result_limit` に一致）の
    /// `ICON_CACHE_RETENTION_FACTOR` 倍。
    ///
    /// 独立した設定キーを持たず派生値とすることで「上限 ≥ ワーキングセット」が**構造的に成立**し
    /// （検証・floor・drift が不要）、`result_limit` 変更時は上限も自動追従する。単一の
    /// `get_icons_batch` が自己 evict することはない。倍率で再検索時の再抽出を抑えつつ無制限増加を止める。
    ///
    /// 保守注意: ここの `working_set` は engine がアイコンを要求する結果リストの fetch 上限と対応する
    /// （`Engine::search` / `capture_folder_list_context` = `effective_result_limit`、
    /// `recent_history` = `effective_recent_limit`）。engine 側でアイコンを要求する新たな
    /// 長尺リスト経路を増やしたら、その上限をこの式にも反映すること。
    pub fn icon_cache_cap(&self) -> usize {
        let working_set = self
            .appearance
            .effective_visible_rows()
            .max(self.search.effective_result_limit())
            .max(self.search.effective_recent_limit());
        working_set.max(1).saturating_mul(ICON_CACHE_RETENTION_FACTOR)
    }

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

    /// Apply post-load migrations: legacy field migration, normalization, system shortcut fallback.
    /// Returns true if any changes were applied.
    /// Called by `load()` (auto-save on change) and import (caller decides when to save).
    pub fn apply_migrations(&mut self) -> bool {
        // 呼び出し順は挙動不変のため元のまま固定する。(1)→(5) は真の順序依存
        // （additional→scan で追加された scan エントリを scan 正規化がまとめて dedup する必要がある）。
        // それ以外は独立だが、diff 最小化のため元の並びを保つ。
        let mut changed = false;
        changed |= self.migrate_legacy_additional_paths(); // (1) additional → scan（(5) の normalize より先）
        changed |= self.migrate_legacy_count_params(); // (2) #388 改名マイグレーション
        self.resolve_count_param_defaults(); // (3) None → Some(default)。(2) より後（補完前提）
        changed |= self.sanitize_fuzzy_history_cap_ratio(); // (4) 範囲外値の補正（#437）
        changed |= self.paths.normalize_scan_paths(); // (5) scan path dedup・正規化。(1) より後必須
        changed |= self.normalize_openers(); // opener ターゲットの正規化・具体度ソート
        changed |= self.migrate_instant_legacy_commands(); // 旧 `command` 単一文字列 → `Url`
        changed |= self.fallback_hotkey_if_system_shortcut(); // system shortcut 検出時のデフォルト復帰
        changed
    }

    /// (1) 旧 `paths.additional` を `paths.scan` へ移行する（`.lnk` 拡張子付き）。
    fn migrate_legacy_additional_paths(&mut self) -> bool {
        if self.paths.additional.is_empty() {
            return false;
        }
        self.migrate_additional_to_scan();
        true
    }

    /// (2) 件数 config キーの改名マイグレーション（#388）。各新フィールドへ legacy を集約する。
    /// take() で legacy 層を常にクリアし、新フィールドが None（= 新キー未明示）のときだけ
    /// get_or_insert で補完する（新キーが明示されていれば上書きしない＝新優先）。
    fn migrate_legacy_count_params(&mut self) -> bool {
        let mut changed = false;
        // visible_rows ← [appearance].max_results（1層）
        if let Some(v) = self.appearance.max_results.take() {
            self.appearance.visible_rows.get_or_insert(v);
            changed = true;
        }
        // result_limit ← [search].top_n_history（中間）> [appearance].top_n_history（最古）。
        // 両 legacy 層を take() で常にクリアし、search 側を優先する（.or は両引数を評価する）。
        if let Some(v) = self
            .search
            .top_n_history
            .take()
            .or(self.appearance.top_n_history.take())
        {
            self.search.result_limit.get_or_insert(v);
            changed = true;
        }
        // recent_limit ← [search].max_history_display（中間）> [appearance].max_history_display（最古）
        if let Some(v) = self
            .search
            .max_history_display
            .take()
            .or(self.appearance.max_history_display.take())
        {
            self.search.recent_limit.get_or_insert(v);
            changed = true;
        }
        changed
    }

    /// (3) 件数 legacy 移行より後で None → Some(default) に解決する。apply_migrations() 呼び出し後は
    /// 常に Some(v) が保証され、設定画面の DragValue::get_or_insert が no-op になり has_changes() の
    /// 誤発火を防ぐ。旧 legacy フィールドへの get_or_insert は行わない（take 後の再 Some 化で
    /// skip_serializing が無効化されるため）。既定値補完は `changed` に寄与しない（常に実行される
    /// no-op 相当の後始末のため、挙動不変のまま元の実装に合わせて戻り値を持たない）。
    fn resolve_count_param_defaults(&mut self) {
        let _ = self.appearance.visible_rows.get_or_insert_with(default_visible_rows);
        let _ = self.search.result_limit.get_or_insert_with(default_result_limit);
        let _ = self.search.recent_limit.get_or_insert_with(default_recent_limit);
    }

    /// (4) fuzzy_history_cap_ratio が不正（非有限 or [0.0, 1.0] 範囲外）なら既定値へ補正する。
    /// `Config::validate()` は同条件で問題を検出するが補正はしない（検出=validate / 補正=migration
    /// の責務分離。旧 `SearchConfig::sanitize()` の直接処理をここへ移設、issue #437）。
    fn sanitize_fuzzy_history_cap_ratio(&mut self) -> bool {
        let ratio = self.search.fuzzy_history_cap_ratio;
        if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) {
            self.search.fuzzy_history_cap_ratio = default_fuzzy_history_cap_ratio();
            true
        } else {
            false
        }
    }

    /// 旧 `command` 単一文字列 → `Url` へ無改変移行（自動分割しない＝ゼロ回帰）。
    fn migrate_instant_legacy_commands(&mut self) -> bool {
        let mut changed = false;
        for cmd in &mut self.instant_commands {
            if let InstantAction::Legacy { command } = &mut cmd.action {
                let url = std::mem::take(command);
                cmd.action = InstantAction::Url { url };
                changed = true;
            }
        }
        changed
    }

    /// システムショートカットと衝突するホットキーを既定値（Alt+Q）へフォールバックする。
    fn fallback_hotkey_if_system_shortcut(&mut self) -> bool {
        if !is_system_shortcut(&self.hotkey.modifier, &self.hotkey.key) {
            return false;
        }
        let default_hotkey = HotkeyConfig {
            modifier: "Alt".to_string(),
            key: "Q".to_string(),
        };
        eprintln!(
            "[config] system shortcut detected ({}+{}), falling back to default ({}+{})",
            self.hotkey.modifier, self.hotkey.key,
            default_hotkey.modifier, default_hotkey.key,
        );
        self.hotkey = default_hotkey;
        true
    }

    /// `Config::default()` に `apply_migrations()` を適用した「正規化済み既定値」を返す。
    ///
    /// `Config::default()` は一部フィールドを `None`（sentinel、明示未設定を表す）のまま返すため、
    /// 読み込み経由（`load()` は必ず `apply_migrations()` を通す）で得た `Some(v)` な Config と
    /// フィールド単位の `PartialEq` を比較すると、`None` を `Some` に解決する順序（DragValue の
    /// `get_or_insert` 等）次第で結果が変わりうる。この関数は正規化を呼び忘れる余地を型レベルで
    /// なくし、`Config::default()` の生値ではなく常にこちらを「比較可能な既定値」として使う。
    pub fn normalized_default() -> Self {
        let mut config = Self::default();
        let _ = config.apply_migrations();
        config
    }

    pub fn load() -> Self {
        Self::load_reporting().0
    }

    /// `load()` と同じ読み込みを行い、結果区分（`LoadOutcome`）も返す。
    /// 退避通知（トレイ）や読込失敗時の保存ガード（設定画面）が結果を判断するために使う。
    /// `config_dir` が解決できない極端な環境では `(default, FirstRun)` を返す。
    pub fn load_reporting() -> (Self, LoadOutcome) {
        let Some(dir) = Self::config_dir() else {
            return (Self::default(), LoadOutcome::FirstRun);
        };
        Self::load_from_dir_reporting(&dir)
    }

    /// `dir`/config.toml を読み込むコア（`config_dir` を注入可能にし統合テストする）。
    /// - parse 成功: migration 後、変化があれば保存 → `Loaded`
    /// - parse 失敗: ログ + `.bak` 退避 + in-memory default（保存しない）→ `RecoveredFromCorrupt`
    /// - read 失敗 (NotFound): first-run。default を生成・保存 → `FirstRun`
    /// - read 失敗 (InvalidData = 非 UTF-8): 壊れた永続データ。`.bak` 退避 + default → `RecoveredFromCorrupt`
    /// - read 失敗 (その他: permission/lock 等): 退避も上書きもせず default → `ReadFailed`
    fn load_from_dir_reporting(dir: &Path) -> (Self, LoadOutcome) {
        let path = dir.join("config.toml");
        match fs::read_to_string(&path) {
            Ok(content) => match toml::from_str::<Self>(&content) {
                Ok(mut config) => {
                    // 正常系: 従来どおり migration → 変化があれば save
                    if config.apply_migrations() {
                        let _ = config.save_to_dir(dir);
                    }
                    (config, LoadOutcome::Loaded)
                }
                Err(e) => {
                    // TOML parse 失敗（ユーザーの構文ミス・破損等）。
                    // 黙ってデフォルトで上書きしない（snotra-core/CLAUDE.md:
                    // deserialize_failed → save() はデータ喪失を招く）。
                    // エラーを可視化し、不正ファイルを .bak へ退避してから
                    // in-memory default で続行する（save() しない）。
                    eprintln!("[config] failed to parse {}: {e}", path.display());
                    Self::backup_invalid(&path);
                    (Self::default(), LoadOutcome::RecoveredFromCorrupt)
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // first-run / ファイル不在: default を生成・保存
                let config = Self::default();
                let _ = config.save_to_dir(dir);
                (config, LoadOutcome::FirstRun)
            }
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                // 不正な UTF-8 = 壊れた永続データ。parse 失敗と同質なので、同じく
                // byte-preserving に .bak へ退避してから default で起動する。
                // canonical path に残すと後続 save() が破損元を上書きし、.bak にも
                // 残らず失われる（parse 失敗との保全方針の非対称を解消）。
                eprintln!("[config] {} is not valid UTF-8: {e}", path.display());
                Self::backup_invalid(&path);
                (Self::default(), LoadOutcome::RecoveredFromCorrupt)
            }
            Err(e) => {
                // permission / sharing violation / ロック等の一時的・環境的 read 失敗。
                // ファイル内容は壊れていない可能性が高く読めないだけなので、退避も
                // 上書きもせず default で起動する（読めないファイルは安全に退避できない）。
                // `Err(_)` 一括 first-run 扱いは一時的 read 失敗で実設定を default に
                // 潰すデータ損失経路になるため避ける。
                eprintln!(
                    "[config] failed to read {}: {e} (running on defaults; file NOT overwritten)",
                    path.display()
                );
                (Self::default(), LoadOutcome::ReadFailed)
            }
        }
    }

    /// Best-effort: 解析不能な config ファイルを `<path>.bak` へ退避（移動）し、
    /// ユーザーが手動復旧できるようにする。結果をログする。panic しない。
    /// 退避に失敗した場合は元ファイルをその場に残し（default で上書きしない）、
    /// ログして default 続行する。
    fn backup_invalid(path: &Path) {
        let bak = path.with_extension("toml.bak");
        match fs::rename(path, &bak) {
            Ok(()) => eprintln!(
                "[config] backed up unparseable config to {} (running on defaults; original NOT overwritten)",
                bak.display()
            ),
            Err(e) => eprintln!(
                "[config] failed to back up unparseable config at {}: {e} (running on defaults; original left in place)",
                path.display()
            ),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = Self::config_dir().ok_or("設定ディレクトリが見つかりません")?;
        self.save_to_dir(&dir)
    }

    /// `dir`/config.toml へ atomic 保存する（`load_from_dir` と対の注入ポイント）。
    fn save_to_dir(&self, dir: &Path) -> Result<(), String> {
        fs::create_dir_all(dir).map_err(|e| format!("ディレクトリ作成失敗: {e}"))?;

        let path = dir.join("config.toml");
        let content =
            toml::to_string_pretty(self).map_err(|e| format!("シリアライズ失敗: {e}"))?;

        // Atomic write: .tmp → rename
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, content).map_err(|e| format!("書き込み失敗: {e}"))?;
        fs::rename(&tmp, &path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("リネーム失敗: {e}")
        })
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
        if !self.hotkey.modifier.trim().is_empty()
            && !self.hotkey.key.trim().is_empty()
            && is_system_shortcut(&self.hotkey.modifier, &self.hotkey.key)
        {
            errors.push(ConfigError::HotkeySystemConflict {
                modifier: self.hotkey.modifier.clone(),
                key: self.hotkey.key.clone(),
            });
        }

        // Appearance validation
        if self.appearance.effective_visible_rows() == 0 {
            errors.push(ConfigError::VisibleRowsZero);
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

        // Instant command prefix validation
        let prefix = &self.search.instant_command_prefix;
        if prefix.is_empty() {
            errors.push(ConfigError::InstantCommandPrefixEmpty);
        } else if prefix.starts_with('/') {
            errors.push(ConfigError::InstantCommandPrefixSlash);
        }

        // Migemo validation
        if self.search.migemo_min_chars == 0 {
            errors.push(ConfigError::MigemoMinCharsZero);
        }

        // 保守注意: アイコンキャッシュ上限は独立 config キーを持たず `Config::icon_cache_cap()` で
        // 表示ワーキングセットから派生する（「上限 ≥ ワーキングセット」が構造的に成立）ため、
        // ここでの検証は不要。

        // Instant command name uniqueness
        {
            let mut seen = std::collections::HashSet::new();
            for cmd in &self.instant_commands {
                if !cmd.name.is_empty() && !seen.insert(&cmd.name) {
                    errors.push(ConfigError::InstantCommandDuplicateName {
                        name: cmd.name.clone(),
                    });
                }
            }
        }

        // Instant command modifier names: reject unknown modifiers in the
        // variable-expansion templates (url / args / legacy command) at save time
        // so they never reach runtime expansion.
        for cmd in &self.instant_commands {
            let template = match &cmd.action {
                InstantAction::Url { url } => url.as_str(),
                InstantAction::Exec { args, .. } => args.as_str(),
                InstantAction::Legacy { command } => command.as_str(),
            };
            for modifier in crate::instant::collect_unknown_modifiers(template) {
                errors.push(ConfigError::InstantCommandUnknownModifier {
                    name: cmd.name.clone(),
                    modifier,
                });
            }
        }

        errors
    }

    pub fn normalize_openers(&mut self) -> bool {
        let normalized = normalize_openers(&self.openers);
        if normalized != self.openers {
            self.openers = normalized;
            return true;
        }
        false
    }

    /// Parse a TOML string into a Config, filling missing keys with defaults.
    /// Does NOT run migration or auto-save (unlike `load()`).
    pub fn from_toml_str(s: &str) -> Result<Self, String> {
        toml::from_str(s).map_err(|e| e.to_string())
    }

    /// Generate a default export filename like `config_202603111430.toml`.
    /// Caller provides local time components (year, month, day, hour, minute).
    pub fn export_filename(year: u16, month: u16, day: u16, hour: u16, minute: u16) -> String {
        format!("config_{year:04}{month:02}{day:02}{hour:02}{minute:02}.toml")
    }
}

/// Forbidden (modifier_normalized, key_normalized) pairs.
/// Entries must be pre-sorted alphabetically (e.g. "alt+ctrl" not "ctrl+alt")
/// because is_system_shortcut() sorts modifier parts before matching.
/// Key aliases are resolved before matching (esc→escape, del→delete, etc.)
/// to align with hotkey.rs parse_vk().
const SYSTEM_SHORTCUTS: &[(&str, &str)] = &[
    ("alt", "f4"),
    ("alt", "space"),         // Alt+Space: Windows system menu (SC_KEYMENU)
    ("alt", "tab"),
    ("alt+ctrl", "delete"),   // Ctrl+Alt+Delete: sorted alt < ctrl
    ("ctrl+shift", "escape"), // Ctrl+Shift+Escape: sorted ctrl < shift
];

/// Normalizes a modifier part to its canonical form, matching hotkey.rs parse_modifier().
/// Input must already be trimmed and lowercased.
fn normalize_modifier_part(part: &str) -> &str {
    match part {
        "control" => "ctrl",
        "super" | "meta" => "win",
        other => other,
    }
}

/// Normalizes a key name to its canonical form, matching hotkey.rs parse_vk().
fn normalize_key(key: &str) -> String {
    match key {
        "esc" => "escape".to_string(),
        "return" => "enter".to_string(),
        "del" => "delete".to_string(),
        other => other.to_string(),
    }
}

/// Returns true if the given modifier+key combination matches a known Windows system shortcut.
/// modifier: e.g. "Ctrl+Shift", "Control+Alt". key: e.g. "F4", "Esc".
/// Empty modifier or key always returns false (empty check runs before this in validate()).
/// Aliases (control→ctrl, esc→escape, del→delete) are resolved to match hotkey.rs behaviour.
pub fn is_system_shortcut(modifier: &str, key: &str) -> bool {
    if modifier.trim().is_empty() || key.trim().is_empty() {
        return false;
    }
    let lowered: Vec<String> = modifier
        .split('+')
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    let mut parts: Vec<&str> = lowered
        .iter()
        .map(|p| normalize_modifier_part(p.as_str()))
        .collect();
    // Win 8+ reserves all Win+* combinations at the shell level.
    if parts.contains(&"win") {
        return true;
    }
    parts.sort_unstable();
    let norm_mod = parts.join("+");
    let norm_key = normalize_key(key.trim().to_lowercase().as_str());
    SYSTEM_SHORTCUTS
        .iter()
        .any(|&(m, k)| m == norm_mod && k == norm_key)
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
        // migration 適用前の raw デシリアライズ値を検証する。
        // 旧キー max_results / top_n_history / max_history_display は legacy slot（Some(v)）に入る。
        // SearchConfig / visible_rows への migration は migrate_oldest_appearance_legacy_to_new_keys で検証。
        let config: Config = toml::from_str(toml_str).expect("parse");
        assert_eq!(config.hotkey.modifier, "Ctrl");
        assert_eq!(config.hotkey.key, "Space");
        assert_eq!(config.appearance.max_results, Some(10));
        assert_eq!(config.appearance.window_width, 700);
        assert_eq!(config.appearance.top_n_history, Some(150));
        assert_eq!(config.appearance.max_history_display, Some(5));
        assert_eq!(config.paths.additional, vec!["C:\\Tools"]);
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
    fn migrate_oldest_appearance_legacy_to_new_keys() {
        // 最古 [appearance] legacy のみに値があるケース: 新フィールドが None なので legacy 値で補完する。
        // max_results → visible_rows、appearance.top_n_history → result_limit、
        // appearance.max_history_display → recent_limit。
        let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            max_results = 8
            window_width = 600
            top_n_history = 300
            max_history_display = 12

            [paths]
            additional = []
        "#;
        let mut config: Config = toml::from_str(toml_str).expect("parse");
        assert!(config.apply_migrations());
        assert_eq!(config.search.result_limit, Some(300));
        assert_eq!(config.search.recent_limit, Some(12));
        assert_eq!(config.appearance.visible_rows, Some(8));
        // Legacy slots are cleared after migration
        assert_eq!(config.appearance.max_results, None);
        assert_eq!(config.appearance.top_n_history, None);
        assert_eq!(config.appearance.max_history_display, None);
    }

    #[test]
    fn migrate_prefers_search_legacy_over_appearance_legacy() {
        // #388 改名後: [search].top_n_history（中間 legacy）と [appearance].top_n_history（最古 legacy）の
        // 両方に値があるケース。両 legacy を result_limit へ集約し、中間（search）を優先する。
        let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            max_results = 8
            window_width = 600
            top_n_history = 50
            max_history_display = 3

            [search]
            top_n_history = 400
            max_history_display = 15

            [paths]
            additional = []
        "#;
        let mut config: Config = toml::from_str(toml_str).expect("parse");
        assert!(config.apply_migrations());
        // 中間 legacy（search）の値が result_limit/recent_limit へ集約され、最古（appearance）に勝つ
        assert_eq!(config.search.result_limit, Some(400));
        assert_eq!(config.search.recent_limit, Some(15));
        // Legacy slots は全層クリーンアップ済み
        assert_eq!(config.search.top_n_history, None);
        assert_eq!(config.search.max_history_display, None);
        assert_eq!(config.appearance.top_n_history, None);
        assert_eq!(config.appearance.max_history_display, None);
    }

    #[test]
    fn apply_migrations_always_resolves_none_to_some() {
        // [search] セクションも [appearance] legacy もない TOML では、
        // apply_migrations() 後に None → Some(default) が補完されることを確認する。
        // これにより設定画面の DragValue::get_or_insert が常に no-op になり、
        // has_changes() の誤発火（draft = Some vs saved = None）を防ぐ。
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
        let mut config: Config = toml::from_str(toml_str).expect("parse");
        config.apply_migrations();
        assert_eq!(config.search.result_limit, Some(200));
        assert_eq!(config.search.recent_limit, Some(8));
        assert_eq!(config.appearance.visible_rows, Some(8));
    }

    #[test]
    fn normalized_default_resolves_all_migration_sentinels() {
        // #439: reset_to_default の apply_migrations() 手動呼び出し回避策をモデル層に閉じ込める。
        // normalized_default() は常に Some(v) を返し、legacy 二層フィールドは take() 済みで None。
        let config = Config::normalized_default();
        assert!(config.appearance.visible_rows.is_some());
        assert!(config.search.result_limit.is_some());
        assert!(config.search.recent_limit.is_some());
        assert_eq!(config.appearance.max_results, None);
        assert_eq!(config.appearance.top_n_history, None);
        assert_eq!(config.appearance.max_history_display, None);
        assert_eq!(config.search.top_n_history, None);
        assert_eq!(config.search.max_history_display, None);
    }

    #[test]
    fn normalized_default_matches_default_plus_manual_migrations() {
        let mut expected = Config::default();
        expected.apply_migrations();
        assert_eq!(Config::normalized_default(), expected);
    }

    #[test]
    fn normalized_default_is_idempotent_under_reapplied_migrations() {
        // モデル層で正規化済みなら、タブ遷移順序の違いを模した再適用（DragValue の
        // get_or_insert 等）で値が変化しない。変化すれば draft/saved の PartialEq が
        // 遷移順序に依存する既知のバグが再発している。
        let mut config = Config::normalized_default();
        let before = config.clone();
        let changed = config.apply_migrations();
        assert!(!changed, "normalized_default() should already be migration-stable");
        assert_eq!(config, before);
    }

    #[test]
    fn migrate_legacy_max_results_to_visible_rows() {
        // 旧キー [appearance].max_results のみ → visible_rows へ移行（#388）。
        let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            max_results = 15
            window_width = 600

            [paths]
            additional = []
        "#;
        let mut config: Config = toml::from_str(toml_str).expect("parse");
        assert!(config.apply_migrations());
        assert_eq!(config.appearance.effective_visible_rows(), 15);
        assert_eq!(config.appearance.max_results, None); // legacy cleared
    }

    #[test]
    fn migrate_search_intermediate_legacy_to_result_limit() {
        // 中間 legacy [search].top_n_history のみ（appearance 最古なし）→ result_limit へ移行（#388）。
        let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            window_width = 600

            [search]
            top_n_history = 333
            max_history_display = 7

            [paths]
            additional = []
        "#;
        let mut config: Config = toml::from_str(toml_str).expect("parse");
        assert!(config.apply_migrations());
        assert_eq!(config.search.result_limit, Some(333));
        assert_eq!(config.search.recent_limit, Some(7));
        assert_eq!(config.search.top_n_history, None); // intermediate legacy cleared
        assert_eq!(config.search.max_history_display, None);
    }

    #[test]
    fn migrate_new_key_wins_over_legacy() {
        // 核心不変条件: 新キー（明示）と legacy が両在 → 新キーが勝つ（get_or_insert が no-op、新優先）。
        // この回帰ガードがないと get_or_insert を `= Some(v)`（無条件上書き）に誤改変しても検知できない。
        let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            visible_rows = 12
            max_results = 99
            window_width = 600

            [search]
            result_limit = 250
            top_n_history = 999
            recent_limit = 6
            max_history_display = 88

            [paths]
            additional = []
        "#;
        let mut config: Config = toml::from_str(toml_str).expect("parse");
        config.apply_migrations();
        // 新キーが legacy で上書きされない
        assert_eq!(config.appearance.visible_rows, Some(12));
        assert_eq!(config.search.result_limit, Some(250));
        assert_eq!(config.search.recent_limit, Some(6));
        // legacy は全層クリア済み
        assert_eq!(config.appearance.max_results, None);
        assert_eq!(config.search.top_n_history, None);
        assert_eq!(config.search.max_history_display, None);
    }

    #[test]
    fn new_keys_round_trip_and_legacy_not_serialized() {
        // 新キーで読み込み → serialize → 旧キーが出力されず（skip_serializing）新キーで再読み込みできる。
        let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            visible_rows = 20
            window_width = 600

            [search]
            result_limit = 250
            recent_limit = 6

            [paths]
            additional = []
        "#;
        let mut config: Config = toml::from_str(toml_str).expect("parse");
        config.apply_migrations();
        assert_eq!(config.appearance.effective_visible_rows(), 20);
        assert_eq!(config.search.effective_result_limit(), 250);
        assert_eq!(config.search.effective_recent_limit(), 6);

        let serialized = toml::to_string(&config).expect("serialize");
        // 旧キーは skip_serializing で出力されない
        assert!(!serialized.contains("max_results"), "old key leaked: {serialized}");
        assert!(!serialized.contains("top_n_history"), "old key leaked: {serialized}");
        assert!(!serialized.contains("max_history_display"), "old key leaked: {serialized}");
        // 新キーは出力される
        assert!(serialized.contains("visible_rows"));
        assert!(serialized.contains("result_limit"));
        assert!(serialized.contains("recent_limit"));

        // 再読み込みで同値
        let reloaded: Config = toml::from_str(&serialized).expect("reparse");
        assert_eq!(reloaded.appearance.effective_visible_rows(), 20);
        assert_eq!(reloaded.search.effective_result_limit(), 250);
        assert_eq!(reloaded.search.effective_recent_limit(), 6);
    }

    #[test]
    fn all_legacy_keys_behavior_unchanged() {
        // 全旧キーを持つ config が改名前と同じ実効値・icon_cache_cap に解決する（挙動不変）。
        let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            max_results = 8
            window_width = 600
            top_n_history = 200
            max_history_display = 8

            [paths]
            additional = []
        "#;
        let mut config: Config = toml::from_str(toml_str).expect("parse");
        config.apply_migrations();
        assert_eq!(config.appearance.effective_visible_rows(), 8);
        assert_eq!(config.search.effective_result_limit(), 200);
        assert_eq!(config.search.effective_recent_limit(), 8);
        assert_eq!(config.icon_cache_cap(), 1000); // 改名前と同値
    }

    #[test]
    fn deserialize_minimal_config_uses_defaults() {
        // Verify that omitting the `[search]` section entirely still yields correct defaults.
        // Each field uses `#[serde(default = "default_...")]`, so serde fills in the
        // function-level defaults (e.g. `default_result_limit` → 200) without needing
        // the section to be present in the TOML.
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
        // [search] セクションが省略されると None になり、accessor がデフォルト値を返す
        assert_eq!(config.search.effective_result_limit(), 200);
        assert_eq!(config.search.effective_recent_limit(), 8);
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
        assert_eq!(config.appearance.effective_visible_rows(), 8);
        assert_eq!(config.appearance.window_width, 600);
        assert_eq!(config.search.effective_result_limit(), 200);
        assert_eq!(config.search.effective_recent_limit(), 8);
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
        // default instant commands
        assert_eq!(config.instant_commands.len(), 2);
        assert_eq!(config.instant_commands[0].name, "g");
        assert_eq!(
            config.instant_commands[0].action,
            InstantAction::Url { url: "https://www.google.com/search?q={query}".to_string() }
        );
        assert_eq!(config.instant_commands[1].name, "gh");
        assert_eq!(
            config.instant_commands[1].action,
            InstantAction::Url { url: "https://github.com/search?q={query}".to_string() }
        );
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
    fn apply_migrations_sanitizes_invalid_fuzzy_history_cap_ratio() {
        // 旧 `SearchConfig::sanitize()` の直接処理を `apply_migrations()` へ移設した
        // 補正ロジックを検証する（issue #437）。範囲外（> 1.0）の値は既定値へ補正される。
        let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            window_width = 600

            [search]
            fuzzy_history_cap_ratio = 1.5

            [paths]
            additional = []
        "#;
        let mut config: Config = toml::from_str(toml_str).expect("parse");
        assert!(config.apply_migrations());
        assert!((config.search.fuzzy_history_cap_ratio - 0.30).abs() < f64::EPSILON);
    }

    #[test]
    fn apply_migrations_leaves_valid_fuzzy_history_cap_ratio_unchanged() {
        // 有効範囲内の値は補正されず、apply_migrations() の changed フラグにも寄与しない
        // （他の移行項目が無い最小 TOML では false を返す）ことを確認する。
        let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [appearance]
            window_width = 600
            visible_rows = 10

            [search]
            fuzzy_history_cap_ratio = 0.5
            result_limit = 200
            recent_limit = 10

            [paths]
            additional = []
        "#;
        let mut config: Config = toml::from_str(toml_str).expect("parse");
        assert!(!config.apply_migrations());
        assert!((config.search.fuzzy_history_cap_ratio - 0.5).abs() < f64::EPSILON);
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

    // opener ロジック本体のテストは `opener.rs` に移設済み（issue #435）。以下 3 件は
    // `Config::normalize_openers()`（config.rs 側メソッド）のテストのため、この test mod
    // ローカルの make_rule ヘルパーを使う（opener.rs 側テストの重複だがテスト専用の小ヘルパーであり
    // モジュール間の pub(crate) 露出を避けるため意図的に複製する）。
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
    fn normalize_openers_returns_true_when_changed() {
        let mut config = Config {
            openers: vec![make_rule("ext:png,jpg", &[("Viewer", "viewer.exe", "")])],
            ..Default::default()
        };

        assert!(config.normalize_openers());
        assert_eq!(config.openers[0].target, "ext:.jpg,.png");
    }

    #[test]
    fn normalize_openers_returns_false_when_no_change() {
        let mut config = Config {
            openers: vec![make_rule("ext:.jpg,.png", &[("Viewer", "viewer.exe", "")])],
            ..Default::default()
        };

        assert!(!config.normalize_openers());
    }

    #[test]
    fn normalize_openers_returns_false_when_multiple_already_sorted() {
        let mut config = Config {
            openers: vec![
                make_rule("folder:c:\\workspace", &[("Terminal", "wt.exe", "")]),
                make_rule("folder", &[("Explorer", "explorer.exe", "")]),
                make_rule("ext:.md", &[("Typora", "typora.exe", "")]),
            ],
            ..Default::default()
        };
        assert!(!config.normalize_openers());
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
        assert!(config.instant_commands.is_empty());
    }

    // ---- instant command prefix validation tests ----

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
                    action: InstantAction::Url { url: "https://google.com".into() },
                },
                InstantCommand {
                    name: "google".to_string(),
                    description: String::new(),
                    action: InstantAction::Url { url: "https://google.co.jp".into() },
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
                    action: InstantAction::Url { url: "https://google.com".into() },
                },
                InstantCommand {
                    name: "bing".to_string(),
                    description: String::new(),
                    action: InstantAction::Url { url: "https://bing.com".into() },
                },
            ],
            ..Default::default()
        };
        let errors = config.validate();
        assert!(
            !errors.iter().any(|e| matches!(e, ConfigError::InstantCommandDuplicateName { .. })),
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
        assert!(errors.contains(&ConfigError::InstantCommandUnknownModifier {
            name: "g".to_string(),
            modifier: "bogus".to_string(),
        }));
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
        assert!(errors.contains(&ConfigError::InstantCommandUnknownModifier {
            name: "ev".to_string(),
            modifier: "nope".to_string(),
        }));
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
    fn instant_command_round_trip_toml() {
        let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"
            [appearance]
            window_width = 600
            [paths]
            additional = []
            [[instant_commands]]
            name = "g"
            command = "https://google.com/search?q={query}"
            [[instant_commands]]
            name = "memo"
            command = "C:\\tools\\editor.exe"
        "#;
        let mut config: Config = toml::from_str(toml_str).expect("parse");
        config.apply_migrations();
        assert_eq!(config.instant_commands.len(), 2);
        assert_eq!(config.instant_commands[0].name, "g");
        assert!(matches!(config.instant_commands[0].action, InstantAction::Url { .. }));
        assert_eq!(config.instant_commands[1].name, "memo");
        assert!(matches!(config.instant_commands[1].action, InstantAction::Url { .. }));
    }

    #[test]
    fn instant_mixed_variants_roundtrip() {
        // Url 1件 + Exec 1件（description 省略）の Vec が serialize → deserialize で両変種と name を保つ。
        let config = Config {
            instant_commands: vec![
                InstantCommand {
                    name: "g".to_string(),
                    description: "Google".to_string(),
                    action: InstantAction::Url {
                        url: "https://www.google.com/search?q={query}".to_string(),
                    },
                },
                InstantCommand {
                    name: "ev".to_string(),
                    description: String::new(),
                    action: InstantAction::Exec {
                        exe: "everything.exe".to_string(),
                        args: "-s {query}".to_string(),
                    },
                },
            ],
            ..Default::default()
        };
        let serialized = toml::to_string_pretty(&config).expect("serialize");
        let parsed: Config = toml::from_str(&serialized).expect("parse");
        assert_eq!(parsed.instant_commands.len(), 2);
        assert_eq!(parsed.instant_commands[0].name, "g");
        assert!(
            matches!(&parsed.instant_commands[0].action, InstantAction::Url { url } if url.contains("google"))
        );
        assert_eq!(parsed.instant_commands[1].name, "ev");
        assert!(
            matches!(&parsed.instant_commands[1].action, InstantAction::Exec { exe, .. } if exe == "everything.exe")
        );
    }

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

    // ---- is_system_shortcut tests ----

    #[test]
    fn system_shortcut_blocked_combos() {
        // Each entry in SYSTEM_SHORTCUTS must be blocked
        let cases = [
            ("Alt", "F4"),
            ("Alt", "Space"),
            ("Ctrl+Shift", "Escape"),
            ("Alt", "Tab"),
            ("Ctrl+Alt", "Delete"),
        ];
        for (modifier, key) in cases {
            assert!(
                is_system_shortcut(modifier, key),
                "expected {modifier}+{key} to be a system shortcut"
            );
        }
    }

    #[test]
    fn system_shortcut_win_modifier_blocked() {
        // Win 8+ reserves all Win+* combinations at the shell level
        assert!(is_system_shortcut("Win", "Q"));
        assert!(is_system_shortcut("Super", "A"));
        assert!(is_system_shortcut("Ctrl+Win", "Space"));
        assert!(is_system_shortcut("Meta", "F1"));
    }

    #[test]
    fn system_shortcut_case_insensitive() {
        assert!(is_system_shortcut("alt", "f4"));
        assert!(is_system_shortcut("ALT", "F4"));
        assert!(is_system_shortcut("Alt", "f4"));
    }

    #[test]
    fn system_shortcut_modifier_order_independent() {
        assert!(is_system_shortcut("Shift+Ctrl", "Escape"));
        assert!(is_system_shortcut("ctrl+shift", "escape"));
        // Ctrl+Alt+Delete: sorted → alt+ctrl, must match
        assert!(is_system_shortcut("Ctrl+Alt", "Delete"));
        assert!(is_system_shortcut("Alt+Ctrl", "Delete"));
    }

    #[test]
    fn system_shortcut_alias_normalization() {
        // modifier alias: control → ctrl
        assert!(is_system_shortcut("Control+Shift", "Escape"));
        assert!(is_system_shortcut("Control+Alt", "Delete"));
        // key alias: esc → escape
        assert!(is_system_shortcut("Ctrl+Shift", "Esc"));
        // combined aliases
        assert!(is_system_shortcut("Control+Shift", "Esc"));
    }

    #[test]
    fn system_shortcut_allowed_combos() {
        assert!(!is_system_shortcut("Alt", "Q"));
        assert!(!is_system_shortcut("Ctrl", "F1"));
        assert!(!is_system_shortcut("Alt+Shift", "F4")); // extra modifier → not Alt+F4
        assert!(!is_system_shortcut("Alt+Shift", "Space")); // extra modifier → not Alt+Space
        assert!(!is_system_shortcut("Ctrl", "Space")); // IME 切替はユーザー判断
        assert!(!is_system_shortcut("Alt", "Escape")); // RegisterHotKey が失敗するので不要
    }

    #[test]
    fn system_shortcut_empty_inputs_return_false() {
        assert!(!is_system_shortcut("", "F4"));
        assert!(!is_system_shortcut("Alt", ""));
        assert!(!is_system_shortcut("", ""));
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
    fn validate_allowed_hotkey_no_conflict_error() {
        let mut config = Config::default();
        config.hotkey.modifier = "Alt".to_string();
        config.hotkey.key = "Q".to_string();
        let errors = config.validate();
        let has_conflict = errors.iter().any(|e| {
            matches!(e, ConfigError::HotkeySystemConflict { .. })
        });
        assert!(!has_conflict, "Alt+Q should not produce a system conflict error");
    }

    // ---- CustomTheme / ThemePreset::Custom tests ----

    #[test]
    fn visual_config_without_custom_theme_deserializes() {
        let toml_str = r##"
            preset = "obsidian"
            background_color = "#282828"
            input_background_color = "#383838"
            text_color = "#E0E0E0"
            selected_row_color = "#505050"
            hint_text_color = "#808080"
            font_family = "Segoe UI"
            font_size = 15
        "##;
        let vc: VisualConfig = toml::from_str(toml_str).expect("parse");
        assert_eq!(vc.preset, ThemePreset::Obsidian);
        assert!(vc.custom_theme.is_none());
    }

    #[test]
    fn visual_config_with_custom_theme_round_trip() {
        let toml_str = r##"
            preset = "custom"
            background_color = "#1a1a2a"
            input_background_color = "#2a2a3a"
            text_color = "#d0d0ff"
            selected_row_color = "#3a3a5a"
            hint_text_color = "#7070a0"
            font_family = "Segoe UI"
            font_size = 15

            [custom_theme]
            background_color = "#1a1a2a"
            input_background_color = "#2a2a3a"
            text_color = "#d0d0ff"
            selected_row_color = "#3a3a5a"
            hint_text_color = "#7070a0"
        "##;
        let vc: VisualConfig = toml::from_str(toml_str).expect("parse");
        assert_eq!(vc.preset, ThemePreset::Custom);
        let ct = vc.custom_theme.as_ref().expect("custom_theme should exist");
        assert_eq!(ct.background_color, "#1a1a2a");
        assert_eq!(ct.text_color, "#d0d0ff");

        // round-trip: serialize then deserialize
        let serialized = toml::to_string_pretty(&vc).expect("serialize");
        let vc2: VisualConfig = toml::from_str(&serialized).expect("re-parse");
        assert_eq!(vc, vc2);
    }

    #[test]
    fn visual_config_custom_theme_omitted_when_none() {
        let vc = VisualConfig::default();
        let serialized = toml::to_string_pretty(&vc).expect("serialize");
        assert!(
            !serialized.contains("custom_theme"),
            "custom_theme should not appear when None"
        );
    }

    #[test]
    fn theme_preset_custom_deserializes() {
        let toml_str = r#"preset = "custom""#;
        #[derive(Deserialize)]
        struct Wrapper {
            preset: ThemePreset,
        }
        let w: Wrapper = toml::from_str(toml_str).expect("parse");
        assert_eq!(w.preset, ThemePreset::Custom);
    }

    // -- Config defense tests (external process writes) --

    #[test]
    fn invalid_toml_falls_back_to_default() {
        // Garbage text that isn't valid TOML → should parse as default
        let config: Config = toml::from_str("{{{{not valid toml!!!!").unwrap_or_default();
        let default = Config::default();
        assert_eq!(config.hotkey.modifier, default.hotkey.modifier);
        assert_eq!(config.hotkey.key, default.hotkey.key);
        assert_eq!(
            config.appearance.effective_visible_rows(),
            default.appearance.effective_visible_rows()
        );
    }

    #[test]
    fn valid_toml_invalid_values_caught_by_validate() {
        // Config with all required sections but invalid field values
        let toml_str = r#"
            [hotkey]
            modifier = ""
            key = ""

            [appearance]
            visible_rows = 0
            window_width = 50

            [paths]
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let errors = config.validate();
        // Should have errors for empty hotkey and invalid visible_rows/window_width
        assert!(errors.len() >= 2, "Expected at least 2 errors, got: {:?}", errors);
    }

    #[test]
    fn partial_toml_falls_back_to_default_via_unwrap_or_default() {
        // Partial TOML missing required sections → toml::from_str fails.
        // Config::load() now matches on the parse error (backing the file up to
        // .bak and falling back to an in-memory default), not unwrap_or_default().
        // This test still pins the serde-level fallback to default values.
        let toml_str = r#"
            [hotkey]
            modifier = "Ctrl"
            key = "Space"
        "#;
        // Direct parse fails (missing required sections)
        assert!(toml::from_str::<Config>(toml_str).is_err());
        // But unwrap_or_default produces a usable config
        let config: Config = toml::from_str(toml_str).unwrap_or_default();
        let default = Config::default();
        assert_eq!(config.hotkey.modifier, default.hotkey.modifier);
        assert_eq!(
            config.appearance.effective_visible_rows(),
            default.appearance.effective_visible_rows()
        );
    }

    // -- backup_invalid: parse 失敗時の .bak 退避（issue #338） --

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("snotra_config_test_{}", tag));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn backup_invalid_renames_to_bak_preserving_content() {
        let dir = temp_dir("backup_rename");
        let path = dir.join("config.toml");
        let bad = "{{{{not valid toml!!!!";
        fs::write(&path, bad).unwrap();

        Config::backup_invalid(&path);

        let bak = path.with_extension("toml.bak");
        // 元ファイルは退避され存在しない（= default で上書きされ得ない）
        assert!(
            !path.exists(),
            "config.toml must be moved away on parse failure (not left for default overwrite)"
        );
        // .bak が元の不正内容を保全している（手動復旧可能）
        assert_eq!(
            fs::read_to_string(&bak).unwrap(),
            bad,
            ".bak must preserve the original (unparseable) content"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_invalid_overwrites_existing_bak() {
        let dir = temp_dir("backup_overwrite");
        let path = dir.join("config.toml");
        let bak = path.with_extension("toml.bak");
        fs::write(&bak, "OLD BAK CONTENT").unwrap();
        let newer_bad = "also = invalid = toml";
        fs::write(&path, newer_bad).unwrap();

        Config::backup_invalid(&path);

        // 単一 .bak を最新の不正内容で上書きする（KISS）
        assert_eq!(fs::read_to_string(&bak).unwrap(), newer_bad);
        assert!(!path.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_invalid_missing_source_is_noop_no_panic() {
        let dir = temp_dir("backup_missing");
        let path = dir.join("config.toml"); // 作成しない
        // rename は Err になるが panic せず、.bak も作られない
        Config::backup_invalid(&path);
        let bak = path.with_extension("toml.bak");
        assert!(
            !bak.exists(),
            "no .bak should be created when source is absent"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // -- load_from_dir: load() 統合経路（config_dir 注入、issue #338） --

    #[test]
    fn load_from_dir_parse_failure_backs_up_and_does_not_save() {
        let dir = temp_dir("load_parse_fail");
        let path = dir.join("config.toml");
        let bad = "{{{ not valid toml";
        fs::write(&path, bad).unwrap();

        let (config, outcome) = Config::load_from_dir_reporting(&dir);

        assert_eq!(outcome, LoadOutcome::RecoveredFromCorrupt);
        // default 値で起動する
        assert_eq!(config.hotkey.modifier, Config::default().hotkey.modifier);
        // parse 失敗時は default で再保存しない（config.toml は .bak へ退避され不在）
        assert!(
            !path.exists(),
            "config.toml must NOT be recreated/overwritten on parse failure"
        );
        let bak = path.with_extension("toml.bak");
        assert_eq!(
            fs::read_to_string(&bak).unwrap(),
            bad,
            ".bak must hold the original broken content"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_from_dir_missing_file_is_first_run_and_saves_default() {
        let dir = temp_dir("load_missing");
        let path = dir.join("config.toml"); // 作らない（NotFound = first-run）

        let (config, outcome) = Config::load_from_dir_reporting(&dir);

        assert_eq!(outcome, LoadOutcome::FirstRun);
        assert_eq!(config.hotkey.modifier, Config::default().hotkey.modifier);
        // first-run は default を保存する
        assert!(path.exists(), "first-run must create config.toml");
        let reparsed: Config = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(reparsed.hotkey.modifier, config.hotkey.modifier);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_from_dir_valid_config_is_parsed() {
        let dir = temp_dir("load_valid");
        let path = dir.join("config.toml");
        let mut written = Config::default();
        written.appearance.window_width = 777;
        fs::write(&path, toml::to_string_pretty(&written).unwrap()).unwrap();

        let (loaded, outcome) = Config::load_from_dir_reporting(&dir);

        assert_eq!(outcome, LoadOutcome::Loaded);
        assert_eq!(loaded.appearance.window_width, 777);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_from_dir_invalid_utf8_is_backed_up() {
        let dir = temp_dir("load_invalid_utf8");
        let path = dir.join("config.toml");
        // 不正な UTF-8 → read_to_string が InvalidData で失敗する。
        // 壊れた永続データなので parse 失敗と同じく .bak へ byte-preserving 退避する。
        let invalid_utf8: &[u8] = &[0xFF, 0xFE, 0x00, 0x80];
        fs::write(&path, invalid_utf8).unwrap();

        let (config, outcome) = Config::load_from_dir_reporting(&dir);

        assert_eq!(outcome, LoadOutcome::RecoveredFromCorrupt);
        assert_eq!(config.hotkey.modifier, Config::default().hotkey.modifier);
        // canonical path には残さず（後続 save() で破損元を失わないため）.bak へ退避
        assert!(
            !path.exists(),
            "corrupt (non-UTF-8) config.toml must be moved to .bak, not left at canonical path"
        );
        let bak = path.with_extension("toml.bak");
        assert_eq!(
            fs::read(&bak).unwrap(),
            invalid_utf8,
            ".bak must byte-preserve the corrupt content"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_from_dir_transient_read_error_leaves_file_intact() {
        let dir = temp_dir("load_transient");
        let path = dir.join("config.toml");
        // config.toml をディレクトリにして read_to_string を NotFound/InvalidData 以外で
        // 失敗させる（permission/lock 等の一時的失敗の代理）。読めないファイルは安全に
        // 退避できないため、退避も上書きもせず据え置く。
        fs::create_dir(&path).unwrap();

        let (config, outcome) = Config::load_from_dir_reporting(&dir);

        assert_eq!(outcome, LoadOutcome::ReadFailed);
        assert_eq!(config.hotkey.modifier, Config::default().hotkey.modifier);
        assert!(
            path.is_dir(),
            "transient read error must NOT move or overwrite the existing path"
        );
        let bak = path.with_extension("toml.bak");
        assert!(
            !bak.exists(),
            "transient read error must NOT create a .bak (the file may be intact, just unreadable)"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn language_serialize_deserialize() {
        // TOML doesn't support bare enum values; test via a wrapper struct
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Wrapper {
            lang: Language,
        }
        let ja = Wrapper { lang: Language::Ja };
        let serialized = toml::to_string(&ja).unwrap();
        assert!(serialized.contains("lang = \"ja\""));
        let deserialized: Wrapper = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized, ja);

        let en = Wrapper { lang: Language::En };
        let serialized = toml::to_string(&en).unwrap();
        assert!(serialized.contains("lang = \"en\""));
        let deserialized: Wrapper = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized, en);
    }

    #[test]
    fn language_default_in_general_config() {
        let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [general]

            [appearance]
            max_results = 8
            window_width = 600

            [paths]
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        // Without explicit language, should use OS-detected default
        let expected = default_language();
        assert_eq!(config.general.language, expected);
    }

    #[test]
    fn language_explicit_in_config() {
        let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"

            [general]
            language = "en"

            [appearance]
            max_results = 8
            window_width = 600

            [paths]
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.general.language, Language::En);
    }

    #[test]
    fn from_toml_str_fills_defaults() {
        // hotkey, appearance, paths are required; general, visual, search, openers, instant_commands have #[serde(default)]
        let toml = r#"
[hotkey]
modifier = "Ctrl"
key = "Space"
[appearance]
max_results = 10
window_width = 700
[paths]
scan = []
"#;
        let config = Config::from_toml_str(toml).expect("parse");
        assert_eq!(config.hotkey.modifier, "Ctrl");
        assert_eq!(config.hotkey.key, "Space");
        // 旧キー max_results は legacy slot に入る（from_toml_str は migration を実行しない）
        assert_eq!(config.appearance.max_results, Some(10));
        // Defaults filled for missing optional sections
        assert!(config.openers.is_empty());
        assert!(config.instant_commands.is_empty());
    }

    #[test]
    fn icon_cache_cap_derives_from_working_set_times_retention() {
        // 既定: visible_rows=8, result_limit=200, recent_limit=8 → working_set=200
        // → cap = 200 × 5 = 1000（旧 default と一致＝既定挙動は不変）。
        let config = Config::default();
        assert_eq!(config.search.effective_result_limit(), 200);
        assert_eq!(config.icon_cache_cap(), 1000);
    }

    #[test]
    fn icon_cache_cap_scales_with_result_limit() {
        // result_limit を上げると上限も自動追従する（検証不要・drift なし）。
        let mut config = Config::default();
        config.search.result_limit = Some(500);
        assert_eq!(config.icon_cache_cap(), 2500); // 500 × 5
    }

    #[test]
    fn icon_cache_cap_uses_max_of_all_list_limits() {
        // working_set = max(visible_rows, result_limit, recent_limit)。
        // ここでは recent_limit が支配的になるケースを検証する。
        let mut config = Config::default();
        config.appearance.visible_rows = Some(8);
        config.search.result_limit = Some(50);
        config.search.recent_limit = Some(120);
        assert_eq!(config.icon_cache_cap(), 600); // max(8,50,120)=120 × 5
    }

    #[test]
    fn icon_cache_cap_visible_rows_dominates_when_list_limits_small() {
        let mut config = Config::default();
        config.appearance.visible_rows = Some(12);
        config.search.result_limit = Some(5);
        config.search.recent_limit = Some(3);
        assert_eq!(config.icon_cache_cap(), 60); // max(12,5,3)=12 × 5
    }

    #[test]
    fn icon_cache_cap_never_collapses_to_zero() {
        // 退行防御: 全リスト上限が 0 でも working_set は 1 で floor され、cap は RETENTION 以上。
        // （visible_rows=0 は validate の VisibleRowsZero 対象だが、cap 導出は panic/0 にならない）。
        let mut config = Config::default();
        config.appearance.visible_rows = Some(0);
        config.search.result_limit = Some(0);
        config.search.recent_limit = Some(0);
        assert_eq!(config.icon_cache_cap(), ICON_CACHE_RETENTION_FACTOR);
        assert!(config.icon_cache_cap() >= 1);
    }

    #[test]
    fn from_toml_str_rejects_invalid() {
        let result = Config::from_toml_str("this is not valid toml {{{}}}");
        assert!(result.is_err());
    }

    #[test]
    fn from_toml_str_rejects_missing_required_section() {
        // Missing [appearance] and [paths] — should fail
        let result = Config::from_toml_str("[hotkey]\nmodifier = \"Alt\"\nkey = \"Q\"\n");
        assert!(result.is_err());
    }

    #[test]
    fn from_toml_str_ignores_unknown_keys() {
        let toml = r#"
[hotkey]
modifier = "Alt"
key = "Q"
[appearance]
max_results = 8
window_width = 600
[paths]
scan = []
unknown_field = "hello"
[unknown_section]
foo = 42
"#;
        let config = Config::from_toml_str(toml).expect("parse");
        assert_eq!(config.hotkey.key, "Q");
    }

    #[test]
    fn export_filename_format() {
        let name = Config::export_filename(2026, 3, 11, 14, 30);
        assert_eq!(name, "config_202603111430.toml");
    }

    #[test]
    fn export_filename_zero_pads() {
        let name = Config::export_filename(2026, 1, 5, 9, 3);
        assert_eq!(name, "config_202601050903.toml");
    }

    #[test]
    fn apply_migrations_normalizes_additional() {
        let mut config = Config::default();
        #[allow(deprecated)]
        config.paths.additional.push("C:\\Legacy".to_string());
        assert!(config.apply_migrations());
        assert!(config.paths.additional.is_empty());
        assert!(!config.paths.scan.is_empty());
    }

    // ---- InstantAction serde gate (release gate: 失敗は全設定リセットを意味する) ----
    fn cfg_with_instant(cmds: Vec<InstantCommand>) -> Config {
        Config { instant_commands: cmds, ..Default::default() }
    }

    #[test] // T2: legacy 行が deserialize できる（最重要・データ損失検出器）
    fn instant_legacy_command_deserializes() {
        let legacy = cfg_with_instant(vec![InstantCommand {
            name: "g".into(), description: String::new(),
            action: InstantAction::Legacy { command: "https://x/?q={query}".into() },
        }]);
        let s = toml::to_string(&legacy).expect("serialize legacy");
        // Legacy は `command = "..."` 形（=旧オンディスク形式）で出力される
        assert!(s.contains("command ="));
        let parsed: Config = toml::from_str(&s).expect("legacy deserialize must succeed");
        assert!(matches!(parsed.instant_commands[0].action, InstantAction::Legacy { .. }));
    }

    #[test] // T15 + T17: legacy → Url 移行（自動分割しない）・冪等
    fn instant_legacy_migrates_to_url_idempotently() {
        let mut cfg = cfg_with_instant(vec![InstantCommand {
            name: "ev".into(), description: String::new(),
            action: InstantAction::Legacy { command: "C:\\tools\\editor.exe".into() },
        }]);
        assert!(cfg.apply_migrations());
        assert_eq!(cfg.instant_commands[0].action,
            InstantAction::Url { url: "C:\\tools\\editor.exe".into() }); // Exec にしない
        // 冪等: 2回目は Legacy が残っていないので action は Url のまま
        cfg.apply_migrations();
        assert_eq!(cfg.instant_commands[0].action,
            InstantAction::Url { url: "C:\\tools\\editor.exe".into() });
    }

    #[test] // T1: Config 全体の serialize 往復で変種が保たれる
    fn instant_exec_roundtrip_preserves_variant() {
        let cfg = cfg_with_instant(vec![InstantCommand {
            name: "ev".into(), description: "Everything".into(),
            action: InstantAction::Exec { exe: "everything.exe".into(), args: "-s {query}".into() },
        }]);
        let s = toml::to_string_pretty(&cfg).expect("serialize");
        let parsed: Config = toml::from_str(&s).expect("deserialize");
        assert_eq!(parsed.instant_commands[0].action,
            InstantAction::Exec { exe: "everything.exe".into(), args: "-s {query}".into() });
    }

    #[test] // T3: url と exe を両方書いた行は Url 先勝ち（untagged 宣言順）
    fn instant_both_url_and_exe_prefers_url() {
        let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"
            [appearance]
            window_width = 600
            [paths]
            additional = []
            [[instant_commands]]
            name = "x"
            url = "https://x"
            exe = "y.exe"
        "#;
        let cfg: Config = toml::from_str(toml_str).expect("parse");
        assert!(matches!(cfg.instant_commands[0].action, InstantAction::Url { .. }));
    }

    #[test] // T4: Exec で args 省略 → 空文字
    fn instant_exec_args_defaults_empty() {
        let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"
            [appearance]
            window_width = 600
            [paths]
            additional = []
            [[instant_commands]]
            name = "n"
            exe = "notepad.exe"
        "#;
        let cfg: Config = toml::from_str(toml_str).expect("parse");
        assert_eq!(cfg.instant_commands[0].action,
            InstantAction::Exec { exe: "notepad.exe".into(), args: String::new() });
    }
}
