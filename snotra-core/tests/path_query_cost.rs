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
//! 比較する 2 つは `normalized_keys` を保持するか導出するかの差である:
//!
//! - **保持**: 事前計算済みの `normalized_key` へ直接 `find`（現行）
//! - **導出**: スレッドローカルの再利用バッファへ `normalize_entry_key` で詰め直してから `find`
//!
//! 差が「`normalized_keys`（実測 35.56 MiB）を捨てる代わりに毎打鍵で払う額」である。

use std::cell::RefCell;
use std::time::Instant;

use rayon::prelude::*;

use snotra_core::config::Config;
use snotra_core::history::HistoryStore;
use snotra_core::indexer::{self, AppEntry};
use snotra_core::search::SearchEngine;

thread_local! {
    /// 導出側の再利用バッファ。**容量を再利用するため暖まった後の確保はゼロ**——
    /// ここを毎回 `String::new()` にすると、測っているものが正規化ではなく確保になる。
    static KEY_BUF: RefCell<String> = const { RefCell::new(String::new()) };
}

/// `indexer::normalize_entry_key` と**同じ規則**を、確保済みバッファへ書き出す。
///
/// 製品側にこの形の関数はまだ無い（本ハーネスは導入前の見積もりを取るためにある）。
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

/// 実 `index.bin` を実起動と同じ経路で読み、比較用に「保持していたら」の側も作る。
///
/// **`normalized_keys` は索引にもオンディスクにも既に無い**（v5 で落とした）。保持側は
/// v4 が持っていたものと同じ内容を、ここで 1 度だけ作って再現する——比較の意味は
/// 「事前計算を持つ」対「毎回導出する」であって、どこから来た文字列かではない。
fn load_real_index() -> Option<(Vec<AppEntry>, Vec<String>)> {
    let config = Config::load();
    let scan = &config.paths.scan;
    if scan.is_empty() {
        return None;
    }
    let result = indexer::load_or_scan_with_stats(scan, config.search.show_hidden_system);
    let keys: Vec<String> = result
        .entries
        .iter()
        .map(|e| indexer::normalize_entry_key(&e.target_path))
        .collect();
    Some((result.entries, keys))
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

/// 空クエリ（窓を開いた瞬間・クエリ消去時）に走る `recent_history` の実コスト。
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
    let n = result.entries.len();
    let engine = match result.cached_masks {
        Some(m) => SearchEngine::new_with_cached_masks(
            result.entries,
            m.char_masks,
            m.file_name_char_masks,
            m.lower_names,
            m.lower_file_names,
            config.search.migemo_enabled,
        ),
        None => SearchEngine::new_with_migemo(result.entries, config.search.migemo_enabled),
    };
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

#[test]
#[ignore = "計測専用。release + --nocapture で手動実行する"]
fn measure_path_query_sweep_cost() {
    let Some((entries, keys)) = load_real_index() else {
        println!("実 config に scan パスが無いため計測をスキップします。");
        return;
    };
    let n = entries.len();
    println!("\n=== パスクエリ全走査のコスト（実 index.bin・{n} 件）===");
    println!("  needle              保持 (ms)  導出:素 (ms)  導出:ASCII (ms)  ASCII 倍率   hits");

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
        println!(
            "  {needle:<20}{best_pre:>9.1}{best_plain:>13.1}{best_fast:>16.1}{:>12.2}x{hits:>7}",
            best_fast / best_pre,
        );
    }
}
