use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Shell::{
    ExtractIconW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE,
    NIM_SETVERSION, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos, GetMessageTime, HICON,
    IDI_APPLICATION, LoadIconW, MF_GRAYED, MF_SEPARATOR, MF_STRING, PostMessageW,
    SetForegroundWindow, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, TrackPopupMenuEx, WM_COMMAND, WM_CONTEXTMENU, WM_LBUTTONUP, WM_NULL,
    WM_RBUTTONUP,
};
use windows::core::PCWSTR;

use crate::commands;
use crate::state::AppState;

use super::WM_TRAY_ICON;

pub(super) const ID_MENU_SETTINGS: usize = 1000;
pub(super) const ID_MENU_EXIT: usize = 1001;
const ID_MENU_RECENT_BASE: usize = 2000;

pub(super) fn recent_history_items(
    app_handle: &AppHandle,
) -> Vec<snotra_core::ui_types::SearchResult> {
    let state = app_handle.state::<AppState>();
    state.engine.lock().unwrap().recent_history()
}

fn format_recent_history_label(item: &snotra_core::ui_types::SearchResult) -> String {
    const MAX_LABEL_LEN: usize = 90;
    let base = if item.name.is_empty() || item.name == item.path {
        item.path.clone()
    } else {
        format!("{} - {}", item.name, item.path)
    };
    if base.chars().count() <= MAX_LABEL_LEN {
        return base;
    }
    let mut truncated: String = base.chars().take(MAX_LABEL_LEN - 3).collect();
    truncated.push_str("...");
    truncated
}

pub(super) fn handle_menu_command(
    wparam: WPARAM,
    app_handle: &AppHandle,
    tray: &Option<TrayIcon>,
) {
    let id = wparam.0 & 0xFFFF;
    match id {
        ID_MENU_SETTINGS => {
            let _ = app_handle.emit("open-settings", ());
        }
        ID_MENU_EXIT => {
            let _ = app_handle.emit("exit-requested", ());
        }
        id if id >= ID_MENU_RECENT_BASE => {
            let id = id as usize;
            if let Some(path) = tray.as_ref().and_then(|t| t.recent_path_for_id(id)) {
                let state = app_handle.state::<AppState>();
                commands::launch_item_with_state(&path, "", &state);
            }
        }
        _ => {}
    }
}

pub(super) fn handle_tray_message(
    tray: &mut Option<TrayIcon>,
    hwnd: HWND,
    lparam: LPARAM,
    app_handle: &AppHandle,
    indexing: bool,
    last_rbuttonup_msg_time: &mut i32,
) {
    let event = (lparam.0 & 0xFFFF) as u32;
    match event {
        x if x == WM_LBUTTONUP => {
            if let Some(tray) = tray.as_mut() {
                tray.show_recent_history_menu(hwnd, app_handle);
            }
        }
        x if x == WM_RBUTTONUP => {
            // Record the message timestamp so WM_CONTEXTMENU can detect whether it
            // was triggered by the same click (duplicate) or by a keyboard shortcut.
            *last_rbuttonup_msg_time = unsafe { GetMessageTime() };
            if let Some(tray) = tray.as_ref() {
                tray.show_context_menu(hwnd, indexing);
            }
        }
        x if x == WM_CONTEXTMENU => {
            // Some Shell environments deliver both WM_RBUTTONUP and WM_CONTEXTMENU for
            // a single right-click.  When the messages belong to the same click they
            // arrive within a few milliseconds of each other; we use a 500 ms threshold
            // to distinguish that case from a keyboard-triggered context menu request
            // (Shift+F10 / Application key), which can arrive any time after the last
            // right-click.
            //
            // A bool flag is NOT sufficient here: Windows 11 does not send
            // WM_CONTEXTMENU for mouse right-clicks, so a flag set by WM_RBUTTONUP
            // would never be cleared and would permanently suppress keyboard requests.
            let now = unsafe { GetMessageTime() };
            let elapsed = now.wrapping_sub(*last_rbuttonup_msg_time);
            if elapsed > 500 {
                if let Some(tray) = tray.as_ref() {
                    tray.show_context_menu(hwnd, indexing);
                }
            }
        }
        _ => {}
    }
}

pub(super) struct TrayIcon {
    nid: NOTIFYICONDATAW,
    owned_icon: Option<HICON>,
    recent_menu_paths: Vec<String>,
}

impl TrayIcon {
    pub(super) fn create(hwnd: HWND) -> Self {
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            // For NOTIFYICON_VERSION_4, show the standard tooltip explicitly.
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: WM_TRAY_ICON,
            ..Default::default()
        };
        nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;

        let tip: Vec<u16> = "Snotra".encode_utf16().chain(std::iter::once(0)).collect();
        let len = tip.len().min(nid.szTip.len());
        nid.szTip[..len].copy_from_slice(&tip[..len]);

