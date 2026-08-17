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

    // **アイコンキャッシュを捨てる（メモリ内 `IconCacheState` と `icons.bin` の両方を
    // 無効化）。** #996 が索引照合の剪定を撤去して以来、この無効化は背景再スキャンの
    // `RescanOutcome::Changed`（main.rs）が担っていた。#1001 で背景再スキャンの
    // spawn と結果適用そのものが撤去されたため、**現在はここが唯一の担い手である**。
    // **判定を置かない**——ユーザー（あるいは config 変更）が再構築を
    // 要求した事実そのものが引き金であり、集合が変わったかを測り直す必要は無い。
    //
    // ここで撃つのは CAS に成功した側だけである（要求のたびに撃つと、走行中ビルドへの
    // 重複要求で無駄に捨てる）。ただし `drain_index` の finish 窓で刺さった変更は自己再 kick
    // として再びここを通るため、直前の無効化から間を置かず 2 回撃たれうる——1 回目の無効化後に
    // ユーザーが検索してアイコンを再抽出していれば、それも巻き添えで捨てる。無害だが無駄。
    // engine ロックは `mark_index_stale()` の中で解放済みで、ロックを跨いだ取得にはならない。
    if let Some(icons) = app.try_state::<crate::icon::IconCacheState>() {
        crate::icon::invalidate_icon_cache(&icons);
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
/// **索引を建てる手順（PATH マージ + `PrebuiltIndex::from_material`）を共有する
/// 呼び出し点は、この crate ではこの drain ループの 1 つに閉じている**（背景再スキャンの
/// 適用は #1001 で撤去済み）。手順を書き写すと、片方だけ PATH マージを忘れる
/// 欠陥が沈黙で起きる——PATH のコマンドが検索から消えるが、検索結果自体は出るので
/// 気づく手段が無い（`normalize_entry_key_into` と同じ理屈）。**起動時のロード直後
/// （`main.rs` の PATH エントリのスキャン + マージ）は同じ手順を別に持つ**——そこは
/// `PrebuiltIndex` ではなく `Engine` を直接建てるため、ここを呼べない。
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

        // **drain ループ自身はアイコンキャッシュに触らない。** 無効化は `start_index_build` が
        // ビルド要求を受理した瞬間に 1 回だけ撃つ（このループの外）。索引照合の剪定は #996 で
        // 撤去したままであり、表示無効化時の破棄も `config_watcher` の true → false のエッジの
        // まま（理由は `icon::drop_icon_cache` の doc が正本）。ゆえに `IndexInputs` は索引を
        // 建て直す入力だけを持つ（この結論は変わらない）。

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

#[cfg(test)]
mod tests {
    /// トップレベル関数の本体を切り出す（**終端は列 0 の閉じ括弧**・内側のブロックは字下げされている）。
    ///
    /// 母集団は狭すぎても広すぎても壊れ、**広すぎる側は沈黙する**——探す文字列はこの `mod tests` に
    /// assert のリテラルとして実在するため、EOF まで伸びた母集団では `contains` が必ず真になる。
    /// ゆえに終端が見つかったことと `canary` が在ることの**両方**を assert する。行の走査を
    /// `str::lines` で行うのは改行コード非依存にするためである——`include_str!` が読むのは checkout
    /// された実ファイルであり、**手元（`core.autocrlf=input`）と Windows の CI（git-for-windows の
    /// system 既定 `core.autocrlf=true`）で改行が食い違う**。**原則の正本は
    /// `docs/development-principles.md`「検証の層と、層と層の隙間」**（ここに写しを置かない）。
    ///
    /// `launcher_controller.rs` の `method_body` とは**終端の形が違う**（あちらはメソッドゆえ
    /// 4 スペース字下げの `}`）。共通化を却下した判断と反転条件は
    /// `docs/adr/ADR-source-text-probe-helper-locality.md`。
    ///
    /// **`anchor` は列 0 から始まる宣言を指すこと。** 字下げされたアンカー（メソッドや `mod tests`
    /// 内の関数）を渡すと、終端の `}` はその外側の閉じ括弧になり、母集団が意図より広く伸びる——
    /// **#1108 の欠陥がそのまま再生する**。下の assert がそれを塞ぐ。
    fn top_level_fn_body(src: &str, anchor: &str, canary: &str) -> String {
        let (before, after) = src
            .split_once(anchor)
            .unwrap_or_else(|| panic!("{anchor} が見つからない（改名したらこの検査も直す）"));
        assert!(
            before.is_empty() || before.ends_with('\n'),
            "{anchor} が列 0 から始まっていない——終端の `}}` が外側の閉じ括弧になり、母集団が\
             意図より広く伸びる（doc 中にアンカー文字列が先行出現した場合もここで落ちる）"
        );
        let mut body = String::new();
        let mut terminated = false;
        for line in after.lines() {
            if line == "}" {
                terminated = true;
                break;
            }
            body.push_str(line);
            body.push('\n');
        }
        assert!(
            terminated,
            "{anchor} の終端（列 0 の `}}`）が見つからない——母集団が EOF まで伸びており、\
             この検査は空虚である"
        );
        assert!(
            body.contains(canary),
            "母集団が {anchor} の本体を含まない——終端の切り出しがずれた。\
             沈黙する検知器は検知器ではない"
        );
        body
    }

    /// [`top_level_fn_body`] が改行コードに依存しないことを、**この作業ツリーの改行コードによらず**
    /// 固定する（#1108）。`include_str!` は checkout された実ファイルを読むため、LF の環境で
    /// `cargo test` が緑でも CRLF の環境で母集団が壊れうる——#1077 で実際に CI がそうなった。
    #[test]
    fn top_level_fn_body_is_line_ending_agnostic() {
        let lf = "pub fn target() {\n    marker();\n}\npub fn next() {\n";
        let crlf = lf.replace('\n', "\r\n");
        for (label, src) in [("LF", lf), ("CRLF", crlf.as_str())] {
            let body = top_level_fn_body(src, "pub fn target(", "marker(");
            assert!(
                !body.contains("fn next("),
                "{label}: 終端を取り逃して次の関数まで飲み込んでいる"
            );
        }
    }

    /// 終端を持たない母集団を [`top_level_fn_body`] が**拒む**ことを固定する。
    ///
    /// **この 3 本（終端・アンカーの列・canary）が無いと、対応する `assert!` の行を消しても
    /// すべて緑のままになる**——母集団を守る assert 自身は、実ファイルを読む検査
    /// （`start_index_build_invalidates_the_icon_cache`）の正常系では一度も発火しないためである。
    #[test]
    #[should_panic(expected = "母集団が EOF まで伸びており")]
    fn top_level_fn_body_rejects_a_population_without_a_terminator() {
        top_level_fn_body(
            "pub fn target() {\n    marker();\n",
            "pub fn target(",
            "marker(",
        );
    }

    /// 字下げされたアンカーを [`top_level_fn_body`] が**拒む**ことを固定する。
    ///
    /// helper 化で新しく生まれた誤用経路である——メソッドや `mod tests` 内の関数を指すと、
    /// 終端の `}` が外側の閉じ括弧になって母集団が広く伸び、**#1108 の欠陥がそのまま再生する**。
    #[test]
    #[should_panic(expected = "列 0 から始まっていない")]
    fn top_level_fn_body_rejects_an_indented_anchor() {
        top_level_fn_body(
            "impl C {\n    fn target(&self) {\n        marker();\n    }\n}\n",
            "fn target(",
            "marker(",
        );
    }

    /// canary を含まない母集団を [`top_level_fn_body`] が**拒む**ことを固定する。
    #[test]
    #[should_panic(expected = "本体を含まない")]
    fn top_level_fn_body_rejects_a_population_without_the_canary() {
        top_level_fn_body("pub fn target() {\n}\n", "pub fn target(", "marker(");
    }

    /// **アイコンキャッシュの無効化の担い手は現在ここ 1 つだけである。** 背景再スキャンの
    /// spawn と結果適用（`RescanOutcome::Changed` を契機とした無効化）は #1001 で
    /// 撤去済み。#996 が再構築時の索引照合剪定を撤去して以来、**メモリ内
    /// `IconCacheState` と `icons.bin` の両方を無効化する**（`invalidate_icon_cache`）
    /// 直接契機はここだけに限られる——表示無効化トグル（`config_watcher`）はメモリ内のみを
    /// 破棄する別経路であり、これには数えない。**ここが落ちると、索引再構築を直接の契機と
    /// する無効化を丸ごと失う**——アイコンが古いまま FIFO 上限まで残る。
    /// 検索結果は正しいままなので挙動テストでは捕まらない。
    ///
    /// **残る死角**: 母集団は `start_index_build` のソーステキストだけであり、呼び出しグラフは
    /// 辿らない。**無効化をこの関数の外のヘルパーへ移すこと自体は、この検査が赤にする**——本体から
    /// 綴りが消えるためである（#1108 で実測）。**ただし測っているのは本体テキストへの部分文字列
    /// 一致であって呼び出しではない**——移設後も本体に `invalidate_icon_cache(` の綴りが残れば
    /// 緑のまま通る（移し先の名前がそれを含む場合だけでなく、**説明コメントへ書き残した場合も
    /// 同じである**。この関数は無効化の理由を説明する長いコメントを実際に抱えている）。
    /// 捕まらないのは、**移した先で無効化が落ちる**退行の方である。なお呼び出しの単純な削除は
    /// `-D warnings` の `dead_code` が先に捕まえる——2026-08-17 時点で `invalidate_icon_cache` を
    /// 呼ぶのはここだけだからであり、呼び出し点が増えればその二重の守りは消えてこの検査だけが残る。
    ///
    /// **母集団自身の壊れ方は [`top_level_fn_body`] が塞ぐ**（何をどう塞ぐかは同関数の doc と
    /// `top_level_fn_body_rejects_*` の回帰テストが正本。ここで数え上げない）。
    ///
    /// **この安全性は「存在形の assert」に依る**——`!contains` の否定形なら canary が真のまま
    /// 対象だけ切り捨てられて沈黙する（#1112）。
    #[test]
    fn start_index_build_invalidates_the_icon_cache() {
        let body = top_level_fn_body(
            include_str!("indexing.rs"),
            "pub fn start_index_build(",
            "try_begin_index_build(",
        );
        assert!(
            body.contains("invalidate_icon_cache("),
            "start_index_build がアイコンキャッシュを無効化していない——背景再スキャンの\
             spawn/適用は #1001 で撤去済みなので、ここが落ちると索引再構築を\
             直接の契機とする無効化を丸ごと失う"
        );
    }
}
