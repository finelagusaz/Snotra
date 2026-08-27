//! 更新 toast のボタン処置と install の spawn（#532 SU5・§20.4）。
//!
//! **toast の状態は managed state（[`crate::egui_shell::UpdaterUiState`]）が持ち、
//! [`LauncherController`] は持たない**——ここに在るのはクリックを borrow の外で捌く遅延 dispatch
//! （[`ToastAction`]）と、その処置だけである。ゆえに `consume_reset_pending` の reset は toast を
//! 触らない（show を跨いで残るのは意図）。
//!
//! ここに**無いもの**: toast の描画とボタンの当たり判定は `view.rs`、`phase` の遷移規則は
//! [`crate::egui_shell::UpdaterUi`] が持つ。

use tauri::Manager;

use super::LauncherController;

/// toast ボタン種別（クリック結果を borrow 外で処理するための遅延 dispatch）。
pub(in crate::egui_shell) enum ToastAction {
    Install,
    Dismiss,
}

impl LauncherController {
    /// toast ボタンの処理（#532 SU5）。install は Update を原子取得して async へ（Task 8）。
    ///
    /// **状態を変えたら `ctx.request_repaint()` する**（Task 10 実機スモークで発見・
    /// `spawn_folder_load` の egui_ctx wake（本ファイル該当箇所のコメント参照）と同じ理由）:
    /// このランタイムはイベント駆動で、click を処理したこのフレームの描画は toast_action の
    /// 遅延 dispatch より前に完了している。ここで状態を変えても誰も次のフレームを起こさないため、
    /// 無関係な入力（マウス移動等）が来るまで旧 toast が画面に残る（dismiss 後の stale 表示）。
    pub(in crate::egui_shell) fn handle_toast_action(
        &mut self,
        action: ToastAction,
        ctx: &egui::Context,
    ) {
        let Some(st) = self
            .app_handle
            .try_state::<crate::egui_shell::UpdaterUiState>()
        else {
            return;
        };
        match action {
            ToastAction::Dismiss => {
                if st.0.lock().unwrap().dismiss() {
                    ctx.request_repaint(); // Installing 中の拒否（false）は表示不変ゆえ不要
                }
            }
            ToastAction::Install => {
                let taken = st.0.lock().unwrap().try_begin_install();
                if let Some(update) = taken {
                    ctx.request_repaint(); // Available→Installing の即時反映（disabled ボタン）
                    self.spawn_install(update);
                } else {
                    crate::trace_main("egui_update_install_noop", serde_json::json!({}));
                }
            }
        }
    }

    /// install 実行（§20.4・spec B 節）。`download_and_install` は Windows では内部で
    /// download → `on_before_exit`（=flush_persistent_state・Task 6 で builder に登録済み）→
    /// installer 起動 → `std::process::exit(0)` し**復帰しない**（updater.rs:865）。
    /// Err 復帰時のみ InstallFailed へ遷移して toast をエラー表示にする（updaterError parity）。
    fn spawn_install(&self, update: Box<tauri_plugin_updater::Update>) {
        let handle = self.app_handle.clone();
        crate::trace_main(
            "egui_update_install_begin",
            serde_json::json!({ "version": update.version }),
        );
        tauri::async_runtime::spawn(async move {
            match update.download_and_install(|_, _| {}, || {}).await {
                Ok(()) => {
                    // Windows では到達しない（内部 exit）。他 OS ビルドや将来変更の防波堤として trace。
                    crate::trace_main("egui_update_install_returned", serde_json::json!({}));
                }
                Err(e) => {
                    // trace と toast の両方が同じ文字列を要する（#654）。**lock を取る前に**
                    // 作る——確保を lock 保持区間へ入れない。
                    let reason = e.to_string();
                    crate::trace_main(
                        "egui_update_install_failed",
                        serde_json::json!({ "error": reason }),
                    );
                    if let Some(st) = handle.try_state::<crate::egui_shell::UpdaterUiState>() {
                        st.0.lock().unwrap().phase =
                            crate::egui_shell::UpdaterPhase::InstallFailed { message: reason };
                    }
                    crate::egui_shell::wake_main(&handle); // 可視中の失敗を即座に描く
                }
            }
        });
    }
}
