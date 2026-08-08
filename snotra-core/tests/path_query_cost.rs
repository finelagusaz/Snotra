//! パスクエリ（`has_path_sep`）の走査コスト実測ハーネス。
//!
//! パスクエリは Fuzzy ビットマスク pre-filter を**スキップする**（`snotra-core/CLAUDE.md`
//! 「モジュール構成」の search.rs 節）。ゆえに全エントリの `normalized_key` に対する
//! 部分文字列検索が毎打鍵で走る、索引規模がそのまま乗る唯一の経路である。
//! それでいて `search/tests/performance.rs` の bench 群も
//! `tests/search_frame_cost.rs` もパス区切りを含むクエリを 1 つも持っていない。
//!
//! **実 `index.bin` に対して測るのが要点である。** パス長がコストを支配し、
//! 実運用点の平均 119.3 B は合成ラダー（66.4 B）の約 2 倍ある（`PERFORMANCE.md`
//! 「索引の常駐の内訳」）。実インデックスが無い環境では自動スキップする。
//!
//! タイミング測定は環境依存ゆえ CI では回さない。手元で release 実行する（コマンドの
//! SSOT は `docs/build-commands.md`）。
//!
//! # 2 つの層を混ぜない
//!
//! - **製品レベル**: [`measure_path_query_frame_cost`] / [`measure_recent_history_cost`] は
//!   実 `index.bin` を実起動と同じ経路で読み、`Engine::search` / `SearchEngine::recent_history`
//!   をそのまま叩く。**判定に使うのはこちらである。**
//! - **走査だけを切り出した写し**: [`measure_path_query_sweep_cost`] は `normalized_key` を
//!   保持するか導出するかを比べた反復 2 の記録であり、スコアリング・履歴照合・top-k 組立が
//!   乗っていない。**見積もりが甘くなる**（実測で写し +3 ms に対し製品は +8〜16 ms）。
//!   撤去条件は当該関数の doc に書いてある。
//!
//! 反復 3 の実装前シミュレーション（`TreeIndex` 一式）は、製品に `search/path_store.rs` が
//! 入って重複したため撤去した。再構築のバイト一致を実データ全件で確かめる役目は
//! `search/tests/path.rs` の `path_store_raw_matches_target_path_over_real_index` /
//! `path_store_cursor_matches_normalize_entry_key_over_real_index` が**製品の `PathStore` に
//! 対して**引き継いでいる。

use std::cell::RefCell;
use std::time::Instant;

use rayon::prelude::*;

use snotra_core::config::Config;
use snotra_core::engine::Engine;
use snotra_core::history::HistoryStore;
use snotra_core::indexer::{self, AppEntry};
use snotra_core::search::SearchEngine;

thread_local! {
    /// 導出側の再利用バッファ。**容量を再利用するため暖まった後の確保はゼロ**——
    /// ここを毎回 `String::new()` にすると、測っているものが正規化ではなく確保になる。
    static KEY_BUF: RefCell<String> = const { RefCell::new(String::new()) };
}

/// `indexer::normalize_entry_key` と**同じ規則**を、1 文字ずつ書き出す形で再現する。
///
/// ASCII 高速路が入る**前**の形であり、製品にこの形はもう無い（比較の基準として残している）。
/// 規則がずれたら測っているものが別物になるので、`normalize_entry_key` を変更したら
/// ここも同時に直す——現物との一致は `derives_same_bytes_as_normalize_entry_key` が固定する。
fn normalize_into(buf: &mut String, path: &str) {
    buf.clear();
    let trimmed = path.trim();
    buf.reserve(trimmed.len());
    for ch in trimmed.chars() {
        if ch == '/' {
            buf.push('\\');
        } else {
            buf.extend(ch.to_lowercase());
        }
    }
}

/// **出荷する実装そのもの**（`indexer::normalize_entry_key_into`）。写しではない。
/// 判定の根拠になる数字は、測る対象を製品と同一にしてから採る。
fn normalize_into_ascii_fast(buf: &mut String, path: &str) {
    indexer::normalize_entry_key_into(buf, path);
}

