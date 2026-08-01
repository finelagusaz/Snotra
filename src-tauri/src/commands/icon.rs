use rayon::prelude::*;
use tauri::State;

use crate::icon::{IconCache, IconCacheState, IconFailure, extract_png};
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

/// 1 パス分のアイコン取得結果。**「取れなかった」を 1 つに潰さない**（#692）——
/// 呼び出し側が再試行の可否を決めるには理由が要る。
pub(crate) enum IconOutcome {
    /// PNG バイト列（キャッシュヒットまたは抽出成功）。
    Png(Vec<u8>),
    /// 抽出が失敗した。`IconFailure::is_transient` が再試行の可否を示す。
    Failed(IconFailure),
    /// キャッシュ自体が使えなかった（`show_icons=false`、または無効化と競合）。
    /// パス固有の失敗ではないので、**恒久的な欠落として扱ってはならない**。
    Unavailable,
}

/// egui worker 用: paths のアイコン PNG を（キャッシュ get-or-extract-insert して）owned で返す。
/// ensure-loaded + 3 段ロック規律（miss 収集 → ロック外抽出 → 挿入）。
/// show_icons=false 時は全 `Unavailable`。
pub(crate) fn load_icon_pngs(
    state: &State<AppState>,
    icons: &State<IconCacheState>,
    paths: Vec<String>,
) -> Vec<(String, IconOutcome)> {
    ensure_icon_cache_loaded_if_enabled(state, icons);
    // Step 1: miss 収集（1 ロック）
    let mut misses: Vec<String> = Vec::new();
    {
        let cache = icons.lock().unwrap();
        match cache.as_ref() {
            None => {
                return paths
                    .into_iter()
                    .map(|p| (p, IconOutcome::Unavailable))
                    .collect();
            }
            Some(c) => {
                for p in &paths {
                    if c.get(p).is_none() {
                        misses.push(p.clone());
                    }
                }
            }
        }
    }
    // Step 2: ロック外抽出（rayon）。**失敗した path と理由も保持する**——落とすと
    // Step 3 で「キャッシュに無い」としか分からず、恒久/一過性の区別が消える（#692）。
    let extracted: Vec<(String, Result<Vec<u8>, IconFailure>)> = misses
        .into_par_iter()
        .map(|p| {
            let outcome = extract_png(&p);
            if let Err(reason) = &outcome {
                // #692: 失敗を握りつぶさず理由を残す。呼び出し側は「アイコンが無い」と
                // 「一時的に取れなかった」を区別できないため、ここが唯一の観測点である。
                crate::commands::trace_command(
                    "icon:extract_failed",
                    serde_json::json!({
                        "path": p,
                        "reason": format!("{reason:?}"),
                        "transient": reason.is_transient(),
                        "exists": std::path::Path::new(&p).exists(),
                    }),
                );
            }
            (p, outcome)
        })
        .collect();
    // Step 3: 挿入して owned で返す（clone・16x16 PNG ≤8 件ゆえ許容）
    let mut cache = icons.lock().unwrap();
    let Some(c) = cache.as_mut() else {
        return paths
            .into_iter()
            .map(|p| (p, IconOutcome::Unavailable))
            .collect();
    };
    // 失敗の理由は path 単位で引けるようにしてから挿入する。
    let mut failures: std::collections::HashMap<String, IconFailure> =
        std::collections::HashMap::new();
    for (p, outcome) in extracted {
        match outcome {
            Ok(png) => c.insert(p, png),
            Err(reason) => {
                failures.insert(p, reason);
            }
        }
    }
    paths
        .into_iter()
        .map(|p| {
            // キャッシュにあれば PNG。無ければ、この呼び出しで失敗した理由を返す。
            // どちらでもない（= miss でも失敗でもない）は起こらないが、起きたときに
            // 恒久扱いしないよう `Unavailable` へ倒す。
            let outcome = match c.get(&p) {
                Some(png) => IconOutcome::Png(png.to_vec()),
                None => match failures.remove(&p) {
                    Some(reason) => IconOutcome::Failed(reason),
                    None => IconOutcome::Unavailable,
                },
            };
            (p, outcome)
        })
        .collect()
}
