use std::sync::mpsc::{self, Receiver, Sender};

use snotra_core::config::HotkeyConfig;
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::{
    ExtractIconW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE,
    NIM_SETVERSION,
    NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DestroyIcon, DestroyMenu, DispatchMessageW,
    GetCursorPos, GetMessageW, HICON, IDC_ARROW, IDI_APPLICATION, LoadIconW, MF_GRAYED,
    MF_SEPARATOR, MF_STRING, MSG, PM_NOREMOVE, PeekMessageW, PostMessageW, PostQuitMessage,
    PostThreadMessageW, RegisterClassExW, SetForegroundWindow, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
    TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx, TranslateMessage,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_COMMAND, WM_CONTEXTMENU, WM_HOTKEY, WM_LBUTTONUP,
    WM_NULL, WM_RBUTTONUP, WNDCLASSEXW,
};
use windows::core::{PCWSTR, w};

use crate::state::AppState;
use crate::{commands, hotkey, ime};

const WM_PLATFORM_WAKE: u32 = WM_APP + 40;
const WM_TRAY_ICON: u32 = WM_APP + 41;
const ID_MENU_SETTINGS: usize = 1000;
const ID_MENU_EXIT: usize = 1001;
const ID_MENU_RECENT_BASE: usize = 2000;

unsafe extern "system" fn platform_default_wnd_proc(
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
        windows::Win32::UI::WindowsAndMessaging::DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

pub enum PlatformCommand {
    SetHotkey {
        config: HotkeyConfig,
        reply: Sender<bool>,
    },
    SetTrayVisible(bool),
    SetIndexing(bool),
    TurnOffIme(usize),
    /// Register the initial hotkey. Sent by main after the hotkey-pressed listener
    /// is ready so that no hotkey event is dropped before there is a receiver.
    RegisterInitialHotkey,
    Exit,
}

pub struct PlatformBridgePending {
    command_tx: Sender<PlatformCommand>,
    thread_id_rx: Receiver<u32>,
}

impl PlatformBridgePending {
    /// Blocks until the platform thread signals its Win32 init is complete.
    pub fn wait(self) -> Option<PlatformBridge> {
        let thread_id = self.thread_id_rx.recv().ok()?;
        if thread_id == 0 {
            return None;
        }
        Some(PlatformBridge {
            command_tx: self.command_tx,
            thread_id,
        })
    }
}

pub struct PlatformBridge {
    command_tx: Sender<PlatformCommand>,
    thread_id: u32,
}

impl PlatformBridge {
    /// Non-blocking: spawns the platform thread and returns a pending handle.
    /// The tray icon is NOT created during init; call SetTrayVisible(true) after
    /// all windows and event listeners are ready.
    pub fn begin(
        app_handle: AppHandle,
        initial_hotkey: HotkeyConfig,
    ) -> Option<PlatformBridgePending> {
        let (command_tx, command_rx) = mpsc::channel();
        let (thread_id_tx, thread_id_rx) = mpsc::channel();

        std::thread::Builder::new()
            .name("snotra-platform".to_string())
            .spawn(move || {
                platform_thread_loop(
                    app_handle,
                    initial_hotkey,
                    false, // tray starts hidden; main sends SetTrayVisible after full setup
                    command_rx,
                    thread_id_tx,
                );
            })
            .ok()?;

        Some(PlatformBridgePending { command_tx, thread_id_rx })
    }

    pub fn send_command(&self, command: PlatformCommand) {
        if self.thread_id == 0 {
            return;
        }
        if self.command_tx.send(command).is_ok() {
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_PLATFORM_WAKE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

fn platform_thread_loop(
    app_handle: AppHandle,
    initial_hotkey: HotkeyConfig,
    show_tray_icon: bool,
    command_rx: Receiver<PlatformCommand>,
    thread_id_tx: Sender<u32>,
) {
    unsafe {
        let mut dummy = MSG::default();
        let _ = PeekMessageW(&mut dummy, None, 0, 0, PM_NOREMOVE);

        let thread_id = GetCurrentThreadId();

        let instance = match GetModuleHandleW(None) {
            Ok(v) => v,
            Err(_) => {
                let _ = thread_id_tx.send(0);
                return;
            }
        };
        let class_name = w!("SnotraPlatformWindow");

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: Default::default(),
            lpfnWndProc: Some(platform_default_wnd_proc),
            hInstance: instance.into(),
            hCursor: windows::Win32::UI::WindowsAndMessaging::LoadCursorW(None, IDC_ARROW)
                .unwrap_or_default(),
            lpszClassName: class_name,
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);

        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("Snotra Platform"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        ) {
            Ok(v) => v,
            Err(_) => {
                let _ = thread_id_tx.send(0);
                return;
            }
        };

        let _ = thread_id_tx.send(thread_id);

        // Hotkey registration is deferred: main sends RegisterInitialHotkey after
        // the hotkey-pressed listener is ready, preventing events from being dropped.
        let mut current_hotkey = initial_hotkey;

        let mut tray = if show_tray_icon {
            Some(TrayIcon::create(hwnd))
        } else {
            None
        };

        let mut indexing_in_progress = false;
        // Tracks whether WM_RBUTTONUP was just handled, so the immediately following
        // WM_CONTEXTMENU (sent by some Shell environments for the same click) can be
        // suppressed to avoid showing the context menu twice.
        let mut rbuttonup_handled = false;

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            match msg.message {
                WM_HOTKEY => {
                    let _ = app_handle.emit("hotkey-pressed", ());
                }
                WM_TRAY_ICON => {
                    handle_tray_message(
                        &mut tray,
                        hwnd,
                        msg.lParam,
                        &app_handle,
                        indexing_in_progress,
                        &mut rbuttonup_handled,
                    );
                }
                WM_COMMAND => {
                    handle_menu_command(msg.wParam, &app_handle, &tray);
                }
                WM_PLATFORM_WAKE => {
                    process_commands(
                        &command_rx,
                        &mut current_hotkey,
                        &mut tray,
                        hwnd,
                        &mut indexing_in_progress,
                        &app_handle,
                    );
                }
                _ => {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }

        hotkey::unregister();
    }
}

fn process_commands(
    command_rx: &Receiver<PlatformCommand>,
    current_hotkey: &mut HotkeyConfig,
    tray: &mut Option<TrayIcon>,
    hwnd: HWND,
    indexing_in_progress: &mut bool,
    app_handle: &AppHandle,
) {
    while let Ok(command) = command_rx.try_recv() {
        match command {
            PlatformCommand::SetHotkey { config, reply } => {
                hotkey::unregister();
                let success = hotkey::register(&config);
                if success {
                    *current_hotkey = config;
                    let _ = reply.send(true);
                } else {
                    let _ = hotkey::register(current_hotkey);
                    let _ = reply.send(false);
                }
            }
            PlatformCommand::SetTrayVisible(show) => {
                if show {
                    if tray.is_none() {
                        *tray = Some(TrayIcon::create(hwnd));
                    }
                } else {
                    *tray = None;
                }
            }
            PlatformCommand::SetIndexing(indexing) => {
                *indexing_in_progress = indexing;
            }
            PlatformCommand::TurnOffIme(hwnd_raw) => {
                // Known: this command is dispatched from the platform thread after
                // show_main_and_emit() calls show()/set_focus() on the main thread.
                // A narrow timing window exists where the window receives focus before
                // IME is disabled. Mitigated by passing HWND directly to avoid an extra
                // lookup. Residual race is theoretical and not observed in practice.
                let hwnd = windows::Win32::Foundation::HWND(hwnd_raw as *mut core::ffi::c_void);
                if !hwnd.is_invalid() {
                    ime::turn_off_ime(hwnd);
                }
            }
            PlatformCommand::RegisterInitialHotkey => {
                if !hotkey::register(current_hotkey) {
                    let _ = app_handle.emit("platform-event", "initial-hotkey-failed");
                }
            }
            PlatformCommand::Exit => unsafe {
                PostQuitMessage(0);
            },
        }
    }
}

fn recent_history_items(app_handle: &AppHandle) -> Vec<snotra_core::ui_types::SearchResult> {
    let state = app_handle.state::<AppState>();
    let max_history_display = {
        let config = state.config.lock().unwrap();
        config.appearance.max_history_display
    };
    let engine = state.engine.lock().unwrap();
    let history = state.history.lock().unwrap();
    engine.recent_history(&history, max_history_display)
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

fn handle_menu_command(wparam: WPARAM, app_handle: &AppHandle, tray: &Option<TrayIcon>) {
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

fn handle_tray_message(
    tray: &mut Option<TrayIcon>,
    hwnd: HWND,
    lparam: LPARAM,
    app_handle: &AppHandle,
    indexing: bool,
    rbuttonup_handled: &mut bool,
) {
    let event = (lparam.0 & 0xFFFF) as u32;
    match event {
        x if x == WM_LBUTTONUP => {
            if let Some(tray) = tray.as_mut() {
                tray.show_recent_history_menu(hwnd, app_handle);
            }
        }
        x if x == WM_RBUTTONUP => {
            *rbuttonup_handled = true;
            if let Some(tray) = tray.as_ref() {
                tray.show_context_menu(hwnd, indexing);
            }
        }
        x if x == WM_CONTEXTMENU => {
            if *rbuttonup_handled {
                // WM_RBUTTONUP already handled this click; clear the flag and skip.
                // Some Shell environments deliver both messages for a single right-click.
                *rbuttonup_handled = false;
            } else {
                // Keyboard-triggered (Shift+F10 / Application key): WM_RBUTTONUP is not
                // sent in this path, so WM_CONTEXTMENU is the only notification.
                if let Some(tray) = tray.as_ref() {
                    tray.show_context_menu(hwnd, indexing);
                }
            }
        }
        _ => {}
    }
}

struct TrayIcon {
    nid: NOTIFYICONDATAW,
    owned_icon: Option<HICON>,
    recent_menu_paths: Vec<String>,
}

impl TrayIcon {
    fn create(hwnd: HWND) -> Self {
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

    fn recent_path_for_id(&self, id: usize) -> Option<String> {
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
