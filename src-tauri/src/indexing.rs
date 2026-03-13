use std::sync::Mutex;
use std::sync::atomic::Ordering;

use snotra_core::indexer;
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
    if let Some(bridge) = app.try_state::<Mutex<PlatformBridge>>()
        && let Ok(b) = bridge.lock()
    {
        b.send_command(PlatformCommand::SetIndexing(true));
    }

    // Notify frontend that indexing has started
    let _ = app.emit("indexing-started", ());

    let app_handle = app.clone();
    std::thread::Builder::new()
        .name("snotra-index-build".to_string())
        .spawn(move || {
            let (scan, show_hidden_system, show_icons, include_path_env) = {
                let state = app_handle.state::<AppState>();
                let engine = state.engine.lock().unwrap();
                (
                    engine.config().paths.scan.clone(),
                    engine.config().search.show_hidden_system,
                    engine.config().appearance.show_icons,
                    engine.config().search.include_path_env,
                )
            };

            let mut entries = indexer::rebuild_and_save(&scan, show_hidden_system);

            // PATH エントリのマージ
            if include_path_env {
                let path_entries = indexer::scan_path_env(&entries, show_hidden_system);
                entries.extend(path_entries);
            }

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

            // SearchEngine の構築（O(N)）は Mutex 外で実施してロック保持時間を最小化する
            let new_index = snotra_core::engine::PrebuiltIndex::new(entries);
            {
                let state = app_handle.state::<AppState>();
                state.engine.lock().unwrap().apply_prebuilt_index(new_index);
            }

            // Mark indexing complete
            {
                let state = app_handle.state::<AppState>();
                state.indexing.store(false, Ordering::SeqCst);
            }

            // Notify platform thread
            if let Some(bridge) = app_handle.try_state::<Mutex<PlatformBridge>>()
                && let Ok(b) = bridge.lock()
            {
                b.send_command(PlatformCommand::SetIndexing(false));
            }

            // Notify frontend
            let _ = app_handle.emit("indexing-complete", ());
        })
        .ok();

    true
}
