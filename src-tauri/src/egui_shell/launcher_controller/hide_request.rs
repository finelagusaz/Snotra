//! hide 要求を出す経路（Escape ラダー・blur 猶予）と、その合流点 `emit_hide`。
//!
//! **hide 要求はイベントを emit するだけで、窓を隠すのはこのモジュールではない**——実体は
//! `main.rs` の listener → [`crate::egui_shell::hide_egui_main`] であり、可視性を変える 5 関数は
//! イベントループスレッドへ閉じてある（`src-tauri/CLAUDE.md`「ウィンドウ生成の制約」の
//! 「可視性を変える操作は…」）。
//!
//! ここに**無いもの**:
//!
//! - **多重 hide の防止フラグ**は共有 `EguiShellState.hide_pending` にある（view-local に
//!   持ってはならない理由は `emit_hide` の doc）
//! - **Escape ラダーと blur 猶予の判定**は [`crate::egui_shell::search_state`] と
//!   [`crate::egui_shell::lifecycle`] の純粋核。ここに在るのは返った処置の実行だけである

use std::sync::atomic::Ordering;
use std::time::Instant;

use snotra_core::config::GeneralConfig;
use tauri::{Emitter, Manager};

use super::LauncherController;
use crate::egui_shell::EscapeOutcome;

impl LauncherController {
    /// hide 要求を emit する。多重防止は共有 EguiShellState.hide_pending（show_egui_main が
    /// クリア・codex #8）。view-local フラグだと hide 後 Focused(true) 非着信で永久 true 化し、
    /// 以後の hide を抑止してしまう。
    pub(super) fn emit_hide(&self) {
        let already = self
            .app_handle
            .try_state::<crate::egui_shell::EguiShellState>()
            .map(|sh| sh.hide_pending.swap(true, Ordering::SeqCst))
            .unwrap_or(false);
        if already {
            return;
        }
        let _ = self.app_handle.emit(crate::events::EGUI_HIDE_REQUESTED, ());
    }

    /// auto_hide_on_focus_lost を実行中 config から都度読む（キャッシュしない・#576 と同設計）。
    ///
    /// **毎フレーム無条件に読むのは意図的である**（`ADR-blur-grace-single-field-state-machine` が
    /// 遅延評価を却下して値渡しを採った）。**ただしその却下理由が置いた費用の前提は #1036 で
    /// 変わった**——当時は `read_visual` と `lang()` も engine lock を取っていたので「1 回増える
    /// だけ」と評価できたが、両者が [`crate::egui_shell::read_config`] へ移ったあとは**ここが
    /// 毎フレーム engine lock を取る唯一の箇所**になっていた。#1076 で読み口だけを移し、
    /// 毎フレーム無条件という決定はそのまま保っている。
    fn auto_hide_enabled(&self) -> bool {
        crate::egui_shell::read_config(
            &self.app_handle,
            |c| c.general.auto_hide_on_focus_lost,
            || GeneralConfig::default().auto_hide_on_focus_lost,
        )
    }

    /// 段 15: Escape の処置（呼ぶのは `key_pressed(Escape)` が真のフレームだけ）。
    /// folder から展開前 query を復元した場合だけ `true` を返す。view はこの信号で、同じ
    /// TextEdit id に残るキャレットを復元 query の末尾へ同期する（#840）。
    #[must_use = "キャレット同期の信号を落とすと、復元した query の末尾へキャレットが寄らない（#840）"]
    pub(in crate::egui_shell) fn on_escape_pressed(&mut self, ctx: &egui::Context) -> bool {
        // Escape ラダー（folder 中は展開前状態へ復帰、top-level は hide 要求・#532 SU3 M2）。
        // TextEdit より前に ctx から拾うので入力欄に focus があっても届く。
        match self.state.on_escape() {
            EscapeOutcome::RestoredSearch => {
                // folder 離脱 → cache/error 破棄、復帰済み results（展開前の plain 行）を描く
                self.folder_cache = None;
                self.folder_error = None;
                self.instant_rows_query = None;
                ctx.request_repaint();
                true
            }
            EscapeOutcome::RestoredFromTool => {
                // tool 解除 → 直下ビュー（folder/results）を復元描画。folder が下に生きて
                // いるため cache/error は破棄しない（RestoredSearch との差・純粋核 doc 参照）
                ctx.request_repaint();
                false
            }
            EscapeOutcome::Hide => {
                self.emit_hide();
                false
            }
        }
    }

    /// 段 16–17: 今フレームの focus を `BlurGrace` へ畳み、返った処置を実行する。
    ///
    /// **旧・段 14（focus 復帰で猶予を捨てる）と旧・段 34（前フレームの focus を畳む）は
    /// この 1 段へ合流した**（#745）。前フレームとの比較は `BlurGrace` が状態として持つ。
    ///
    /// **`now` はここで 1 回だけ読む**——多重読みが underflow を招く機序は `BlurGrace` の doc。
    pub(in crate::egui_shell) fn on_focus_changed(&mut self, focused: bool, ctx: &egui::Context) {
        // **`let` へ束縛してから渡す**——`self.blur_grace.observe(.., self.auto_hide_enabled())`
        // は two-phase borrow に依存する形になり、意図が読み取りにくい。
        let auto_hide = self.auto_hide_enabled();
        match self.blur_grace.observe(focused, Instant::now(), auto_hide) {
            crate::egui_shell::BlurAction::Hide => self.emit_hide(),
            // 契約③: 予約はフレームの到来を約束しない（worker は最も早い deadline だけを
            // 単一スロットで持ち、dispatch で take() するため、より早い要求が 1 つ割り込むと
            // 猶予の deadline は黙って消える）。armed の間は毎フレーム残余を要求し直す
            // ——検索 debounce・通知期限・起動タイムアウトと同じ流儀（#711）。
            crate::egui_shell::BlurAction::Rearm(remaining) => ctx.request_repaint_after(remaining),
            // 時間経過では解消しない不成立。再要求すると永久スピンになる（純粋核の doc）。
            crate::egui_shell::BlurAction::Idle => {}
        }
    }
}
