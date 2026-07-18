//! 設定 GUI（`snotra-settings.exe`）のエントリポイント。eframe を起動する。
//!
//! 本体（`src-tauri`）とは別プロセスで動作し、連携は `config.toml` ファイル 1 点のみ。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod font;
mod hotkey_input;
pub(crate) mod i18n;
mod style;
mod tabs;

use snotra_core::config::Config;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let first_run = args.iter().any(|a| a == "--first-run");

    let initial_tab = args
        .windows(2)
        .find(|w| w[0] == "--tab")
        .map(|w| w[1].clone());

    let (config, load_outcome) = Config::load_reporting();

    app::run(config, first_run, initial_tab, load_outcome)
}
