pub(crate) mod tray;
pub mod hotkey;
mod wndproc;

use std::sync::mpsc::{self, Receiver, Sender};

use snotra_core::config::{HotkeyConfig, Language};
use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DispatchMessageW, GetMessageW, IDC_ARROW, MSG, PM_NOREMOVE, PeekMessageW,
    PostQuitMessage, PostThreadMessageW, RegisterClassExW, TranslateMessage, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_APP, WM_COMMAND, WM_HOTKEY, WNDCLASSEXW,
};
use windows::core::w;

use crate::ime;
use tray::{TrayIcon, handle_menu_command, handle_tray_message};
use wndproc::platform_default_wnd_proc;

pub(crate) const WM_PLATFORM_WAKE: u32 = WM_APP + 40;
pub(crate) const WM_TRAY_ICON: u32 = WM_APP + 41;

pub enum PlatformCommand {
    SetHotkey {
        config: HotkeyConfig,
        reply: Sender<bool>,
    },
    SetTrayVisible(bool),
    SetIndexing(bool),
    TurnOffIme(usize),
    SetLanguage(Language),
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
        initial_language: Language,
    ) -> Option<PlatformBridgePending> {
        let (command_tx, command_rx) = mpsc::channel();
        let (thread_id_tx, thread_id_rx) = mpsc::channel();

        std::thread::Builder::new()
            .name("snotra-platform".to_string())
            .spawn(move || {
                platform_thread_loop(
                    app_handle,
                    initial_hotkey,
                    initial_language,
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
    initial_language: Language,
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
        let mut current_language = initial_language;

        let mut tray = if show_tray_icon {
            Some(TrayIcon::create(hwnd, current_language))
        } else {
            None
        };

        let mut indexing_in_progress = false;
        // Timestamp (GetMessageTime) of the last WM_RBUTTONUP we processed.
        // Used to suppress the WM_CONTEXTMENU that some Shell environments deliver
        // immediately after WM_RBUTTONUP for the same right-click.
        // A bool flag is insufficient because Windows 11 does NOT send WM_CONTEXTMENU
        // for mouse right-clicks, so the flag would stay set and later suppress
        // keyboard-triggered WM_CONTEXTMENU (Shift+F10 / Application key).
        let mut last_rbuttonup_msg_time: i32 = 0;

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
                        &mut last_rbuttonup_msg_time,
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
                        &mut current_language,
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
    current_language: &mut Language,
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
                        *tray = Some(TrayIcon::create(hwnd, *current_language));
                    }
                } else {
                    *tray = None;
                }
            }
            PlatformCommand::SetIndexing(indexing) => {
                *indexing_in_progress = indexing;
            }
            PlatformCommand::SetLanguage(lang) => {
                *current_language = lang;
                if let Some(t) = tray.as_mut() {
                    t.set_language(lang);
                }
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
                    let hotkey_str =
                        format!("{}+{}", current_hotkey.modifier, current_hotkey.key);
                    let _ = app_handle.emit(
                        "platform-event",
                        serde_json::json!({
                            "event": "initial-hotkey-failed",
                            "hotkey": hotkey_str,
                        }),
                    );
                }
            }
            PlatformCommand::Exit => unsafe {
                PostQuitMessage(0);
            },
        }
    }
}
