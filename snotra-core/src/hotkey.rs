//! ホットキー設定文字列の解析と、Windows システムショートカット競合判定。
//!
//! 永続形式の [`HotkeyConfig`] を OS 非依存の [`ParsedHotkey`] へ変換する。文字列の別名・
//! 大文字小文字・区切りの解釈はこのモジュールだけが所有し、Win32 の modifier bit / VK への
//! 変換は `src-tauri` の platform 層に残す。

use serde::{Deserialize, Serialize};
use std::fmt;

/// `config.toml` に保存するホットキー表現。
///
/// 文字列形式は後方互換のため維持し、利用前に必ず [`Self::parse`] で意味型へ変換する。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyConfig {
    #[serde(default = "default_hotkey_modifier")]
    pub modifier: String,
    #[serde(default = "default_hotkey_key")]
    pub key: String,
}

fn default_hotkey_modifier() -> String {
    "Alt".to_string()
}

fn default_hotkey_key() -> String {
    "Q".to_string()
}

impl Default for HotkeyConfig {
    /// **既定ホットキーのリテラルは `default_hotkey_modifier` / `default_hotkey_key` の
    /// 各 1 か所だけである**（#795）。`Config::default()`・ロード時の不正ホットキー補正・
    /// serde のキー欠落補完（#824）はすべてその 2 関数を読む。
    fn default() -> Self {
        Self {
            modifier: default_hotkey_modifier(),
            key: default_hotkey_key(),
        }
    }
}

/// 解析済みホットキーのメインキー。
///
/// payload を持つ公開 variant を置かず、parser を通らない不正値を構築不能にする。
/// platform 層は全 variant を網羅して Win32 VK へ変換する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyKey {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Space,
    Enter,
    Tab,
    Backspace,
    Escape,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
}

/// 永続文字列を正規化した、OS 非依存のホットキー表現。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyModifiers {
    ctrl: bool,
    alt: bool,
    shift: bool,
    win: bool,
}

impl HotkeyModifiers {
    pub fn has_ctrl(self) -> bool {
        self.ctrl
    }

    pub fn has_alt(self) -> bool {
        self.alt
    }

    pub fn has_shift(self) -> bool {
        self.shift
    }

    pub fn has_win(self) -> bool {
        self.win
    }
}

/// parser が生成する、modifier 集合とメインキーの組。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedHotkey {
    modifiers: HotkeyModifiers,
    key: HotkeyKey,
}

impl ParsedHotkey {
    pub fn modifiers(self) -> HotkeyModifiers {
        self.modifiers
    }

    pub fn key(self) -> HotkeyKey {
        self.key
    }

    /// Windows が予約する、または Snotra が奪ってはならない組み合わせかを返す。
    ///
    /// modifier は parser で集合化済みなので、重複表記（`Alt+Alt`）で判定を回避できない。
    pub fn is_system_shortcut(self) -> bool {
        if self.modifiers.win {
            return true;
        }

        let alt_only = self.modifiers.alt && !self.modifiers.ctrl && !self.modifiers.shift;
        let ctrl_alt_only = self.modifiers.ctrl && self.modifiers.alt && !self.modifiers.shift;
        let ctrl_shift_only = self.modifiers.ctrl && self.modifiers.shift && !self.modifiers.alt;

        (alt_only && matches!(self.key, HotkeyKey::F4 | HotkeyKey::Space | HotkeyKey::Tab))
            || (ctrl_alt_only && self.key == HotkeyKey::Delete)
            || (ctrl_shift_only && self.key == HotkeyKey::Escape)
    }
}

/// ホットキー設定文字列を意味型へ変換できない理由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyParseError {
    ModifierEmpty,
    UnknownModifier { modifier: String },
    KeyEmpty,
    UnsupportedKey { key: String },
}

