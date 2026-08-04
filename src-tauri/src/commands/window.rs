use std::process::{Child, Command};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use serde_json::json;
use tauri::{AppHandle, Manager, State};

use crate::indexing;
use crate::state::AppState;

use super::trace_command;

/// インデックス構築中に設定を開こうとしたときのエラーコード
/// （`open_settings` / `rebuild_index` が共有。旧フロント側の対は #532 SU7 で消滅）。
pub(crate) const ERR_INDEXING_IN_PROGRESS: &str = "indexing_in_progress";

/// Managed state for tracking the snotra-settings child process.
pub type SettingsProcessState = Mutex<Option<Child>>;

/// Launch `snotra-settings` as a child process with optional extra arguments.
///
/// Deduplicates: if a settings process is already running, this is a no-op.
/// Temporarily disables main window alwaysOnTop while the child is alive
/// and restores it when the child exits.
///
/// **子は親の環境変数を継承する**（`Command` の既定）。`snotra-settings` が本体と同じ
/// `config.toml` を見るのはこれに依存しており、`SNOTRA_CONFIG_DIR`（`SPEC.md` §13）で
/// プロファイルを切り替えたときも同じ場所を見る。**`.env_clear()` / `.env_remove()` を
/// 足すと、この一致が沈黙して壊れる。**
///
/// # Errors
/// Returns `Err` if the executable is not found or spawning fails.
/// On first-run, failure leaves `indexing=true` permanently unless the caller
/// provides a fallback (e.g. `indexing::start_index_build`).
#[must_use = "failure during first-run leaves indexing=true; handle Err with a fallback"]
pub(crate) fn launch_settings_process(app: &AppHandle, extra_args: &[&str]) -> Result<(), String> {
    let proc_state = app
        .try_state::<SettingsProcessState>()
        .ok_or("SettingsProcessState not managed")?;

    let mut guard = proc_state.lock().unwrap();

    // Check if a settings process is already running.
    if let Some(child) = guard.as_mut() {
        match child.try_wait() {
            Ok(Some(_)) => {
                // Process has exited; clear stale handle and proceed to spawn.
                *guard = None;
            }
            Ok(None) => {
                // Still running — do nothing.
                trace_command(
                    "cmd:launch_settings_process:already_running",
                    json!({ "pid": child.id() }),
                );
                return Ok(());
            }
            Err(e) => {
                // Error checking status; clear handle and try to spawn.
                eprintln!("[settings-process] try_wait error: {e}");
                *guard = None;
            }
        }
    }

    // Find snotra-settings executable next to our own binary.
    let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_dir = exe_path.parent().ok_or("cannot determine exe directory")?;
    let settings_exe = exe_dir.join("snotra-settings.exe");

    if !settings_exe.exists() {
        let msg = format!(
            "snotra-settings.exe not found at {}",
            settings_exe.display()
        );
        trace_command(
            "cmd:launch_settings_process:not_found",
            json!({ "path": settings_exe.display().to_string() }),
        );
        return Err(msg);
    }

    let child = Command::new(&settings_exe)
        .args(extra_args)
        .spawn()
        .map_err(|e| format!("failed to spawn snotra-settings: {e}"))?;

    let pid = child.id();
    trace_command(
        "cmd:launch_settings_process:spawned",
        json!({ "pid": pid, "args": extra_args }),
    );

    *guard = Some(child);
    drop(guard);

    // Temporarily disable main window alwaysOnTop so snotra-settings can be focused.
    // egui 窓は webview 無しゆえ get_webview_window では取れない。get_window で取る
    // （codex #3・SPEC §8.5）。results 窓にも対称適用する（#646 PR2）——片方だけ解除すると
    // 設定画面の上に結果カードが浮く（/symmetric-check 対象・plan-review 独立導出の指摘）。
    if let Some(main) = app.get_window("main") {
        let _ = main.set_always_on_top(false);
    }
    if let Some(results) = app.try_state::<crate::egui_shell::ResultsWindow>() {
        // results は tauri の set_always_on_top を使えない（tao の差分適用が VISIBLE を
        // false と信じて SW_HIDE を撃つ・#646 PR2）。Z オーダーのみ動かす専用経路を通す。
        results.set_topmost(false);
    }

    // Spawn a monitoring thread to restore alwaysOnTop when the process exits.
    let handle_for_monitor = app.clone();
    std::thread::spawn(move || {
        // Poll child process status. Child is kept in SettingsProcessState so
        // the dedup check in launch_settings_process works.
        loop {
            std::thread::sleep(std::time::Duration::from_millis(250));
            let Some(proc_state) = handle_for_monitor.try_state::<SettingsProcessState>() else {
                eprintln!(
                    "[settings-monitor] SettingsProcessState not managed; exiting monitor thread"
                );
                break;
            };
            let mut guard = proc_state.lock().unwrap();
            if let Some(child) = guard.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        trace_command(
                            "cmd:launch_settings_process:exited",
                            json!({ "pid": pid, "status": status.code() }),
                        );
                        *guard = None;
                        break;
                    }
                    Ok(None) => {} // Still running.
                    Err(e) => {
                        eprintln!("[settings-process] monitor try_wait error: {e}");
                        *guard = None;
                        break;
                    }
                }
            } else {
                // Child handle was already cleared (e.g. by exit handler).
                break;
            }
        }

        // Restore main window alwaysOnTop（egui は get_window・codex #3・SPEC §8.5）。
        // results 窓にも対称適用する（#646 PR2・上の解除と対）。
        //
        // **既知の hazard（機序・重篤度・是正は #923 が正本）**: この復元はイベントループの
        // 外——監視スレッド——から撃つため、hidden な main へ tao の差分適用が `SW_HIDE` を
        // 漏らす（`src-tauri/CLAUDE.md`「Win32 / Tauri 注意事項」の窓ごとの層を参照）。
        // **#746 で auto_hide 有効時に「設定終了時 main は hidden」が常態化したため、この
        // 経路を毎回通るようになった**（従来も Escape 経由で到達可能ではあった）。
        if let Some(main) = handle_for_monitor.get_window("main") {
            let _ = main.set_always_on_top(true);
        }
        if let Some(results) = handle_for_monitor.try_state::<crate::egui_shell::ResultsWindow>() {
            results.set_topmost(true);
        }

        // First-run: if indexing is pending and not started, kick off index build.
        if let Some(state) = handle_for_monitor.try_state::<AppState>()
            && state.indexing.load(Ordering::SeqCst)
            && !state.index_build_started.load(Ordering::SeqCst)
        {
            indexing::start_index_build(&handle_for_monitor);
        }
    });

    Ok(())
}

pub fn open_settings(state: State<AppState>, app: AppHandle) -> Result<(), String> {
    trace_command("cmd:open_settings:start", json!({}));
    if state.indexing.load(Ordering::SeqCst) {
        trace_command("cmd:open_settings:noop_indexing", json!({}));
        return Err(ERR_INDEXING_IN_PROGRESS.to_string());
    }

    launch_settings_process(&app, &[])
}
