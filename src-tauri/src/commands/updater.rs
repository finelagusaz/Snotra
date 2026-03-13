use tauri::command;
use tauri::AppHandle;

/// アップデーターによるダウンロード完了後、アプリを再起動する。
/// フロントエンドが @tauri-apps/plugin-updater で downloadAndInstall() を
/// 完了させてから呼び出す。
#[command]
pub fn restart_app(app: AppHandle) {
    app.restart();
}
