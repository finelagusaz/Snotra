pub mod hotkey;
pub(crate) mod tray;
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
    /// config が壊れて既定値にフォールバックした旨をトレイバルーンで通知する。
    /// トレイ生成後（`SetTrayVisible(true)` の後）に送ること。
    ShowConfigRecoveryBalloon,
    Exit,
}

pub struct PlatformBridgePending {
    command_tx: Sender<PlatformCommand>,
    thread_id_rx: Receiver<u32>,
}

/// platform bridge の失敗分類。
///
/// **`Option` ではなくこれを返すのは、起動の終端が理由を要るからである**——どの失敗でも
/// `None` に潰すと、`startup:failed` が出せずハーネスの「タイムアウト」に化ける
/// （`crate::startup` の `//!`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeError {
    /// platform スレッドの spawn に失敗した。
    Spawn,
    /// platform スレッドの Win32 初期化に失敗した（`thread_id` に 0 が返る）。
    Init,
    /// 初期化結果を受け取れなかった（channel 切断）。
    Handshake,
    /// command の送信に失敗した（channel 切断）。
    Disconnected,
}

impl PlatformBridgePending {
    /// Blocks until the platform thread signals its Win32 init is complete.
    pub fn wait(self) -> Result<PlatformBridge, BridgeError> {
        let thread_id = self
            .thread_id_rx
            .recv()
            .map_err(|_| BridgeError::Handshake)?;
        if thread_id == 0 {
            return Err(BridgeError::Init);
        }
        Ok(PlatformBridge {
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
    ) -> Result<PlatformBridgePending, BridgeError> {
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
            .map_err(|_| BridgeError::Spawn)?;

        Ok(PlatformBridgePending {
            command_tx,
            thread_id_rx,
        })
    }

    /// 初回ホットキー登録コマンドの送信。**成否を返す唯一の送信経路である。**
    ///
    /// 他の送信（[`Self::send_command`]）は fire-and-forget でよい——起動の終端に関わらない
    /// ためである。こちらだけが結果を返すのは、**bridge が生きていない起動を
    /// `startup:failed` として出せないと、ハーネスの「タイムアウト」に化ける**からである。
    pub fn send_initial_hotkey_registration(&self) -> Result<(), BridgeError> {
        if self.thread_id == 0 {
            return Err(BridgeError::Init);
        }
        self.command_tx
            .send(PlatformCommand::RegisterInitialHotkey)
            .map_err(|_| BridgeError::Disconnected)?;
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_PLATFORM_WAKE, WPARAM(0), LPARAM(0));
        }
        Ok(())
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
                    let _ = app_handle.emit(crate::events::HOTKEY_PRESSED, ());
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
                let success = hotkey::replace(current_hotkey, &config);
                if success {
                    *current_hotkey = config;
                    let _ = reply.send(true);
                } else {
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
                // 送信元は egui_shell::show_egui_main（旧 show_main_and_emit は #532 SU7 で削除）。
                // 呼び出し順は set_focus() → 本コマンド送信であり、「focus より先に IME を切る」
                // ことは意図的に避けている（前に置くと IME オフが対象窓に効かない）。
                // **かつて両者の間に在った `SendMessageTimeoutW` のフォーカス同期待ちは、
                // show がイベントループスレッドへ移って no-op 化したため撤去された**——機構と
                // 「導出し直せない」理由は `egui_shell/window_coordinator.rs` の `set_focus()`
                // 直後のコメントが正本。HWND を直接渡すのは再 lookup を避けるため。
                let hwnd = windows::Win32::Foundation::HWND(hwnd_raw as *mut core::ffi::c_void);
                if !hwnd.is_invalid() {
                    ime::turn_off_ime(hwnd);
                }
            }
            PlatformCommand::RegisterInitialHotkey => {
                // 視覚スモーク専用のエスケープハッチ（`SNOTRA_EGUI_FAKE_UPDATE` と同じ流儀）。
                // 実機で RegisterHotKey を失敗させるには他アプリが同じキーを握っている必要が
                // あり再現性が無いため、**登録は成功させたまま失敗イベントだけを流す**。
                // ゆえにハッチ使用中もホットキーは動く（#652 の受け口検証用）。
                let registered = hotkey::register(current_hotkey)
                    && !crate::trace::env_flag("SNOTRA_FAKE_INITIAL_HOTKEY_FAILURE");
                if !registered {
                    let hotkey_str = format!("{}+{}", current_hotkey.modifier, current_hotkey.key);
                    // 独立イベント名で emit する（#673 spec 決定 3）。旧実装は汎用チャネル
                    // 1 本に `{event, hotkey}` を詰めており、内側種別は最後までこの 1 種
                    // だけだった——袋の中身は grep に映らず、受け口の有無を機構で問えない
                    // （#652 の gap 特定が構造的に不可能だった原因）。payload は
                    // `hotkey-registration-failed` と同じ素の String に揃える。
                    let _ = app_handle.emit(crate::events::INITIAL_HOTKEY_FAILED, hotkey_str);
                }
                // 起動の終端（成功側）。**ここが「押せば窓が出る状態」の成立点である**
                // ——listener は送信より前に登録され、窓は `egui_shell::create` で既に在る
                // （根拠は `crate::startup` の `//!`）。失敗側の終端は `main.rs` にもある。
                crate::startup::finish(if registered {
                    Ok(())
                } else {
                    Err(crate::startup::StartupFailure::HotkeyRegistration)
                });
            }
            PlatformCommand::ShowConfigRecoveryBalloon => {
                // トレイ未生成（show_tray_icon=false 等）なら no-op。
                if let Some(t) = tray.as_mut() {
                    t.show_config_recovery_balloon();
                }
            }
            PlatformCommand::Exit => unsafe {
                PostQuitMessage(0);
            },
        }
    }
}
