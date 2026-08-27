//! フレーム毎に呼ばれる消費と回収の段（reset-on-show・外部 pending・非同期の到着物）。
//!
//! **並びを決めるのは `view.rs` である**（親モジュールの `//!`「フレームを所有しない」）。
//! ここに在る 3 メソッドは呼ばれる側だが、**互いの順序に不変条件を持つ**——`poll_async` は
//! `consume_reset_pending` の**後**でなければならない（前に置くと show 直後フレームで stale な
//! 起動結果が reset より先に処理され、再 show した窓を hide で撃つ・spec C 節 不変条件 2）。
//!
//! ここに**無いもの**:
//!
//! - **回収そのもの**は各責務の子が持つ（`drain_launch` は `activation.rs`、`drain_folder` は
//!   `folder_nav.rs`）。`poll_async` はそれらを 1 フレームへ束ねる並びだけを持つ
//! - **`reset()` が何をクリアするか**は [`crate::egui_shell::search_state`]。ここはそこへ
//!   届かないフィールド（cache・in-flight 起動・通知・blur 猶予）を並べて消す側である

use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::Manager;

use super::LauncherController;
use crate::egui_shell::Debouncer;

impl LauncherController {
    /// 段 3: show 直後の resetForShow を消費し、**今フレームが reset フレームなら `true`**。
    /// **返り値の破棄は `#[must_use]` が禁じる**（#934。それまでこの散文が唯一の歯止めだった）——
    /// 同一フレームの `ResultsWindow::reset_size_guard()` は
    /// view 側に残る（#749 の位置不変条件・理由は `view.rs` の呼び出し点のコメント）ため、
    /// view が reset フレームを知る手段はこの返り値だけである。
    #[must_use = "消費（swap(false)）した後に reset フレームを知る手段はこの返り値だけである（#749 の位置不変条件・#934）"]
    pub(in crate::egui_shell) fn consume_reset_pending(&mut self) -> bool {
        // show 直後の resetForShow（EguiShellState.reset_pending を消費）。stale な debounce
        // armed 状態が再表示後に誤発火しないよう、debounce も併せて作り直す。
        if let Some(sh) = self
            .app_handle
            .try_state::<crate::egui_shell::EguiShellState>()
            && sh.reset_pending.swap(false, Ordering::SeqCst)
        {
            self.state.reset(); // hide を跨いだ in-flight もここで失効する（#1039）
            self.folder_cache = None;
            self.folder_error = None;
            self.instant_rows_query = None; // §19.7: resetForShow で instant モード解除
            self.search_debounce = Debouncer::new(Duration::from_millis(50), true);
            // scroll gate（#632: 再表示後に確実に一度 scroll し直す）は results 窓の
            // ResultsView::update() 側（実ゲート）に移設済み——main はもう読み書きしない。
            // icon パイプライン（icon_textures/icon_missing/icon_pending）も Task 5 で
            // results 窓へ移設済み——main はもう保持しない。hide 中の常駐テクスチャは
            // results 側の retain_visible が空 rows で自然に全クリアする（Task 5 申し送り）。
            // SU5: in-flight 起動と一時通知は show を跨がない（resetForShow の
            // setLaunching(false) + clearLaunchNotice parity）。rx ごと drop するため
            // hide 中に完了した遅着結果もここで自然消滅する（stale Ok が再 show 窓を
            // hide で撃つ事故の backstop・並行性レビュー High）。updater toast は触らない。
            self.launching = None;
            self.notice.clear();
            // #745: blur 猶予も hide を跨がない。**これを消すと、猶予 armed のまま別経路で
            // hide された後の再 show で、初フレームが `focused == false` なら自動 hide される**
            //（**この呼び出しの消失は `dead_code` が捕まえる**——射程と脆さ、および残る欠落は
            // `BlurGrace::reset` の doc が正本）。
            self.blur_grace.reset();
            true
        } else {
            false
        }
    }

    /// 段 5–6: 外部から届いた pending の消費（index build 完了世代・hotkey 登録失敗）。
    /// **2 段を 1 メソッドに束ねているのは連続する塊だからであって、両者に関係があるからでは
    /// ない**——順序の理由はそれぞれの本文のコメントが持つ。
    pub(in crate::egui_shell) fn consume_external_pending(&mut self, ctx: &egui::Context) {
        // #633: index build 完了の世代検知 → 現クエリで再検索（runRefresh parity・SU6 spec 決定 3）。
        // reset_pending 消費の後に置く（show 直後は reset 済み空クエリの no-op になるだけ）。
        // folder 中は fs 由来 cache の再フィルタ、tool 中は no-op——run_search が view_kind で分岐済み。
        // 順序不変条件: このブロックが後段の indexing() 読み（run_search 内・show_results ゲート）
        // より前にあることは、完了フレームをフリッカーなしで新結果にするために効いている
        // （世代 SeqCst acquire が後続 Relaxed 読みへ happens-before を運ぶ）。後ろへ動かしても
        // 正しさは壊れないが 1 フレームのフリッカーが出る。
        if let Some(s) = self.app_handle.try_state::<crate::AppState>() {
            let generation = s.index_generation.load(Ordering::SeqCst);
            if crate::egui_shell::needs_index_refresh(self.last_seen_index_generation, generation) {
                self.last_seen_index_generation = generation;
                self.run_search();
            }
        }

        // hotkey 登録失敗の pending 消費（SU6 spec 追補 2 + #652）。reset_pending 消費より後
        //（順序不変条件——reset の notice.clear() がこの set を消さないため）。整形はここで
        // lang() live-read: config-applied wake のフレームは update_config 後なので言語同時
        // 変更でも新言語で整形される。hidden 中の失敗は次 show のこの消費で表示される
        //（WebView2 は hidden 中に期限切れ・改善方向の受容差異・SU6 spec 追補 2）。
        if let Some(sh) = self
            .app_handle
            .try_state::<crate::egui_shell::EguiShellState>()
            && let Some((kind, hk)) = sh.pending_hotkey_failure.lock().unwrap().take()
        {
            let msg = match kind {
                crate::egui_shell::HotkeyFailureKind::Initial => {
                    crate::egui_shell::ui_strings::hotkey_initial_failed(self.lang(), &hk)
                }
                crate::egui_shell::HotkeyFailureKind::Change => {
                    crate::egui_shell::ui_strings::hotkey_change_failed(self.lang(), &hk)
                }
            };
            self.notice.set(
                msg,
                self.notice_base.elapsed(),
                crate::egui_shell::NOTICE_HOTKEY,
            );
            ctx.request_repaint();
        }
    }

    /// 段 10–12: 非同期の到着物を回収する（起動結果 → 通知期限 → folder 列挙）。
    /// **この 3 者が同じフレームで呼ばれることが `drain_launch` の通知の期限を成立させている**
    /// （`drain_launch` の `notice.set` 3 分岐は自前の repaint を持たない・`//!` 参照）。
    pub(in crate::egui_shell) fn poll_async(&mut self, ctx: &egui::Context) {
        // 起動結果の回収（#631）。reset_pending 消費の後に置くこと（spec C 節 不変条件 2）。
        self.drain_launch(ctx);
        // 一時通知の期限管理（期限切れで repaint・表示中は残余で wake 予約）。
        if self.notice.poll(self.notice_base.elapsed()) {
            ctx.request_repaint();
        }
        if let Some(remaining) = self.notice.remaining(self.notice_base.elapsed()) {
            ctx.request_repaint_after(remaining);
        }
        // ナビ結果の drain（`folder_nav.rs`）。前後関係は `drain_folder` の doc が持つ。
        self.drain_folder(ctx);
    }
}
