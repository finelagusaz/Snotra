use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::Duration;

use notify::{Event, EventKind, RecursiveMode, Watcher};
use snotra_core::config::Config;
use tauri::{AppHandle, Emitter, LogicalSize, Manager};

use crate::indexing;
use crate::platform::{PlatformBridge, PlatformCommand};
use crate::state::AppState;

/// Start watching `config.toml` for external changes (e.g. from snotra-settings).
///
/// Returns the watcher handle which must be kept alive for the duration of the app.
/// Dropping the handle stops watching.
pub fn start(app_handle: &AppHandle) -> Option<notify::RecommendedWatcher> {
    let config_path = Config::config_path()?;
    let config_dir = config_path.parent()?;
    let config_filename = config_path.file_name()?.to_owned();

    let handle = app_handle.clone();
    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        let Ok(event) = res else { return };

        // Only react to write/create/rename events
        match event.kind {
            EventKind::Modify(_) | EventKind::Create(_) => {}
            _ => return,
        }

        // Only react if config.toml was the file affected
        let is_config = event
            .paths
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == config_filename));
        if !is_config {
            return;
        }

        // Debounce: small delay to let atomic rename complete
        std::thread::sleep(Duration::from_millis(100));

        apply_config_change(&handle);
    })
    .ok()?;

    // Watch the directory (not the file) because atomic write creates a new file
    watcher.watch(config_dir, RecursiveMode::NonRecursive).ok()?;

    Some(watcher)
}

/// Load config from disk and apply changes, mirroring save_config logic.
fn apply_config_change(app: &AppHandle) {
    let new_config = Config::load();

    let state = app.state::<AppState>();
    let old_config = state.engine.lock().unwrap().config().clone();

    // Detect changes
    let show_icons_changed = new_config.appearance.show_icons != old_config.appearance.show_icons;
    let new_show_icons = new_config.appearance.show_icons;
    let index_changed = new_config.paths.scan != old_config.paths.scan
        || new_config.search.show_hidden_system != old_config.search.show_hidden_system
        || show_icons_changed;
    let visual_changed = new_config.visual != old_config.visual;
    let max_results_changed =
        new_config.appearance.max_results != old_config.appearance.max_results;
    let new_max_results = new_config.appearance.max_results;
    let width_changed =
        new_config.appearance.window_width != old_config.appearance.window_width;
    let new_visual = if visual_changed {
        Some(new_config.visual.clone())
    } else {
        None
    };
    let new_width = new_config.appearance.window_width;

    // Hotkey change — best-effort, log on failure (don't block)
    if let Some(bridge) = app.try_state::<Mutex<PlatformBridge>>()
        && let Ok(b) = bridge.lock()
    {
        if new_config.hotkey != old_config.hotkey {
            let (tx, rx) = std::sync::mpsc::channel();
            b.send_command(PlatformCommand::SetHotkey {
                config: new_config.hotkey.clone(),
                reply: tx,
            });
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(false) | Err(_) => {
                    eprintln!(
                        "[config-watcher] hotkey registration failed: {} + {}",
                        new_config.hotkey.modifier, new_config.hotkey.key
                    );
                }
                Ok(true) => {}
            }
        }
        if new_config.general.show_tray_icon != old_config.general.show_tray_icon {
            b.send_command(PlatformCommand::SetTrayVisible(
                new_config.general.show_tray_icon,
            ));
        }
    }

    // Update engine config
    {
        state.engine.lock().unwrap().update_config(new_config);
    }

    // Trigger reindex if needed
    let indexing_in_progress = state.indexing.load(Ordering::SeqCst);
    if index_changed && !indexing_in_progress {
        state.index_build_started.store(false, Ordering::SeqCst);
        indexing::start_index_build(app);
    }

    // Emit visual config change
    if let Some(visual) = new_visual {
        let _ = app.emit("visual-config-changed", &visual);
    }

    // Emit show_icons change
    if show_icons_changed {
        let _ = app.emit("show-icons-changed", new_show_icons);
    }

    // Emit max_results change
    if max_results_changed {
        let _ = app.emit("max-results-changed", new_max_results);
    }

    // Resize windows if width changed
    if width_changed && new_width > 0 {
        for label in &["main", "results"] {
            if let Some(w) = app.get_webview_window(label)
                && let Ok(size) = w.inner_size()
                && let Ok(sf) = w.scale_factor()
            {
                let logical = size.to_logical::<f64>(sf);
                let _ = w.set_size(LogicalSize::new(f64::from(new_width), logical.height));
            }
        }
    }
}
