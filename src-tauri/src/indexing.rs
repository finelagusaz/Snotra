//! バックグラウンドでのインデックス構築。
//!
//! `start_index_build` は stale マーク → CAS → spawn の順で、drain ループ（現在 config の
//! `IndexInputs` snapshot → ロック外で再構築 → swap + re-diff）を stale が消えるまで回す。
//! ビルド本体は `catch_unwind` で包み、panic 時の flag 固着（永久「構築中」）を防ぐ。

use std::sync::Mutex;

use snotra_core::indexer;
use tauri::{AppHandle, Emitter, Manager};

use crate::icon;
use crate::platform::{PlatformBridge, PlatformCommand};
use crate::state::AppState;

/// インデックスビルドを開始（または kick）する。ビルドスレッドを起動したら `true`、
/// 既に走行中なら `false` を返す。
///
/// ビルド要求の全経路（config 変更 reindex / first-run / 手動 rebuild / 自己再 kick）が通る単一入口。
/// 先に `mark_index_stale` で index を stale にし、CAS に失敗（既に in-flight）しても走行中ビルドの
/// drain ループ / finish 後再チェックが取りこぼさず拾う（lost-update を塞ぐ、issue #347/#348-A）。
pub fn start_index_build(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();

    // ビルド要求: index を stale とマーク（CAS の前に立てる＝CAS 失敗側でも in-flight が拾える）。
    state.engine.lock().unwrap().mark_index_stale();

    if !state.try_begin_index_build() {
        return false;
    }

    notify_indexing_started(app);

    let app_handle = app.clone();
    let spawn_result = std::thread::Builder::new()
        .name("snotra-index-build".to_string())
        .spawn(move || {
            // ビルド本体（drain ループ）を catch_unwind で包む。挙動は **panic 戦略依存**:
            // - unwind ビルド（debug/test、または release で panic="unwind"）: panic をここで捕捉し、
            //   下の finish_index_build で flag を戻す → flag 固着（wedge）を防ぐ。主な panic 発火点
            //   （rebuild_and_save / IndexMaterial::extend_with_path_entries / PrebuiltIndex の構築）はロック外で engine ロックを保持しないため poison しない。
            // - release（このワークスペースは Cargo.toml で panic="abort"）: build スレッドの panic は
            //   プロセスを abort させ、ここには到達しない。ただし silent wedge にもならない（プロセスごと
            //   終了し、次回起動で fresh build される）。どちらの戦略でも「flag 固着で UI が永久構築中」は起きない。
            let build_result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drain_index(&app_handle)));

            // 完了処理（unwind 経路）: flag を必ず戻して wedge を防ぐ。abort 経路はここに来ないが、
            // プロセス終了済みのため wedge は発生しない。
            app_handle.state::<AppState>().finish_index_build();
            notify_indexing_complete(&app_handle);

            match build_result {
                Ok(()) => {
                    // finish 窓: complete が clear した後〜finish までに config 変更（CAS 失敗）が
                    // 刺さった場合を拾う。stale が残っていれば再 kick（CAS は finish 後なので成功する）。
                    let stale = app_handle
                        .state::<AppState>()
                        .engine
                        .lock()
                        .unwrap()
                        .is_index_stale();
                    if stale {
                        start_index_build(&app_handle);
                    }
                }
                Err(_) => {
                    // unwind で panic を捕捉した場合（debug/test 等）: 再 kick しない
                    // （決定論的 panic の無限リトライ回避）。index_stale は残るので次の config 変更 /
                    // 手動 rebuild で回復する。release(panic="abort") ではこの分岐に来ない（プロセス abort 済み）。
                    eprintln!(
                        "[indexing] build thread panicked (unwind); index_stale retained, recovery on next config change / manual rebuild"
                    );
                }
            }
        });

    if spawn_result.is_err() {
        // スレッド生成失敗。flag をリセットし platform/frontend にも完了を通知して、
        // index_build_started=true のまま wedge するのを防ぐ（嘘の true を返さない）。
        state.finish_index_build();
        notify_indexing_complete(app);
        return false;
    }

    true
}

