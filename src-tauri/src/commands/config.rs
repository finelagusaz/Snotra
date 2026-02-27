use std::sync::atomic::Ordering;

use snotra_core::config::Config;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, State};

use crate::indexing;
use crate::platform::{PlatformBridge, PlatformCommand};
use crate::state::AppState;

// Note: config commands do not use trace_command or ensure_settings_window directly.

#[derive(serde::Serialize, Clone)]
pub struct SaveConfigResult {
    pub reindex_started: bool,
}

#[derive(serde::Serialize, Clone)]
pub struct BootstrapGeneralConfig {
    pub auto_hide_on_focus_lost: bool,
}

#[derive(serde::Serialize, Clone)]
pub struct BootstrapPayload {
    pub visual: snotra_core::config::VisualConfig,
    pub general: BootstrapGeneralConfig,
    pub indexing: bool,
}

#[tauri::command]
pub fn load_config() -> Config {
    Config::load()
}

#[tauri::command]
pub fn save_config(
    config: Config,
    state: State<AppState>,
    app: AppHandle,
) -> Result<SaveConfigResult, String> {
    // Clone old config and drop the engine lock before platform bridge communication
    let old_config = state.engine.lock().unwrap().config().clone();

    // Detect what changed before moving config into state
    let index_changed = config.paths.scan != old_config.paths.scan
        || config.search.show_hidden_system != old_config.search.show_hidden_system
        || config.appearance.show_icons != old_config.appearance.show_icons;
    let visual_changed = config.visual != old_config.visual;
    let width_changed = config.appearance.window_width != old_config.appearance.window_width;
    let new_visual = if visual_changed {
        Some(config.visual.clone())
    } else {
        None
    };
    let new_width = config.appearance.window_width;

    // Notify platform bridge of hotkey/tray changes.
    // Hotkey registration is checked BEFORE saving to disk: if registration fails,
    // we return Err without persisting the invalid hotkey.
    // Engine lock is NOT held during platform bridge communication.
    if let Some(bridge) = app.try_state::<std::sync::Mutex<PlatformBridge>>()
        && let Ok(b) = bridge.lock()
    {
        if config.hotkey != old_config.hotkey {
            let (tx, rx) = std::sync::mpsc::channel();
            b.send_command(PlatformCommand::SetHotkey {
                config: config.hotkey.clone(),
                reply: tx,
            });
            match rx.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(false) | Err(_) => return Err("hotkey_registration_failed".to_string()),
                Ok(true) => {}
            }
        }
        if config.general.show_tray_icon != old_config.general.show_tray_icon {
            b.send_command(PlatformCommand::SetTrayVisible(
                config.general.show_tray_icon,
            ));
        }
    }

    // Save to disk only after hotkey validation succeeded
    config.save();

    // Re-lock engine to update config
    {
        state.engine.lock().unwrap().update_config(config);
    }

    // First-run path: initial indexing is pending (indexing=true) but build not started yet.
    // Do not treat regular reindex-in-progress as first run.
    let is_first_run_pending =
        state.indexing.load(Ordering::SeqCst) && !state.index_build_started.load(Ordering::SeqCst);
    if is_first_run_pending {
        indexing::start_index_build(&app);
        if let Some(w) = app.get_webview_window("settings") {
            let _ = w.close();
        }
    }

    // Trigger reindex if index-related settings changed.
    // Never restart while a build is already running, otherwise multiple
    // index threads can race and last-writer wins.
    let mut reindex_started = false;
    let indexing_in_progress = state.indexing.load(Ordering::SeqCst);
    if index_changed && !is_first_run_pending && !indexing_in_progress {
        state.index_build_started.store(false, Ordering::SeqCst);
        reindex_started = indexing::start_index_build(&app);
    }

    // Emit visual config change for live theme update
    if let Some(visual) = new_visual {
        let _ = app.emit("visual-config-changed", &visual);
    }

    // Resize main and results windows if window_width changed
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

    Ok(SaveConfigResult { reindex_started })
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Config {
    state.engine.lock().unwrap().config().clone()
}

#[tauri::command]
pub fn get_bootstrap_payload(state: State<AppState>) -> BootstrapPayload {
    let engine = state.engine.lock().unwrap();
    BootstrapPayload {
        visual: engine.config().visual.clone(),
        general: BootstrapGeneralConfig {
            auto_hide_on_focus_lost: engine.config().general.auto_hide_on_focus_lost,
        },
        indexing: state.indexing.load(Ordering::SeqCst),
    }
}
