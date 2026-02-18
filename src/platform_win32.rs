use std::sync::mpsc::{self, Receiver, Sender};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_SETVERSION,
    NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DestroyMenu, DispatchMessageW, GetCursorPos,
    GetForegroundWindow, GetMessageW, LoadIconW, PeekMessageW, PostMessageW, PostQuitMessage,
    PostThreadMessageW, RegisterClassExW, SetForegroundWindow, TrackPopupMenuEx, TranslateMessage,
    IDC_ARROW, IDI_APPLICATION, MF_SEPARATOR, MF_STRING, MSG, PM_NOREMOVE, TPM_BOTTOMALIGN,
    TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_APP, WM_COMMAND, WM_CONTEXTMENU, WM_HOTKEY, WM_LBUTTONDBLCLK, WM_RBUTTONUP, WNDCLASSEXW,
};

use crate::config::HotkeyConfig;
use crate::{hotkey, ime};

const WM_PLATFORM_WAKE: u32 = WM_APP + 40;
const WM_TRAY_ICON: u32 = WM_APP + 41;
const ID_MENU_SETTINGS: usize = 1000;
const ID_MENU_EXIT: usize = 1001;

pub const PLATFORM_WINDOW_CLASS: &str = "SnotraPlatformWindow";

unsafe extern "system" fn platform_default_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    windows::Win32::UI::WindowsAndMessaging::DefWindowProcW(hwnd, msg, wparam, lparam)
}

#[derive(Debug)]
pub enum PlatformEvent {
    HotkeyPressed,
    OpenSettings,
    ExitRequested,
    InitialHotkeyFailed,
}

pub enum PlatformCommand {
    SetHotkey {
        config: HotkeyConfig,
        reply: Sender<bool>,
    },
    SetTrayVisible(bool),
    TurnOffImeForForeground,
    Exit,
}

pub struct PlatformBridge {
    event_rx: Receiver<PlatformEvent>,
    command_tx: Sender<PlatformCommand>,
    thread_id: u32,
}

impl PlatformBridge {
    pub fn start(initial_hotkey: HotkeyConfig, show_tray_icon: bool) -> Option<Self> {
        let (event_tx, event_rx) = mpsc::channel();
        let (command_tx, command_rx) = mpsc::channel();
        let (thread_id_tx, thread_id_rx) = mpsc::channel();

        std::thread::Builder::new()
            .name("snotra-platform".to_string())
            .spawn(move || {
                platform_thread_loop(
                    initial_hotkey,
                    show_tray_icon,
                    event_tx,
                    command_rx,
                    thread_id_tx,
                );
            })
            .ok()?;

        let thread_id = thread_id_rx.recv().ok()?;
        if thread_id == 0 {
            return None;
        }
        Some(Self {
            event_rx,
            command_tx,
            thread_id,
        })
    }

    pub fn send_command(&self, command: PlatformCommand) {
        if self.command_tx.send(command).is_ok() {
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_PLATFORM_WAKE, WPARAM(0), LPARAM(0));
            }
        }
    }

    pub fn try_recv_event(&self) -> Option<PlatformEvent> {
        self.event_rx.try_recv().ok()
    }
}

fn platform_thread_loop(
    initial_hotkey: HotkeyConfig,
    show_tray_icon: bool,
    event_tx: Sender<PlatformEvent>,
    command_rx: Receiver<PlatformCommand>,
    thread_id_tx: Sender<u32>,
) {
    unsafe {
        // Ensure the thread has a message queue before sharing thread_id.
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

        let mut current_hotkey = initial_hotkey;
        if !hotkey::register(&current_hotkey) {
            let _ = event_tx.send(PlatformEvent::InitialHotkeyFailed);
        }

        let mut tray = if show_tray_icon {
            Some(TrayIcon::create(hwnd))
        } else {
            None
        };

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            match msg.message {
                WM_HOTKEY => {
                    let _ = event_tx.send(PlatformEvent::HotkeyPressed);
                }
                WM_TRAY_ICON => {
                    handle_tray_message(&mut tray, hwnd, msg.lParam, &event_tx);
                }
                WM_COMMAND => {
                    handle_menu_command(msg.wParam, &event_tx);
                }
                WM_PLATFORM_WAKE => {
                    process_commands(&command_rx, &mut current_hotkey, &mut tray, hwnd);
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
            PlatformCommand::TurnOffImeForForeground => unsafe {
                let active = GetForegroundWindow();
                if !active.is_invalid() {
                    ime::turn_off_ime(active);
                }
            },
            PlatformCommand::Exit => unsafe {
                PostQuitMessage(0);
            },
        }
    }
}

fn handle_menu_command(wparam: WPARAM, event_tx: &Sender<PlatformEvent>) {
    let id = wparam.0 & 0xFFFF;
    match id {
        ID_MENU_SETTINGS => {
            let _ = event_tx.send(PlatformEvent::OpenSettings);
        }
        ID_MENU_EXIT => {
            let _ = event_tx.send(PlatformEvent::ExitRequested);
        }
        _ => {}
    }
}

fn handle_tray_message(
    tray: &mut Option<TrayIcon>,
    hwnd: HWND,
    lparam: LPARAM,
    event_tx: &Sender<PlatformEvent>,
) {
    let event = (lparam.0 & 0xFFFF) as u32;
    match event {
        x if x == WM_CONTEXTMENU => {
            if let Some(tray) = tray.as_ref() {
                tray.show_context_menu(hwnd);
            }
        }
        x if x == WM_LBUTTONDBLCLK => {
            let _ = event_tx.send(PlatformEvent::HotkeyPressed);
        }
        x if x == WM_RBUTTONUP => {}
        _ => {}
    }
}

struct TrayIcon {
    nid: NOTIFYICONDATAW,
}

impl TrayIcon {
    fn create(hwnd: HWND) -> Self {
        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY_ICON;
        nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;

        let tip: Vec<u16> = "Snotra".encode_utf16().chain(std::iter::once(0)).collect();
        let len = tip.len().min(nid.szTip.len());
        nid.szTip[..len].copy_from_slice(&tip[..len]);

        nid.hIcon = unsafe { LoadIconW(None, IDI_APPLICATION) }.unwrap_or_default();

        unsafe {
            let _ = Shell_NotifyIconW(NIM_ADD, &nid);
            let _ = Shell_NotifyIconW(NIM_SETVERSION, &nid);
        }

        Self { nid }
    }

    fn show_context_menu(&self, hwnd: HWND) {
        unsafe {
            let Ok(hmenu) = CreatePopupMenu() else {
                return;
            };

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
    }
}
