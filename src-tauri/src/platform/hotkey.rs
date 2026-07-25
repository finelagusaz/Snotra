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

/// 注入用（`keybd_event`）の VK 列を押下順で返す。**解放は逆順**で行うこと。
///
/// `RegisterHotKey` の修飾ビット（`MOD_ALT` 等）と注入用 VK（`VK_MENU` 等）は別の値で
/// あり、その対応はここが唯一の場所である。smoke（`scripts/smoke-egui.ps1`）は
/// `hotkey:registered` trace 経由でこの列を受け取り、**自前の対応表を持たない**。
/// 修飾の解析は `parse_modifier` を、キーの解決は `parse_vk` を再利用する（表を 2 つにしない）。
pub fn injection_vks(config: &HotkeyConfig) -> Vec<u32> {
    const VK_SHIFT: u32 = 0x10;
    const VK_CONTROL: u32 = 0x11;
    const VK_MENU: u32 = 0x12; // Alt
    const VK_LWIN: u32 = 0x5B;
    let mods = parse_modifier(&config.modifier);
    let has = |m: HOT_KEY_MODIFIERS| (mods.0 & m.0) != 0;
    let mut vks = Vec::new();
    if has(MOD_CONTROL) {
        vks.push(VK_CONTROL);
    }
    if has(MOD_SHIFT) {
        vks.push(VK_SHIFT);
    }
    if has(MOD_ALT) {
        vks.push(VK_MENU);
    }
    if has(MOD_WIN) {
        vks.push(VK_LWIN);
    }
    let key = parse_vk(&config.key);
    if key != 0 {
        vks.push(key);
    }
    vks
}

pub fn register(config: &HotkeyConfig) -> bool {
    let modifiers = parse_modifier(&config.modifier);
    let vk = parse_vk(&config.key);
    let ok = unsafe { RegisterHotKey(Some(HWND::default()), HOTKEY_ID, modifiers, vk) }.is_ok();
    // 実際に登録した内容を 1 行残す（初回登録・設定変更の再登録の双方がここを通る）。
    // smoke はこの `vks` をそのまま注入するため、対応表を持たずに済む。
    // ユーザーの「ホットキーが効かない」報告に対する一次情報でもある。
    crate::trace::trace(
        "hotkey:registered",
        serde_json::json!({
            "modifier": config.modifier,
            "key": config.key,
            "vks": injection_vks(config),
            "ok": ok,
        }),
    );
    ok
}

pub fn unregister() {
    let _ = unsafe { UnregisterHotKey(Some(HWND::default()), HOTKEY_ID) };
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

    #[test]
    fn injection_vks_alt_q() {
        let c = HotkeyConfig { modifier: "Alt".into(), key: "Q".into() };
        assert_eq!(injection_vks(&c), vec![0x12, b'Q' as u32]);
    }

    #[test]
    fn injection_vks_ctrl_k() {
        let c = HotkeyConfig { modifier: "Ctrl".into(), key: "K".into() };
        assert_eq!(injection_vks(&c), vec![0x11, b'K' as u32]);
    }

    #[test]
    fn injection_vks_compound_modifier_is_press_ordered() {
        // 押下順の契約: Ctrl → Shift → Alt → Win → キー（解放は逆順）。
        let c = HotkeyConfig { modifier: "Shift+Ctrl".into(), key: "A".into() };
        assert_eq!(injection_vks(&c), vec![0x11, 0x10, b'A' as u32]);
    }

    #[test]
    fn injection_vks_named_key_reuses_parse_vk_table() {
        // 要石: 名前付きキーの表は parse_vk の 1 か所だけに存在する。
        let c = HotkeyConfig { modifier: "Alt".into(), key: "F12".into() };
        assert_eq!(injection_vks(&c), vec![0x12, 0x7B]);
    }

    #[test]
    fn injection_vks_unknown_key_is_omitted() {
        // parse_vk が 0 を返すキーは列から落ちる（RegisterHotKey も同じ理由で失敗する）。
        let c = HotkeyConfig { modifier: "Alt".into(), key: "f13".into() };
        assert_eq!(injection_vks(&c), vec![0x12]);
    }
}
