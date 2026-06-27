use rayon::prelude::*;
use tauri::State;
use tauri::ipc::Response;

use crate::icon::{encode_batch_binary, extract_png, IconCache, IconCacheState};
use crate::state::AppState;

fn ensure_icon_cache_loaded_if_enabled(state: &State<AppState>, icons: &State<IconCacheState>) {
    // Read config values under a single engine lock, then drop it before locking icon cache
    // (engine ロックを跨いで I/O しない)。
    let (show_icons, configured_cap, max_results) = {
        let engine = state.engine.lock().unwrap();
        let cfg = engine.config();
        (
            cfg.appearance.show_icons,
            cfg.cache.icon_cache_cap,
            cfg.appearance.max_results,
        )
    };
    // 表示中アイコン（可視バッチ = `items.slice(0, max_results)`）が evict されフォールバック絵文字に
    // 落ちないよう、実効 cap を max_results で floor する。可視バッチは 1 回の get_icons_batch で
    // max_results 件以下のミスしか挿入しないため、cap >= max_results なら**バッチ自身のミス同士は
    // 退避し合わず**、新規抽出アイコンは応答に正しく載る（その後オフスクリーンバッチで古い順に
    // evict されてもフロントは Blob URL を保持済み）。validate() をすり抜けた手編集の極小 cap への防御でもある。
    // 厳密には、cap 飽和時に同一バッチが要求する**既存ヒットのうち最古エントリ**が、そのバッチの新規
    // insert で退避され Step3 末尾の get で None になる窓はありうる（次バッチで再抽出され自己回復する
    // 1 フレームの揺らぎ）。これは既定 cap=1000 では到達困難。
    // また空クエリのオフスクリーン先読みバッチは最大 top_n_history 件になりうるため、表示ワーキングセット
    // 全体（max_results + top_n_history）を確実にキャッシュするには cap をそれ以上に取る必要がある
    // → 既定 cap 1000 はこれを上回る（issue #333 の設計: validation は max_results、既定を大きく取る）。
    let effective_cap = configured_cap.max(max_results);
    let mut cache = icons.lock().unwrap();
    if !show_icons {
        *cache = None;
        return;
    }
    if cache.is_none() {
        *cache = Some(IconCache::load(effective_cap));
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

    // Step 1: check cache for all paths in one lock — clone せずミスのみ収集
    let mut misses: Vec<(usize, String)> = Vec::new();
    {
        let cache = icons.lock().unwrap();
        match cache.as_ref() {
            None => return Response::new(encode_batch_binary(&vec![None; paths.len()])),
            Some(c) => {
                for (i, path) in paths.iter().enumerate() {
                    if c.get(path).is_none() {
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

    // Step 3: insert extracted → cache, then build binary response in one lock
    // ロック内でスライス参照を使うことで clone を完全に排除する。
    let mut cache = icons.lock().unwrap();
    if let Some(c) = cache.as_mut() {
        for (_, path, png) in extracted {
            c.insert(path, png);
        }
        // Build binary frame from cache slices (zero-copy)
        let refs: Vec<Option<&[u8]>> = paths.iter()
            .map(|path| c.get(path))
            .collect();
        Response::new(encode_batch_binary(&refs))
    } else {
        Response::new(encode_batch_binary(&vec![None; paths.len()]))
    }
}
