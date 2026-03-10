use std::process::{Child, Command};
use std::sync::atomic::Ordering;
use std::sync::Mutex;

use serde_json::json;
use snotra_core::window_data::{self, WindowPlacement};
use tauri::{AppHandle, Manager, State, WebviewWindow};

use crate::indexing;
use crate::state::AppState;

use super::trace_command;

/// Managed state for tracking the snotra-settings child process.
pub type SettingsProcessState = Mutex<Option<Child>>;

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
pub fn get_search_placement() -> Option<WindowPlacement> {
    window_data::load_search_placement()
}

/// Save the main window's current position as physical-pixel coordinates
/// relative to the monitor work area origin.
///
/// The Rust side reads the window position directly via HWND, so the
/// frontend only needs to signal "save now" without passing coordinates.
#[tauri::command]
pub fn save_search_placement(app: AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        save_relative_placement(&main);
    }
}

/// Convert the window's absolute physical position to monitor-relative
/// coordinates and persist them.
fn save_relative_placement(window: &WebviewWindow) {
    let Ok(pos) = window.outer_position() else {
        return;
    };

    #[cfg(windows)]
    {
        let Ok(hwnd) = window.hwnd() else { return };
        let Some(wa) = crate::monitor::window_monitor_work_area(hwnd.0 as isize) else {
            return;
        };
        let relative = WindowPlacement {
            x: pos.x - wa.left,
            y: pos.y - wa.top,
        };
        window_data::save_search_placement(relative);
    }

    #[cfg(not(windows))]
    {
        // Non-Windows: save absolute position as-is (no monitor API).
        window_data::save_search_placement(WindowPlacement { x: pos.x, y: pos.y });
    }
}