impl fmt::Display for HotkeyParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ModifierEmpty => write!(f, "hotkey modifier is empty"),
            Self::UnknownModifier { modifier } => {
                write!(f, "unknown hotkey modifier: {modifier}")
            }
            Self::KeyEmpty => write!(f, "hotkey key is empty"),
            Self::UnsupportedKey { key } => write!(f, "unsupported hotkey key: {key}"),
        }
    }
}

impl std::error::Error for HotkeyParseError {}

impl HotkeyConfig {
    /// 永続文字列を、設定検証と platform 登録が共有する意味型へ変換する。
    ///
    /// - modifier は `+` 分割、trim、ASCII case-insensitive。重複と空 segment は無視する。
    /// - 既知 modifier と未知語が混在していても、未知語を黙って捨てず全体をエラーにする。
    /// - 単一文字キーは Win32 VK と値が一致する ASCII 英数字だけを受理する。
    pub fn parse(&self) -> Result<ParsedHotkey, HotkeyParseError> {
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut win = false;

        for raw_part in self.modifier.split('+') {
            let part = raw_part.trim();
            if part.is_empty() {
                continue;
            }
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "alt" => alt = true,
                "shift" => shift = true,
                "win" | "super" | "meta" => win = true,
                _ => {
                    return Err(HotkeyParseError::UnknownModifier {
                        modifier: part.to_string(),
                    });
                }
            }
        }

        if !ctrl && !alt && !shift && !win {
            return Err(HotkeyParseError::ModifierEmpty);
        }

        let raw_key = self.key.trim();
        if raw_key.is_empty() {
            return Err(HotkeyParseError::KeyEmpty);
        }
        let normalized_key = raw_key.to_ascii_lowercase();
        let key = match normalized_key.as_str() {
            "space" => HotkeyKey::Space,
            "enter" | "return" => HotkeyKey::Enter,
            "tab" => HotkeyKey::Tab,
            "backspace" => HotkeyKey::Backspace,
            "escape" | "esc" => HotkeyKey::Escape,
            "f1" => HotkeyKey::F1,
            "f2" => HotkeyKey::F2,
            "f3" => HotkeyKey::F3,
            "f4" => HotkeyKey::F4,
            "f5" => HotkeyKey::F5,
            "f6" => HotkeyKey::F6,
            "f7" => HotkeyKey::F7,
            "f8" => HotkeyKey::F8,
            "f9" => HotkeyKey::F9,
            "f10" => HotkeyKey::F10,
            "f11" => HotkeyKey::F11,
            "f12" => HotkeyKey::F12,
            "home" => HotkeyKey::Home,
            "end" => HotkeyKey::End,
            "pageup" => HotkeyKey::PageUp,
            "pagedown" => HotkeyKey::PageDown,
            "insert" => HotkeyKey::Insert,
            "delete" | "del" => HotkeyKey::Delete,
            "a" => HotkeyKey::A,
            "b" => HotkeyKey::B,
            "c" => HotkeyKey::C,
            "d" => HotkeyKey::D,
            "e" => HotkeyKey::E,
            "f" => HotkeyKey::F,
            "g" => HotkeyKey::G,
            "h" => HotkeyKey::H,
            "i" => HotkeyKey::I,
            "j" => HotkeyKey::J,
            "k" => HotkeyKey::K,
            "l" => HotkeyKey::L,
            "m" => HotkeyKey::M,
            "n" => HotkeyKey::N,
            "o" => HotkeyKey::O,
            "p" => HotkeyKey::P,
            "q" => HotkeyKey::Q,
            "r" => HotkeyKey::R,
            "s" => HotkeyKey::S,
            "t" => HotkeyKey::T,
            "u" => HotkeyKey::U,
            "v" => HotkeyKey::V,
            "w" => HotkeyKey::W,
            "x" => HotkeyKey::X,
            "y" => HotkeyKey::Y,
            "z" => HotkeyKey::Z,
            "0" => HotkeyKey::Digit0,
            "1" => HotkeyKey::Digit1,
            "2" => HotkeyKey::Digit2,
            "3" => HotkeyKey::Digit3,
            "4" => HotkeyKey::Digit4,
            "5" => HotkeyKey::Digit5,
            "6" => HotkeyKey::Digit6,
            "7" => HotkeyKey::Digit7,
            "8" => HotkeyKey::Digit8,
            "9" => HotkeyKey::Digit9,
            _ => {
                return Err(HotkeyParseError::UnsupportedKey {
                    key: raw_key.to_string(),
                });
            }
        };

