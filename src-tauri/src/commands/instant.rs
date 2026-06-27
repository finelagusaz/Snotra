use std::time::Duration;

use snotra_core::instant::{expand_instant_command, filter_instant_commands};
use tauri::Manager;
use tokio::time::timeout;

use crate::state::AppState;

use super::launch::LaunchResult;

const LAUNCH_TIMEOUT_MS: u64 = 4_000;

#[tauri::command]
pub fn get_instant_commands(
    prefix_input: String,
    app: tauri::AppHandle,
) -> Result<Vec<super::launch::InstantCommandDto>, String> {
    let state = app.state::<AppState>();
    let engine = state.engine.lock().unwrap();
    let commands = &engine.config().instant_commands;
    Ok(filter_instant_commands(commands, &prefix_input)
        .into_iter()
        .map(super::launch::InstantCommandDto::from)
        .collect())
}

#[tauri::command]
pub async fn execute_instant_command(
    name: String,
    query: String,
    app: tauri::AppHandle,
) -> Result<LaunchResult, String> {
    // action をロック内で取得し、即座にロックを解放する。
    // クリップボード読み取り (Win32 API) がブロックする可能性があるため、
    // engine mutex の外で行う。
    let action = {
        let state = app.state::<AppState>();
        let engine = state.engine.lock().unwrap();
        engine
            .config()
            .instant_commands
            .iter()
            .find(|c| c.name == name)
            .ok_or_else(|| format!("instant command not found: {name}"))?
            .action
            .clone()
    };

    let clipboard = arboard::Clipboard::new()
        .and_then(|mut cb| cb.get_text())
        .unwrap_or_default();

    // 変数展開: URL テンプレートは自動エンコード、それ以外は生展開。
    // セキュリティモデル: コマンドテンプレートはユーザーが config.toml で
    // 自身で定義したものであり、信頼済みコンテンツとして扱う。
    let join = tauri::async_runtime::spawn_blocking(move || {
        use snotra_core::config::InstantAction;
        match action {
            InstantAction::Url { url } => {
                let expanded = expand_instant_command(&url, &query, &clipboard);
                super::launch::launch_item_core(&expanded)
            }
            InstantAction::Exec { exe, args } => {
                super::launch::launch_exec_core(&exe, &args, &query, &clipboard)
            }
            // load 後は移行済みで到達しないが、防御的に Url 扱い
            InstantAction::Legacy { command } => {
                let expanded = expand_instant_command(&command, &query, &clipboard);
                super::launch::launch_item_core(&expanded)
            }
        }
    });
    let result = match timeout(Duration::from_millis(LAUNCH_TIMEOUT_MS), join).await {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => LaunchResult::failed(-1, format!("launch_worker_join_error: {e}")),
        Err(_) => LaunchResult::timeout(LAUNCH_TIMEOUT_MS),
    };

    Ok(result)
}
