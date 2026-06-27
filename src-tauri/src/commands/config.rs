use std::sync::atomic::Ordering;

use tauri::State;

use snotra_core::config::{AutoUpdateMode, Language};

use crate::state::AppState;

#[derive(serde::Serialize, Clone)]
pub struct BootstrapGeneralConfig {
    pub auto_hide_on_focus_lost: bool,
    pub auto_update: AutoUpdateMode,
}

#[derive(serde::Serialize, Clone)]
pub struct BootstrapAppearanceConfig {
    pub show_icons: bool,
    pub visible_rows: usize,
}

#[derive(serde::Serialize, Clone)]
pub struct BootstrapPayload {
    pub visual: snotra_core::config::VisualConfig,
    pub general: BootstrapGeneralConfig,
    pub appearance: BootstrapAppearanceConfig,
    pub language: String,
    pub indexing: bool,
    pub instant_command_prefix: String,
    pub result_limit: usize,
}

#[tauri::command]
pub fn get_bootstrap_payload(state: State<AppState>) -> BootstrapPayload {
    let engine = state.engine.lock().unwrap();
    BootstrapPayload {
        visual: engine.config().visual.clone(),
        general: BootstrapGeneralConfig {
            auto_hide_on_focus_lost: engine.config().general.auto_hide_on_focus_lost,
            auto_update: engine.config().general.auto_update,
        },
        appearance: BootstrapAppearanceConfig {
            show_icons: engine.config().appearance.show_icons,
            visible_rows: engine.config().appearance.effective_visible_rows(),
        },
        language: match engine.config().general.language {
            Language::Ja => "ja".to_string(),
            Language::En => "en".to_string(),
        },
        indexing: state.indexing.load(Ordering::SeqCst),
        instant_command_prefix: engine.config().search.instant_command_prefix.clone(),
        result_limit: engine.config().search.effective_result_limit(),
    }
}
