//! バックグラウンドでのインデックス構築。
//!
//! `start_index_build` は stale マーク → CAS → spawn の順で、drain ループ（現在 config の
//! `IndexInputs` snapshot → ロック外で再構築 → swap + re-diff）を stale が消えるまで回す。
//! ビルド本体は `catch_unwind` で包み、panic 時の flag 固着（永久「構築中」）を防ぐ。
//!
//! **再構築は走査するとは限らない**（#1456）。走査が要るか（`IndexDrain::must_scan`）は engine の
//! ledger が決め、要らなければ `index.bin` を読み直して建てる（読めなければ走査する）。
//! 走査した反復だけが、所要を `scan-log.jsonl` へ記録し（[`crate::scan_log`]）、走査したフォルダ数を
//! status 行の進捗として `AppState` に置き、アイコンキャッシュを無効化する。

use std::sync::Mutex;
use std::time::Instant;

use snotra_core::engine::{DrainOutcome, IndexDrain, IndexInputs, PrebuiltIndex};
use snotra_core::indexer;
use tauri::{AppHandle, Emitter, Manager};

use crate::platform::{PlatformBridge, PlatformCommand};
use crate::scan_log;
use crate::state::AppState;

/// 再構築の要求の種類（#1456）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildRequest {
    /// 必ず走査し、ファイルシステムの最新を取り込む。
    Rescan,
    /// 索引を建て直す。走査するかは drain が決める——走査対象が変わっていれば `index.bin` の
    /// 照合に外れるので、結果として走査する。
    Rebuild,
}