/// `index_stale` が解消されるまでドレインする:
/// begin（現在 config の `IndexInputs` スナップショット）→ ロック外で重い構築 → complete（O(1) スワップ + re-diff）。
/// ビルド中に config が変わっていれば complete が stale を残し、次の begin が再び snapshot を返して再ビルドする。
/// 各反復で engine ロックを保持するのは snapshot 取得とスワップの一瞬だけ（ロック最小化を維持）。
fn drain_index(app_handle: &AppHandle) {
    loop {
        let inputs = {
            let state = app_handle.state::<AppState>();
            let engine = state.engine.lock().unwrap();
            engine.begin_index_drain()
        };
        let Some(inputs) = inputs else { break };

        // **保存が返した派生データをそのまま索引の表現に使う**（`rebuild_and_save` の doc）。捨てて建て直していた頃の額は `PERFORMANCE.md`「採用: `PrebuiltIndex` を `CachedMasks` 込みで建てる」。
        let mut material = indexer::rebuild_and_save(&inputs.scan, inputs.show_hidden_system);

        // PATH エントリのマージ。**木とマスクは組のまま持つ**ので、片方だけ伸ばす形はここでは書けない（正本は `IndexMaterial` の doc）。
        if inputs.include_path_env {
            let path_entries = indexer::scan_path_env(material.tree(), inputs.show_hidden_system);
            material.extend_with_path_entries(path_entries);
        }

        // Sync icon cache with current index
        {
            let icon_state = app_handle.state::<icon::IconCacheState>();
            if !inputs.show_icons {
                // show_icons disabled — drop the cache entirely（snapshot は要らない）
                *icon_state.lock().unwrap() = None;
            } else {
                // **判定は lock の外で行う。** 索引の全件走査（312,625 件・実測 36 ms）を
                // lock の中で回すと、表示中のアイコン取得（`commands::icon::load_icon_pngs`）が
                // その間ずっと待つ。lock を持つのは snapshot と除去の一瞬だけにする。
                // **その窓が安全である理由と受容残余は `IconCache::remove_paths` の doc が正本。**
                let keys = icon_state
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(icon::IconCache::keys);
                if let Some(keys) = keys {
                    let dead = dead_icon_paths(material.tree(), keys);
                    // **消すものが無ければ lock を取らない。** アプリが 1 つも消えていない
                    // 通常の再構築では `dead` は空で、そこで lock を取るのはこの節が削ろうと
                    // している lock そのものである。
                    //
                    // 窓の間に `invalidate_icon_cache` / show_icons=false が `None` へ
                    // 落としていることがあるので、取り直して確かめる。
                    if !dead.is_empty()
                        && let Some(c) = icon_state.lock().unwrap().as_mut()
                    {
                        c.remove_paths(&dead);
                    }
                }
            }
        }

        // SearchEngine の構築（O(N)）は Mutex 外で実施してロック保持時間を最小化する。
        // migemo 無効時は kana_lower_names を構築しない（issue #337）。
        //
        // **ここで分岐しない。** 派生データの有無で建て方が分かれるのは `SearchEngine::from_material` の 1 か所だけで、その条件の正本は `indexer::save_cache_sorted` の分岐である。
        let new_index =
            snotra_core::engine::PrebuiltIndex::from_material(material, inputs.migemo_enabled);
        {
            let state = app_handle.state::<AppState>();
            state
                .engine
                .lock()
                .unwrap()
                .complete_index_drain(new_index, &inputs);
        }
    }
}

