use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, LoadIconW, PostMessageW,
    SetForegroundWindow, TrackPopupMenu, IDI_APPLICATION, MF_STRING, TPM_BOTTOMALIGN,
    TPM_LEFTALIGN, WM_COMMAND,
};
use windows::core::PCWSTR;
use crate::window;

pub const WM_TRAY_ICON: u32 = 0x0400 + 1; // WM_APP + 1
pub const IDM_EXIT: u16 = 1001;

pub struct Tray {
    hwnd: HWND,
    nid: NOTIFYICONDATAW,
}

impl Tray {
    pub fn create(hwnd: HWND) -> Self {
        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY_ICON;

        // Set tooltip "Snotra"
        let tip: Vec<u16> = "Snotra".encode_utf16().chain(std::iter::once(0)).collect();
        let len = tip.len().min(nid.szTip.len());
        nid.szTip[..len].copy_from_slice(&tip[..len]);

        // Use default application icon
        nid.hIcon = unsafe { LoadIconW(None, IDI_APPLICATION) }.unwrap_or_default();

        unsafe {
            let _ = Shell_NotifyIconW(NIM_ADD, &nid);
        }

        Tray { hwnd, nid }
    }

    pub fn show_context_menu(&self) {
        unsafe {
            let hmenu = CreatePopupMenu().unwrap();
            let exit_text: Vec<u16> = "終了(&X)".encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AppendMenuW(hmenu, MF_STRING, IDM_EXIT as usize, PCWSTR(exit_text.as_ptr()));

            let mut pt = Default::default();
            let _ = GetCursorPos(&mut pt);
            let _ = SetForegroundWindow(self.hwnd);
            let _ = TrackPopupMenu(hmenu, TPM_LEFTALIGN | TPM_BOTTOMALIGN, pt.x, pt.y, 0, self.hwnd, None);
            let _ = PostMessageW(self.hwnd, WM_COMMAND, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(hmenu);
        }
    }

    pub fn remove(&self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.nid);
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.remove();
    }
}

pub fn handle_tray_message(tray: &Tray, lparam: LPARAM, search_hwnd: HWND) {
    let event = (lparam.0 & 0xFFFF) as u32;
    use windows::Win32::UI::WindowsAndMessaging::{WM_LBUTTONDBLCLK, WM_RBUTTONUP};
    match event {
        x if x == WM_RBUTTONUP => tray.show_context_menu(),
        x if x == WM_LBUTTONDBLCLK => {
            window::toggle_window(search_hwnd);
        }
        _ => {}
    }
}
