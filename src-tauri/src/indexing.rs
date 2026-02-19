use std::sync::atomic::Ordering;
use std::sync::Mutex;

use snotra_core::indexer;
use snotra_core::search::SearchEngine;
use tauri::{AppHandle, Emitter, Manager};

use crate::icon;
use crate::platform::{PlatformBridge, PlatformCommand};
use crate::state::AppState;

/// Start index build in a background thread.
/// Returns `true` if the build was started, `false` if already running.
pub fn start_index_build(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    if state
        .index_build_started
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return false;
    }

    state.indexing.store(true, Ordering::SeqCst);

    // Notify platform thread
    if let Some(bridge) = app.try_state::<Mutex<PlatformBridge>>() {
        if let Ok(b) = bridge.lock() {
            b.send_command(PlatformCommand::SetIndexing(true));
        }
    }

    let app_handle = app.clone();
    std::thread::Builder::new()
        .name("snotra-index-build".to_string())
        .spawn(move || {
            let (scan, show_hidden_system, show_icons) = {
                let state = app_handle.state::<AppState>();
                let config = state.config.lock().unwrap();
                (
                    config.paths.scan.clone(),
                    config.search.show_hidden_system,
                    config.appearance.show_icons,
                )
            };

            let entries = indexer::rebuild_and_save(&scan, show_hidden_system);

            // Sync icon cache with current show_icons setting
            {
                let icon_state = app_handle.state::<icon::IconCacheState>();
                let mut current = icon_state.lock().unwrap();
                if show_icons {
                    // Clear stale icons — re-extracted on next search
                    if let Some(c) = current.as_mut() {
                        c.clear();
                    }
                } else {
                    // show_icons disabled — drop the cache entirely
                    *current = None;
                }
            }

            // Update search engine
            {
                let state = app_handle.state::<AppState>();
                let mut engine = state.engine.lock().unwrap();
                *engine = SearchEngine::new(entries);
            }

            // Mark indexing complete
            {
                let state = app_handle.state::<AppState>();
                state.indexing.store(false, Ordering::SeqCst);
            }

            // Notify platform thread
            if let Some(bridge) = app_handle.try_state::<Mutex<PlatformBridge>>() {
                if let Ok(b) = bridge.lock() {
                    b.send_command(PlatformCommand::SetIndexing(false));
                }
            }

            // Notify frontend
            let _ = app_handle.emit("indexing-complete", ());
        })
        .ok();

    true
}