/// アイコンキャッシュのキーのうち、**索引に無いと確かめられた**ものを返す（純関数）。
///
/// 木はフルパスを持たないので組み直して照合する。**バッファを 1 本使い回し、集合は
/// 索引側ではなくキャッシュ側で作る。** 索引の全パスを所有 `String` の集合にすると
/// 312,625 件ぶんの確保が一時的に積み上がる——**`index.bin` v7 がディスクから消したのと
/// 同じ額**（約 36 MiB）を剪定のためだけに作り直すことになる。アイコンキャッシュは
/// `Config::icon_cache_cap()` で頭打ち（既定 1,000 件）なので、索引の側は走るだけにする。
///
/// **返すのが「残す集合」ではなく「落とす集合」であることは、呼び出し側の安全性の要石で
/// ある**——理由と受容残余は [`icon::IconCache::remove_paths`] の doc が正本とする。
///
/// `keys` が空なら走査ごと省く。**初回起動はここではない**——そのときキャッシュは
/// `None`（`icons.bin` は初回の表示まで遅延ロード・`SPEC.md` §3.4）ゆえ、呼び出し側の
/// `as_ref()` が手前で止める。効くのは `Some(空)`——ロードは走ったがまだ 1 件も挿入されて
/// いない狭い区間だけである。**それでも置くのは、外したときに払うのが 312,625 回の
/// 組み直しだからである。**
fn dead_icon_paths(
    tree: &snotra_core::index_tree::IndexTree,
    keys: Vec<String>,
) -> std::collections::HashSet<String> {
    let mut dead: std::collections::HashSet<String> = keys.into_iter().collect();
    if dead.is_empty() {
        return dead;
    }
    let mut buf = String::new();
    for i in 0..tree.len() {
        tree.path_into(&mut buf, i);
        // 索引に在ると分かったキーは落とす集合から外す。**ヒットしても確保しない**
        // （`alive` を積む形は 1 件ごとに `String` を確保していた）。
        dead.remove(&buf);
    }
    dead
}

fn notify_indexing_started(app: &AppHandle) {
    // Notify platform thread
    if let Some(bridge) = app.try_state::<Mutex<PlatformBridge>>()
        && let Ok(b) = bridge.lock()
    {
        b.send_command(PlatformCommand::SetIndexing(true));
    }
    // Notify frontend that indexing has started
    let _ = app.emit(crate::events::INDEXING_STARTED, ());
}

fn notify_indexing_complete(app: &AppHandle) {
    // Notify platform thread
    if let Some(bridge) = app.try_state::<Mutex<PlatformBridge>>()
        && let Ok(b) = bridge.lock()
    {
        b.send_command(PlatformCommand::SetIndexing(false));
    }
    // Notify frontend
    let _ = app.emit(crate::events::INDEXING_COMPLETE, ());
}

#[cfg(test)]
mod tests {
    use super::dead_icon_paths;
    use snotra_core::index_tree::IndexTree;

    /// `C:\app` の下に `tool.exe` と `sub`、その下に `deep.exe` を持つ木。
    /// `parent` の番兵 `u32::MAX` は根を表す（`aux` は `table` のフルパスを指す）。
    fn tree() -> IndexTree {
        IndexTree::from_parts(
            vec![
                String::new(), // 0: 根（フルパスは table[1]）
                "tool".into(), // 1: C:\app\tool.exe
                "sub".into(),  // 2: C:\app\sub
                "deep".into(), // 3: C:\app\sub\deep.exe
            ],
            vec![true, false, true, false],
            vec![u32::MAX, 0, 0, 2],
            vec![1, 2, 0, 2],
            vec![String::new(), "C:\\app".into(), ".exe".into()],
            false,
        )
        .expect("fixture の列は健全である")
    }

    #[test]
    fn dead_icon_paths_returns_only_keys_absent_from_the_tree() {
        let tree = tree();
        let dead = dead_icon_paths(
            &tree,
            vec![
                "C:\\app\\tool.exe".into(),
                "C:\\app\\sub\\deep.exe".into(),
                "C:\\app\\removed.exe".into(),
            ],
        );
        assert_eq!(dead.len(), 1, "索引に在る 2 件は落とす集合に入らない");
        assert!(dead.contains("C:\\app\\removed.exe"));
    }

    /// **キャッシュが空なら索引を走査しない**（到達条件は関数の doc を正本とする）。
    #[test]
    fn dead_icon_paths_is_empty_for_an_empty_cache() {
        assert!(dead_icon_paths(&tree(), Vec::new()).is_empty());
    }

    /// 照合は**フルパスの原文**に対して行う（末尾成分や正規化キーではない）。
    /// 大文字小文字が違うキーは別物として落ちる——アイコンキャッシュのキーは
    /// エントリの `target_path` そのものなので、実運用では一致する。
    #[test]
    fn dead_icon_paths_compares_full_paths_verbatim() {
        let dead = dead_icon_paths(&tree(), vec!["tool.exe".into(), "C:\\APP\\tool.exe".into()]);
        assert_eq!(
            dead.len(),
            2,
            "末尾成分だけ・大小が違う形はどちらも一致しない"
        );
    }
}
