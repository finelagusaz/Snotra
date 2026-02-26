use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::json;
use snotra_core::config::Config;
use snotra_core::folder;
use snotra_core::search::{HistoryBoostConfig, SearchMode};
use snotra_core::ui_types::SearchResult;
use snotra_core::window_data::{self, WindowPlacement, WindowSize};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, State};
use tauri::{WebviewUrl, WebviewWindowBuilder};
use tokio::time::timeout;

use crate::icon::{IconCache, IconCacheState};
use crate::indexing;
use crate::platform::{PlatformBridge, PlatformCommand};
use crate::state::AppState;

#[derive(serde::Serialize, Clone)]
pub struct SaveConfigResult {
    pub reindex_started: bool,
}

#[derive(serde::Serialize, Clone)]
pub struct BootstrapGeneralConfig {
    pub auto_hide_on_focus_lost: bool,
}

#[derive(serde::Serialize, Clone)]
pub struct BootstrapPayload {
    pub visual: snotra_core::config::VisualConfig,
    pub general: BootstrapGeneralConfig,
    pub indexing: bool,
}

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

fn trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        let Ok(v) = std::env::var("SNOTRA_TRACE") else {
            return false;
        };
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn trace_command(event: &str, data: serde_json::Value) {
    if !trace_enabled() {
        return;
    }
    static TRACE_SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = TRACE_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    let ts_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    eprintln!(
        "[trace] {}",
        json!({
            "seq": seq,
            "ts_ms": ts_ms,
            "event": event,
            "data": data,
        })
    );
}

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
            if settings_hidden {
                if let Some(main) = handle_for_about_close.get_webview_window("main") {
                    let _ = main.set_always_on_top(true);
                }
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
            api.prevent_close();
            if let Some(w) = handle_for_close.get_webview_window("settings") {
                let _ = w.hide();
            }
            // about も非表示なら main の alwaysOnTop を戻す
            let about_hidden = handle_for_close
                .get_webview_window("about")
                .map(|w| !w.is_visible().unwrap_or(true))
                .unwrap_or(true);
            if about_hidden {
                if let Some(main) = handle_for_close.get_webview_window("main") {
                    let _ = main.set_always_on_top(true);
                }
            }
            // First-run: start index build when settings is dismissed.
            let state = handle_for_close.state::<AppState>();
            if state.indexing.load(Ordering::SeqCst)
                && !state.index_build_started.load(Ordering::SeqCst)
            {
                indexing::start_index_build(&handle_for_close);
            }
        }
    });
    Ok(())
}

fn ensure_icon_cache_loaded_if_enabled(state: &State<AppState>, icons: &State<IconCacheState>) {
    let show_icons = {
        let cfg = state.config.lock().unwrap();
        cfg.appearance.show_icons
    };
    let mut cache = icons.lock().unwrap();
    if !show_icons {
        *cache = None;
        return;
    }
    if cache.is_none() {
        *cache = Some(IconCache::load());
    }
}

#[tauri::command]
pub fn search(query: String, state: State<AppState>) -> Vec<SearchResult> {
    trace_command(
        "cmd:search:start",
        json!({ "query_len": query.chars().count() }),
    );
    let config = state.config.lock().unwrap();
    let mut engine = state.engine.lock().unwrap();
    let history = state.history.lock().unwrap();
    let mode: SearchMode = config.search.normal_mode.into();
    let history_boost_config: HistoryBoostConfig = (&config.search).into();
    let results = engine.search_with_history_boost(
        &query,
        config.appearance.max_results,
        &history,
        mode,
        history_boost_config,
    );
    trace_command(
        "cmd:search:ok",
        json!({
            "query_len": query.chars().count(),
            "result_count": results.len(),
        }),
    );
    results
}

#[tauri::command]
pub fn get_history_results(state: State<AppState>) -> Vec<SearchResult> {
    trace_command("cmd:get_history_results:start", json!({}));
    let config = state.config.lock().unwrap();
    let engine = state.engine.lock().unwrap();
    let history = state.history.lock().unwrap();
    let results = engine.recent_history(&history, config.appearance.max_history_display);
    trace_command(
        "cmd:get_history_results:ok",
        json!({ "result_count": results.len() }),
    );
    results
}

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

