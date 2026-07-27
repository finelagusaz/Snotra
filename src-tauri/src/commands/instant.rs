use snotra_core::instant::{expand_instant_command, filter_instant_commands};
use tauri::Manager;

use crate::state::AppState;

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

/// instant action の種別ディスパッチ核（egui 経路 = `egui_shell::launcher_controller::LauncherController::execute_instant_selected`
/// が呼ぶ・#532 SU3 M3。IPC 経路は SU7 PR3 のフロント撤去で消滅）。
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
