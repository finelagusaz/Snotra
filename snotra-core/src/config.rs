use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub use crate::error::ConfigError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Ja,
    En,
}

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
    pub command: String,
}

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

/// オープナーターゲットからパス条件を抽出する。
/// - `"folder"` → None
/// - `"folder:C:\\workspace"` → Some("C:\\workspace")
/// - `"ext:.md"` → None
/// - `"ext:.md:C:\\projects"` → Some("C:\\projects")
pub fn extract_path_condition(target: &str) -> Option<&str> {
    if let Some(rest) = target.strip_prefix("folder:") {
        if !rest.is_empty() {
            return Some(rest);
        }
    } else if let Some(after_ext) = target.strip_prefix("ext:") {
        return split_ext_and_path(after_ext).1;
    }
    None
}

/// ターゲットから拡張子リスト部分のみ取得する（パス条件を除外）。
pub fn extract_ext_part(target: &str) -> &str {
    debug_assert!(target.starts_with("ext:"), "extract_ext_part called on non-ext target: {target}");
    let after_ext = &target["ext:".len()..];
    if let Some(path_cond) = extract_path_condition(target) {
        // パス条件の直前のコロンまでが拡張子部分
        let path_start = after_ext.len() - path_cond.len() - 1; // -1 for ':'
        &after_ext[..path_start]
    } else {
        after_ext
    }
}

/// パスとフォルダフラグに対してマッチするツール一覧を返す。
/// 最も具体的にマッチした1ルールを返す（パス条件が長い方が具体的）。
/// マッチするルールがなければ空スライスを返す（呼び出し側でフォールバック処理）。
pub fn find_matching_tools<'a>(
    path: &str,
    is_folder: bool,
    rules: &'a [OpenerRule],
) -> &'a [OpenerTool] {
    let path_lower = path.to_lowercase().replace('/', "\\");
    let path_ext = path_lower
        .rfind('.')
        .map(|i| &path_lower[i..])
        .unwrap_or("");

    let mut best: Option<(usize, usize)> = None; // (rule_index, specificity)
    // specificity: 0 = no path condition, N = path condition length

    for (idx, rule) in rules.iter().enumerate() {
        let target = &rule.target;
        let path_cond = extract_path_condition(target);

        // パス条件チェック（パス境界で一致を検証）
        if let Some(cond) = path_cond {
            let cond_lower = cond.to_lowercase().replace('/', "\\");
            if !path_lower.starts_with(&cond_lower) {
                continue;
            }
            // パス条件がパス境界で終わっていることを確認
            // 例: 条件 "C:\workspace" はパス "C:\workspace123" にマッチしない
            // 条件自体がパス区切りで終わっている場合はすでに境界OK
            let cond_ends_with_sep =
                cond_lower.ends_with('\\') || cond_lower.ends_with('/');
            if !cond_ends_with_sep {
                let next_byte = path_lower.as_bytes().get(cond_lower.len());
                if next_byte.is_some()
                    && next_byte != Some(&b'\\')
                    && next_byte != Some(&b'/')
                {
                    continue;
                }
            }
        }

        let kind_match = if is_folder {
            target == "folder" || target.starts_with("folder:")
        } else if target.starts_with("ext:") {
            let ext_part = extract_ext_part(target);
            ext_part.split(',').any(|raw_ext| {
                let ext = raw_ext.trim().to_lowercase();
                let ext_with_dot = if ext.starts_with('.') {
                    ext
                } else {
                    format!(".{ext}")
                };
                path_ext == ext_with_dot
            })
        } else {
            false
        };

        if !kind_match {
            continue;
        }

        let specificity = path_cond.map_or(0, |c| c.len());

        if let Some((_, best_spec)) = best {
            if specificity > best_spec {
                best = Some((idx, specificity));
            }
            // 同具体度は先のルール（定義順）が勝つので更新しない
        } else {
            best = Some((idx, specificity));
        }
    }

    match best {
        Some((idx, _)) => &rules[idx].tools,
        None => &[],
    }
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

