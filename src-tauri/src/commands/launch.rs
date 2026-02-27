use std::time::Duration;

use serde_json::json;
use tauri::{AppHandle, Manager};
use tokio::time::timeout;

use crate::state::AppState;

use super::trace_command;

#[derive(serde::Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LaunchStatus {
    Ok,
    Failed,
    Timeout,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResult {
    pub status: LaunchStatus,
    pub code: i32,
    pub message: Option<String>,
}

impl LaunchResult {
    fn ok(code: i32) -> Self {
        Self {
            status: LaunchStatus::Ok,
            code,
            message: None,
        }
    }

    fn failed(code: i32, message: impl Into<String>) -> Self {
        Self {
            status: LaunchStatus::Failed,
            code,
            message: Some(message.into()),
        }
    }

    fn timeout(timeout_ms: u64) -> Self {
        Self {
            status: LaunchStatus::Timeout,
            code: -1,
            message: Some(format!("launch_timeout_{}ms", timeout_ms)),
        }
    }

    fn is_ok(&self) -> bool {
        self.status == LaunchStatus::Ok
    }
}

const LAUNCH_TIMEOUT_MS: u64 = 4_000;

#[tauri::command]
pub async fn launch_item(
    path: String,
    query: String,
    app: AppHandle,
) -> Result<LaunchResult, String> {
    trace_command(
        "cmd:launch_item:start",
        json!({
            "path": path,
            "query_len": query.chars().count(),
            "timeout_ms": LAUNCH_TIMEOUT_MS,
        }),
    );
    let launch_path = path.clone();
    let join = tauri::async_runtime::spawn_blocking(move || launch_item_core(&launch_path));
    let result = match timeout(Duration::from_millis(LAUNCH_TIMEOUT_MS), join).await {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => LaunchResult::failed(-1, format!("launch_worker_join_error: {e}")),
        Err(_) => LaunchResult::timeout(LAUNCH_TIMEOUT_MS),
    };

    if result.is_ok() {
        let state = app.state::<AppState>();
        let mut history = state.history.lock().unwrap();
        history.record_launch(&path, &query);
        history.save_if_dirty(5);
    }

    trace_command(
        "cmd:launch_item:done",
        json!({
            "path": path,
            "status": result.status,
            "code": result.code,
            "message": result.message,
        }),
    );
    Ok(result)
}

pub fn launch_item_with_state(path: &str, query: &str, state: &AppState) -> LaunchResult {
    let result = launch_item_core(path);
    if result.is_ok() {
        let mut history = state.history.lock().unwrap();
        history.record_launch(path, query);
        history.save_if_dirty(5);
    }
    result
}

fn shell_execute_error_message(code: i32) -> &'static str {
    match code {
        0 => "out_of_memory_or_resources",
        2 => "file_not_found",
        3 => "path_not_found",
        5 => "access_denied",
        8 => "out_of_memory",
        26 => "sharing_violation",
        27 => "association_incomplete",
        28 => "dde_timeout",
        29 => "dde_failed",
        30 => "dde_busy",
        31 => "no_association",
        32 => "dll_not_found",
        _ => "shell_execute_failed",
    }
}

fn launch_item_core(path: &str) -> LaunchResult {
    #[cfg(windows)]
    {
        use windows::Win32::System::Com::{
            COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize,
        };
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::HSTRING;
        unsafe {
            // S_OK / S_FALSE の場合に CoUninitialize が必要。
            let com_ok = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
            let raw_code = ShellExecuteW(
                None,
                &HSTRING::from("open"),
                &HSTRING::from(path),
                None,
                None,
                SW_SHOWNORMAL,
            )
            .0 as isize;
            if com_ok {
                CoUninitialize();
            }
            if raw_code > 32 {
                return LaunchResult::ok(raw_code as i32);
            }
            let code = raw_code as i32;
            return LaunchResult::failed(code, shell_execute_error_message(code));
        }
    }

    #[cfg(not(windows))]
    {
        let _ = path;
        LaunchResult::failed(-1, "unsupported_platform")
    }
}
