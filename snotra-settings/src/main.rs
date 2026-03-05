#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod about;
mod app;
mod font;
mod hotkey_input;
mod tabs;

use snotra_core::config::Config;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let about_mode = args.iter().any(|a| a == "--about");
    let first_run = args.iter().any(|a| a == "--first-run");

    let initial_tab = args
        .windows(2)
        .find(|w| w[0] == "--tab")
        .map(|w| w[1].clone());

    let config = Config::load();

    if about_mode {
        about::run()
    } else {
        app::run(config, first_run, initial_tab)
    }
}
