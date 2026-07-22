//! egui/softbuffer メインウィンドウの外殻（#532 SU2）。WebView2 と並行する
//! egui 専用 window 生成・show/hide・blur 自動非表示・位置永続。WebView2 経路は触らない。
mod lifecycle;
mod view;

// Task 4 の hotkey listener が消費するまで未使用。clippy -D warnings 回避のため許可し、Task 4 で除去する。
#[allow(unused_imports)]
pub(crate) use lifecycle::{HotkeyPlan, plan_hotkey};