/// 上の写しが現物と 1 バイトも違わないことを、実インデックスの全パスで固定する。
/// **ここがずれると以降の測定値は別物の計測になる。**
#[test]
fn derives_same_bytes_as_normalize_entry_key() {
    let Some((entries, _)) = load_real_index() else {
        println!("実インデックスが無いためスキップします。");
        return;
    };
    let mut buf = String::new();
    let mut fast = String::new();
    let mut non_ascii = 0usize;
    for entry in &entries {
        let expected = indexer::normalize_entry_key(&entry.target_path);
        normalize_into(&mut buf, &entry.target_path);
        normalize_into_ascii_fast(&mut fast, &entry.target_path);
        assert_eq!(
            buf, expected,
            "写しが現物とずれている: {}",
            entry.target_path
        );
        assert_eq!(
            fast, expected,
            "ASCII 高速路が現物とずれている: {}",
            entry.target_path
        );
        if !entry.target_path.trim().is_ascii() {
            non_ascii += 1;
        }
    }
    println!(
        "{} 件のパスで写し 2 種と現物が一致しました（非 ASCII を含むパス: {non_ascii} 件・{:.1}%）。",
        entries.len(),
        non_ascii as f64 * 100.0 / entries.len() as f64
    );
}

/// 実 `index.bin` に**載っているときだけ**読み、比較用に「保持していたら」の側も作る。
///
/// **`normalized_keys` は索引にもオンディスクにも既に無い**（v5 で落とした）。保持側は
/// v4 が持っていたものと同じ内容を、ここで 1 度だけ作って再現する——比較の意味は
/// 「事前計算を持つ」対「毎回導出する」であって、どこから来た文字列かではない。
///
/// 走査しない入口（`indexer::load_cached_entries`）を通すのは必須である。
/// [`derives_same_bytes_as_normalize_entry_key`] は `#[ignore]` を持たず既定のスイートで走るため、
/// `load_or_scan_with_stats` だと cold cache の `cargo test` が全走査と `index.bin` の書き込みを
/// 起こす（理由の全文は当該入口の doc）。
fn load_real_index() -> Option<(Vec<AppEntry>, Vec<String>)> {
    let config = Config::load();
    let scan = &config.paths.scan;
    if scan.is_empty() {
        return None;
    }
    let entries = indexer::load_cached_entries(scan, config.search.show_hidden_system)?;
    let keys: Vec<String> = entries
        .iter()
        .map(|e| indexer::normalize_entry_key(&e.target_path))
        .collect();
    Some((entries, keys))
}

/// 全件走査 1 回の壁時計を返す（`find` の結果は数え上げて捨てない）。
///
/// **`par_iter` で測るのが要点である。** 製品の候補走査は rayon 並列（`search.rs` の
/// `into_par_iter`）ゆえ、単スレッドで測ると 1 打鍵あたりの絶対値をコア数ぶん過大に見る。
/// 倍率は単スレッドでも同じだが、採否を決めるのは絶対値のほうである。
fn sweep_prebuilt(keys: &[String], needle: &str) -> (f64, usize) {
    let start = Instant::now();
    let hits = keys.par_iter().filter(|k| k.contains(needle)).count();
    (start.elapsed().as_secs_f64() * 1000.0, hits)
}

/// 導出側の全件走査。`normalize` に写しの実装を差し替えて 2 変種を同条件で測る。
fn sweep_derived(
    entries: &[AppEntry],
    needle: &str,
    normalize: fn(&mut String, &str),
) -> (f64, usize) {
    let start = Instant::now();
    // バッファは rayon の worker ごとに 1 本（`search/scoring.rs` の thread-local `MATCHER` と
    // 同じ形）。worker 数ぶんしか確保されないので、常駐への寄与は無視できる。
    let hits = entries
        .par_iter()
        .filter(|e| {
            KEY_BUF.with(|cell| {
                let mut buf = cell.borrow_mut();
                normalize(&mut buf, &e.target_path);
                buf.contains(needle)
            })
        })
        .count();
    (start.elapsed().as_secs_f64() * 1000.0, hits)
}

