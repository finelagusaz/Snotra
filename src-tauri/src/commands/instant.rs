use snotra_core::instant::{expand_instant_command, filter_instant_commands};
use tauri::Manager;

use crate::state::AppState;

use super::launch::{run_launch_blocking, LaunchResult};

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

    let result =
        run_launch_blocking(move || execute_instant_action_core(action, &query, &clipboard)).await;

    Ok(result)
}

/// instant action の種別ディスパッチ共有核（IPC 経路 = 上の `execute_instant_command` /
/// egui 経路 = `egui_shell::view::execute_instant_selected` の両方が呼ぶ・#532 SU3 M3
/// /code-review）。二重実装だと展開/エンコードや Legacy fallback の将来修正が片側にだけ
/// 当たり、同じ config に対し WebView2 と egui が別のコマンドを起動する drift を生むため集約。
/// clipboard は呼び出し側がエンジンロック外で読んで渡す（Win32 がブロックしうるため）。
///
/// 変数展開: URL テンプレートは自動エンコード、それ以外は生展開。
/// セキュリティモデル: コマンドテンプレートはユーザーが config.toml で
/// 自身で定義したものであり、信頼済みコンテンツとして扱う。
pub(crate) fn execute_instant_action_core(
    action: snotra_core::config::InstantAction,
    query: &str,
    clipboard: &str,
) -> super::launch::LaunchResult {
    use snotra_core::config::InstantAction;
    match action {
        InstantAction::Url { url } => {
            let expanded = expand_instant_command(&url, query, clipboard);
            super::launch::launch_item_core(&expanded)
        }
        InstantAction::Exec { exe, args } => {
            super::launch::launch_exec_core(&exe, &args, query, clipboard)
        }
        // load 後は移行済みで到達しないが、防御的に Url 扱い
        InstantAction::Legacy { command } => {
            let expanded = expand_instant_command(&command, query, clipboard);
            super::launch::launch_item_core(&expanded)
        }
    }
}