        Ok(ParsedHotkey {
            modifiers: HotkeyModifiers {
                ctrl,
                alt,
                shift,
                win,
            },
            key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(modifier: &str, key: &str) -> Result<ParsedHotkey, HotkeyParseError> {
        HotkeyConfig {
            modifier: modifier.to_string(),
            key: key.to_string(),
        }
        .parse()
    }

    /// **保証は狭い**: modifier alias の一覧を持つのは `parse()` の非網羅 match（`_ =>` を持つ）で
    /// あり、ここに並ぶのはその標本である。match へ alias を足してもここへ書き足さなければ検査対象に
    /// ならない（2026-08-09 実測: `| "cmd"` を足しても本モジュールのテストは全数緑・#1008）——
    /// 「将来の追加を守る」機構ではなく、現に書いた alias が同じ 1 つの modifier へ解決されること
    /// （順序・重複・空セグメントに揺れないこと）を固定するだけ。
    ///
    /// **本モジュールの alias / key 系テストはどれも同じ形である**——射程の正本はここに置く。
    #[test]
    fn modifier_aliases_order_duplicates_and_empty_segments_form_one_set() {
        let parsed = parse(" Shift + control + Ctrl ++ Alt ", "Q").unwrap();
        assert!(parsed.modifiers().has_ctrl());
        assert!(parsed.modifiers().has_alt());
        assert!(parsed.modifiers().has_shift());
        assert!(!parsed.modifiers().has_win());

        for alias in ["Win", "Super", "Meta"] {
            assert!(
                parse(alias, "Q").unwrap().modifiers().has_win(),
                "alias={alias}"
            );
        }
    }

    #[test]
    fn unknown_modifier_is_not_silently_discarded() {
        assert_eq!(
            parse("Hyper", "Q"),
            Err(HotkeyParseError::UnknownModifier {
                modifier: "Hyper".to_string()
            })
        );
        assert_eq!(
            parse("Ctrl+Hyper", "Q"),
            Err(HotkeyParseError::UnknownModifier {
                modifier: "Hyper".to_string()
            })
        );
    }

    /// **保証は狭い**（射程の正本は `modifier_aliases_order_duplicates_and_empty_segments_form_one_set`
    /// の doc）: key alias も `parse()` の非網羅 match が持ち、ここに並ぶのは標本である
    /// （2026-08-09 実測: Enter へ 3 つ目の alias を足しても緑・#1008）。
    #[test]
    fn key_aliases_share_one_semantic_key() {
        for alias in ["Enter", "return"] {
            assert_eq!(parse("Alt", alias).unwrap().key(), HotkeyKey::Enter);
        }
        for alias in ["Escape", "esc"] {
            assert_eq!(parse("Alt", alias).unwrap().key(), HotkeyKey::Escape);
        }
        for alias in ["Delete", "del"] {
            assert_eq!(parse("Alt", alias).unwrap().key(), HotkeyKey::Delete);
        }
    }

    /// **保証は狭い**（射程の正本は `modifier_aliases_order_duplicates_and_empty_segments_form_one_set`
    /// の doc）: `HotkeyKey` の variant のうち下に並ぶ `cases` だけを見る抜き取りであり、大小文字を
    /// 問わないことをその範囲で固定する。母集団は `parse()` の非網羅な文字列 match なので、
    /// **既存 variant への alias 追加はここでは止まらない**（2026-08-09 実測・#1008）。
    /// variant の新設だけは下流 `src-tauri` の `key_vk()` が網羅 match ゆえ E0004 で止めるが、
    /// それはこの検査の働きではない。
    #[test]
    fn supported_key_set_parses_case_insensitively() {
        let cases = [
            ("q", HotkeyKey::Q),
            ("7", HotkeyKey::Digit7),
            ("SPACE", HotkeyKey::Space),
            ("Tab", HotkeyKey::Tab),
            ("Backspace", HotkeyKey::Backspace),
            ("F1", HotkeyKey::F1),
            ("f12", HotkeyKey::F12),
            ("Home", HotkeyKey::Home),
            ("End", HotkeyKey::End),
            ("PageUp", HotkeyKey::PageUp),
            ("PageDown", HotkeyKey::PageDown),
            ("Insert", HotkeyKey::Insert),
        ];
        for (input, expected) in cases {
            assert_eq!(
                parse("Alt", input).unwrap().key(),
                expected,
                "input={input}"
            );
        }
    }

    #[test]
    fn unsupported_key_does_not_produce_a_raw_vk() {
        for key in ["F0", "F13", "xyz", "é", "あ", "１"] {
            assert_eq!(
                parse("Alt", key),
                Err(HotkeyParseError::UnsupportedKey {
                    key: key.to_string()
                }),
                "key={key}"
            );
        }

        for byte in 0_u8..=127 {
            let ch = char::from(byte);
            if !ch.is_ascii_punctuation() {
                continue;
            }
            let key = ch.to_string();
            assert_eq!(
                parse("Alt", &key),
                Err(HotkeyParseError::UnsupportedKey { key: key.clone() }),
                "ASCII punctuation must be rejected: {key:?}"
            );
        }
    }

    #[test]
    fn empty_modifier_or_key_is_rejected() {
        assert_eq!(parse("++", "Q"), Err(HotkeyParseError::ModifierEmpty));
        assert_eq!(parse("Alt", "  "), Err(HotkeyParseError::KeyEmpty));
    }

    #[test]
    fn persisted_hotkey_shape_round_trips_without_rewriting_aliases() {
        let original = HotkeyConfig {
            modifier: " Control++Control ".to_string(),
            key: "return".to_string(),
        };
        let encoded = toml::to_string(&original).unwrap();
        let decoded: HotkeyConfig = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded, original);
        assert!(decoded.parse().is_ok());
    }

    /// **保証は狭い**: 判定の本体は `is_system_shortcut()` にあり、下の `blocked` はその標本である。
    /// 判定へ組み合わせを足してもここへ書き足さなければ検査対象にならない（2026-08-09 実測:
    /// `alt_only && key == Home` を足しても本モジュールのテストは全数緑・#1008）。見ているのは
    /// **判定が意味解析の後に走ること**（alias・重複・大小文字の揺れを吸収した形で当たること）で
    /// あって、**予約ショートカット一覧の網羅ではない。**
    #[test]
    fn system_shortcuts_are_checked_after_semantic_normalization() {
        let blocked = [
            ("Alt", "F4"),
            ("Alt+Alt", "F4"),
            ("Alt", "Space"),
            ("Alt", "Tab"),
            ("Control+Alt", "del"),
            ("Shift+Ctrl", "Esc"),
            ("Meta", "Q"),
        ];
        for (modifier, key) in blocked {
            assert!(
                parse(modifier, key).unwrap().is_system_shortcut(),
                "expected blocked: {modifier}+{key}"
            );
        }
    }

    #[test]
    fn non_conflicting_extra_modifiers_remain_allowed() {
        for (modifier, key) in [
            ("Alt", "Q"),
            ("Alt+Shift", "F4"),
            ("Alt+Shift", "Space"),
            ("Ctrl", "Space"),
            ("Alt", "Escape"),
        ] {
            assert!(
                !parse(modifier, key).unwrap().is_system_shortcut(),
                "expected allowed: {modifier}+{key}"
            );
        }
    }

    #[test]
    fn default_hotkey_is_parseable_and_allowed() {
        let parsed = HotkeyConfig::default().parse().unwrap();
        assert!(!parsed.is_system_shortcut());
    }
}