/// "ext:" プレフィックスの後の部分から拡張子リストとパス条件を分離する。
/// 例: "md,txt" → ("md,txt", None)
/// 例: ".md:C:\\projects" → (".md", Some("C:\\projects"))
/// ドライブレターパターン `:X:\` or `:X:/` でパス条件の開始を検出する。
fn split_ext_and_path(rest: &str) -> (&str, Option<&str>) {
    let bytes = rest.as_bytes();
    for i in 0..bytes.len().saturating_sub(2) {
        if bytes[i] == b':'
            && bytes[i + 1].is_ascii_alphabetic()
            && i + 2 < bytes.len()
            && (bytes[i + 2] == b':' || bytes[i + 2] == b'\\' || bytes[i + 2] == b'/')
        {
            let ext_part = &rest[..i];
            let path_part = rest[i + 1..].trim();
            if !path_part.is_empty() {
                return (ext_part, Some(path_part));
            }
        }
    }
    (rest, None)
}

fn normalize_opener_target(target: &str) -> String {
    let trimmed = target.trim();

    // folder, folder:<path>, ext:<exts>, ext:<exts>:<path>
    if let Some((kind, rest)) = trimmed.split_once(':') {
        if kind.eq_ignore_ascii_case("folder") {
            let path_trimmed = rest.trim();
            if path_trimmed.is_empty() {
                return "folder".to_string();
            }
            let normalized_path = normalize_scan_path_key(path_trimmed);
            if normalized_path.is_empty() {
                return "folder".to_string();
            }
            return format!("folder:{normalized_path}");
        }
        if kind.eq_ignore_ascii_case("ext") {
            // rest から拡張子部分とパス条件を分離
            let (raw_exts, path_suffix) = split_ext_and_path(rest);
            let exts = normalize_extensions(
                &raw_exts
                    .split(',')
                    .map(|ext| ext.to_string())
                    .collect::<Vec<_>>(),
            );
            let ext_str = exts.join(",");
            return if let Some(path) = path_suffix {
                let normalized_path = normalize_scan_path_key(path);
                if normalized_path.is_empty() {
                    format!("ext:{ext_str}")
                } else {
                    format!("ext:{ext_str}:{normalized_path}")
                }
            } else {
                format!("ext:{ext_str}")
            };
        }
    }

    if trimmed.eq_ignore_ascii_case("folder") {
        return "folder".to_string();
    }

    trimmed.to_string()
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

pub fn normalize_openers(openers: &[OpenerRule]) -> Vec<OpenerRule> {
    let mut result: Vec<OpenerRule> = Vec::new();
    let mut targets: Vec<String> = Vec::new();

    for rule in openers {
        let target = normalize_opener_target(&rule.target);

        if let Some(pos) = targets.iter().position(|existing| existing == &target) {
            result[pos].tools.extend(rule.tools.iter().cloned());
        } else {
            targets.push(target.clone());
            result.push(OpenerRule {
                target,
                tools: rule.tools.clone(),
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
            instant_commands: Vec::new(),
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
                if config.normalize_openers() {
                    needs_save = true;
                }
                // Fallback to default hotkey if config contains a system shortcut
                if is_system_shortcut(&config.hotkey.modifier, &config.hotkey.key) {
                    let default_hotkey = HotkeyConfig {
                        modifier: "Alt".to_string(),
                        key: "Q".to_string(),
                    };
                    eprintln!(
                        "[config] system shortcut detected ({}+{}), falling back to default ({}+{})",
                        config.hotkey.modifier, config.hotkey.key,
                        default_hotkey.modifier, default_hotkey.key,
                    );
                    config.hotkey = default_hotkey;
                    needs_save = true;
                }
                if needs_save {
                    let _ = config.save();
                }
                config
            }
            Err(_) => {
                let config = Self::default();
                let _ = config.save();
                config
            }
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = Self::config_dir().ok_or("設定ディレクトリが見つかりません")?;
        fs::create_dir_all(&dir).map_err(|e| format!("ディレクトリ作成失敗: {e}"))?;

        let path = Self::config_path().ok_or("設定パスが見つかりません")?;
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

// --- Opener presets ---

/// A detected opener preset available for one-click addition.
pub struct OpenerPreset {
    pub name: &'static str,
    pub exe: String,
    pub args: &'static str,
    pub target: &'static str,
}

/// Search for `filename` in PATH directories. Returns the full path if found.
fn find_in_path(filename: &str) -> Option<String> {
    let path_var = std::env::var("PATH").ok()?;
    for dir in path_var.split(';') {
        let candidate = Path::new(dir).join(filename);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

/// Detect opener presets available on this system.
/// Checks PATH and known install locations. Explorer is always included.
pub fn detect_opener_presets() -> Vec<OpenerPreset> {
    let mut presets = Vec::new();

    // VSCode: PATH 上の code.cmd、または既知のインストールパス
    let vscode_exe = find_in_path("code.cmd").or_else(|| {
        let local_app_data = std::env::var("LOCALAPPDATA").ok()?;
        let known = Path::new(&local_app_data)
            .join("Programs")
            .join("Microsoft VS Code")
            .join("Code.exe");
        if known.is_file() {
            Some(known.to_string_lossy().into_owned())
        } else {
            None
        }
    });
    if let Some(exe) = vscode_exe {
        presets.push(OpenerPreset {
            name: "Visual Studio Code",
            exe,
            args: "",
            target: "folder",
        });
    }

    // Windows Terminal: PATH 上の wt.exe
    if let Some(exe) = find_in_path("wt.exe") {
        presets.push(OpenerPreset {
            name: "Windows Terminal",
            exe,
            args: "-d {path}",
            target: "folder",
        });
    }

    // Explorer: 常に利用可能
    presets.push(OpenerPreset {
        name: "Explorer",
        exe: "explorer.exe".to_string(),
        args: "",
        target: "folder",
    });

    presets
}

/// Check if a preset's exe is already present in the opener rules (case-insensitive).
pub fn is_preset_already_added(openers: &[OpenerRule], preset_exe: &str) -> bool {
    let preset_lower = preset_exe.to_lowercase();
    openers.iter().any(|rule| {
        rule.tools
            .iter()
            .any(|tool| tool.exe.to_lowercase() == preset_lower)
    })
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
            instant_command_prefix: "@".to_string(),
            ..SearchConfig::default()
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

    #[test]
    fn normalize_opener_target_adds_dot_and_sorts_extensions() {
        assert_eq!(
            normalize_opener_target("ext: png, .JPG, gif , png"),
            "ext:.gif,.jpg,.png"
        );
    }

    #[test]
    fn normalize_openers_merges_equivalent_targets() {
        let openers = vec![
            make_rule("ext:png,jpg", &[("Viewer 1", "viewer.exe", "")]),
            make_rule("ext:.jpg,.png", &[("Viewer 2", "viewer2.exe", "")]),
        ];

        let normalized = normalize_openers(&openers);

        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].target, "ext:.jpg,.png");
        assert_eq!(normalized[0].tools.len(), 2);
        assert_eq!(normalized[0].tools[0].name, "Viewer 1");
        assert_eq!(normalized[0].tools[1].name, "Viewer 2");
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

    #[test]
    fn normalize_openers_returns_true_when_changed() {
        let mut config = Config::default();
        config.openers = vec![make_rule("ext:png,jpg", &[("Viewer", "viewer.exe", "")])];

        assert!(config.normalize_openers());
        assert_eq!(config.openers[0].target, "ext:.jpg,.png");
    }

    #[test]
    fn normalize_openers_returns_false_when_no_change() {
        let mut config = Config::default();
        config.openers = vec![make_rule("ext:.jpg,.png", &[("Viewer", "viewer.exe", "")])];

        assert!(!config.normalize_openers());
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

    // ---- path condition tests ----

    #[test]
    fn find_matching_tools_folder_with_path_condition() {
        let rules = vec![
            make_rule("folder:C:\\workspace", &[("VSCode", "code.cmd", "")]),
            make_rule("folder", &[("Explorer", "explorer.exe", "")]),
        ];
        let tools = find_matching_tools("C:\\workspace\\Snotra", true, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "VSCode");
    }

    #[test]
    fn find_matching_tools_folder_path_no_match_falls_back() {
        let rules = vec![
            make_rule("folder:C:\\workspace", &[("VSCode", "code.cmd", "")]),
            make_rule("folder", &[("Explorer", "explorer.exe", "")]),
        ];
        let tools = find_matching_tools("D:\\other\\dir", true, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "Explorer");
    }

    #[test]
    fn find_matching_tools_most_specific_path_wins() {
        let rules = vec![
            make_rule("folder:C:\\workspace", &[("VSCode", "code.cmd", "")]),
            make_rule("folder:C:\\workspace\\Snotra", &[("Terminal", "wt.exe", "-d {path}")]),
            make_rule("folder", &[("Explorer", "explorer.exe", "")]),
        ];
        let tools = find_matching_tools("C:\\workspace\\Snotra\\src", true, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "Terminal");
    }

    #[test]
    fn find_matching_tools_path_condition_case_insensitive() {
        let rules = vec![
            make_rule("folder:C:\\Workspace", &[("VSCode", "code.cmd", "")]),
        ];
        let tools = find_matching_tools("c:\\workspace\\project", true, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "VSCode");
    }

    #[test]
    fn find_matching_tools_ext_with_path_condition() {
        let rules = vec![
            make_rule("ext:md:C:\\projects", &[("VSCode", "code.cmd", "")]),
            make_rule("ext:md", &[("Typora", "typora.exe", "")]),
        ];
        let tools = find_matching_tools("C:\\projects\\readme.md", false, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "VSCode");
    }

    #[test]
    fn find_matching_tools_ext_path_no_match_falls_back() {
        let rules = vec![
            make_rule("ext:md:C:\\projects", &[("VSCode", "code.cmd", "")]),
            make_rule("ext:md", &[("Typora", "typora.exe", "")]),
        ];
        let tools = find_matching_tools("D:\\docs\\readme.md", false, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "Typora");
    }

    #[test]
    fn find_matching_tools_same_specificity_first_wins() {
        let rules = vec![
            make_rule("folder:C:\\a", &[("Tool1", "tool1.exe", "")]),
            make_rule("folder:C:\\b", &[("Tool2", "tool2.exe", "")]),
        ];
        // C:\a のパス → Tool1
        let tools = find_matching_tools("C:\\a\\sub", true, &rules);
        assert_eq!(tools[0].name, "Tool1");
    }

    #[test]
    fn find_matching_tools_path_condition_slash_normalized() {
        let rules = vec![
            make_rule("folder:C:\\workspace", &[("VSCode", "code.cmd", "")]),
        ];
        // パスにスラッシュが混在
        let tools = find_matching_tools("C:/workspace/project", true, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "VSCode");
    }

    #[test]
    fn find_matching_tools_path_condition_boundary_check() {
        let rules = vec![
            make_rule("folder:C:\\workspace", &[("VSCode", "code.cmd", "")]),
            make_rule("folder", &[("Explorer", "explorer.exe", "")]),
        ];
        // "C:\workspaces" はパス境界で一致しないのでフォールバック
        let tools = find_matching_tools("C:\\workspaces\\project", true, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "Explorer");
    }

    #[test]
    fn find_matching_tools_path_condition_exact_match() {
        let rules = vec![
            make_rule("folder:C:\\workspace", &[("VSCode", "code.cmd", "")]),
        ];
        // 完全一致もマッチする
        let tools = find_matching_tools("C:\\workspace", true, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "VSCode");
    }

    #[test]
    fn normalize_opener_target_folder_with_path() {
        assert_eq!(
            normalize_opener_target("folder:C:\\workspace"),
            "folder:c:\\workspace"
        );
    }

    #[test]
    fn normalize_opener_target_folder_with_empty_path() {
        assert_eq!(normalize_opener_target("folder:"), "folder");
        assert_eq!(normalize_opener_target("folder:  "), "folder");
    }

    #[test]
    fn normalize_opener_target_ext_with_path() {
        assert_eq!(
            normalize_opener_target("ext:md:C:\\projects"),
            "ext:.md:c:\\projects"
        );
    }

    #[test]
    fn normalize_opener_target_ext_with_path_normalizes_exts() {
        assert_eq!(
            normalize_opener_target("ext: PNG, .jpg :C:\\projects"),
            "ext:.jpg,.png:c:\\projects"
        );
    }

    #[test]
    fn split_ext_and_path_no_path() {
        assert_eq!(split_ext_and_path("md,txt"), ("md,txt", None));
    }

    #[test]
    fn split_ext_and_path_with_drive_path() {
        assert_eq!(
            split_ext_and_path(".md:C:\\projects"),
            (".md", Some("C:\\projects"))
        );
    }

    #[test]
    fn split_ext_and_path_forward_slash() {
        assert_eq!(
            split_ext_and_path(".md:D:/repos"),
            (".md", Some("D:/repos"))
        );
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
        let mut config = Config::default();
        config.instant_commands = vec![
            InstantCommand {
                name: "google".to_string(),
                command: "https://google.com".to_string(),
            },
            InstantCommand {
                name: "google".to_string(),
                command: "https://google.co.jp".to_string(),
            },
        ];
        let errors = config.validate();
        assert!(errors.contains(&ConfigError::InstantCommandDuplicateName {
            name: "google".to_string(),
        }));
    }

    #[test]
    fn validate_instant_command_unique_names_ok() {
        let mut config = Config::default();
        config.instant_commands = vec![
            InstantCommand {
                name: "google".to_string(),
                command: "https://google.com".to_string(),
            },
            InstantCommand {
                name: "bing".to_string(),
                command: "https://bing.com".to_string(),
            },
        ];
        let errors = config.validate();
        assert!(
            !errors.iter().any(|e| matches!(e, ConfigError::InstantCommandDuplicateName { .. })),
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
            max_results = 8
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
        let config: Config = toml::from_str(toml_str).expect("parse");
        assert_eq!(config.instant_commands.len(), 2);
        assert_eq!(config.instant_commands[0].name, "g");
        assert_eq!(config.instant_commands[1].name, "memo");
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
        assert_eq!(config.appearance.max_results, default.appearance.max_results);
    }

    #[test]
    fn valid_toml_invalid_values_caught_by_validate() {
        // Config with all required sections but invalid field values
        let toml_str = r#"
            [hotkey]
            modifier = ""
            key = ""

            [appearance]
            max_results = 0
            window_width = 50

            [paths]
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let errors = config.validate();
        // Should have errors for empty hotkey and invalid max_results/window_width
        assert!(errors.len() >= 2, "Expected at least 2 errors, got: {:?}", errors);
    }

    #[test]
    fn partial_toml_falls_back_to_default_via_unwrap_or_default() {
        // Partial TOML missing required sections → toml::from_str fails,
        // but Config::load() uses unwrap_or_default() to handle this.
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
        assert_eq!(config.appearance.max_results, default.appearance.max_results);
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
    fn detect_opener_presets_returns_at_least_explorer() {
        let presets = detect_opener_presets();
        assert!(
            presets.iter().any(|p| p.name == "Explorer"),
            "Explorer should always be present"
        );
    }

    #[test]
    fn detect_opener_presets_explorer_fields() {
        let presets = detect_opener_presets();
        let explorer = presets.iter().find(|p| p.name == "Explorer").unwrap();
        assert_eq!(explorer.exe, "explorer.exe");
        assert_eq!(explorer.args, "");
        assert_eq!(explorer.target, "folder");
    }

    #[test]
    fn find_in_path_returns_none_for_nonexistent() {
        assert!(find_in_path("__snotra_nonexistent_binary_xyz__").is_none());
    }

    #[test]
    fn find_matching_tools_path_condition_trailing_separator() {
        let rules = vec![
            make_rule("folder:C:\\workspace\\", &[("VSCode", "code.cmd", "")]),
            make_rule("folder", &[("Explorer", "explorer.exe", "")]),
        ];
        // 末尾 \ 付き条件は子孫パスにマッチする
        let tools = find_matching_tools("C:\\workspace\\Snotra\\src", true, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "VSCode");

        // 末尾 / 付き条件でも同様
        let rules2 = vec![
            make_rule("folder:C:\\workspace/", &[("VSCode", "code.cmd", "")]),
        ];
        let tools2 = find_matching_tools("C:\\workspace\\project", true, &rules2);
        assert_eq!(tools2.len(), 1);
        assert_eq!(tools2[0].name, "VSCode");
    }

    #[test]
    fn normalize_opener_target_folder_path_case_and_slash_normalized() {
        // 大文字小文字が正規化される
        assert_eq!(
            normalize_opener_target("folder:C:\\Workspace"),
            "folder:c:\\workspace"
        );
        // / → \ に正規化される
        assert_eq!(
            normalize_opener_target("folder:c:/workspace"),
            "folder:c:\\workspace"
        );
        // 末尾 \ が除去される
        assert_eq!(
            normalize_opener_target("folder:C:\\workspace\\"),
            "folder:c:\\workspace"
        );
        // ext のパス部分も正規化される
        assert_eq!(
            normalize_opener_target("ext:md:C:/Projects"),
            "ext:.md:c:\\projects"
        );
        // 区切り文字だけのパス条件は汎用ルールに畳み込まれる
        assert_eq!(normalize_opener_target("folder:\\"), "folder");
        assert_eq!(normalize_opener_target("folder:/"), "folder");
    }

    #[test]
    fn is_preset_already_added_case_insensitive() {
        let rules = vec![OpenerRule {
            target: "folder".to_string(),
            tools: vec![OpenerTool {
                name: "Explorer".to_string(),
                exe: "Explorer.EXE".to_string(),
                args: String::new(),
            }],
        }];
        assert!(is_preset_already_added(&rules, "explorer.exe"));
        assert!(!is_preset_already_added(&rules, "code.cmd"));
    }
}
