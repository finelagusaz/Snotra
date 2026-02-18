use snotra_core::config::HotkeyConfig;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN,
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
        s if s.len() == 1 => s.chars().next().unwrap().to_ascii_uppercase() as u32,
        _ => 0x20,
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