/// インデックスビルドを開始（または kick）する。ビルドスレッドを起動したら `true`、
/// 既に走行中なら `false` を返す。
///
/// ビルド要求の全経路（config 変更 reindex / first-run / `index.bin` を読めなかった起動 / 手動 rebuild /
/// 自己再 kick）が通る単一入口。
/// 先に ledger へ要求を立て（`request_rescan` / `mark_index_stale`）、CAS に失敗（既に in-flight）
/// しても走行中ビルドの drain ループ / finish 後再チェックが取りこぼさず拾う（lost-update を塞ぐ、
/// issue #347/#348-A）。走査の要求も ledger が世代で持つので、同じ形で取りこぼさない（#1456）。
///
/// **finish 後に `is_index_stale` を再チェック**し、finish ウィンドウで刺さった変更を自己再 kick で拾う。
/// **unwind の panic 経路では再 kick しない**——決定論的な panic を無限にリトライしないため。
pub fn start_index_build(app: &AppHandle, request: BuildRequest) -> bool {
    let state = app.state::<AppState>();

    // ビルド要求: index を stale とマーク（CAS の前に立てる＝CAS 失敗側でも in-flight が拾える）。
    {
        let mut engine = state.engine.lock().unwrap();
        match request {
            BuildRequest::Rescan => engine.request_rescan(),
            BuildRequest::Rebuild => engine.mark_index_stale(),
        }
    }

    if !state.try_begin_index_build() {
        return false;
    }

    // アイコンキャッシュの無効化は、走査するかが決まる `drain_index` が撃つ（#1456）。

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
                    // finish ウィンドウ: complete が clear した後〜finish までに config 変更（CAS 失敗）が
                    // 刺さった場合を拾う。stale が残っていれば再 kick（CAS は finish 後なので成功する）。
                    let stale = app_handle
                        .state::<AppState>()
                        .engine
                        .lock()
                        .unwrap()
                        .is_index_stale();
                    // 再 kick は新しい要求ではない——走査の要求が残っていれば ledger が持っている。
                    if stale {
                        start_index_build(&app_handle, BuildRequest::Rebuild);
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
/// 呼び出し点は、この crate では drain ループ（`drain_index`）に閉じている**（背景再スキャンの
/// 適用は #1001 で撤去済み）。手順を書き写すと、片方だけ PATH マージを忘れる
/// 欠陥が黙って起きる——PATH のコマンドが検索から消えるが、検索結果自体は出るので
/// 気づく手段が無い（`normalize_entry_key_into` と同じ理屈）。**起動時のロード直後
/// （`main.rs` の PATH エントリのスキャン + マージ）は同じ手順を別に持つ**——そこは
/// `PrebuiltIndex` ではなく `Engine` を直接建てるため、ここを呼べない。**PATH マージの規則を
/// 変えるときはその起動経路も同じ変更で直す。**
pub(crate) fn build_index_from_material(
    mut material: indexer::IndexMaterial,
    inputs: &IndexInputs,
) -> PrebuiltIndex {
    // **ツリーとマスクは組のまま持つ**ので、片方だけ伸ばす形はここでは書けない
    // （正本は `IndexMaterial` の doc）。
    if inputs.include_path_env {
        let path_entries = indexer::scan_path_env(material.tree(), inputs.show_hidden_system);
        material.extend_with_path_entries(path_entries);
    }
    // **ここで分岐しない。** 派生データの有無で建て方が分かれるのは
    // `SearchEngine::from_material` の 1 か所だけである。
    PrebuiltIndex::from_material(material, inputs.migemo_enabled)
}

/// 走査しないで建ててよいなら、`load` で読んだ材料を返す（#1456）。**`must_scan` なら `load` を
/// 呼ばない**——`/s` と初回構築が `index.bin` を読んで済ませると、ファイルシステムの最新を
/// 取り込まないまま黙って進む。`None` なら呼び出し側は走査する。
///
/// **`bool` ではなく `&IndexDrain` を取る。** `IndexDrain` は `begin_index_drain` でしか作れない
/// （private のフィールドを持つ）ので、呼び出し側が ledger の判定を飛ばしてリテラルを渡す形が
/// 書けない。
fn material_without_scan(
    drain: &IndexDrain,
    load: impl FnOnce(&IndexInputs) -> Option<indexer::IndexMaterial>,
) -> Option<indexer::IndexMaterial> {
    if drain.must_scan() {
        return None;
    }
    load(&drain.inputs)
}

/// `index_stale` が解消されるまでドレインする:
/// begin（現在 config の `IndexInputs` スナップショット）→ ロック外で重い構築 → complete（O(1) スワップ + re-diff）。
/// ビルド中に config が変わっていれば complete が stale を残し、次の begin が再び snapshot を返して再ビルドする。
/// 各反復で engine ロックを保持するのは snapshot 取得とスワップの一瞬だけ（ロック最小化を維持）。
///
/// **反復は走査するとは限らない**（#1456）。ledger が走査を要求していなければ（`must_scan` が偽）、
/// `index.bin` を読み直して建てる。読めなければ（ファイルが無い・壊れている・走査対象が違う）
/// 走査する。
///
/// **走査する反復ごとに `scan-log.jsonl` へ 2 行を書く**（走査の前に `start`、差し替えの後に `end`。
/// 形式と「構築の入力にしない」理由は [`scan_log`] の `//!`）。あわせて status 行の進捗
/// （[`AppState::begin_scan_progress`]）を置き、アイコンキャッシュを無効化する。読み直して建てる
/// 反復はどれもしない。
fn drain_index(app_handle: &AppHandle) {
    let state = app_handle.state::<AppState>();
    loop {
        let drain = state.engine.lock().unwrap().begin_index_drain();
        let Some(drain) = drain else { break };
        let inputs = &drain.inputs;

        // **走査しない再構築**（#1456）。`include_path_env` と migemo の切り替えは `index.bin` の
        // 中身を変えないので、読み直して PATH のマージと構築をやり直せば全走査と同じ索引になる。
        if let Some(material) = material_without_scan(&drain, |i| {
            indexer::load_cached_material(&i.scan, i.show_hidden_system)
        }) {
            // アイコンキャッシュは捨てない——アイコンはパスで引き、読み直しは対象のファイルを変えない。
            let new_index = build_index_from_material(material, inputs);
            state.engine.lock().unwrap().complete_index_drain(
                new_index,
                &drain,
                DrainOutcome::Loaded,
            );
            continue;
        }

        // **記録（`scan-log.jsonl`）は構築の振る舞いを決めない**（`scan_log` の `//!`）。ここで
        // 記録から読むのは進捗表示の分母だけで、その値は `begin_scan_progress` にしか渡らない。
        // 保存先は反復ごとに 1 回だけ引き、`start` と `end` で同じ値を使う（引けなければどちらも
        // 書かない）。
        let log = snotra_core::config::Config::config_dir().map(|dir| {
            let sid = scan_log::new_sid();
            let scan_id = scan_log::scan_id(indexer::scan_identity_hash(
                &inputs.scan,
                inputs.show_hidden_system,
            ));
            let expected = scan_log::begin_in(
                &dir,
                scan_log::now_ts_ms(),
                &sid,
                &scan_id,
                inputs.scan.len(),
            );
            (dir, sid, scan_id, expected)
        });
        state.begin_scan_progress(log.as_ref().and_then(|(.., expected)| *expected));
        let total_started = Instant::now();

        // **アイコンキャッシュを捨てる（メモリ内 `IconCacheState` と `icons.bin` の両方を
        // 無効化）。** 契機は「走査した」ことであって、再構築の要求ではない（#1456）——読み直して
        // 建てる反復はアイコンの対象を変えない。#996 が索引照合の剪定を、#1001 が背景再スキャンを
        // 撤去したので、**索引の再構築を契機とする無効化の担い手はここだけである**。集合が
        // 変わったかは測り直さない。反復のたびに撃つので、続けて 2 回走査すれば 2 回捨てる
        // （1 回目の後に再抽出したアイコンも捨てる。無害だが無駄）。表示無効化時の破棄は
        // `config_watcher` の true → false のエッジの別経路である（`icon::drop_icon_cache`）。
        if let Some(icons) = app_handle.try_state::<crate::icon::IconCacheState>() {
            crate::icon::invalidate_icon_cache(&icons);
        }

        // **保存が返した派生データをそのまま索引の表現に使う**。捨てて建て直していた頃の額は `PERFORMANCE.md`「採用: `PrebuiltIndex` を `CachedMasks` 込みで建てる」。
        let (material, stats) = indexer::rebuild_and_save(
            &inputs.scan,
            inputs.show_hidden_system,
            state.scan_dirs_counter(),
        );

        // SearchEngine の構築（O(N)）は Mutex 外で実施してロック保持時間を最小化する。
        // migemo 無効時は kana_lower_names を構築しない（issue #337）。
        let build_started = Instant::now();
        let new_index = build_index_from_material(material, inputs);
        let build_took = build_started.elapsed();
        state.engine.lock().unwrap().complete_index_drain(
            new_index,
            &drain,
            DrainOutcome::Scanned { saved: stats.saved },
        );
        let total = total_started.elapsed();

        if let Some((dir, sid, scan_id, _)) = &log {
            // **名前付きのフィールドで組む**——`scanned` と `dirs` は同じ型である。
            let rec = scan_log::ScanRecord {
                scan: stats.scan,
                sort: stats.sort,
                save: stats.save,
                build: build_took,
                scanned: stats.scanned,
                dirs: stats.dirs,
            };
            scan_log::append_in(
                dir,
                &scan_log::end_line(scan_log::now_ts_ms(), sid, scan_id, &rec, total),
            );
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
    use super::{indexer, material_without_scan};

    /// トップレベル関数の本体を切り出す（**終端は列 0 の閉じ括弧**・内側のブロックは字下げされている）。
    ///
    /// 母集団は狭すぎても広すぎても壊れ、**広すぎる側は沈黙する**——探す文字列はこの `mod tests` に
    /// assert のリテラルとして実在するため、EOF まで伸びた母集団では `contains` が必ず真になる。
    /// ゆえに終端が見つかったことと `canary` が在ることの**両方**を assert する。行の走査を
    /// `str::lines` で行うのは改行コード非依存にするためである——`include_str!` が読むのは checkout
    /// された実ファイルであり、**手元（`core.autocrlf=input`）と Windows の CI（git-for-windows の
    /// system 既定 `core.autocrlf=true`）で改行が食い違う**。**原則の正本は
    /// `docs/development-principles.md`「検証の手段と、見ていないもの」**（ここに写しを置かない）。
    ///
    /// `launcher_controller/activation/tests.rs` の `method_body` とは**終端の形が違う**（あちらはメソッドゆえ
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

    /// 回帰テスト: `drain_index` は走査（`rebuild_and_save`）の**前**に `start` 行を書く
    /// （`scan_log` の `//!`「2 行 1 組」が正本）。`start` を走査の後ろへ移すと、完走しなかった
    /// 構築が記録に残らなくなる（#1014。振る舞いで測らない理由は
    /// `ADR-scan-log-start-order-by-source-text`）。
    ///
    /// **縛るのはテキストの順序であって実行順ではない。** 各行の `//` 以降は落とすので、
    /// コメントに残った綴りでは通らない。偶発的な並べ替え・本体の外への移設（綴りが 0 回になる）・
    /// 重複は赤になる。通るのは、検査を欺く書き換えである。たとえば、ブロックコメントに綴りを
    /// 残して実際の呼び出しを別名で後ろへ置く、走査の前に作ったクロージャを後で呼ぶ、到達しない
    /// 分岐に置く、がそれに当たる。
    ///
    /// **綴りが「ちょうど 1 回」であることも assert する。** 位置の比較だけだと、母集団の先頭側に
    /// 別の出現が紛れたとき、順序が壊れていても通る（`ADR-source-text-probe-predicate-polarity`）。
    /// 母集団自身の壊れ方は [`top_level_fn_body`] が塞ぐ。
    #[test]
    fn drain_index_appends_start_before_the_scan() {
        let body = top_level_fn_body(
            include_str!("indexing.rs"),
            "fn drain_index(",
            "begin_index_drain(",
        );
        // 文字列リテラルの中の `//` を下の `//` 除去が誤って切らないよう、`"` そのものを禁じる。
        // コメント中の引用符でも赤になる（誤検知は受け入れ、見逃しを避ける）。
        assert!(
            !body.contains('"'),
            "drain_index の本体に `\"` が入った（文字列リテラル、またはコメント中の引用符）——\
             下の `//` 除去の前提が崩れている"
        );
        let code = body
            .lines()
            .map(|l| l.split_once("//").map_or(l, |(code, _)| code))
            .collect::<Vec<_>>()
            .join("\n");
        let start = "scan_log::begin_in(";
        let scan = "indexer::rebuild_and_save(";
        for spelling in [start, scan] {
            assert_eq!(
                code.matches(spelling).count(),
                1,
                "drain_index のコード（コメントを除く）に {spelling} がちょうど 1 回現れない——\
                 位置の比較が意味を持たない"
            );
        }
        assert!(
            code.find(start) < code.find(scan),
            "drain_index が走査（{scan}）の後で start 行（{start}）を書いている——完走しなかった\
             構築が記録に残らない（scan_log の //!「2 行 1 組」）"
        );
    }

    /// **アイコンキャッシュの無効化の担い手は現在ここ 1 つだけである。** 背景再スキャンの
    /// spawn と結果適用（`RescanOutcome::Changed` を契機とした無効化）は #1001 で
    /// 撤去済み。#996 が再構築時の索引照合剪定を撤去して以来、**メモリ内
    /// `IconCacheState` と `icons.bin` の両方を無効化する**（`invalidate_icon_cache`）
    /// 直接契機はここだけに限られる——表示無効化トグル（`config_watcher`）はメモリ内のみを
    /// 破棄する別経路であり、これには数えない。**ここが落ちると、走査を直接の契機と
    /// する無効化を丸ごと失う**——アイコンが古いまま FIFO 上限まで残る。
    /// 検索結果は正しいままなので挙動テストでは検出されない。
    ///
    /// **契機は走査である**（#1456 でビルド要求の受理時から移した）。ゆえに走査（`rebuild_and_save`）の
    /// **前**に、`drain_index` の本体にちょうど 1 回現れることを測る。読み直して建てる分岐は
    /// 走査の手前で `continue` するので、そこでは撃たれない。
    ///
    /// **縛るのはテキストの順序であって実行順ではない**（死角は
    /// [`drain_index_appends_start_before_the_scan`] と同じ）。無効化をこの関数の外のヘルパーへ
    /// 移すことは、本体から綴りが消えるので赤になる（#1108 で実測した形）。コメントの綴りは
    /// `//` 以降を落として数えないので、説明コメントに書き残しても通らない。なお呼び出しの単純な
    /// 削除は `-D warnings` の `dead_code` も検出する——`invalidate_icon_cache` を呼ぶのはここだけ
    /// だからであり、呼び出し点が増えればその二重の守りは消えてこの検査だけが残る。
    ///
    /// **母集団自身の壊れ方は [`top_level_fn_body`] が塞ぐ**。**この安全性は「存在形の assert」に
    /// 依る**——`!contains` の否定形なら canary が真のまま対象だけ切り捨てられて沈黙する（#1112）。
    #[test]
    fn drain_index_invalidates_the_icon_cache_before_the_scan() {
        let body = top_level_fn_body(
            include_str!("indexing.rs"),
            "fn drain_index(",
            "begin_index_drain(",
        );
        let code = body
            .lines()
            .map(|l| l.split_once("//").map_or(l, |(code, _)| code))
            .collect::<Vec<_>>()
            .join("\n");
        let invalidate = "crate::icon::invalidate_icon_cache(";
        let scan = "indexer::rebuild_and_save(";
        for spelling in [invalidate, scan] {
            assert_eq!(
                code.matches(spelling).count(),
                1,
                "drain_index のコード（コメントを除く）に {spelling} がちょうど 1 回現れない"
            );
        }
        assert!(
            code.find(invalidate) < code.find(scan),
            "drain_index が走査（{scan}）の前にアイコンキャッシュを無効化していない——\
             走査を契機とする無効化を失う"
        );
    }

    /// ledger から drain を 1 つ取り出す。`rescan` なら走査を要求し、そうでなければ同期した
    /// `index.bin` を前提に設定変更の再構築を要求する。
    fn drain(rescan: bool) -> snotra_core::engine::IndexDrain {
        let mut engine = snotra_core::engine::Engine::new(
            Vec::new(),
            // 実 `history.bin` を読まない（#963）。存在しないパスを渡すと `load_in` は空で返す。
            snotra_core::history::HistoryStore::load_in(&std::env::temp_dir().join(format!(
                "snotra-indexing-test-{}-unused",
                std::process::id()
            ))),
            snotra_core::config::Config::default(),
        );
        engine.set_cache_in_sync(true);
        if rescan {
            engine.request_rescan();
        } else {
            engine.mark_index_stale();
        }
        engine.begin_index_drain().expect("stale なら drain を返す")
    }

    fn empty_material() -> Option<indexer::IndexMaterial> {
        Some(indexer::IndexMaterial::from_tree(
            snotra_core::index_tree::IndexTree::empty(),
        ))
    }

    /// `must_scan` なら `index.bin` を読まない（#1456）。読んで済ませると `/s` と初回構築が走査しない。
    #[test]
    fn material_without_scan_never_loads_when_a_scan_is_required() {
        let mut called = false;
        let got = material_without_scan(&drain(true), |_| {
            called = true;
            empty_material()
        });
        assert!(got.is_none(), "走査が要るなら材料を返さない");
        assert!(!called, "走査が要るなら index.bin を読みにいかない");
    }

    #[test]
    fn material_without_scan_loads_when_no_scan_is_required() {
        assert!(
            material_without_scan(&drain(false), |_| empty_material()).is_some(),
            "走査が要らなければ読んだ材料を使う"
        );
        assert!(
            material_without_scan(&drain(false), |_| None).is_none(),
            "読めなければ走査へ回す"
        );
    }

    /// **`drain_index` は `index.bin` を `material_without_scan` 越しにだけ読む**（#1456）。直に
    /// `load_cached_material` を呼ぶと、`must_scan` の判定を飛ばせる——`/s` と初回構築が黙って
    /// 走査しなくなる。縛るのは、コード（`//` 以降を落とした本体）に `material_without_scan(&drain,`
    /// と `load_cached_material(` がちょうど 1 回ずつ、この順に現れること（後者は前者へ渡す
    /// closure の中に在る）。死角は [`drain_index_appends_start_before_the_scan`] と同じ。
    #[test]
    fn drain_index_loads_the_cache_only_through_the_scan_check() {
        let body = top_level_fn_body(
            include_str!("indexing.rs"),
            "fn drain_index(",
            "begin_index_drain(",
        );
        let code = body
            .lines()
            .map(|l| l.split_once("//").map_or(l, |(code, _)| code))
            .collect::<Vec<_>>()
            .join("\n");
        let check = "material_without_scan(&drain,";
        let load = "load_cached_material(";
        for spelling in [check, load] {
            assert_eq!(
                code.matches(spelling).count(),
                1,
                "drain_index のコード（コメントを除く）に {spelling} がちょうど 1 回現れない"
            );
        }
        assert!(
            code.find(check) < code.find(load),
            "drain_index が {check} を通さずに index.bin を読んでいる"
        );
    }

    /// **`/s` と初回構築は `BuildRequest::Rescan` を渡す**（#1456）。`Rebuild` を渡すと、`index.bin`
    /// が同期していれば走査せずに済ませ、ファイルシステムの変化を取り込まないまま黙って進む。
    /// 結果は古いだけで正しく見えるので、挙動テストでは検出されない。
    ///
    /// 縛るのは、各ファイルのコード（`//` 以降を落とした行）で `start_index_build(` を含む行が
    /// すべて `BuildRequest::Rescan` を含むことと、その行が 1 つ以上あること（存在形の canary）。
    /// 呼び出しを複数行へ折り返されると赤になる（誤検知は受け入れる）。設定変更の経路
    /// （`config_watcher.rs`）は `Rebuild` を渡すことを同じ形で縛る。
    ///
    /// **母集団は名指しした 4 ファイルだけである。** 別のファイルへ呼び出しを足しても、この検査は
    /// それを見ない——足すときは、ここへ並べる。
    #[test]
    fn rescan_callers_request_a_rescan() {
        let cases: [(&str, &str, &str); 4] = [
            (
                "commands/system.rs",
                include_str!("commands/system.rs"),
                "BuildRequest::Rescan",
            ),
            (
                "commands/window.rs",
                include_str!("commands/window.rs"),
                "BuildRequest::Rescan",
            ),
            ("main.rs", include_str!("main.rs"), "BuildRequest::Rescan"),
            (
                "config_watcher.rs",
                include_str!("config_watcher.rs"),
                "BuildRequest::Rebuild",
            ),
        ];
        for (file, src, expected) in cases {
            let calls: Vec<&str> = src
                .lines()
                .map(|l| l.split_once("//").map_or(l, |(code, _)| code))
                .filter(|l| l.contains("start_index_build("))
                .collect();
            assert!(
                !calls.is_empty(),
                "{file} に start_index_build( の呼び出しが無い"
            );
            for line in calls {
                assert!(
                    line.contains(expected),
                    "{file} の呼び出しが {expected} を渡していない: {line}"
                );
            }
        }
    }
}
