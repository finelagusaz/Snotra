use tauri::State;

use crate::icon::{IconCache, IconCacheState};
use crate::state::AppState;

fn ensure_icon_cache_loaded_if_enabled(state: &State<AppState>, icons: &State<IconCacheState>) {
    let show_icons = {
        let cfg = state.config.lock().unwrap();
        cfg.appearance.show_icons
    };
    let mut cache = icons.lock().unwrap();
    if !show_icons {
        *cache = None;
        return;
    }
    if cache.is_none() {
        *cache = Some(IconCache::load());
    }
}

#[tauri::command]
pub fn get_icon_base64(
    path: String,
    state: State<AppState>,
    icons: State<IconCacheState>,
) -> Option<String> {
    ensure_icon_cache_loaded_if_enabled(&state, &icons);
    let mut cache = icons.lock().unwrap();
    cache.as_mut()?.get_or_extract(&path)
}

#[tauri::command]
pub fn get_icons_batch(
    paths: Vec<String>,
    state: State<AppState>,
    icons: State<IconCacheState>,
) -> std::collections::HashMap<String, String> {
    ensure_icon_cache_loaded_if_enabled(&state, &icons);
    let mut cache = icons.lock().unwrap();
    match cache.as_mut() {
        Some(c) => c.get_or_extract_batch(&paths),
        None => std::collections::HashMap::new(),
    }
}
