//! Alt+Q ホットキー分岐の純粋な決定核（Win32 非依存）。SU1 spike で実証済み・#532 SU2 で移植。
#![allow(dead_code)] // Task 4 で hotkey listener が消費するまで dead-code。Task 4 完了時に除去する。

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
}
