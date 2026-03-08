use snotra_core::config::InstantCommand;
use snotra_core::instant::{expand_instant_command, filter_instant_commands};
use tauri::Manager;

use crate::state::AppState;

use super::launch::LaunchResult;

#[tauri::command]
pub async fn get_instant_commands(
    prefix_input: String,
    app: tauri::AppHandle,
) -> Result<Vec<InstantCommand>, String> {
    let state = app.state::<AppState>();
    let engine = state.engine.lock().unwrap();
    let commands = &engine.config().instant_commands;
    Ok(filter_instant_commands(commands, &prefix_input)
        .into_iter()
        .cloned()
        .collect())
}

#[tauri::command]
pub async fn execute_instant_command(
    name: String,
    query: String,
    app: tauri::AppHandle,
) -> Result<LaunchResult, String> {
    let expanded = {
        let state = app.state::<AppState>();
        let engine = state.engine.lock().unwrap();
        let config = engine.config();

        let cmd = config
            .instant_commands
            .iter()
            .find(|c| c.name == name)
            .ok_or_else(|| format!("instant command not found: {name}"))?;

        let clipboard = arboard::Clipboard::new()
            .and_then(|mut cb| cb.get_text())
            .unwrap_or_default();

        expand_instant_command(&cmd.command, &query, &clipboard)
    };

    // Run ShellExecuteW on a new thread with COM STA (reuse launch_item_core)
    let result = tokio::task::spawn_blocking(move || {
        super::launch::launch_item_core(&expanded)
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?;

    Ok(result)
}
