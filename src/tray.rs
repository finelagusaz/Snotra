use crate::window;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_SETVERSION,
    NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, IsWindowVisible, LoadIconW,
    PostMessageW, SetForegroundWindow, ShowWindow, TrackPopupMenuEx, IDI_APPLICATION, MF_SEPARATOR,
    MF_STRING, SW_HIDE, SW_SHOWNOACTIVATE, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_COMMAND,
};

pub const WM_TRAY_ICON: u32 = 0x8000 + 1; // WM_APP + 1
pub const IDM_SETTINGS: u16 = 1000;
pub const IDM_EXIT: u16 = 1001;

pub struct Tray {
    callback_hwnd: HWND,
    menu_owner_hwnd: HWND,
    nid: NOTIFYICONDATAW,
}

impl Tray {
    pub fn create(callback_hwnd: HWND, menu_owner_hwnd: HWND) -> Self {
        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = callback_hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY_ICON;
        nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;

        // Set tooltip "Snotra"
        let tip: Vec<u16> = "Snotra".encode_utf16().chain(std::iter::once(0)).collect();
        let len = tip.len().min(nid.szTip.len());
        nid.szTip[..len].copy_from_slice(&tip[..len]);

        // Use default application icon
        nid.hIcon = unsafe { LoadIconW(None, IDI_APPLICATION) }.unwrap_or_default();

        unsafe {
            let _ = Shell_NotifyIconW(NIM_ADD, &nid);
            let _ = Shell_NotifyIconW(NIM_SETVERSION, &nid);
        }

        Tray {
            callback_hwnd,
            menu_owner_hwnd,
            nid,
        }
    }

    pub fn show_context_menu(&self) {
        unsafe {
            let hmenu = CreatePopupMenu().unwrap();
            let settings_text: Vec<u16> = "設定(&S)"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let exit_text: Vec<u16> = "終了(&X)"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let _ = AppendMenuW(
                hmenu,
                MF_STRING,
                IDM_SETTINGS as usize,
                PCWSTR(settings_text.as_ptr()),
            );
            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
            let _ = AppendMenuW(
                hmenu,
                MF_STRING,
                IDM_EXIT as usize,
                PCWSTR(exit_text.as_ptr()),
            );

            let mut pt = Default::default();
            let _ = GetCursorPos(&mut pt);
            let owner = self.menu_owner_hwnd;
            let owner_was_visible = IsWindowVisible(owner).as_bool();
            if !owner_was_visible {
                let _ = ShowWindow(owner, SW_SHOWNOACTIVATE);
            }
            let _ = SetForegroundWindow(owner);
            let command = TrackPopupMenuEx(
                hmenu,
                (TPM_LEFTALIGN
                    | TPM_BOTTOMALIGN
                    | TPM_RIGHTBUTTON
                    | TPM_NONOTIFY
                    | TPM_RETURNCMD)
                    .0,
                pt.x,
                pt.y,
                owner,
                None,
            );
            if command.0 != 0 {
                let _ = PostMessageW(
                    self.callback_hwnd,
                    WM_COMMAND,
                    WPARAM(command.0 as usize),
                    LPARAM(0),
                );
            }
            if !owner_was_visible {
                let _ = ShowWindow(owner, SW_HIDE);
            }
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
    use windows::Win32::UI::WindowsAndMessaging::{WM_CONTEXTMENU, WM_LBUTTONDBLCLK, WM_RBUTTONUP};
    match event {
        x if x == WM_CONTEXTMENU => tray.show_context_menu(),
        x if x == WM_RBUTTONUP => {}
        x if x == WM_LBUTTONDBLCLK => {
            window::toggle_window(search_hwnd);
        }
        _ => {}
    }
}