        let owned_icon = load_tray_icon_from_exe();
        nid.hIcon = owned_icon
            .unwrap_or_else(|| unsafe { LoadIconW(None, IDI_APPLICATION) }.unwrap_or_default());

        unsafe {
            let _ = Shell_NotifyIconW(NIM_ADD, &nid);
            let _ = Shell_NotifyIconW(NIM_SETVERSION, &nid);
        }

        Self {
            nid,
            owned_icon,
            recent_menu_paths: Vec::new(),
        }
    }

    pub(super) fn recent_path_for_id(&self, id: usize) -> Option<String> {
        let offset = id.checked_sub(ID_MENU_RECENT_BASE)?;
        self.recent_menu_paths.get(offset).cloned()
    }

    fn show_context_menu(&self, hwnd: HWND, indexing: bool) {
        unsafe {
            let Ok(hmenu) = CreatePopupMenu() else {
                return;
            };

            if indexing {
                let indexing_text: Vec<u16> = "インデックス再構築中"
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                let settings_text: Vec<u16> = "設定(&S)"
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                let exit_text: Vec<u16> = "終了(&X)"
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();

                let _ = AppendMenuW(hmenu, MF_GRAYED, 0, PCWSTR(indexing_text.as_ptr()));
                let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
                let _ = AppendMenuW(
                    hmenu,
                    MF_GRAYED,
                    ID_MENU_SETTINGS,
                    PCWSTR(settings_text.as_ptr()),
                );
                let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
                let _ = AppendMenuW(hmenu, MF_GRAYED, ID_MENU_EXIT, PCWSTR(exit_text.as_ptr()));
            } else {
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
                    ID_MENU_SETTINGS,
                    PCWSTR(settings_text.as_ptr()),
                );
                let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
                let _ = AppendMenuW(hmenu, MF_STRING, ID_MENU_EXIT, PCWSTR(exit_text.as_ptr()));
            }

            let mut pt = Default::default();
            let _ = GetCursorPos(&mut pt);
            let _ = SetForegroundWindow(hwnd);

            let command = TrackPopupMenuEx(
                hmenu,
                (TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON | TPM_NONOTIFY | TPM_RETURNCMD)
                    .0,
                pt.x,
                pt.y,
                hwnd,
                None,
            );

            if command.0 != 0 {
                let _ = PostMessageW(
                    Some(hwnd),
                    WM_COMMAND,
                    WPARAM(command.0 as usize),
                    LPARAM(0),
                );
            }

            // MSDN: send WM_NULL after TrackPopupMenuEx so the menu dismisses correctly.
            let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(hmenu);
        }
    }

    fn show_recent_history_menu(&mut self, hwnd: HWND, app_handle: &AppHandle) {
        unsafe {
            let Ok(hmenu) = CreatePopupMenu() else {
                return;
            };

            self.recent_menu_paths.clear();
            let recent = recent_history_items(app_handle);
            if recent.is_empty() {
                let empty_text: Vec<u16> =
                    "履歴なし".encode_utf16().chain(std::iter::once(0)).collect();
                let _ = AppendMenuW(hmenu, MF_GRAYED, 0, PCWSTR(empty_text.as_ptr()));
            } else {
                for (idx, item) in recent.iter().enumerate() {
                    let id = ID_MENU_RECENT_BASE + idx;
                    let label = format_recent_history_label(item);
                    let label_wide: Vec<u16> =
                        label.encode_utf16().chain(std::iter::once(0)).collect();
                    let _ = AppendMenuW(hmenu, MF_STRING, id, PCWSTR(label_wide.as_ptr()));
                    self.recent_menu_paths.push(item.path.clone());
                }
            }

            let mut pt = Default::default();
            let _ = GetCursorPos(&mut pt);
            let _ = SetForegroundWindow(hwnd);

            let command = TrackPopupMenuEx(
                hmenu,
                (TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_NONOTIFY | TPM_RETURNCMD).0,
                pt.x,
                pt.y,
                hwnd,
                None,
            );
            if command.0 != 0 {
                let _ = PostMessageW(
                    Some(hwnd),
                    WM_COMMAND,
                    WPARAM(command.0 as usize),
                    LPARAM(0),
                );
            }

            let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(hmenu);
        }
    }

    fn remove(&self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.nid);
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        self.remove();
        if let Some(icon) = self.owned_icon.take() {
            unsafe {
                let _ = DestroyIcon(icon);
            }
        }
    }
}

fn load_tray_icon_from_exe() -> Option<HICON> {
    let exe_path = std::env::current_exe().ok()?;
    let wide_path: Vec<u16> = exe_path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    // Extract the first icon from the running executable so tray icon and app icon stay aligned.
    let icon = unsafe { ExtractIconW(None, PCWSTR(wide_path.as_ptr()), 0) };
    if (icon.0 as usize) <= 1 {
        return None;
    }
    Some(icon)
}