#[tauri::command]
pub fn list_folder(dir: String, filter: String, state: State<AppState>) -> Vec<SearchResult> {
    trace_command(
        "cmd:list_folder:start",
        json!({
            "dir": dir,
            "filter_len": filter.chars().count(),
        }),
    );
    let config = state.config.lock().unwrap();
    let history = state.history.lock().unwrap();
    let mode: SearchMode = config.search.folder_mode.into();
    let results = folder::list_folder(
        Path::new(&dir),
        &filter,
        mode,
        config.search.show_hidden_system,
        &history,
        config.appearance.max_results,
    );
    trace_command(
        "cmd:list_folder:ok",
        json!({
            "dir": dir,
            "result_count": results.len(),
        }),
    );
    results
}

#[tauri::command]
pub fn load_config() -> Config {
    Config::load()
}

#[tauri::command]
pub fn save_config(
    config: Config,
    state: State<AppState>,
    app: AppHandle,
) -> Result<SaveConfigResult, String> {
    let old_config = state.config.lock().unwrap().clone();

    // Detect what changed before moving config into state
    let index_changed = config.paths.scan != old_config.paths.scan
        || config.search.show_hidden_system != old_config.search.show_hidden_system
        || config.appearance.show_icons != old_config.appearance.show_icons;
    let visual_changed = config.visual != old_config.visual;
    let width_changed = config.appearance.window_width != old_config.appearance.window_width;
    let new_visual = if visual_changed {
        Some(config.visual.clone())
    } else {
        None
    };
    let new_width = config.appearance.window_width;

    // Notify platform bridge of hotkey/tray changes.
    // Hotkey registration is checked BEFORE saving to disk: if registration fails,
    // we return Err without persisting the invalid hotkey.
    if let Some(bridge) = app.try_state::<std::sync::Mutex<PlatformBridge>>()
        && let Ok(b) = bridge.lock()
    {
        if config.hotkey != old_config.hotkey {
            let (tx, rx) = std::sync::mpsc::channel();
            b.send_command(PlatformCommand::SetHotkey {
                config: config.hotkey.clone(),
                reply: tx,
            });
            match rx.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(false) | Err(_) => return Err("hotkey_registration_failed".to_string()),
                Ok(true) => {}
            }
        }
        if config.general.show_tray_icon != old_config.general.show_tray_icon {
            b.send_command(PlatformCommand::SetTrayVisible(
                config.general.show_tray_icon,
            ));
        }
    }

    // Save to disk only after hotkey validation succeeded
    config.save();

    {
        let mut current = state.config.lock().unwrap();
        *current = config;
    }

    // First-run path: initial indexing is pending (indexing=true) but build not started yet.
    // Do not treat regular reindex-in-progress as first run.
    let is_first_run_pending =
        state.indexing.load(Ordering::SeqCst) && !state.index_build_started.load(Ordering::SeqCst);
    if is_first_run_pending {
        indexing::start_index_build(&app);
        if let Some(w) = app.get_webview_window("settings") {
            let _ = w.close();
        }
    }

    // Trigger reindex if index-related settings changed.
    // Never restart while a build is already running, otherwise multiple
    // index threads can race and last-writer wins.
    let mut reindex_started = false;
    let indexing_in_progress = state.indexing.load(Ordering::SeqCst);
    if index_changed && !is_first_run_pending && !indexing_in_progress {
        state.index_build_started.store(false, Ordering::SeqCst);
        reindex_started = indexing::start_index_build(&app);
    }

    // Emit visual config change for live theme update
    if let Some(visual) = new_visual {
        let _ = app.emit("visual-config-changed", &visual);
    }

    // Resize main and results windows if window_width changed
    if width_changed && new_width > 0 {
        for label in &["main", "results"] {
            if let Some(w) = app.get_webview_window(label)
                && let Ok(size) = w.inner_size()
                && let Ok(sf) = w.scale_factor()
            {
                let logical = size.to_logical::<f64>(sf);
                let _ = w.set_size(LogicalSize::new(f64::from(new_width), logical.height));
            }
        }
    }

    Ok(SaveConfigResult { reindex_started })
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Config {
    state.config.lock().unwrap().clone()
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
pub fn get_icon_base64(
    path: String,
    state: State<AppState>,
    icons: State<IconCacheState>,
) -> Option<String> {
    ensure_icon_cache_loaded_if_enabled(&state, &icons);
    let mut cache = icons.lock().unwrap();
    cache.as_mut()?.get_or_extract(&path)
}

#[tauri::command]
pub fn get_icons_batch(
    paths: Vec<String>,
    state: State<AppState>,
    icons: State<IconCacheState>,
) -> std::collections::HashMap<String, String> {
    ensure_icon_cache_loaded_if_enabled(&state, &icons);
    let mut cache = icons.lock().unwrap();
    match cache.as_mut() {
        Some(c) => c.get_or_extract_batch(&paths),
        None => std::collections::HashMap::new(),
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

#[tauri::command]
pub fn notify_result_clicked(path: String, app: AppHandle) -> Result<(), String> {
    app.emit("result-clicked", path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn notify_result_double_clicked(index: usize, app: AppHandle) -> Result<(), String> {
    app.emit("result-double-clicked", index)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_indexing_state(state: State<AppState>) -> bool {
    state.indexing.load(Ordering::SeqCst)
}

#[tauri::command]
pub fn rebuild_index(state: State<AppState>, app: AppHandle) -> bool {
    if state.indexing.load(Ordering::SeqCst) {
        return false;
    }
    // Reset the guard so start_index_build can proceed
    state.index_build_started.store(false, Ordering::SeqCst);
    indexing::start_index_build(&app)
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    // Reuse the existing exit-requested listener (main.rs)
    // which flushes history/icons, notifies platform, and exits
    let _ = app.emit("exit-requested", ());
}

#[tauri::command]
pub fn list_system_fonts() -> Vec<String> {
    #[cfg(windows)]
    {
        use std::collections::BTreeSet;
        use windows::Win32::Foundation::LPARAM;
        use windows::Win32::Graphics::Gdi::*;

        unsafe extern "system" fn enum_callback(
            logfont: *const LOGFONTW,
            _text_metric: *const TEXTMETRICW,
            _font_type: u32,
            lparam: LPARAM,
        ) -> i32 {
            unsafe {
                let fonts = &mut *(lparam.0 as *mut BTreeSet<String>);
                let lf = &*logfont;
                let name_len = lf.lfFaceName.iter().position(|&c| c == 0).unwrap_or(32);
                let name = String::from_utf16_lossy(&lf.lfFaceName[..name_len]);
                // @ 始まりは縦書き用フォント、除外
                if !name.starts_with('@') {
                    fonts.insert(name);
                }
                1 // 列挙を続行
            }
        }

        let mut fonts = BTreeSet::<String>::new();
        let hdc = unsafe { GetDC(None) };
        let mut lf: LOGFONTW = unsafe { std::mem::zeroed() };
        lf.lfCharSet = DEFAULT_CHARSET;

        unsafe {
            EnumFontFamiliesExW(
                hdc,
                &lf,
                Some(enum_callback),
                LPARAM(&mut fonts as *mut _ as isize),
                0,
            );
            ReleaseDC(None, hdc);
        }

        fonts.into_iter().collect()
    }
    #[cfg(not(windows))]
    Vec::new()
}

#[tauri::command]
pub fn record_folder_expansion(path: String, state: State<AppState>) {
    let mut history = state.history.lock().unwrap();
    history.record_folder_expansion(&path);
    history.save_if_dirty(5);
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
pub fn get_bootstrap_payload(state: State<AppState>) -> BootstrapPayload {
    let config = state.config.lock().unwrap();
    BootstrapPayload {
        visual: config.visual.clone(),
        general: BootstrapGeneralConfig {
            auto_hide_on_focus_lost: config.general.auto_hide_on_focus_lost,
        },
        indexing: state.indexing.load(Ordering::SeqCst),
    }
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
