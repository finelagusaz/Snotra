//! egui メインウィンドウの placeholder view（#532 SU2）。show/hide/focus/位置を視覚検証できる
//! 最小 chrome を描く。検索本体は SU3。font-first（jp_font を index 0）は SU1 申し送りの義務。

use std::sync::OnceLock;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use snotra_core::ui_types::SearchResult;
use snotra_egui_runtime::{EguiView, RuntimeFrame};
use tauri::{Emitter, Manager};

use crate::egui_shell::{Debouncer, QueryIntent, SearchState};

static JP_FONT_BYTES: OnceLock<Box<[u8]>> = OnceLock::new();

fn japanese_font_definitions(bytes: &'static [u8]) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let mut font = egui::FontData::from_static(bytes);
    font.tweak = egui::FontTweak {
        scale: 1.0,
        y_offset_factor: 0.3,
        y_offset: 0.0,
        ..Default::default()
    };
    fonts.font_data.insert("jp_font".to_owned(), font.into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        // insert(0)＝先頭。jp_font を最優先にして単一フォント化する。push（末尾 fallback）だと
        // Latin=egui 既定 / CJK=Yu Gothic に分離し、被覆 AA 無の softbuffer でベースラインずれ（#579/#399）。
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "jp_font".to_owned());
    }
    fonts
}

