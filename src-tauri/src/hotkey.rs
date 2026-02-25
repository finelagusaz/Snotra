use snotra_core::config::HotkeyConfig;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
    UnregisterHotKey,
};

pub const HOTKEY_ID: i32 = 1;

pub fn parse_modifier(s: &str) -> HOT_KEY_MODIFIERS {
    let mut mods = MOD_NOREPEAT;
    for part in s.split('+').map(|p| p.trim()) {
        match part.to_lowercase().as_str() {
            "alt" => mods |= MOD_ALT,
            "ctrl" | "control" => mods |= MOD_CONTROL,
            "shift" => mods |= MOD_SHIFT,
            "win" | "super" => mods |= MOD_WIN,
            _ => {}
        }
    }
    mods
}

pub fn parse_vk(s: &str) -> u32 {
    match s.to_lowercase().as_str() {
        "space" => 0x20,
        "enter" | "return" => 0x0D,
        "tab" => 0x09,
        "backspace" => 0x08,
        "escape" | "esc" => 0x1B,
        // ファンクションキー
        "f1"  => 0x70, "f2"  => 0x71, "f3"  => 0x72, "f4"  => 0x73,
        "f5"  => 0x74, "f6"  => 0x75, "f7"  => 0x76, "f8"  => 0x77,
        "f9"  => 0x78, "f10" => 0x79, "f11" => 0x7A, "f12" => 0x7B,
        // ナビゲーションキー
        "home"     => 0x24, "end"      => 0x23,
        "pageup"   => 0x21, "pagedown" => 0x22,
        "insert"   => 0x2D, "delete"   => 0x2E,
        // 単一文字キー
        s if s.len() == 1 => s.chars().next().unwrap().to_ascii_uppercase() as u32,
        // 不明なキー → 0（RegisterHotKey が失敗し、呼び出し元がエラー検出できる）
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_vk_unknown_returns_zero() {
        assert_eq!(parse_vk("f13"), 0);
        assert_eq!(parse_vk(""), 0);
        assert_eq!(parse_vk("xyz_invalid"), 0);
    }

    #[test]
    fn parse_vk_known_keys() {
        assert_eq!(parse_vk("f1"), 0x70);
        assert_eq!(parse_vk("F12"), 0x7B);
        assert_eq!(parse_vk("home"), 0x24);
        assert_eq!(parse_vk("Q"), b'Q' as u32);
        assert_eq!(parse_vk("space"), 0x20);
        assert_eq!(parse_vk("enter"), 0x0D);
        assert_eq!(parse_vk("pageup"), 0x21);
        assert_eq!(parse_vk("delete"), 0x2E);
    }
}

pub fn register(config: &HotkeyConfig) -> bool {
    let modifiers = parse_modifier(&config.modifier);
    let vk = parse_vk(&config.key);
    unsafe { RegisterHotKey(Some(HWND::default()), HOTKEY_ID, modifiers, vk) }.is_ok()
}

pub fn unregister() {
    let _ = unsafe { UnregisterHotKey(Some(HWND::default()), HOTKEY_ID) };
}
