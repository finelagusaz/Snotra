use tauri::ipc::Response;
use tauri::State;

use crate::icon::{extract_png, IconCache, IconCacheState};
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

    // Step 1: check cache under minimal lock
    {
        let cache = icons.lock().unwrap();
        match cache.as_ref() {
            None => return Err(()), // icons disabled
            Some(c) => {
                if let Some(png) = c.get(&path) {
                    return Ok(Response::new(png));
                }
            }
        }
    }

    // Step 2: extract outside the lock (SHGetFileInfoW + PNG encode)
    let png = extract_png(&path).ok_or(())?;

    // Step 3: insert into cache under minimal lock and return
    // A concurrent request for the same path may have already inserted; overwrite is benign.
    let mut cache = icons.lock().unwrap();
    if let Some(c) = cache.as_mut() {
        c.insert(path, png.clone());
    }
    Ok(Response::new(png))
}
