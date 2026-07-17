use rayon::prelude::*;
use tauri::State;
use tauri::ipc::Response;

use crate::icon::{encode_batch_binary, extract_png, IconCache, IconCacheState};
use crate::state::AppState;

fn ensure_icon_cache_loaded_if_enabled(state: &State<AppState>, icons: &State<IconCacheState>) {
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

    // Step 2: ミスしたアイコンをロック外で抽出する（SHGetFileInfoW + PNG エンコード）
    // rayon で並列化: extract_png は各スレッドで独立した Win32 ハンドルを取得・破棄するためスレッドセーフ
    let extracted: Vec<(usize, String, Vec<u8>)> = misses
        .into_par_iter()
        .filter_map(|(i, path)| extract_png(&path).map(|png| (i, path, png)))
        .collect();

    // Step 3: 抽出結果を cache へ挿入し、同一ロック内でバイナリレスポンスを構築する
    // ロック内でスライス参照を使うことで clone を完全に排除する。
    //
    // 既知の残余（意図的に許容・#522 adversarial review で指摘）: Step 2 はロック外
    // なので、抽出中に invalidate → 別リクエストの再ロードが挟まると、ここで
    // 「旧インデックス由来のパス」のアイコンを新キャッシュへ挿入しうる。ただし
    // 挿入されるのは extract_png が**現在のディスクから**取った正しい PNG のみ
    // （消えたファイルは抽出失敗で落ちる）で、旧 icons.bin のデータではない。
    // インデックス外エントリは要求されず cap で有界、次回ビルドの retain_paths が
    // 剪定する。世代カウンタでの排除は無害な残余に対する状態追加ゆえ見送り（YAGNI）。
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
