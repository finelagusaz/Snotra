use snotra_core::config::HotkeyConfig;
use snotra_core::hotkey::{HotkeyKey, HotkeyParseError, ParsedHotkey};
use std::fmt;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
    UnregisterHotKey,
};

pub const HOTKEY_ID: i32 = 1;

/// parser 済みの値から導いた、登録と smoke 注入が共有する Win32 表現。
#[derive(Debug)]
struct PreparedHotkey {
    modifiers: HOT_KEY_MODIFIERS,
    vk: u32,
    injection_vks: Vec<u32>,
}

#[derive(Debug, PartialEq, Eq)]
enum PrepareError {
    Parse(HotkeyParseError),
    SystemShortcut,
}

impl fmt::Display for PrepareError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(formatter),
            Self::SystemShortcut => write!(formatter, "system shortcut conflict"),
        }
    }
}

fn key_vk(key: HotkeyKey) -> u32 {
    match key {
        HotkeyKey::A => 0x41,
        HotkeyKey::B => 0x42,
        HotkeyKey::C => 0x43,
        HotkeyKey::D => 0x44,
        HotkeyKey::E => 0x45,
        HotkeyKey::F => 0x46,
        HotkeyKey::G => 0x47,
        HotkeyKey::H => 0x48,
        HotkeyKey::I => 0x49,
        HotkeyKey::J => 0x4A,
        HotkeyKey::K => 0x4B,
        HotkeyKey::L => 0x4C,
        HotkeyKey::M => 0x4D,
        HotkeyKey::N => 0x4E,
        HotkeyKey::O => 0x4F,
        HotkeyKey::P => 0x50,
        HotkeyKey::Q => 0x51,
        HotkeyKey::R => 0x52,
        HotkeyKey::S => 0x53,
        HotkeyKey::T => 0x54,
        HotkeyKey::U => 0x55,
        HotkeyKey::V => 0x56,
        HotkeyKey::W => 0x57,
        HotkeyKey::X => 0x58,
        HotkeyKey::Y => 0x59,
        HotkeyKey::Z => 0x5A,
        HotkeyKey::Digit0 => 0x30,
        HotkeyKey::Digit1 => 0x31,
        HotkeyKey::Digit2 => 0x32,
        HotkeyKey::Digit3 => 0x33,
        HotkeyKey::Digit4 => 0x34,
        HotkeyKey::Digit5 => 0x35,
        HotkeyKey::Digit6 => 0x36,
        HotkeyKey::Digit7 => 0x37,
        HotkeyKey::Digit8 => 0x38,
        HotkeyKey::Digit9 => 0x39,
        HotkeyKey::Space => 0x20,
        HotkeyKey::Enter => 0x0D,
        HotkeyKey::Tab => 0x09,
        HotkeyKey::Backspace => 0x08,
        HotkeyKey::Escape => 0x1B,
        HotkeyKey::F1 => 0x70,
        HotkeyKey::F2 => 0x71,
        HotkeyKey::F3 => 0x72,
        HotkeyKey::F4 => 0x73,
        HotkeyKey::F5 => 0x74,
        HotkeyKey::F6 => 0x75,
        HotkeyKey::F7 => 0x76,
        HotkeyKey::F8 => 0x77,
        HotkeyKey::F9 => 0x78,
        HotkeyKey::F10 => 0x79,
        HotkeyKey::F11 => 0x7A,
        HotkeyKey::F12 => 0x7B,
        HotkeyKey::Home => 0x24,
        HotkeyKey::End => 0x23,
        HotkeyKey::PageUp => 0x21,
        HotkeyKey::PageDown => 0x22,
        HotkeyKey::Insert => 0x2D,
        HotkeyKey::Delete => 0x2E,
    }
}

impl From<ParsedHotkey> for PreparedHotkey {
    fn from(parsed: ParsedHotkey) -> Self {
        const VK_SHIFT: u32 = 0x10;
        const VK_CONTROL: u32 = 0x11;
        const VK_MENU: u32 = 0x12;
        const VK_LWIN: u32 = 0x5B;

        let modifiers = parsed.modifiers();
        let mut native_modifiers = MOD_NOREPEAT;
        let mut injection_vks = Vec::new();
        if modifiers.has_ctrl() {
            native_modifiers |= MOD_CONTROL;
            injection_vks.push(VK_CONTROL);
        }
        if modifiers.has_shift() {
            native_modifiers |= MOD_SHIFT;
            injection_vks.push(VK_SHIFT);
        }
        if modifiers.has_alt() {
            native_modifiers |= MOD_ALT;
            injection_vks.push(VK_MENU);
        }
        if modifiers.has_win() {
            native_modifiers |= MOD_WIN;
            injection_vks.push(VK_LWIN);
        }
        let vk = key_vk(parsed.key());
        injection_vks.push(vk);
        Self {
            modifiers: native_modifiers,
            vk,
            injection_vks,
        }
    }
}

