use rayon::prelude::*;
use tauri::State;
use tauri::ipc::Response;

use crate::icon::{encode_batch_binary, extract_png, IconCache, IconCacheState};
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
/// 戻り値は長さプレフィクス付きバイナリバッチ（tauri::ipc::Response）。
#[tauri::command]
pub fn get_icons_batch(
    paths: Vec<String>,
    state: State<AppState>,
    icons: State<IconCacheState>,
) -> Response {
    ensure_icon_cache_loaded_if_enabled(&state, &icons);

    // Step 1: check cache for all paths in one lock
    let mut results: Vec<Option<Vec<u8>>> = Vec::with_capacity(paths.len());
    let mut misses: Vec<(usize, String)> = Vec::new();
    {
        let cache = icons.lock().unwrap();
        match cache.as_ref() {
            None => return Response::new(encode_batch_binary(&vec![None; paths.len()])),
            Some(c) => {
                for (i, path) in paths.iter().enumerate() {
                    if let Some(png) = c.get(path) {
                        results.push(Some(png.to_vec()));
                    } else {
                        results.push(None);
                        misses.push((i, path.clone()));
                    }
                }
            }
        }
    }

    // Step 2: extract missing icons outside the lock (SHGetFileInfoW + PNG encode)
    // rayon で並列化: extract_png は各スレッドで独立した Win32 ハンドルを取得・破棄するためスレッドセーフ
    let extracted: Vec<(usize, String, Vec<u8>)> = misses
        .into_par_iter()
        .filter_map(|(i, path)| extract_png(&path).map(|png| (i, path, png)))
        .collect();

    // Step 3: insert into cache and fill in results
    {
        let mut cache = icons.lock().unwrap();
        if let Some(c) = cache.as_mut() {
            for (i, path, png) in extracted {
                results[i] = Some(png.clone());
                c.insert(path, png);
            }
        }
    }

    // Build binary frame: borrow slices from owned Vecs
    let refs: Vec<Option<&[u8]>> = results.iter().map(|r| r.as_deref()).collect();
    Response::new(encode_batch_binary(&refs))
}
