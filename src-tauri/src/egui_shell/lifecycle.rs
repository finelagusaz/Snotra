//! Alt+Q ホットキー分岐・blur 判定の純粋な決定核（Win32 非依存）。SU1 spike で実証済み・#532 SU2。

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HotkeyPlan {
    HideNow,
    ShowAfterAltRelease,
    ShowNow,
}

/// Alt+Q 押下時の分岐（製品 WebView2 経路と同じ意味論）。表示中かつ hotkey_toggle=true なら
/// 即 hide。それ以外（非表示、または表示中でも hotkey_toggle=false ＝ 既に見えている窓を
/// 再フォーカス/再配置）は show 側へ回り、Alt が押されている限り解放を待ってから show する。
pub(crate) fn plan_hotkey(visible: bool, alt_pressed: bool, hotkey_toggle: bool) -> HotkeyPlan {
    if visible && hotkey_toggle {
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
        // (visible, alt_pressed, hotkey_toggle)
        assert_eq!(plan_hotkey(true, false, true), HotkeyPlan::HideNow); // 表示+toggle → hide
        assert_eq!(plan_hotkey(true, true, true), HotkeyPlan::HideNow);
        assert_eq!(plan_hotkey(true, false, false), HotkeyPlan::ShowNow); // 表示+非toggle → 再フォーカス
        assert_eq!(
            plan_hotkey(true, true, false),
            HotkeyPlan::ShowAfterAltRelease // 表示+非toggle+Alt → 解放待ち show
        );
        assert_eq!(plan_hotkey(false, true, true), HotkeyPlan::ShowAfterAltRelease);
        assert_eq!(plan_hotkey(false, false, true), HotkeyPlan::ShowNow);
        assert_eq!(plan_hotkey(false, false, false), HotkeyPlan::ShowNow);
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
