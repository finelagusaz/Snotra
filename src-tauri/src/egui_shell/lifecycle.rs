//! Alt+Q ホットキー分岐・blur 判定の純粋な決定核（Win32 非依存）。SU1 spike で実証済み・#532 SU2。

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HotkeyPlan {
    HideNow,
    ShowAfterAltRelease,
    ShowNow,
}

/// Alt+Q 押下時の分岐。表示中なら即 hide、非表示中は Alt が押されている限り
/// 解放を待ってから show する（製品 WebView2 経路と同じ意味論）。
pub(crate) fn plan_hotkey(visible: bool, alt_pressed: bool) -> HotkeyPlan {
    if visible {
        HotkeyPlan::HideNow
    } else if alt_pressed {
        HotkeyPlan::ShowAfterAltRelease
    } else {
        HotkeyPlan::ShowNow
    }
}

/// blur（focus 喪失）猶予明けに hide 要求を出すべきかの純粋判定。焦点が戻っておらず、
/// 100ms 猶予が明け、auto_hide が有効で、設定サイドカーが非起動のときだけ hide する。
/// 猶予タイマの発火・repaint 予約という状態は view 側（update）に残す。
pub(crate) fn blur_should_hide(
    focused: bool,
    grace_elapsed: bool,
    auto_hide: bool,
    settings_running: bool,
) -> bool {
    !focused && grace_elapsed && auto_hide && !settings_running
}

#[cfg(test)]
mod tests {
    use super::{HotkeyPlan, plan_hotkey};

    #[test]
    fn hotkey_branches_match_product_semantics() {
        assert_eq!(plan_hotkey(true, false), HotkeyPlan::HideNow);
        assert_eq!(plan_hotkey(true, true), HotkeyPlan::HideNow);
        assert_eq!(plan_hotkey(false, true), HotkeyPlan::ShowAfterAltRelease);
        assert_eq!(plan_hotkey(false, false), HotkeyPlan::ShowNow);
    }

    #[test]
    fn blur_hides_only_when_all_gates_pass() {
        use super::blur_should_hide;
        // focused, grace_elapsed, auto_hide, settings_running
        assert!(blur_should_hide(false, true, true, false)); // 全成立 → hide
        assert!(!blur_should_hide(true, true, true, false)); // 焦点復帰 → hide しない
        assert!(!blur_should_hide(false, false, true, false)); // 猶予未明け → hide しない
        assert!(!blur_should_hide(false, true, false, false)); // auto_hide 無効 → hide しない
        assert!(!blur_should_hide(false, true, true, true)); // 設定起動中 → hide しない
    }
}
