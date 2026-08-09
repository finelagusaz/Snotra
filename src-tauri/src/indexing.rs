//! バックグラウンドでのインデックス構築。
//!
//! `start_index_build` は stale マーク → CAS → spawn の順で、drain ループ（現在 config の
//! `IndexInputs` snapshot → ロック外で再構築 → swap + re-diff）を stale が消えるまで回す。
//! ビルド本体は `catch_unwind` で包み、panic 時の flag 固着（永久「構築中」）を防ぐ。

use std::sync::Mutex;

use snotra_core::engine::{IndexInputs, PrebuiltIndex};
use snotra_core::indexer;
use tauri::{AppHandle, Emitter, Manager};

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

/// 材料から索引を建てる。**PATH エントリのマージを含む。**
///
/// **drain ループと背景再スキャンの適用が同じここを通ることが、両者が一致することの
/// 根拠である。** 手順を写すと、片方だけ PATH マージを忘れる欠陥が沈黙で起きる
/// ——PATH のコマンドが検索から消えるが、検索結果自体は出るので気づく手段が無い
/// （`normalize_entry_key_into` と同じ理屈）。
pub(crate) fn build_index_from_material(
    mut material: indexer::IndexMaterial,
    inputs: &IndexInputs,
) -> PrebuiltIndex {
    // **木とマスクは組のまま持つ**ので、片方だけ伸ばす形はここでは書けない
    // （正本は `IndexMaterial` の doc）。
    if inputs.include_path_env {
        let path_entries = indexer::scan_path_env(material.tree(), inputs.show_hidden_system);
        material.extend_with_path_entries(path_entries);
    }
    // **ここで分岐しない。** 派生データの有無で建て方が分かれるのは
    // `SearchEngine::from_material` の 1 か所だけである。
    PrebuiltIndex::from_material(material, inputs.migemo_enabled)
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
        let material = indexer::rebuild_and_save(&inputs.scan, inputs.show_hidden_system);

        // **アイコンキャッシュにはもう触らない。** 索引照合の剪定は #996 で撤去し、無効化時の
        // 破棄も `config_watcher` の true → false のエッジへ移した（理由は `icon::drop_icon_cache`
        // の doc が正本）。ゆえに `IndexInputs` は索引を建て直す入力だけを持つ。

        // SearchEngine の構築（O(N)）は Mutex 外で実施してロック保持時間を最小化する。
        // migemo 無効時は kana_lower_names を構築しない（issue #337）。
        let new_index = build_index_from_material(material, &inputs);
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
