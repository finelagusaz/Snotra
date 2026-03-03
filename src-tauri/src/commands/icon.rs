use base64::{Engine as _, engine::general_purpose::STANDARD};
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

/// 複数パスのアイコンを 1 IPC 呼び出しで一括取得する。
/// キャッシュヒットは 1 回のロックで一括確認し、ミスは Mutex 外で抽出してから
/// まとめて挿入することでロック保持時間を最小化する。
/// 戻り値は入力 paths と同じ長さの配列。None = アイコン無効または取得失敗。
/// Some(s) = base64 エンコードされた PNG バイト列。
#[tauri::command]
pub fn get_icons_batch(
    paths: Vec<String>,
    state: State<AppState>,
    icons: State<IconCacheState>,
) -> Vec<Option<String>> {
    ensure_icon_cache_loaded_if_enabled(&state, &icons);

    // Step 1: check cache for all paths in one lock
    let mut results: Vec<Option<String>> = Vec::with_capacity(paths.len());
    let mut misses: Vec<(usize, String)> = Vec::new();
    {
        let cache = icons.lock().unwrap();
        match cache.as_ref() {
            None => return vec![None; paths.len()], // icons disabled
            Some(c) => {
                for (i, path) in paths.iter().enumerate() {
                    if let Some(png) = c.get(path) {
                        results.push(Some(STANDARD.encode(&png)));
                    } else {
                        results.push(None);
                        misses.push((i, path.clone()));
                    }
                }
            }
        }
    }

    // Step 2: extract missing icons outside the lock (SHGetFileInfoW + PNG encode)
    let extracted: Vec<(usize, String, Vec<u8>)> = misses
        .into_iter()
        .filter_map(|(i, path)| extract_png(&path).map(|png| (i, path, png)))
        .collect();

    // Step 3: insert into cache and fill in base64-encoded results
    {
        let mut cache = icons.lock().unwrap();
        if let Some(c) = cache.as_mut() {
            for (i, path, png) in &extracted {
                results[*i] = Some(STANDARD.encode(png));
                c.insert(path.clone(), png.clone());
            }
        }
    }

    results
}
