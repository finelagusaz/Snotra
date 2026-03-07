use std::process::{Child, Command};
use std::sync::atomic::Ordering;
use std::sync::Mutex;

use serde_json::json;
use snotra_core::window_data::{self, WindowPlacement};
use tauri::{AppHandle, Manager, State};
use tauri::{WebviewUrl, WebviewWindowBuilder};

use crate::indexing;
use crate::state::AppState;

use super::trace_command;

/// Managed state for tracking the snotra-settings child process.
pub type SettingsProcessState = Mutex<Option<Child>>;

pub(crate) fn ensure_results_window(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window("results").is_some() {
        trace_command("cmd:ensure_results_window:exists", json!({}));
        return Ok(());
    }
    trace_command("cmd:ensure_results_window:create", json!({}));
    WebviewWindowBuilder::new(app, "results", WebviewUrl::App("results.html".into()))
        .title("")
        .inner_size(600.0, 300.0)
        .visible(false)
        .decorations(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .resizable(false)
        .focused(false)
        .build()?;
    let _ = set_window_no_activate(app.clone());
    Ok(())
}

/// Launch `snotra-settings` as a child process with optional extra arguments.
///
/// Deduplicates: if a settings process is already running, this is a no-op.
/// Temporarily disables main window alwaysOnTop while the child is alive
/// and restores it when the child exits.
///
/// # Errors
/// Returns `Err` if the executable is not found or spawning fails.
/// On first-run, failure leaves `indexing=true` permanently unless the caller
/// provides a fallback (e.g. `indexing::start_index_build`).
#[must_use = "failure during first-run leaves indexing=true; handle Err with a fallback"]
pub(crate) fn launch_settings_process(app: &AppHandle, extra_args: &[&str]) -> Result<(), String> {
    let proc_state = app
        .try_state::<SettingsProcessState>()
        .ok_or("SettingsProcessState not managed")?;

    let mut guard = proc_state.lock().unwrap();

    // Check if a settings process is already running.
    if let Some(child) = guard.as_mut() {
        match child.try_wait() {
            Ok(Some(_)) => {
                // Process has exited; clear stale handle and proceed to spawn.
                *guard = None;
            }
            Ok(None) => {
                // Still running — do nothing.
                trace_command(
                    "cmd:launch_settings_process:already_running",
                    json!({ "pid": child.id() }),
                );
                return Ok(());
            }
            Err(e) => {
                // Error checking status; clear handle and try to spawn.
                eprintln!("[settings-process] try_wait error: {e}");
                *guard = None;
            }
        }
    }

    // Find snotra-settings executable next to our own binary.
    let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_dir = exe_path.parent().ok_or("cannot determine exe directory")?;
    let settings_exe = exe_dir.join("snotra-settings.exe");

    if !settings_exe.exists() {
        let msg = format!("snotra-settings.exe not found at {}", settings_exe.display());
        trace_command(
            "cmd:launch_settings_process:not_found",
            json!({ "path": settings_exe.display().to_string() }),
        );
        return Err(msg);
    }

    let child = Command::new(&settings_exe)
        .args(extra_args)
        .spawn()
        .map_err(|e| format!("failed to spawn snotra-settings: {e}"))?;

    let pid = child.id();
    trace_command(
        "cmd:launch_settings_process:spawned",
        json!({ "pid": pid, "args": extra_args }),
    );

    *guard = Some(child);
    drop(guard);

    // Temporarily disable main window alwaysOnTop so snotra-settings can be focused.
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.set_always_on_top(false);
    }

    // Spawn a monitoring thread to restore alwaysOnTop when the process exits.
    let handle_for_monitor = app.clone();
    std::thread::spawn(move || {
        // Poll child process status. Child is kept in SettingsProcessState so
        // the dedup check in launch_settings_process works.
        loop {
            std::thread::sleep(std::time::Duration::from_millis(250));
            let proc_state = handle_for_monitor
                .try_state::<SettingsProcessState>()
                .expect("SettingsProcessState not managed");
            let mut guard = proc_state.lock().unwrap();
            if let Some(child) = guard.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        trace_command(
                            "cmd:launch_settings_process:exited",
                            json!({ "pid": pid, "status": status.code() }),
                        );
                        *guard = None;
                        break;
                    }
                    Ok(None) => {} // Still running.
                    Err(e) => {
                        eprintln!("[settings-process] monitor try_wait error: {e}");
                        *guard = None;
                        break;
                    }
                }
            } else {
                // Child handle was already cleared (e.g. by exit handler).
                break;
            }
        }

        // Restore main window alwaysOnTop.
        if let Some(main) = handle_for_monitor.get_webview_window("main") {
            let _ = main.set_always_on_top(true);
        }

        // First-run: if indexing is pending and not started, kick off index build.
        if let Some(state) = handle_for_monitor.try_state::<AppState>()
            && state.indexing.load(Ordering::SeqCst)
            && !state.index_build_started.load(Ordering::SeqCst)
        {
            indexing::start_index_build(&handle_for_monitor);
        }
    });

    Ok(())
}

