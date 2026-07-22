use std::time::Duration;

use serde_json::json;
use snotra_core::config::{find_matching_tools, InstantAction, InstantCommand, OpenerTool};
use snotra_core::instant::{expand_exec_args, split_args};
use std::process::Stdio;
use tauri::{AppHandle, Manager};
use tokio::time::timeout;

use crate::state::AppState;

use super::trace_command;

#[derive(Debug, serde::Serialize, Clone, Copy, PartialEq, Eq)]
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

    pub(crate) fn failed(code: i32, message: impl Into<String>) -> Self {
        Self {
            status: LaunchStatus::Failed,
            code,
            message: Some(message.into()),
        }
    }

    pub(crate) fn timeout(timeout_ms: u64) -> Self {
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
const PATH_PLACEHOLDER: &str = "{path}";

/// Run a blocking launch closure on the blocking pool, bounded by
/// `LAUNCH_TIMEOUT_MS`. Common wrapper for the three launch entry points
/// (`launch_with_tool`, `launch_item`, `commands::instant::execute_instant_command`),
/// all of which spawn a blocking Win32/process call that must not stall the
/// async runtime forever.
pub(crate) async fn run_launch_blocking<F>(f: F) -> LaunchResult
where
    F: FnOnce() -> LaunchResult + Send + 'static,
{
    let join = tauri::async_runtime::spawn_blocking(f);
    match timeout(Duration::from_millis(LAUNCH_TIMEOUT_MS), join).await {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => LaunchResult::failed(-1, format!("launch_worker_join_error: {e}")),
        Err(_) => LaunchResult::timeout(LAUNCH_TIMEOUT_MS),
    }
}

/// Record a successful launch in history and persist if the pending-write
/// threshold is reached. Common tail of all launch entry points
/// (`launch_with_tool`, `launch_item`, `launch_item_with_state`,
/// `launch_with_tool_with_state`, `launch_default_with_state`).
pub(crate) fn record_and_save(state: &AppState, path: &str, query: &str) {
    // Serialize the consistent snapshot while the Engine is protected, but run
    // the potentially slow filesystem write only after releasing that lock.
    let save = {
        let mut engine = state.engine.lock().unwrap();
        engine.record_launch(path, query);
        engine.prepare_history_save_if_dirty(5)
    };
    if let Some(save) = save {
        let _ = save.save();
    }
}

/// フロントエンドへ返すオープナーツール情報（serde シリアライズ用）
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OpenerToolDto {
    pub name: String,
    pub exe: String,
    pub args: String,
}

impl From<&OpenerTool> for OpenerToolDto {
    fn from(t: &OpenerTool) -> Self {
        Self {
            name: t.name.clone(),
            exe: t.exe.clone(),
            args: t.args.clone(),
        }
    }
}

/// パスとフォルダフラグに対してマッチするオープナーツール一覧を返す。
#[tauri::command]
pub fn get_matching_tools(
    path: String,
    is_folder: bool,
    app: AppHandle,
) -> Result<Vec<OpenerToolDto>, String> {
    let state = app.state::<AppState>();
    let engine = state.engine.lock().unwrap();
    let tools = find_matching_tools(&path, is_folder, &engine.config().openers);
    Ok(tools.iter().map(OpenerToolDto::from).collect())
}

/// 指定ツール（exe + args）でパスを起動し、成功時に履歴を記録する。
#[tauri::command]
pub async fn launch_with_tool(
    path: String,
    query: String,
    tool_exe: String,
    tool_args: String,
    app: AppHandle,
) -> Result<LaunchResult, String> {
    trace_command(
        "cmd:launch_with_tool:start",
        json!({
            "path": path,
            "tool_exe": tool_exe,
            "query_len": query.chars().count(),
            "timeout_ms": LAUNCH_TIMEOUT_MS,
        }),
    );
    let launch_path = path.clone();
    let exe = tool_exe.clone();
    let args = tool_args.clone();
    let result = run_launch_blocking(move || launch_with_tool_core(&launch_path, &exe, &args)).await;

    if result.is_ok() {
        let state = app.state::<AppState>();
        record_and_save(&state, &path, &query);
    }

    trace_command(
        "cmd:launch_with_tool:done",
        json!({
            "path": path,
            "tool_exe": tool_exe,
            "status": result.status,
            "code": result.code,
            "message": result.message,
        }),
    );
    Ok(result)
}

fn launch_with_tool_core(path: &str, exe: &str, args: &str) -> LaunchResult {
    let mut cmd = std::process::Command::new(exe);
    for arg in build_launch_args(args, path) {
        cmd.arg(arg);
    }
    match cmd.spawn() {
        Ok(_) => LaunchResult::ok(0),
        Err(e) => LaunchResult::failed(-1, format!("spawn_failed: {e}")),
    }
}

fn build_launch_args(args: &str, path: &str) -> Vec<String> {
    let mut expanded = Vec::new();
    let mut has_placeholder = false;

    for token in split_args(args) {
        if token.contains(PATH_PLACEHOLDER) {
            has_placeholder = true;
            expanded.push(token.replace(PATH_PLACEHOLDER, path));
        } else {
            expanded.push(token);
        }
    }

    if !has_placeholder {
        expanded.push(path.to_string());
    }

    expanded
}

#[tauri::command]
pub async fn launch_item(
    path: String,
    query: String,
    app: AppHandle,
) -> Result<LaunchResult, String> {
    // まずオープナールールを検索（is_dir はロック外・ロック内は純 CPU。resolve_opener の doc / #524 参照）
    let opener_tool = {
        let state = app.state::<AppState>();
        resolve_opener(&path, &state)
    };

    if let Some((exe, args)) = opener_tool {
        // オープナーが設定されていれば launch_with_tool で起動
        return launch_with_tool(path, query, exe, args, app).await;
    }

    trace_command(
        "cmd:launch_item:start",
        json!({
            "path": path,
            "query_len": query.chars().count(),
            "timeout_ms": LAUNCH_TIMEOUT_MS,
        }),
    );
    // launch_item_core does ShellExecuteW — must NOT hold the engine lock
    let launch_path = path.clone();
    let result = run_launch_blocking(move || launch_item_core(&launch_path)).await;

    if result.is_ok() {
        let state = app.state::<AppState>();
        record_and_save(&state, &path, &query);
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

/// パスに対して先頭のオープナーツール (exe, args) を返す。
/// 0/1 ツール判定の共通ロジック。
///
/// `is_dir()`（FS I/O）は**必ず engine ロックの外**で行う — 死んだ UNC パスでは
/// SMB タイムアウトまで最大 21 秒ブロックする実測があり、ロック内で呼ぶと
/// その間 `engine.lock()` を試みる全機能が待たされる（#524）。is_dir は engine
/// 状態に依存しないためロック前で評価でき、ロック内は純 CPU（`find_matching_tools`
/// + 小文字列 clone）のみに保つ。呼び出しスレッド自身の is_dir 待ちは仕様上残る。
fn resolve_opener(path: &str, state: &AppState) -> Option<(String, String)> {
    let is_folder = std::path::Path::new(path).is_dir();
    let engine = state.engine.lock().unwrap();
    let tools = find_matching_tools(path, is_folder, &engine.config().openers);
    tools.first().map(|t| (t.exe.clone(), t.args.clone()))
}

pub fn launch_item_with_state(path: &str, query: &str, state: &AppState) -> LaunchResult {
    let opener_tool = resolve_opener(path, state);

    // launch_item_core does ShellExecuteW — must NOT hold the engine lock
    let result = if let Some((exe, args)) = opener_tool {
        launch_with_tool_core(path, &exe, &args)
    } else {
        launch_item_core(path)
    };

    if result.is_ok() {
        record_and_save(state, path, query);
    }
    result
}

/// トレイ履歴のツール選択後の起動（同期版）。
pub fn launch_with_tool_with_state(path: &str, exe: &str, args: &str, state: &AppState) -> LaunchResult {
    // launch_with_tool_core does NOT use ShellExecuteW; COM STA は不要
    let result = launch_with_tool_core(path, exe, args);
    if result.is_ok() {
        record_and_save(state, path, "");
    }
    result
}

/// トレイ履歴の「標準」起動（ShellExecuteW 直接、オープナールールを無視、同期版）。
pub fn launch_default_with_state(path: &str, state: &AppState) -> LaunchResult {
    let result = launch_item_core(path);
    if result.is_ok() {
        record_and_save(state, path, "");
    }
    result
}

/// トレイサブメニュー構築用: パスに対するツール一覧を (name, exe, args) で返す。
/// `is_dir()` は engine ロックの外で行う（`resolve_opener` と対称。理由はそちらの doc 参照、#524）。
pub fn resolve_all_openers(path: &str, state: &AppState) -> Vec<(String, String, String)> {
    let is_folder = std::path::Path::new(path).is_dir();
    let engine = state.engine.lock().unwrap();
    let tools = find_matching_tools(path, is_folder, &engine.config().openers);
    tools.iter().map(|t| (t.name.clone(), t.exe.clone(), t.args.clone())).collect()
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

pub(crate) fn launch_item_core(path: &str) -> LaunchResult {
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
            LaunchResult::failed(code, shell_execute_error_message(code))
        }
    }

    #[cfg(not(windows))]
    {
        let _ = path;
        LaunchResult::failed(-1, "unsupported_platform")
    }
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// フロントへ返すインスタントコマンド情報（種別の内部構造を隠す）
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InstantCommandDto {
    pub name: String,
    pub description: String,
    pub display: String,
}

impl From<&InstantCommand> for InstantCommandDto {
    fn from(c: &InstantCommand) -> Self {
        let display = match &c.action {
            InstantAction::Url { url } => url.clone(),
            InstantAction::Exec { exe, args } => {
                if args.is_empty() { exe.clone() } else { format!("{exe} {args}") }
            }
            InstantAction::Legacy { command } => command.clone(),
        };
        Self { name: c.name.clone(), description: c.description.clone(), display }
    }
}

/// 環境変数 `%VAR%` を展開する（Win32 ExpandEnvironmentStringsW）。非 Windows は素通し。
pub(crate) fn expand_env(input: &str) -> String {
    #[cfg(windows)]
    {
        use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
        use windows::core::HSTRING;
        let src = HSTRING::from(input);
        unsafe {
            let needed = ExpandEnvironmentStringsW(&src, None);
            if needed == 0 { return input.to_string(); }
            let mut buf = vec![0u16; needed as usize];
            let written = ExpandEnvironmentStringsW(&src, Some(&mut buf));
            if written == 0 { return input.to_string(); }
            // 末尾 NUL を除いて UTF-16 → String
            let len = (written as usize).saturating_sub(1).min(buf.len());
            String::from_utf16_lossy(&buf[..len])
        }
    }
    #[cfg(not(windows))]
    {
        input.to_string()
    }
}

/// exec 種別の起動。COM 不要（CreateProcessW 直叩き）。コンソール窓抑止。
pub(crate) fn launch_exec_core(exe: &str, args: &str, query: &str, clipboard: &str) -> LaunchResult {
    let exe_expanded = expand_env(exe);
    let arg_tokens = expand_exec_args(args, query, clipboard, expand_env);

    let mut cmd = std::process::Command::new(&exe_expanded);
    cmd.args(&arg_tokens);
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    match cmd.spawn() {
        Ok(_) => LaunchResult::ok(0),
        Err(e) => LaunchResult::failed(-1, format!("spawn_failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::build_launch_args;
    use super::InstantCommandDto;
    use snotra_core::config::{InstantAction, InstantCommand};

    #[test]
    fn instant_dto_display_url() {
        let c = InstantCommand { name: "g".into(), description: "d".into(),
            action: InstantAction::Url { url: "https://x".into() } };
        assert_eq!(InstantCommandDto::from(&c).display, "https://x");
    }
    #[test]
    fn instant_dto_display_exec_with_args() {
        let c = InstantCommand { name: "ev".into(), description: String::new(),
            action: InstantAction::Exec { exe: "everything.exe".into(), args: "-s {query}".into() } };
        assert_eq!(InstantCommandDto::from(&c).display, "everything.exe -s {query}");
    }
    #[test]
    fn instant_dto_display_exec_no_args_has_no_trailing_space() {
        let c = InstantCommand { name: "n".into(), description: String::new(),
            action: InstantAction::Exec { exe: "notepad.exe".into(), args: String::new() } };
        assert_eq!(InstantCommandDto::from(&c).display, "notepad.exe");
    }

    #[test]
    fn build_launch_args_appends_path_when_args_empty() {
        assert_eq!(build_launch_args("", "C:\\file.txt"), vec!["C:\\file.txt"]);
    }

    #[test]
    fn build_launch_args_appends_path_when_no_placeholder() {
        assert_eq!(
            build_launch_args("--new-window", "C:\\file.txt"),
            vec!["--new-window", "C:\\file.txt"]
        );
    }

    #[test]
    fn build_launch_args_replaces_placeholder_and_skips_append() {
        assert_eq!(
            build_launch_args("-d {path}", "C:\\file.txt"),
            vec!["-d", "C:\\file.txt"]
        );
    }

    #[test]
    fn build_launch_args_replaces_inline_placeholder() {
        assert_eq!(
            build_launch_args("--open={path}", "C:\\file.txt"),
            vec!["--open=C:\\file.txt"]
        );
    }

    #[test]
    fn build_launch_args_keeps_space_in_replaced_path_as_single_argument() {
        assert_eq!(
            build_launch_args("-d {path}", "C:\\My Folder\\file.txt"),
            vec!["-d", "C:\\My Folder\\file.txt"]
        );
    }

    #[test]
    fn build_launch_args_replaces_multiple_placeholders() {
        assert_eq!(
            build_launch_args("{path} --compare {path}", "C:\\file.txt"),
            vec!["C:\\file.txt", "--compare", "C:\\file.txt"]
        );
    }

    #[test]
    fn build_launch_args_quoted_fixed_args_with_path() {
        assert_eq!(
            build_launch_args(r#"--workspace "C:\My Projects""#, "C:\\file.txt"),
            vec!["--workspace", "C:\\My Projects", "C:\\file.txt"]
        );
    }
}