fn configure_japanese_font(context: &egui::Context) {
    let candidates = [
        "C:/Windows/Fonts/YuGothM.ttc",
        "C:/Windows/Fonts/yugothic.ttf",
        "C:/Windows/Fonts/msgothic.ttc",
        "C:/Windows/Fonts/meiryo.ttc",
    ];
    if JP_FONT_BYTES.get().is_none() {
        for path in candidates {
            if let Ok(bytes) = std::fs::read(path) {
                let _ = JP_FONT_BYTES.set(bytes.into_boxed_slice());
                break;
            }
        }
    }
    if let Some(bytes) = JP_FONT_BYTES.get() {
        // OnceLock の中身は以後不変ゆえ 'static として安全に借用できる。
        let static_bytes: &'static [u8] =
            unsafe { std::mem::transmute::<&[u8], &'static [u8]>(bytes) };
        context.set_fonts(japanese_font_definitions(static_bytes));
    }
}

pub(crate) struct SearchWindowView {
    app_handle: tauri::AppHandle,
    was_focused: bool,
    unfocus_at: Option<Instant>,
    state: SearchState,
    search_debounce: Debouncer,
    last_input_at: Instant,
    // query フィールドは SearchState.query へ移譲（削除）。
    // emit dedup は共有 EguiShellState.hide_pending（show がクリア・codex #8）。view-local には持たない。
}

impl SearchWindowView {
    pub(crate) fn new(app_handle: tauri::AppHandle) -> Self {
        Self {
            app_handle,
            was_focused: false,
            unfocus_at: None,
            state: SearchState::new(),
            search_debounce: Debouncer::new(Duration::from_millis(50), true),
            last_input_at: Instant::now(),
        }
    }

    /// hide 要求を emit する。多重防止は共有 EguiShellState.hide_pending（show_egui_main が
    /// クリア・codex #8）。view-local フラグだと hide 後 Focused(true) 非着信で永久 true 化し、
    /// 以後の hide を抑止してしまう。
    fn emit_hide(&self) {
        let already = self
            .app_handle
            .try_state::<crate::egui_shell::EguiShellState>()
            .map(|sh| sh.hide_pending.swap(true, Ordering::SeqCst))
            .unwrap_or(false);
        if already {
            return;
        }
        let _ = self.app_handle.emit("egui-hide-requested", ());
    }

    /// index 行を起動し、成功なら履歴記録して hide 要求を出す（§4.8 シングルクリック / Enter）。
    /// launch_item_core は ShellExecuteW（エンジンロック外で呼ぶ・launch.rs:226）。成功時のみ
    /// record_and_save で履歴を記録（§4.3/§5 の query_count 加点・全起動経路の共通末尾を再利用）。
    /// エラー行（is_error）は起動しない。
    fn activate(&self, index: usize) {
        use crate::commands::launch::{LaunchStatus, launch_item_core, record_and_save};
        let Some(result) = self.state.results().get(index) else { return };
        if result.is_error {
            return;
        }
        let path = result.path.clone();
        let query = self.state.query().to_string();
        let outcome = launch_item_core(&path); // ロック外・ShellExecuteW
        crate::trace_main(
            "egui_launch",
            serde_json::json!({ "index": index, "status": format!("{:?}", outcome.status) }),
        );
        if matches!(outcome.status, LaunchStatus::Ok) {
            if let Some(state) = self.app_handle.try_state::<crate::AppState>() {
                record_and_save(&state, &path, &query); // 履歴記録 + 保存（ロックは内部で最小保持）
            }
            // 起動成功時のみ hide（SU2 の hide 合流点へ・view から window を直接触らない）。
            self.emit_hide();
        }
    }

    /// auto_hide_on_focus_lost を実行中 config から都度読む（キャッシュしない・#576 と同設計）。
    fn auto_hide_enabled(&self) -> bool {
        self.app_handle
            .try_state::<crate::AppState>()
            .map(|s| {
                s.engine
                    .lock()
                    .unwrap()
                    .config()
                    .general
                    .auto_hide_on_focus_lost
            })
            .unwrap_or(true) // config.rs 既定と一致
    }

    /// 設定サイドカー起動中は blur で hide しない（設定が focus を奪っても本体を消さない）。
    fn settings_running(&self) -> bool {
        self.app_handle
            .try_state::<crate::SettingsProcessState>()
            .map(|p| p.lock().unwrap().is_some())
            .unwrap_or(false)
    }

    /// instant prefix を実行中 config から都度読む（キャッシュしない・#576 と同設計）。
    /// フィールドは config.search.instant_command_prefix（config.rs:956 で確認済み）。
    fn instant_prefix(&self) -> String {
        self.app_handle
            .try_state::<crate::AppState>()
            .map(|s| s.engine.lock().unwrap().config().search.instant_command_prefix.clone())
            .unwrap_or_else(|| "@".to_string())
    }

    /// index 構築中か（AppState.indexing: AtomicBool・state.rs:14 で確認済み）。
    fn indexing(&self) -> bool {
        self.app_handle
            .try_state::<crate::AppState>()
            .map(|s| s.indexing.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(false)
    }

    /// 現在の state.query に対して検索を実行し結果を注入する（同期・直 Engine）。
    /// results + plain + !indexing のみ通常検索。空クエリは結果クリア（§4.6）。
    /// instant/command/folder は M3/M2 で分岐を足す（現状 plain のみ実装）。
    fn run_search(&mut self) {
        let prefix = self.instant_prefix();
        match self.state.interp(&prefix) {
            QueryIntent::Plain => {
                if self.state.query().trim().is_empty() || self.indexing() {
                    self.state.set_results(Vec::new());
                    return;
                }
                let query = self.state.query().to_string();
                let results = {
                    let state = match self.app_handle.try_state::<crate::AppState>() {
                        Some(s) => s,
                        None => return,
                    };
                    let mut engine = state.engine.lock().unwrap();
                    engine.search(&query)
                }; // lock 解放
                self.state.set_results(results);
            }
            // command/instant は M2/M3。M1 では結果を出さない（空維持）。
            _ => {
                self.state.set_results(Vec::new());
            }
        }
    }

    /// 1 行を描画。selected ならハイライト + scroll_to_me。返り値: single_clicked。
    /// ダブルクリックは扱わない（ユーザー決定: §4.8 の double-click=選択は as-built でも
    /// 到達不能ゆえ落とす。単クリック=起動のみ）。self を借りない関連関数（借用衝突回避）。
    fn draw_result_row(
        ui: &mut egui::Ui,
        index: usize,
        result: &SearchResult,
        selected: bool,
    ) -> bool {
        let row_h = 30.0;
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), row_h),
            egui::Sense::click(),
        );
        if selected {
            ui.painter().rect_filled(rect, 4.0, ui.visuals().selection.bg_fill);
            response.scroll_to_me(Some(egui::Align::Center));
        }
        // アイコンスロット（SU4 が埋める）: 左に 24px 空ける。
        let text_x = rect.left() + 28.0;
        let name_color = ui.visuals().text_color();
        let path_color = ui.visuals().weak_text_color(); // 淡色パス
        let painter = ui.painter();
        painter.text(
            egui::pos2(text_x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            &result.name,
            egui::FontId::proportional(14.0),
            name_color,
        );
        // 名前の右にパスを淡色で（簡易・galley 省略は egui 既定に委ねる）。
        painter.text(
            egui::pos2(rect.right() - 8.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            &result.path,
            egui::FontId::proportional(11.0),
            path_color,
        );
        let _ = index;
        response.clicked()
    }
}

impl EguiView for SearchWindowView {
    fn setup(&mut self, context: &egui::Context) {
        configure_japanese_font(context);
    }

    fn update(&mut self, ui: &mut egui::Ui, _frame: &mut RuntimeFrame) {
        let ctx = ui.ctx().clone();
        let focused = ctx.input(|i| i.focused);
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));

        let was_focused = self.was_focused;
        // 再表示直後の stale 猶予をリセット: focused に戻ったら pending 破棄（codex #8）。
        // emit dedup（hide_pending）は show_egui_main がクリアするので view では触らない。
        if focused {
            self.unfocus_at = None;
        }
        // Escape → 即 hide 要求（内側モード優先は SU3）。TextEdit より前に ctx から拾うので
        // 入力欄に focus があっても届く。
        if escape {
            self.emit_hide();
        }
        // focus 喪失 → 100ms 猶予を張り、猶予明けに repaint させる。
        if was_focused && !focused {
            self.unfocus_at = Some(Instant::now());
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        // 猶予明け判定は純粋核 blur_should_hide に委ねる（focus 復帰・auto_hide・設定起動を AND）。
        if let Some(at) = self.unfocus_at {
            let grace_elapsed = at.elapsed() >= Duration::from_millis(100);
            if crate::egui_shell::blur_should_hide(
                focused,
                grace_elapsed,
                self.auto_hide_enabled(),
                self.settings_running(),
            ) {
                self.unfocus_at = None;
                self.emit_hide();
            }
        }

        // ↑↓ ナビ（結果があるとき）。TextEdit より前に ctx から拾い、入力欄 focus 中も効かせる。
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            self.state.move_selection(1);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            self.state.move_selection(-1);
        }

        // Enter: 選択項目を起動（結果があるとき）。TextEdit の Enter より先に ctx で拾う。
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !self.state.results().is_empty() {
            self.activate(self.state.selected());
        }

        // 検索入力欄。state.query を編集し、変化があれば debounce leading で同期検索。
        let mut buf = self.state.query().to_string();
        let response = ui.add(
            egui::TextEdit::singleline(&mut buf)
                .hint_text("検索…")
                .desired_width(f32::INFINITY),
        );
        if response.changed() {
            self.state.set_query(buf);
            self.last_input_at = Instant::now();
            if self.search_debounce.on_input() {
                self.run_search(); // leading（Task 8 で trailing を足す）
            }
        }
        // 窓に focus があるのに入力欄が focus を持たないなら移す（Alt+Q 表示直後に打てる）。
        // was_focused に依存しないので、hide→reshow で was_focused が stale でも確実に戻る。
        if focused && !response.has_focus() {
            response.request_focus();
        }

        // 結果リスト（shouldShowResults 相当。M1: results 軸・plain のみ。空なら描かない）。
        let show_results = !self.state.results().is_empty();
        let mut clicked: Option<usize> = None;
        if show_results {
            // 借用衝突回避: results を clone してから描画（draw_result_row は関連関数で self 非借用）。
            let results = self.state.results().to_vec();
            let selected = self.state.selected();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, result) in results.iter().enumerate() {
                    if Self::draw_result_row(ui, i, result, i == selected) {
                        clicked = Some(i); // シングルクリック（§4.8 単=起動）。double は扱わない
                    }
                }
            });
        }
        // シングルクリック＝起動（§4.8 単=起動）。double-click は扱わない（ユーザー決定・
        // as-built でも double-click=選択は到達不能。SPEC §4.8 を as-built へ同期済み）。
        if let Some(i) = clicked {
            self.activate(i);
        }

        self.was_focused = focused;
    }
}

#[cfg(test)]
mod tests {
    use super::japanese_font_definitions;

    #[test]
    fn jp_font_is_registered_at_index_zero_for_both_families() {
        let dummy: &'static [u8] = &[0u8; 4];
        let fonts = japanese_font_definitions(dummy);
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let list = fonts.families.get(&family).expect("family present");
            assert_eq!(
                list.first().map(String::as_str),
                Some("jp_font"),
                "jp_font must be index 0 for {family:?}（push=末尾だと #579 再発）"
            );
        }
    }
}