#[tauri::command]
pub fn open_settings(state: State<AppState>, app: AppHandle) -> Result<(), String> {
    trace_command("cmd:open_settings:start", json!({}));
    if state.indexing.load(Ordering::SeqCst) {
        trace_command("cmd:open_settings:noop_indexing", json!({}));
        return Ok(());
    }

    launch_settings_process(&app, &[])
}

#[tauri::command]
pub fn ensure_window(label: String, app: AppHandle) -> Result<bool, String> {
    trace_command(
        "cmd:ensure_window:start",
        json!({ "label": label.as_str() }),
    );
    match label.as_str() {
        "results" => {
            let existed = app.get_webview_window("results").is_some();
            if let Err(e) = ensure_results_window(&app) {
                let msg = e.to_string();
                trace_command(
                    "cmd:ensure_window:error",
                    json!({
                        "label": "results",
                        "error": msg,
                    }),
                );
                return Err(msg);
            }
            trace_command(
                "cmd:ensure_window:ok",
                json!({
                    "label": "results",
                    "created": !existed,
                }),
            );
            Ok(!existed)
        }
        // about/settings are now separate processes; ensure_window is not applicable.
        _ => {
            trace_command(
                "cmd:ensure_window:error",
                json!({
                    "label": label,
                    "error": "unsupported_window_label",
                }),
            );
            Err("unsupported_window_label".to_string())
        }
    }
}

#[tauri::command]
pub fn get_search_placement() -> Option<WindowPlacement> {
    window_data::load_search_placement()
}

#[tauri::command]
pub fn save_search_placement(x: i32, y: i32) {
    window_data::save_search_placement(WindowPlacement { x, y });
}

#[tauri::command]
pub fn is_main_foreground(_app: AppHandle) -> bool {
    // "main が foreground か" ではなく "foreground が自プロセス所属か" で判定する。
    // WS_EX_NOACTIVATE を設定しても WebView2 が SetForegroundWindow() を内部で呼ぶため
    // results ウィンドウが foreground になることがある。自プロセス所属なら app 内操作と判断し非表示をスキップ。
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
        unsafe {
            let foreground = GetForegroundWindow();
            let our_pid = std::process::id();
            let mut fg_pid = 0u32;
            GetWindowThreadProcessId(foreground, Some(&mut fg_pid));
            return fg_pid == our_pid;
        }
    }
    #[allow(unreachable_code)]
    false
}

#[tauri::command]
pub fn set_window_no_activate(app: AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GetWindowLongW, SetWindowLongW, WS_EX_NOACTIVATE,
        };
        if let Some(w) = app.get_webview_window("results") {
            let raw_hwnd = w.hwnd().map_err(|e| e.to_string())?;
            let hwnd = HWND(raw_hwnd.0);
            unsafe {
                let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
                SetWindowLongW(hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as i32);
            }
        }
    }
    Ok(())
}
