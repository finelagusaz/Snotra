use std::sync::atomic::Ordering;

use serde_json::json;
use snotra_core::window_data::{self, WindowPlacement, WindowSize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri::{WebviewUrl, WebviewWindowBuilder};

use crate::indexing;
use crate::state::AppState;

use super::trace_command;

pub(crate) fn ensure_results_window(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window("results").is_some() {
        trace_command("cmd:ensure_results_window:exists", json!({}));
        return Ok(());
    }
    trace_command("cmd:ensure_results_window:create", json!({}));
    WebviewWindowBuilder::new(app, "results", WebviewUrl::App(Default::default()))
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

pub(crate) fn ensure_about_window(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window("about").is_some() {
        trace_command("cmd:ensure_about_window:exists", json!({}));
        return Ok(());
    }
    trace_command("cmd:ensure_about_window:create", json!({}));
    let about_window = WebviewWindowBuilder::new(app, "about", WebviewUrl::App(Default::default()))
        .title("Snotra について")
        .inner_size(400.0, 300.0)
        .resizable(false)
        .visible(false)
        .build()?;

    let handle_for_about_close = app.clone();
    about_window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            if let Some(w) = handle_for_about_close.get_webview_window("about") {
                let _ = w.hide();
            }
            // settings も非表示なら main の alwaysOnTop を戻す
            let settings_hidden = handle_for_about_close
                .get_webview_window("settings")
                .map(|w| !w.is_visible().unwrap_or(true))
                .unwrap_or(true);
            if settings_hidden
                && let Some(main) = handle_for_about_close.get_webview_window("main")
            {
                let _ = main.set_always_on_top(true);
            }
        }
    });
    Ok(())
}

pub fn ensure_settings_window(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window("settings").is_some() {
        trace_command("cmd:ensure_settings_window:exists", json!({}));
        return Ok(());
    }
    trace_command("cmd:ensure_settings_window:create", json!({}));
    let settings_window =
        WebviewWindowBuilder::new(app, "settings", WebviewUrl::App(Default::default()))
            .title("Snotra 設定")
            .inner_size(760.0, 560.0)
            .min_inner_size(520.0, 360.0)
            .resizable(true)
            .visible(false)
            .build()?;

    // Keep window alive to avoid repeated WebView initialization.
    let handle_for_close = app.clone();
    settings_window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            // Safety net: JS が preventDefault() し忘れた場合のフォールバック。
            // 通常は JS 側で CloseRequested を prevent し、hide_settings IPC で閉じる。
            api.prevent_close();
            if let Some(w) = handle_for_close.get_webview_window("settings") {
                let _ = w.hide();
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn hide_settings(state: State<AppState>, app: AppHandle) {
    trace_command("cmd:hide_settings:start", json!({}));
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.hide();
    }
    // about も非表示なら main の alwaysOnTop を戻す
    let about_hidden = app
        .get_webview_window("about")
        .map(|w| !w.is_visible().unwrap_or(true))
        .unwrap_or(true);
    if about_hidden
        && let Some(main) = app.get_webview_window("main")
    {
        let _ = main.set_always_on_top(true);
    }
    // First-run: start index build when settings is dismissed.
    if state.indexing.load(Ordering::SeqCst)
        && !state.index_build_started.load(Ordering::SeqCst)
    {
        indexing::start_index_build(&app);
    }
    trace_command("cmd:hide_settings:ok", json!({}));
}

#[tauri::command]
pub fn open_settings(state: State<AppState>, app: AppHandle) -> Result<(), String> {
    trace_command("cmd:open_settings:start", json!({}));
    if state.indexing.load(Ordering::SeqCst) {
        trace_command("cmd:open_settings:noop_indexing", json!({}));
        return Ok(());
    }

    if let Err(e) = ensure_settings_window(&app) {
        let msg = e.to_string();
        trace_command("cmd:open_settings:error", json!({ "error": msg }));
        return Err(msg);
    }
    if let Some(w) = app.get_webview_window("settings") {
        // main は alwaysOnTop のため、settings が前面に出るよう一時的に外す（settings close 時に戻す）
        if let Some(main) = app.get_webview_window("main") {
            let _ = main.set_always_on_top(false);
        }
        // show() の前に保存済みの位置・サイズを復元する。
        // JS 側の onMount でも復元するが非同期のため show() に間に合わない。
        // Rust 側で同期的に復元することで初回表示時の白画面フラッシュを防ぐ。
        if let Some(size) = window_data::load_settings_size() {
            let _ = w.set_size(tauri::LogicalSize::new(size.width, size.height));
        }
        if let Some(placement) = window_data::load_settings_placement() {
            let _ = w.set_position(tauri::LogicalPosition::new(placement.x, placement.y));
        }
        let _ = app.emit("settings-shown", ());
        let _ = w.show();
        let _ = w.set_focus();
        trace_command("cmd:open_settings:ok", json!({ "window_found": true }));
    } else {
        trace_command("cmd:open_settings:ok", json!({ "window_found": false }));
    }
    Ok(())
}

#[tauri::command]
pub fn open_about(app: AppHandle) -> Result<(), String> {
    trace_command("cmd:open_about:start", json!({}));
    if let Err(e) = ensure_about_window(&app) {
        let msg = e.to_string();
        trace_command("cmd:open_about:error", json!({ "error": msg }));
        return Err(msg);
    }
    if let Some(w) = app.get_webview_window("about") {
        // main は alwaysOnTop のため、about が前面に出るよう一時的に外す（about close 時に戻す）
        if let Some(main) = app.get_webview_window("main") {
            let _ = main.set_always_on_top(false);
        }
        let _ = w.show();
        let _ = w.set_focus();
        trace_command("cmd:open_about:ok", json!({ "window_found": true }));
    } else {
        trace_command("cmd:open_about:ok", json!({ "window_found": false }));
    }
    Ok(())
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
        "about" => {
            let existed = app.get_webview_window("about").is_some();
            if let Err(e) = ensure_about_window(&app) {
                let msg = e.to_string();
                trace_command(
                    "cmd:ensure_window:error",
                    json!({
                        "label": "about",
                        "error": msg,
                    }),
                );
                return Err(msg);
            }
            trace_command(
                "cmd:ensure_window:ok",
                json!({
                    "label": "about",
                    "created": !existed,
                }),
            );
            Ok(!existed)
        }
        "settings" => {
            let existed = app.get_webview_window("settings").is_some();
            if let Err(e) = ensure_settings_window(&app) {
                let msg = e.to_string();
                trace_command(
                    "cmd:ensure_window:error",
                    json!({
                        "label": "settings",
                        "error": msg,
                    }),
                );
                return Err(msg);
            }
            trace_command(
                "cmd:ensure_window:ok",
                json!({
                    "label": "settings",
                    "created": !existed,
                }),
            );
            Ok(!existed)
        }
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
pub fn get_settings_placement() -> (Option<WindowPlacement>, Option<WindowSize>) {
    (
        window_data::load_settings_placement(),
        window_data::load_settings_size(),
    )
}

#[tauri::command]
pub fn save_settings_placement(x: i32, y: i32) {
    window_data::save_settings_placement(WindowPlacement { x, y });
}

#[tauri::command]
pub fn save_settings_size(width: i32, height: i32) {
    window_data::save_settings_size(WindowSize { width, height });
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
