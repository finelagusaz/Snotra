use std::path::Path;

use serde_json::json;
use snotra_core::folder;
use snotra_core::ui_types::SearchResult;
use tauri::{AppHandle, Manager, State};

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

// 戻り値型は Result<_, String> だが、現在の実装ではすべてのエラーパスが
// is_error エントリを Ok() で包んで返すため、Err 変体は実際には返されない。
// フロントエンドとの IPC 型として Result を使い続けているため、将来の拡張に備えて維持する。
#[tauri::command]
pub async fn list_folder(
    dir: String,
    filter: String,
    app: AppHandle,
) -> Result<Vec<SearchResult>, String> {
    trace_command(
        "cmd:list_folder:start",
        json!({
            "dir": dir,
            "filter_len": filter.chars().count(),
        }),
    );
    let ctx = {
        let state = app.state::<AppState>();
        let engine = state.engine.lock().unwrap();
        engine.capture_folder_list_context()
    };

    let dir_for_io = dir.clone();
    let filter_for_io = filter.clone();
    let join = tauri::async_runtime::spawn_blocking(move || {
        ctx.read_dir_entries(Path::new(&dir_for_io), &filter_for_io)
    });

    let entries = match join.await {
        Ok(Ok(entries)) => entries,
        Ok(Err(_)) | Err(_) => {
            let results = folder::error_result(Path::new(&dir));
            trace_command(
                "cmd:list_folder:ok",
                json!({
                    "dir": dir,
                    "result_count": results.len(),
                    "error": true,
                }),
            );
            return Ok(results);
        }
    };

    let results = {
        let state = app.state::<AppState>();
        let engine = state.engine.lock().unwrap();
        engine.finalize_folder_list(entries, ctx)
    };

    trace_command(
        "cmd:list_folder:ok",
        json!({
            "dir": dir,
            "result_count": results.len(),
        }),
    );
    Ok(results)
}