/// パス区切りを含むクエリの**製品レベル**フレームコスト（実 `index.bin` × `Engine::search`）。
///
/// **既存の計器はどれもここを測れない。** `tests/search_frame_cost.rs` は合成索引であり
/// パス区切りクエリを 1 本も持たない（しかもパス長 66.4 B・浅い一様木で、実運用の
/// 119.3 B・深さ平均 6.05 段とは別物）。`measure_path_query_sweep_cost` は走査だけを
/// 切り出した写しであって、`Engine::search`（config 実効 limit・履歴ブースト・top-k 組立込み）
/// ではない。
///
/// **`has_path_sep` は incremental cache を無条件で無効化する**（`IncrementalCache::can_reuse`）。
/// ゆえにパスを打っている間は毎打鍵が全件走査になり、ここが UI スレッドで払う額そのものになる。
/// クエリは「パスを 1 文字ずつ打っていく」形に並べる——区切りを打った瞬間から全件走査へ
/// 切り替わるので、その前後を同じ表で見られるようにする。
#[test]
#[ignore = "計測専用。release + --nocapture で手動実行する"]
fn measure_path_query_frame_cost() {
    let config = Config::load();
    if config.paths.scan.is_empty() {
        println!("実 config に scan パスが無いため計測をスキップします。");
        return;
    }
    let result =
        indexer::load_or_scan_with_stats(&config.paths.scan, config.search.show_hidden_system);
    let n = result.tree.len();
    let history = HistoryStore::load();
    let mut engine = match result.cached_masks {
        Some(masks) => Engine::new_from_cache(result.tree, masks, history, config),
        // **木を `Vec<AppEntry>` へ戻さない。** `Engine::new` を通すと実体化と木の再構築で
        // 同じ木を 2 度建てることになる（`Engine::new_from_tree` の doc）。
        None => Engine::new_from_tree(result.tree, history, config),
    };

    println!("\n=== パスクエリのフレームコスト（実 index.bin・{n} 件・Engine::search）===");
    println!("  query                  results     min      p50      max");
    for query in [
        "users",       // 区切り無し = bitmask pre-filter が効く（比較の基準）
        "c:\\",        // 区切りを打った瞬間から全件走査
        "c:\\users",   //
        "c:\\users\\", //
        "\\program files\\",
        "\\zzz-no-such-path\\",
    ] {
        // 2 回の暖機のあと 20 回。min/p50/max を出す——**平均は出さない**。1 フレームを
        // 落とすかどうかを決めるのは中央値と最悪値であって平均ではない。
        for _ in 0..2 {
            let _ = engine.search(query);
        }
        let mut samples_us = Vec::with_capacity(20);
        let mut results = 0usize;
        for _ in 0..20 {
            let started = Instant::now();
            let out = engine.search(query);
            samples_us.push(started.elapsed().as_micros() as u64);
            results = out.len();
        }
        samples_us.sort_unstable();
        println!(
            "  {query:<22}{results:>7}{:>8}{:>9}{:>9}",
            samples_us[0],
            samples_us[samples_us.len() / 2],
            samples_us[samples_us.len() - 1],
        );
    }
    println!("  （µs。60fps の 1 フレームは 16,700 µs）");
}

