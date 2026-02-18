#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "windows")]
mod app;
#[cfg(target_os = "windows")]
mod binfmt;
#[cfg(target_os = "windows")]
mod config;
#[cfg(target_os = "windows")]
mod folder;
#[cfg(target_os = "windows")]
mod history;
#[cfg(target_os = "windows")]
mod hotkey;
#[cfg(target_os = "windows")]
mod icon;
#[cfg(target_os = "windows")]
mod ime;
#[cfg(target_os = "windows")]
mod indexer;
#[cfg(target_os = "windows")]
mod launcher;
#[cfg(target_os = "windows")]
mod platform_win32;
#[cfg(target_os = "windows")]
mod query;
#[cfg(target_os = "windows")]
mod search;
#[cfg(target_os = "windows")]
mod ui_types;
#[cfg(target_os = "windows")]
mod window_data;

#[cfg(target_os = "windows")]
fn main() {
    use std::rc::Rc;

    use eframe::egui;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

    use config::Config;

    let class_name = HSTRING::from(platform_win32::PLATFORM_WINDOW_CLASS);
    if unsafe { FindWindowW(PCWSTR(class_name.as_ptr()), None).is_ok() } {
        return;
    }

    let mut config = Config::load();
    config.appearance.max_history_display = config
        .appearance
        .max_history_display
        .min(config.appearance.max_results);

    let (entries, rescanned) = indexer::load_or_scan(
        &config.paths.additional,
        &config.paths.scan,
        config.search.show_hidden_system,
    );

    let icon_cache = if config.appearance.show_icons {
        if rescanned {
            let cache = icon::IconCache::build(&entries);
            cache.save();
            Some(Rc::new(cache))
        } else {
            Some(Rc::new(icon::IconCache::load().unwrap_or_else(|| {
                let cache = icon::IconCache::build(&entries);
                cache.save();
                cache
            })))
        }
    } else {
        None
    };

    let engine = search::SearchEngine::new(entries);
    let history = history::HistoryStore::load(
        config.appearance.top_n_history,
        config.appearance.max_history_display,
    );

    let Some(platform) = platform_win32::PlatformBridge::start(
        config.hotkey.clone(),
        config.general.show_tray_icon,
    ) else {
        return;
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Snotra")
        .with_decorations(config.general.show_title_bar)
        .with_inner_size([
            config.appearance.window_width as f32,
            app::search_window_height(config.appearance.max_results),
        ]);

    if let Some(placement) = window_data::load_search_placement() {
        viewport = viewport.with_position([placement.x as f32, placement.y as f32]);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let init = app::AppInit {
        config,
        engine,
        history,
        icon_cache,
        platform,
    };

    let _ = eframe::run_native(
        "Snotra",
        native_options,
        Box::new(move |cc| Ok(Box::new(app::SnotraApp::new(cc, init)))),
    );
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("Snotra is Windows-only.");
}
