//! `config.toml` のセクション型・要素型と、その既定値。
//!
//! ここが持つのは「TOML の形」である——読み書き（`super::io`）・移行（`super::migrate`）・
//! 検証（`super::validate`）はこの形を入力に取る側で、逆向きの依存を持たない。
//!
//! **新しいセクション・設定キーには serde の既定を付ける**（欠けた 1 キーが `config.toml` 全体を
//! `.bak` 退避へ落とさないための不変条件。射程と検知器は `snotra-core/CLAUDE.md`）。

use serde::{Deserialize, Serialize};

use super::Config;

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
    Full, // チェック + インストール（インストーラー版向け）
    CheckOnly, // チェックのみ・通知する（ポータブル版向け）
    Disabled,  // チェックしない
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
    Url {
        url: String,
    },
    Exec {
        exe: String,
        #[serde(default)]
        args: String,
    },
    Legacy {
        command: String,
    },
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
            hotkey_toggle: default_hotkey_toggle(),
            show_on_startup: default_show_on_startup(),
            auto_hide_on_focus_lost: default_auto_hide_on_focus_lost(),
            show_tray_icon: default_show_tray_icon(),
            ime_off_on_show: default_ime_off_on_show(),
            follow_cursor_monitor: default_follow_cursor_monitor(),
            // `auto_update` だけは名前つき既定関数を持たず `#[serde(default)]` ——
            // serde が使うのと同じ `AutoUpdateMode::default()`（= `#[default]` の Full）を通す
            auto_update: AutoUpdateMode::default(),
        }
    }
}

pub(super) fn default_result_limit() -> usize {
    200
}

pub(super) fn default_recent_limit() -> usize {
    8
}

pub(super) fn default_visible_rows() -> usize {
    8
}

fn default_show_icons() -> bool {
    true
}

fn default_window_width() -> u32 {
    600
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

pub(super) fn default_fuzzy_history_cap_ratio() -> f64 {
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
            normal_mode: default_search_mode(),
            folder_mode: default_search_mode(),
            show_hidden_system: default_show_hidden_system(),
            history_normalization: default_history_normalization(),
            fuzzy_history_cap_ratio: default_fuzzy_history_cap_ratio(),
            instant_command_prefix: default_instant_command_prefix(),
            // `migemo_enabled` / `include_path_env` は名前つき既定関数を持たず `#[serde(default)]`
            // ——serde が使うのと同じ `bool::default()`（= false）であり、無い関数を新設しない
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
    #[serde(default = "default_window_width")]
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

impl Default for AppearanceConfig {
    /// 全フィールドが serde の既定関数を経由する（`window_width` の既定リテラルは
    /// `default_window_width` が持つ唯一の定義点である・#795、#824）。
    ///
    /// legacy な `Option` 3 本は **`None` でなければならない**——`Some(v)` にすると
    /// `migrate_legacy_count_params` が黙って `visible_rows` へ昇格させる。
    fn default() -> Self {
        Self {
            visible_rows: None,
            window_width: default_window_width(),
            show_icons: default_show_icons(),
            max_results: None,
            top_n_history: None,
            max_history_display: None,
        }
    }
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

/// #646 PR1: 行高の余白(row_height = font_size + path_size + row_padding + 4)。
fn default_row_padding() -> u32 {
    6
}

/// #646 PR1: バー高の余白(bar_height = font_size + bar_padding)。28 は現行 52px を
/// 「font_size=24 でのチューニング結果」と読み直した値(24 + 28 = 52)。
fn default_bar_padding() -> u32 {
    28
}

/// #646 PR2: メイン窓と結果窓の隙間 px(透明ギャップ・決定 6)。
fn default_window_gap() -> u32 {
    4
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
}

impl Default for CustomTheme {
    /// 5 色の既定リテラルは `default_*_color()` が持つ（`VisualConfig` と共有する
    /// 同じ関数である）。`#[derive(Default)]` は使えない——`String::default()` は
    /// `""` であって色ではない。
    fn default() -> Self {
        Self {
            background_color: default_background_color(),
            input_background_color: default_input_background_color(),
            text_color: default_text_color(),
            selected_row_color: default_selected_row_color(),
            hint_text_color: default_hint_text_color(),
        }
    }
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
    #[serde(default = "default_row_padding")]
    pub row_padding: u32,
    #[serde(default = "default_bar_padding")]
    pub bar_padding: u32,
    #[serde(default = "default_window_gap")]
    pub window_gap: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_theme: Option<CustomTheme>,
}

impl Default for VisualConfig {
    fn default() -> Self {
        Self {
            preset: default_theme_preset(),
            background_color: default_background_color(),
            input_background_color: default_input_background_color(),
            text_color: default_text_color(),
            selected_row_color: default_selected_row_color(),
            hint_text_color: default_hint_text_color(),
            font_family: default_font_family(),
            font_size: default_font_size(),
            row_padding: default_row_padding(),
            bar_padding: default_bar_padding(),
            window_gap: default_window_gap(),
            custom_theme: None,
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
    /// 可視行 = `visible_rows`）の
    /// `ICON_CACHE_RETENTION_FACTOR` 倍。
    ///
    /// 独立した設定キーを持たず派生値とすることで「上限 ≥ ワーキングセット」が**構造的に成立**し
    /// （検証・floor・drift が不要）、`result_limit` 変更時は上限も自動追従する。単一の
    /// `get_icons_batch` が自己 evict することはない。倍率で再検索時の再抽出を抑えつつ無制限増加を止める。
    ///
    /// **この上限が対応するのは「保持」集合であって「抽出」範囲ではない。** UI がアイコン抽出を
    /// 要求するのは viewport 近傍だけだが（`src-tauri` の `egui_shell::layout::icon_prefetch_range`）、
    /// テクスチャとキャッシュは結果リスト全件ぶん保持する。**要求が減ったことを理由にこの上限を
    /// 縮めてはならない**——縮めるとスクロールや再検索のたびに再抽出が走る。
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
        working_set
            .max(1)
            .saturating_mul(ICON_CACHE_RETENTION_FACTOR)
    }
}

#[cfg(test)]
mod tests;