/// `recent_history` の実コスト。**窓を開いた瞬間・クエリ消去時には走らない**（頻度と呼び出し元の正本は `SearchEngine::recent_history` の doc）。**この計測が測るのは「呼ばれたときの 1 回」である。**
///
/// `recent_launches` が返すのは高々 `recent_limit` 件（既定 8）だが、現行の実装は照合表を
/// **全エントリぶん**組み立てる。`normalized_keys` を廃止するなら、この経路も
/// 走査時導出へ移ることになるため、先に現状を測る。
#[test]
#[ignore = "計測専用。release + --nocapture で手動実行する"]
fn measure_recent_history_cost() {
    let config = Config::load();
    if config.paths.scan.is_empty() {
        println!("実 config に scan パスが無いため計測をスキップします。");
        return;
    }
    let result =
        indexer::load_or_scan_with_stats(&config.paths.scan, config.search.show_hidden_system);
    let n = result.tree.len();
    let engine = match result.cached_masks {
        Some(m) => {
            SearchEngine::new_with_cached_masks(result.tree, m, config.search.migemo_enabled)
        }
        None => SearchEngine::new_from_tree(result.tree, config.search.migemo_enabled),
    };
    // **ここは実 `history.bin` を読むのが正しい**（#963 でユニットテスト側の fixture は
    // 空へ移したが、計測ハーネスは実運用の姿を測るのが目的である）。統合テストは
    // 別クレートゆえ `HistoryStore::empty()`（`#[cfg(test)]`）に手も届かない。
    let history = HistoryStore::load();
    let limit = config.search.recent_limit.unwrap_or(8);

    println!("\n=== 空クエリの履歴候補（recent_history）のコスト（実 index.bin・{n} 件）===");
    let mut best = f64::MAX;
    let mut hits = 0usize;
    for _ in 0..5 {
        let start = Instant::now();
        let out = engine.recent_history(&history, limit);
        best = best.min(start.elapsed().as_secs_f64() * 1000.0);
        hits = out.len();
    }
    println!("  recent_limit = {limit}, 返した件数 = {hits}, 最小 {best:.1} ms");
    println!("  = 窓を開くたび・クエリを消すたびに払う額（{n} 件ぶんの照合表を毎回組む現行の形）");
}

/// `normalized_key` を保持するか導出するかを比べた**反復 2 の記録**（走査だけを切り出した写し）。
///
/// **判定にそのまま使わないこと。** ここにはスコアリングも履歴照合も top-k 組立も乗っておらず、
/// 同じ再構築の上にそれらが乗る製品では増分が数倍になる（実測: 写し +3 ms に対し製品 +8〜16 ms）。
/// 製品レベルの相当物は [`measure_path_query_frame_cost`] である。
///
/// **撤去条件**: 3 列（保持 / 導出:素 / 導出:ASCII）はどれも現在の製品の形ではない
/// ——`normalized_keys` は v5 で無くなり、走査は `search/path_store.rs` の `PathCursor` からの
/// 組み立てに移った。`PERFORMANCE.md`「パスクエリ全走査のコスト」が反復 2 の判定記録として
/// 参照されるあいだだけ残す。その節を履歴へ落とすとき、本関数と `sweep_prebuilt` /
/// `sweep_derived` / `normalize_into` / `load_real_index` の `keys` 側を一緒に消す。
#[test]
#[ignore = "計測専用。release + --nocapture で手動実行する"]
fn measure_path_query_sweep_cost() {
    let Some((entries, keys)) = load_real_index() else {
        println!("実 config に scan パスが無いため計測をスキップします。");
        return;
    };
    let n = entries.len();
    println!("\n=== パスクエリ全走査のコスト（実 index.bin・{n} 件）===");
    println!("  needle              保持 (ms)  導出:素 (ms)  導出:ASCII (ms)   hits");

    // 一致数が 0 のもの・多いものを混ぜる。`find` は一致で打ち切るため、
    // **一致が多いほど走査は速く終わる**——一致数を併記しないと倍率を読み違える。
    for needle in [
        "\\workspace\\",
        "\\users\\",
        "\\node_modules\\",
        "\\zzz-no-such-path\\",
    ] {
        // 各 3 回の最小値を採る（外れ値は他プロセスの割り込みで上振れする側にしか出ない）。
        let mut best_pre = f64::MAX;
        let mut best_plain = f64::MAX;
        let mut best_fast = f64::MAX;
        let mut hits = 0usize;
        for _ in 0..3 {
            let (ms, h) = sweep_prebuilt(&keys, needle);
            best_pre = best_pre.min(ms);
            hits = h;
            let (ms, h2) = sweep_derived(&entries, needle, normalize_into);
            best_plain = best_plain.min(ms);
            assert_eq!(h, h2, "保持と導出:素 で一致数が違う（写しがずれている）");
            let (ms, h3) = sweep_derived(&entries, needle, normalize_into_ascii_fast);
            best_fast = best_fast.min(ms);
            assert_eq!(h, h3, "保持と導出:ASCII で一致数が違う（写しがずれている）");
        }
        println!("  {needle:<20}{best_pre:>9.1}{best_plain:>13.1}{best_fast:>16.1}{hits:>7}");
    }
}
