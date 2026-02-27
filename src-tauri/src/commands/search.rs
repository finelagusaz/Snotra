use std::path::Path;

use serde_json::json;
use snotra_core::config::Config;
use snotra_core::folder;
use snotra_core::search::{HistoryBoostConfig, SearchMode};
use snotra_core::ui_types::SearchResult;
use tauri::State;

use crate::state::AppState;

use super::trace_command;

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
