use tauri::ipc::Response;
use tauri::State;

use crate::icon::{IconCache, IconCacheState};
use crate::state::AppState;

fn ensure_icon_cache_loaded_if_enabled(state: &State<AppState>, icons: &State<IconCacheState>) {
    // Read config value and drop engine lock before locking icon cache
    let show_icons = state.engine.lock().unwrap().config().appearance.show_icons;
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
pub fn get_icon_png(
    path: String,
    state: State<AppState>,
    icons: State<IconCacheState>,
) -> Result<Response, ()> {
    ensure_icon_cache_loaded_if_enabled(&state, &icons);
    let mut cache = icons.lock().unwrap();
    cache
        .as_mut()
        .and_then(|c| c.get_or_extract(&path))
        .map(Response::new)
        .ok_or(())
}
