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
    let results = {
        let mut engine = state.engine.lock().unwrap();
        engine.search(&query)
    }; // lock released before trace_command
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

// IPC 返り値契約（src-tauri/CLAUDE.md「IPC コマンドの返り値契約」）の「読み取り・検索系」:
// 失敗は is_error エントリを含む SearchResult で表現し、素の Vec<SearchResult> を返す。
// Result<_, String> にしない理由: 全エラーパスが is_error エントリで表現され、Err 変体が
// 決して返されない死蔵の型になるため（#434）。
// wire 互換性: Tauri IPC は Result<T, E>::Ok(v) と v を成功パスで同一シリアライズするため、
// Err を返さないこのコマンドではどちらの型でもフロントエンドから見た表現は同一。
#[tauri::command]
pub async fn list_folder(dir: String, filter: String, app: AppHandle) -> Vec<SearchResult> {
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
            return results;
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
    results
}
