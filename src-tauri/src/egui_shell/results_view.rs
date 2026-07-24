//! 結果リスト窓（"results"）の egui view（#646 PR2）。main（SearchWindowView）が発行する
//! RowsSnapshot を描くだけの従属 view——検索状態の所有者は main のまま（一方向データフロー・
//! spec 決定 5）。クリックは ResultsShared.clicked へ積んで main を wake する（遅延 dispatch）。
//! 窓の可視性・サイズ・位置の driver は main 側（hidden 窓は update() が走らないため）。
//!
//! **禁止: この view で `frame.hide_window()` を呼ばない** — results は focusable(false) で
//! `Focused(true)` が永遠に来ないため、runtime の visible フラグが false に固着し永久非描画
//! になる（復帰経路は Focused(true) のみ・runtime.rs）。hide は必ず外部（`hide_egui_main` /
//! main の drive）の `window.hide()` で行う。

use tauri::Manager;

use crate::egui_shell::EguiShellState;

/// main が毎フレーム発行する描画用スナップショット（spec 決定 5）。
#[derive(Clone, Default, PartialEq)]
pub(crate) struct RowsSnapshot {
    pub rows: Vec<snotra_core::ui_types::SearchResult>, // ui_types が正（engine ではない）・PartialEq/Eq derive 済み
    pub selected: usize,
    pub show: bool,
}

/// main と results が共有する一方向フローの入れ物（managed state）。
#[derive(Default)]
pub(crate) struct ResultsShared {
    #[allow(dead_code)] // Task 4 で main が書き込み・results の update() が読む
    pub snapshot: std::sync::Mutex<RowsSnapshot>,
    /// クリックされた行 index（last-wins）。main の update() が take して起動処理する。
    #[allow(dead_code)] // Task 4/5 で results の click ハンドラが書き込み・main が take する
    pub clicked: std::sync::Mutex<Option<usize>>,
}

pub(crate) struct ResultsView {
    app_handle: tauri::AppHandle,
    /// font_family hot-reload 用（main の applied_font_family と同型・ctx が窓ごとに独立
    /// なため複製必須 — plan-review rev-egui の指摘）。
    #[allow(dead_code)] // Task 4 で hot-reload 判定に消費される
    applied_font_family: String,
}

impl ResultsView {
    pub(crate) fn new(app_handle: tauri::AppHandle) -> Self {
        Self {
            app_handle,
            applied_font_family: String::new(),
        }
    }
}

impl snotra_egui_runtime::EguiView for ResultsView {
    fn setup(&mut self, context: &egui::Context) {
        // 日本語フォント: main と同じ config font_family を適用（ctx は窓ごとに独立）。
        let font_family = self
            .app_handle
            .try_state::<crate::AppState>()
            .map(|s| s.engine.lock().unwrap().config().visual.font_family.clone())
            .unwrap_or_else(|| "Segoe UI".to_string());
        crate::egui_shell::view::configure_japanese_font(context, &font_family);
        self.applied_font_family = font_family;
        // 外部 wake 用に ctx を登録（main の egui_ctx と同型・EguiShellState.results_ctx）。
        if let Some(sh) = self.app_handle.try_state::<EguiShellState>()
            && let Ok(mut guard) = sh.results_ctx.lock()
        {
            *guard = Some(context.clone());
        }
    }

    fn update(&mut self, _ui: &mut egui::Ui, _frame: &mut snotra_egui_runtime::RuntimeFrame) {
        // Task 4 で snapshot 描画を実装。骨組みでは何も描かない（窓は visible:false のまま）。
    }
}
