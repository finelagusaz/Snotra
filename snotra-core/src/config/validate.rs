//! 保存前の整合性検証。
//!
//! **検出だけを行い、補正はしない**——不正値を既定値へ戻すのは `super::migrate` の責務である
//! （責務分離の経緯は #437）。import は自動補正より前に [`Config::validate_hotkey`] を呼び、
//! 不正なバックアップを黙って既定値へ置き換えない。

use crate::error::ConfigError;
use crate::hotkey::HotkeyParseError;

use super::Config;
use super::schema::InstantAction;

impl Config {
    /// Validates config consistency. Call before save.
    pub fn validate(&self) -> Vec<ConfigError> {
        let mut errors = self.validate_hotkey();

        // Appearance validation
        if self.appearance.effective_visible_rows() == 0 {
            errors.push(ConfigError::VisibleRowsZero);
        }
        if self.appearance.window_width < 200 {
            errors.push(ConfigError::WindowWidthTooSmall(
                self.appearance.window_width,
            ));
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

    /// 永続文字列のままのホットキーを検証する。
    ///
    /// import は自動補正を行う `apply_migrations()` より前にこれを呼び、不正なバックアップを
    /// 黙って既定値へ置換しない。modifier と key が両方空なら従来どおり 2 件を返す。
    pub fn validate_hotkey(&self) -> Vec<ConfigError> {
        let mut errors = Vec::new();
        if self
            .hotkey
            .modifier
            .split('+')
            .all(|part| part.trim().is_empty())
        {
            errors.push(ConfigError::HotkeyModifierEmpty);
        }
        if self.hotkey.key.trim().is_empty() {
            errors.push(ConfigError::HotkeyKeyEmpty);
        }
        if !errors.is_empty() {
            return errors;
        }

        match self.hotkey.parse() {
            Ok(parsed) if parsed.is_system_shortcut() => {
                errors.push(ConfigError::HotkeySystemConflict {
                    modifier: self.hotkey.modifier.clone(),
                    key: self.hotkey.key.clone(),
                });
            }
            Ok(_) => {}
            Err(HotkeyParseError::ModifierEmpty) => {
                errors.push(ConfigError::HotkeyModifierEmpty);
            }
            Err(HotkeyParseError::UnknownModifier { modifier }) => {
                errors.push(ConfigError::HotkeyUnknownModifier { modifier });
            }
            Err(HotkeyParseError::KeyEmpty) => {
                errors.push(ConfigError::HotkeyKeyEmpty);
            }
            Err(HotkeyParseError::UnsupportedKey { key }) => {
                errors.push(ConfigError::HotkeyUnsupportedKey { key });
            }
        }
        errors
    }
}

#[cfg(test)]
mod tests;
