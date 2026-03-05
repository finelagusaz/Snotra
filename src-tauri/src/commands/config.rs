use std::sync::atomic::Ordering;

use tauri::State;

use crate::state::AppState;

#[derive(serde::Serialize, Clone)]
pub struct BootstrapGeneralConfig {
    pub auto_hide_on_focus_lost: bool,
}

#[derive(serde::Serialize, Clone)]
pub struct BootstrapAppearanceConfig {
    pub show_icons: bool,
}

#[derive(serde::Serialize, Clone)]
pub struct BootstrapPayload {
    pub visual: snotra_core::config::VisualConfig,
    pub general: BootstrapGeneralConfig,
    pub appearance: BootstrapAppearanceConfig,
    pub indexing: bool,
}

#[tauri::command]
pub fn get_bootstrap_payload(state: State<AppState>) -> BootstrapPayload {
    let engine = state.engine.lock().unwrap();
    BootstrapPayload {
        visual: engine.config().visual.clone(),
        general: BootstrapGeneralConfig {
            auto_hide_on_focus_lost: engine.config().general.auto_hide_on_focus_lost,
        },
        appearance: BootstrapAppearanceConfig {
            show_icons: engine.config().appearance.show_icons,
        },
        indexing: state.indexing.load(Ordering::SeqCst),
    }
}