fn prepare(config: &HotkeyConfig) -> Result<PreparedHotkey, PrepareError> {
    let parsed = config.parse().map_err(PrepareError::Parse)?;
    if parsed.is_system_shortcut() {
        return Err(PrepareError::SystemShortcut);
    }
    Ok(PreparedHotkey::from(parsed))
}

fn trace_registration(config: &HotkeyConfig, vks: &[u32], ok: bool) {
    crate::trace::trace(
        "hotkey:registered",
        serde_json::json!({
            "modifier": config.modifier,
            "key": config.key,
            "vks": vks,
            "ok": ok,
        }),
    );
}

fn register_prepared(config: &HotkeyConfig, prepared: &PreparedHotkey) -> bool {
    let ok = unsafe {
        RegisterHotKey(
            Some(HWND::default()),
            HOTKEY_ID,
            prepared.modifiers,
            prepared.vk,
        )
    }
    .is_ok();
    trace_registration(config, &prepared.injection_vks, ok);
    ok
}

pub fn register(config: &HotkeyConfig) -> bool {
    match prepare(config) {
        Ok(prepared) => register_prepared(config, &prepared),
        Err(error) => {
            eprintln!(
                "[hotkey] invalid config ({}+{}): {error}",
                config.modifier, config.key
            );
            trace_registration(config, &[], false);
            false
        }
    }
}

/// 新設定を parse できた場合だけ現登録を解除し、登録失敗時は旧設定を復元する。
pub fn replace(current: &HotkeyConfig, new: &HotkeyConfig) -> bool {
    let prepared = match prepare(new) {
        Ok(prepared) => prepared,
        Err(error) => {
            eprintln!(
                "[hotkey] invalid config ({}+{}): {error}",
                new.modifier, new.key
            );
            trace_registration(new, &[], false);
            return false;
        }
    };

    unregister();
    if register_prepared(new, &prepared) {
        true
    } else {
        let _ = register(current);
        false
    }
}

pub fn unregister() {
    let _ = unsafe { UnregisterHotKey(Some(HWND::default()), HOTKEY_ID) };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(modifier: &str, key: &str) -> HotkeyConfig {
        HotkeyConfig {
            modifier: modifier.into(),
            key: key.into(),
        }
    }

    #[test]
    fn prepare_rejects_unknown_modifier_and_unsupported_key_before_native_call() {
        assert!(prepare(&config("Hyper", "Q")).is_err());
        assert!(prepare(&config("Alt", "!")).is_err());
        assert!(prepare(&config("Alt", "F13")).is_err());
        assert_eq!(
            prepare(&config("Alt+Alt", "F4")).unwrap_err(),
            PrepareError::SystemShortcut
        );
    }

    #[test]
    fn register_rejects_invalid_config_without_reaching_register_hotkey() {
        // `register` は prepare 成功後にだけ `register_prepared`（Win32 API 所有点）を呼ぶ。
        assert!(!register(&config("Hyper", "Q")));
        assert!(!register(&config("Alt", "!")));
    }

    #[test]
    fn prepared_alt_q_has_matching_registration_and_injection_values() {
        let prepared = prepare(&config("Alt", "Q")).unwrap();
        assert_eq!(prepared.modifiers, MOD_NOREPEAT | MOD_ALT);
        assert_eq!(prepared.vk, 0x51);
        assert_eq!(prepared.injection_vks, vec![0x12, 0x51]);
    }

    #[test]
    fn prepared_modifiers_are_deduplicated_and_press_ordered() {
        let prepared = prepare(&config("Alt+Ctrl+Alt+Shift", "A")).unwrap();
        assert_eq!(
            prepared.modifiers,
            MOD_NOREPEAT | MOD_CONTROL | MOD_SHIFT | MOD_ALT
        );
        assert_eq!(prepared.injection_vks, vec![0x11, 0x10, 0x12, 0x41]);
    }

    #[test]
    fn prepared_named_key_aliases_use_the_same_typed_mapping() {
        for key in ["Delete", "del"] {
            let prepared = prepare(&config("Alt", key)).unwrap();
            assert_eq!(prepared.vk, 0x2E, "key={key}");
            assert_eq!(prepared.injection_vks, vec![0x12, 0x2E], "key={key}");
        }
    }
}
