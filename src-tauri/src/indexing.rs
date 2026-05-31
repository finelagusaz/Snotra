use std::sync::Mutex;

use snotra_core::indexer;
use tauri::{AppHandle, Emitter, Manager};

use crate::icon;
use crate::platform::{PlatformBridge, PlatformCommand};
use crate::state::AppState;

/// Start index build in a background thread.
/// Returns `true` if the build was started, `false` if already running.
pub fn start_index_build(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    if !state.try_begin_index_build() {
        return false;
    }

    // Notify platform thread
    if let Some(bridge) = app.try_state::<Mutex<PlatformBridge>>()
        && let Ok(b) = bridge.lock()
    {
        b.send_command(PlatformCommand::SetIndexing(true));
    }

    // Notify frontend that indexing has started
    let _ = app.emit("indexing-started", ());

    let app_handle = app.clone();
    let spawn_result = std::thread::Builder::new()
        .name("snotra-index-build".to_string())
        .spawn(move || {
            let (scan, show_hidden_system, show_icons, include_path_env, migemo_enabled) = {
                let state = app_handle.state::<AppState>();
                let engine = state.engine.lock().unwrap();
                (
                    engine.config().paths.scan.clone(),
                    engine.config().search.show_hidden_system,
                    engine.config().appearance.show_icons,
                    engine.config().search.include_path_env,
                    engine.config().search.migemo_enabled,
                )
            };

            let mut entries = indexer::rebuild_and_save(&scan, show_hidden_system);

            // PATH エントリのマージ
            if include_path_env {
                let path_entries = indexer::scan_path_env(&entries, show_hidden_system);
                entries.extend(path_entries);
            }

            // Sync icon cache with current index
            {
                let icon_state = app_handle.state::<icon::IconCacheState>();
                let mut current = icon_state.lock().unwrap();
                if show_icons {
                    // Prune stale icons — retain only entries present in the current index.
                    // Unlike clear(), this preserves valid icons and avoids re-extraction cost.
                    if let Some(c) = current.as_mut() {
                        let valid: std::collections::HashSet<String> = entries
                            .iter()
                            .map(|e| e.target_path.clone())
                            .collect();
                        c.retain_paths(&valid);
                    }
                } else {
                    // show_icons disabled — drop the cache entirely
                    *current = None;
                }
            }

            // SearchEngine の構築（O(N)）は Mutex 外で実施してロック保持時間を最小化する。
            // migemo 無効時は kana_lower_names を構築しない（issue #337）。
            let new_index = snotra_core::engine::PrebuiltIndex::new(entries, migemo_enabled);
            {
                let state = app_handle.state::<AppState>();
                state.engine.lock().unwrap().apply_prebuilt_index(new_index);
            }

            // ビルド中に設定が変わったか確認し、差異があれば再ビルドを予約
            let needs_rebuild = {
                let state = app_handle.state::<AppState>();
                let engine = state.engine.lock().unwrap();
                let cfg = engine.config();
                cfg.paths.scan != scan
                    || cfg.search.show_hidden_system != show_hidden_system
                    || cfg.appearance.show_icons != show_icons
                    || cfg.search.include_path_env != include_path_env
                    || cfg.search.migemo_enabled != migemo_enabled
            };

            // Mark indexing complete
            app_handle.state::<AppState>().finish_index_build();

            // Notify platform thread
            if let Some(bridge) = app_handle.try_state::<Mutex<PlatformBridge>>()
                && let Ok(b) = bridge.lock()
            {
                b.send_command(PlatformCommand::SetIndexing(false));
            }

            // Notify frontend
            let _ = app_handle.emit("indexing-complete", ());

            // 設定がビルド中に変わっていた場合、再ビルドを起動
            if needs_rebuild {
                start_index_build(&app_handle);
            }
        });

    if spawn_result.is_err() {
        // スレッド生成失敗。フラグをリセットし platform/frontend にも完了を通知して、
        // index_build_started=true のまま wedge するのを防ぐ（嘘の true を返さない）。
        state.finish_index_build();
        if let Some(bridge) = app.try_state::<Mutex<PlatformBridge>>()
            && let Ok(b) = bridge.lock()
        {
            b.send_command(PlatformCommand::SetIndexing(false));
        }
        let _ = app.emit("indexing-complete", ());
        return false;
    }

    true
}
