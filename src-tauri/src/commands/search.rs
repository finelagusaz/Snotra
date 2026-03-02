use serde_json::json;
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
    let mut engine = state.engine.lock().unwrap();
    let results = engine.search(&query);
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
    let results = state.engine.lock().unwrap().recent_history();
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
    let results = state.engine.lock().unwrap().list_folder(&dir, &filter);
    trace_command(
        "cmd:list_folder:ok",
        json!({
            "dir": dir,
            "result_count": results.len(),
        }),
    );
    results
}
