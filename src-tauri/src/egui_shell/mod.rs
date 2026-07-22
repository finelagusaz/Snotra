//! egui/softbuffer メインウィンドウの外殻（#532 SU2）。WebView2 と並行する
//! egui 専用 window 生成・show/hide・blur 自動非表示・位置永続。WebView2 経路は触らない。
mod lifecycle;
mod view;

// Task 4 の hotkey listener が消費するまで未使用。clippy -D warnings 回避のため許可し、Task 4 で除去する。
#[allow(unused_imports)]
pub(crate) use lifecycle::{HotkeyPlan, plan_hotkey};

use std::sync::atomic::{AtomicBool, AtomicU64};

use snotra_egui_runtime::EguiRuntime;

use crate::egui_shell::view::SearchWindowView;

/// egui 経路の show/hide を跨ぐ共有状態（managed state）。
/// - hotkey_generation: alt 解放待ち show の世代。hide が bump して保留 show を無効化する（codex #5/(B)#2）。
/// - hide_pending: view の emit dedup。show がクリアして「hide 後に Focused(true) が来ず抑止が残る」を断つ（codex #8）。
#[derive(Default)]
#[allow(dead_code)] // フィールドは Task 4/5（show/hide/blur 配線）で読む。Task 4 で除去する。
pub(crate) struct EguiShellState {
    pub(crate) hotkey_generation: AtomicU64,
    pub(crate) hide_pending: AtomicBool,
}

/// フラグ ON の窓生成。EguiRuntime を install し webview 無しの "main" 窓を生成して attach。setup 限定。
/// 宣言窓の全プロパティ（52px 高は固定・width は config の window_width・skipTaskbar・
/// alwaysOnTop・decorations:false・resizable:false・visible:false）を再現する（codex #11・(B)#1）。
pub(crate) fn create(
    app: &mut tauri::App,
    window_width: f64,
) -> Result<(), snotra_egui_runtime::RuntimeError> {
    let runtime = EguiRuntime::new();
    runtime.install(app); // install(&self, &mut App<Wry>)（runtime.rs:77）
    let app_handle = app.handle().clone();
    let window = tauri::Window::builder(app, "main")
        .title("Snotra")
        .inner_size(window_width, 52.0) // 保存幅を尊重（codex #11）
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true) // 宣言窓 skipTaskbar:true の再現（(B)#1）
        .always_on_top(true) // 宣言窓 alwaysOnTop:true の再現（(B)#1）
        .visible(false)
        .build()?; // tauri::Error → RuntimeError（#[from]・runtime.rs:46）
    runtime.attach(window, SearchWindowView::new(app_handle))
}
