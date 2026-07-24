use rayon::prelude::*;
use tauri::State;

use crate::icon::{extract_png, IconCache, IconCacheState};
use crate::state::AppState;

pub(crate) fn ensure_icon_cache_loaded_if_enabled(
    state: &State<AppState>,
    icons: &State<IconCacheState>,
) {
    // config は単一の engine ロック内で読み、icon cache のロックを取る前に解放する
    // (engine ロックを跨いで I/O しない)。cap は `Config::icon_cache_cap()` が表示ワーキングセット
    // から派生する（独立 config キー・検証・floor を持たず「cap ≥ ワーキングセット」が構造的に成立。
    // 詳細は snotra-core の同メソッド doc を参照）。
    let (show_icons, cap) = {
        let engine = state.engine.lock().unwrap();
        let cfg = engine.config();
        (cfg.appearance.show_icons, cfg.icon_cache_cap())
    };
    let mut cache = icons.lock().unwrap();
    if !show_icons {
        *cache = None;
        return;
    }
    if cache.is_none() {
        *cache = Some(IconCache::load(cap));
    }
}

/// egui worker 用: paths のアイコン PNG を（キャッシュ get-or-extract-insert して）owned で返す。
/// ensure-loaded + 3 段ロック規律（miss 収集 → ロック外抽出 → 挿入）。show_icons=false 時は全 None。
pub(crate) fn load_icon_pngs(
    state: &State<AppState>,
    icons: &State<IconCacheState>,
    paths: Vec<String>,
) -> Vec<(String, Option<Vec<u8>>)> {
    ensure_icon_cache_loaded_if_enabled(state, icons);
    // Step 1: miss 収集（1 ロック）
    let mut misses: Vec<String> = Vec::new();
    {
        let cache = icons.lock().unwrap();
        match cache.as_ref() {
            None => return paths.into_iter().map(|p| (p, None)).collect(),
            Some(c) => {
                for p in &paths {
                    if c.get(p).is_none() {
                        misses.push(p.clone());
                    }
                }
            }
        }
    }
    // Step 2: ロック外抽出（rayon）
    let extracted: Vec<(String, Vec<u8>)> = misses
        .into_par_iter()
        .filter_map(|p| extract_png(&p).map(|png| (p, png)))
        .collect();
    // Step 3: 挿入して owned で返す（clone・16x16 PNG ≤8 件ゆえ許容）
    let mut cache = icons.lock().unwrap();
    if let Some(c) = cache.as_mut() {
        for (p, png) in extracted {
            c.insert(p, png);
        }
        paths
            .into_iter()
            .map(|p| {
                let png = c.get(&p).map(|s| s.to_vec());
                (p, png)
            })
            .collect()
    } else {
        paths.into_iter().map(|p| (p, None)).collect()
    }
}
