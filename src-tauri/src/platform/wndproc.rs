use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{DefWindowProcW, PostThreadMessageW, WM_CONTEXTMENU};

use super::WM_TRAY_ICON;

pub(super) unsafe extern "system" fn platform_default_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        // Shell may deliver WM_TRAY_ICON via SendMessage (bypassing GetMessageW queue).
        // Re-post it as a thread message so the message loop can handle it.
        if msg == WM_TRAY_ICON {
            let _ = PostThreadMessageW(GetCurrentThreadId(), WM_TRAY_ICON, wparam, lparam);
            return LRESULT(0);
        }
        // Keyboard-triggered context menu (Shift+F10 / Application key on tray icon focus)
        // is delivered as direct WM_CONTEXTMENU to this window proc, NOT through uCallbackMessage.
        // Re-post as WM_TRAY_ICON with NOTIFYICON_VERSION_4 lParam format so the message loop
        // can route it through handle_tray_message.
        // lParam format: LOWORD = event (WM_CONTEXTMENU), HIWORD = icon ID (1)
        if msg == WM_CONTEXTMENU {
            let synthesized = LPARAM(((1_isize) << 16) | (WM_CONTEXTMENU as isize));
            let _ = PostThreadMessageW(GetCurrentThreadId(), WM_TRAY_ICON, wparam, synthesized);
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}
