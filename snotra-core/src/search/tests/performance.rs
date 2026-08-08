//! パフォーマンス計測（`#[ignore]`: kana/lower_name メモリ実体・検索/構築ベンチ）。
//!
//! `cargo test -p snotra-core <name> -- --ignored --nocapture` で実行する。

use super::common::empty_history;
use crate::indexer::AppEntry;
use crate::query::{to_kana, to_lower_folded};
use crate::search::*;

fn make_bench_entries(n: usize) -> Vec<AppEntry> {
    // 実際のアプリ名に近い多様な文字列を生成する
    let prefixes = [
        "Microsoft",
        "Adobe",
        "Google",
        "Apple",
        "Mozilla",
        "Visual",
        "Windows",
        "System",
        "App",
        "Tool",
        "Launcher",
        "Manager",
        "Explorer",
        "Editor",
    ];
    let suffixes = [
        "Studio",
        "Reader",
        "Player",
        "Code",
        "Settings",
        "Control",
        "Panel",
        "Viewer",
        "Browser",
        "Assistant",
        "Helper",
        "Updater",
        "Installer",
    ];
    (0..n)
        .map(|i| {
            let name = format!(
                "{} {} {}",
                prefixes[i % prefixes.len()],
                suffixes[i % suffixes.len()],
                i
            );
            AppEntry {
                target_path: format!("C:\\Program Files\\App{}\\app{}.lnk", i, i),
                name,
                is_folder: false,
            }
        })
        .collect()
}

/// カタカナ主体のエントリ生成。migemo の主対象（かな名）で kana 実体の上界を測るため。
fn make_bench_entries_katakana(n: usize) -> Vec<AppEntry> {
    let prefixes = [
        "マイクロソフト",
        "アドビ",
        "グーグル",
        "アップル",
        "システム",
        "ツール",
        "ランチャー",
        "マネージャー",
        "エクスプローラー",
        "エディター",
    ];
    let suffixes = [
        "スタジオ",
        "リーダー",
        "プレイヤー",
        "コード",
        "セッティング",
        "コントロール",
        "ビューアー",
        "ブラウザ",
        "アシスタント",
        "ヘルパー",
    ];
    (0..n)
        .map(|i| AppEntry {
            name: format!(
                "{}{}{}",
                prefixes[i % prefixes.len()],
                suffixes[i % suffixes.len()],
                i
            ),
            target_path: format!("C:\\Program Files\\App{}\\app{}.lnk", i, i),
            is_folder: false,
        })
        .collect()
}

/// kana_lower_names（`Vec<Box<str>>`）のメモリ実体を分解計測する。
/// migemo 無効時はこの構造体ごと空になるため、ここで測る合計が削減量に相当する。
/// - ポインタ配列: `capacity * size_of::<Box<str>>()`（ファットポインタ 16B/要素）
/// - 文字列実体: 各 Box<str> のバイト長総和（要求バイト）
/// - 16B 粒度丸め: 各実体を個別ヒープ確保とみなし 16B 境界へ切り上げた実 RSS 寄りの上界
fn measure_kana_footprint(label: &str, entries: &[AppEntry]) {
    let kana: Vec<Box<str>> = entries
        .iter()
        .map(|e| to_kana(&to_lower_folded(&e.name)).into_boxed_str())
        .collect();
    let n = kana.len().max(1);
    let ptr_bytes = kana.capacity() * std::mem::size_of::<Box<str>>();
    let content_bytes: usize = kana.iter().map(|s| s.len()).sum();
    let rounded_content: usize = kana.iter().map(|s| s.len().max(1).div_ceil(16) * 16).sum();
    let transformed = entries
        .iter()
        .filter(|e| {
            let lf = to_lower_folded(&e.name);
            to_kana(&lf) != lf
        })
        .count();
    let requested = ptr_bytes + content_bytes;
    let rss_ish = ptr_bytes + rounded_content;
    let mb = |b: usize| b as f64 / (1024.0 * 1024.0);
    println!(
        "[{label}] n={n} かな変換割合={pct:.0}%\n  \
             ポインタ {ptr_bytes}B ({:.2}MB) + 実体 {content_bytes}B ({:.2}MB) \
             = 要求 {requested}B ({:.2}MB) | 16B丸め {rss_ish}B ({:.2}MB)\n  \
             1件あたり 要求 {}B / 丸め {}B",
        mb(ptr_bytes),
        mb(content_bytes),
        mb(requested),
        mb(rss_ish),
        requested / n,
        rss_ish / n,
        pct = transformed as f64 * 100.0 / n as f64,
    );
}

#[test]
#[ignore]
fn measure_kana_memory_footprint() {
    for &n in &[1_000usize, 10_000, 50_000, 100_000] {
        measure_kana_footprint(&format!("ascii    n={n}"), &make_bench_entries(n));
        measure_kana_footprint(&format!("katakana n={n}"), &make_bench_entries_katakana(n));
    }
}

// `measure_lower_name_footprint` / `..._report`（issue #336 の ROI ゲート）は撤去した。
// 役目は果たし終えている——`lower_names` の `name` との共有は実装済みで、率も削減量も
// `tests/memory_footprint.rs` の内訳が**構築後の索引そのもの**から測る。
//
// **残しておくほうが危険だった。** あの計器は `Config::default_scan_paths()`（Start Menu +
// Desktop + PATH）を「本番相当」と称しており、実運用点が `[[paths.scan]] path = 'C:\'` へ
// 移った今は製品でないものを測る。事実 #336 はその形で一致率 43.2% と読んで見送られ、
// 現運用点の実測は 86.6% だった。判定の経緯は `PERFORMANCE.md` を正本とする。

fn bench_search(label: &str, n: usize, queries: &[&str]) {
    use std::time::Instant;
    let entries = make_bench_entries(n);
    let mut engine = SearchEngine::new(entries);
    let history = empty_history();

    // ウォームアップ（rayon スレッドプールの初期化を除外）
    for q in queries {
        let _ = engine.search(q, 10, &history, SearchMode::Fuzzy);
    }

    let iters = 20usize;
    let mut total_ns = 0u128;
    for _ in 0..iters {
        for q in queries {
            let t = Instant::now();
            let _ = engine.search(q, 10, &history, SearchMode::Fuzzy);
            total_ns += t.elapsed().as_nanos();
        }
    }

    let avg_us = total_ns / (iters * queries.len()) as u128 / 1000;
    println!(
        "[{label}] entries={n}, avg={avg_us}µs ({} queries × {iters} iters)",
        queries.len()
    );
}

#[test]
#[ignore]
fn bench_fuzzy_search_scaling() {
    let queries = ["vis", "code", "micro", "app", "sett"];
    for &n in &[1_000, 10_000, 50_000, 100_000, 300_000] {
        bench_search("fuzzy", n, &queries);
    }
}

#[test]
#[ignore]
fn bench_fuzzy_path_query_scaling() {
    // パス区切り含有クエリ: has_path_sep=true でビットマスク pre-filter がスキップされる
    let queries = ["fake\\vis", "fake\\code", "fake\\micro"];
    for &n in &[1_000, 10_000, 50_000, 100_000, 300_000] {
        bench_search("fuzzy_path", n, &queries);
    }
}

fn bench_new(label: &str, n: usize) {
    use std::time::Instant;
    let entries = make_bench_entries(n);

    // rayon スレッドプール初期化を計測から除外するためのウォームアップ
    let _ = SearchEngine::new(entries.clone());

    // 構築自体にかかる時間のみを計測する。
    // entries.clone() のコストは Vec<AppEntry> の単純コピーであり、
    // new() 内の lower_fold・ビットマスク計算・normalized_keys に比べ微小。
    let iters = 5usize;
    let mut total_ns = 0u128;
    for _ in 0..iters {
        let cloned = entries.clone();
        let t = Instant::now();
        let _ = SearchEngine::new(cloned);
        total_ns += t.elapsed().as_nanos();
    }

    let avg_ms = total_ns / iters as u128 / 1_000_000;
    println!("[{label}] entries={n}, avg={avg_ms}ms ({iters} iters)");
}

#[test]
#[ignore]
fn bench_new_scaling() {
    for &n in &[1_000, 10_000, 50_000, 100_000, 300_000] {
        bench_new("new", n);
    }
}

/// migemo on/off の構築コスト差を計測する（issue #337）。
/// kana 構築（to_kana の全エントリ map）をスキップした分の差を可視化する。
/// 構築はロック外（PrebuiltIndex::new）で行われ、ロック保持時間は
/// apply_prebuilt_index の O(1) ムーブのみ（migemo 状態に依存しない）。
fn bench_new_migemo(label: &str, n: usize, migemo_enabled: bool) {
    use std::time::Instant;
    let entries = make_bench_entries_katakana(n);
    let _ = SearchEngine::new_with_migemo(entries.clone(), migemo_enabled); // warmup
    let iters = 5usize;
    let mut total_ns = 0u128;
    for _ in 0..iters {
        let cloned = entries.clone();
        let t = Instant::now();
        let _ = SearchEngine::new_with_migemo(cloned, migemo_enabled);
        total_ns += t.elapsed().as_nanos();
    }
    let avg_us = total_ns / iters as u128 / 1000;
    println!("[{label}] entries={n}, migemo={migemo_enabled}, avg={avg_us}µs ({iters} iters)");
}

#[test]
#[ignore]
fn bench_new_migemo_on_off() {
    for &n in &[1_000, 10_000, 50_000, 100_000] {
        bench_new_migemo("new_migemo_on ", n, true);
        bench_new_migemo("new_migemo_off", n, false);
    }
}

/// フルパスを `PathStore` から**原文のまま**組み直す全件コストを測る（実 `index.bin`）。
///
/// **採用済みの案の常設計器である。** ディスクの `target_path` は木表現へ移り（IndexCache v7）、
/// ロード結果は実体を持たないので、背景再スキャンの比較に使う [`crate::indexer`] の digest は
/// **組み直しながら**取っている。実測は `PERFORMANCE.md`「採用: `target_path` の木表現を
/// ディスクへ」（`digest` 11 → 17 ms）。ここはその組み直しぶんを切り出して測る。
///
/// **忠実性はここでは測らない。** 原文とのバイト一致は `search/tests/path.rs` の
/// [`super::path::path_store_raw_matches_target_path_over_real_index`] が実データ全件で持つ
/// （`#[ignore]`・原文はファイルシステムの走査から取る）。digest が混ぜるのは
/// `name` / `target_path` / `is_folder` の 3 つだけなので、`target_path` がバイト一致し
/// 他の 2 つが不変なら digest の値は**構成上**一致する。**その「構成上」を実際に検算するのは
/// `indexer` の `digest_over_tree_matches_digest_over_scanned_entries`** であり、そちらは
/// 合成 fixture ゆえ CI でも走る。ここが測るのは時間だけである。
///
/// 並列版の刻みは digest の `CHUNK` と同じ 8192 に揃えてある——別の刻みで測ると、実際に
/// 走らせる形とは違うものの数字を報告することになる。
///
/// **撤去条件**: digest がフルパスの組み直しをやめたとき（`PERFORMANCE.md`「見送った: digest を
/// 「パス」ではなく「木」に対して取る」を採る日）。そのとき組み直しは digest 経路から消えるので、
/// この計器も一緒に消す。
#[test]
#[ignore]
fn measure_raw_path_rebuild_cost_over_real_index() {
    use crate::search::path_store::PathStore;
    use rayon::prelude::*;
    use std::time::Instant;

    /// digest の畳み込みと同じ刻み。
    const CHUNK: usize = 8192;

    let Some(entries) = super::common::real_index_entries() else {
        println!("実インデックスが無いためスキップします。");
        return;
    };
    let n = entries.len();
    let store = PathStore::build(entries);
    let ranges: Vec<(usize, usize)> = (0..n)
        .step_by(CHUNK)
        .map(|s| (s, (s + CHUNK).min(n)))
        .collect();

    // rayon プールの立ち上げを計測から締め出す（`tests/memory_footprint.rs` の `//!` が記す罠と
    // 同じもの——最初に走った区間がプールの確保を被る）。
    let _: usize = ranges.par_iter().map(|&(s, e)| e - s).sum();

    let iters = 3usize;
    let (mut seq_min, mut par_min) = (u128::MAX, u128::MAX);
    for _ in 0..iters {
        // 逐次: 1 本のバッファを使い回す（エントリごとの確保をしない形）。
        let mut buf = String::new();
        let t = Instant::now();
        let mut seq_bytes = 0usize;
        for i in 0..n {
            store.raw_into(&mut buf, i);
            // 組み立てを最適化で消させないために読む。
            seq_bytes += buf.len();
        }
        seq_min = seq_min.min(t.elapsed().as_micros());

        // 並列: worker ごとに 1 本ずつ持つ（digest と同じ塊の切り方）。
        let t = Instant::now();
        let par_bytes: usize = ranges
            .par_iter()
            .map(|&(s, e)| {
                let mut buf = String::new();
                let mut bytes = 0usize;
                for i in s..e {
                    store.raw_into(&mut buf, i);
                    bytes += buf.len();
                }
                bytes
            })
            .sum();
        par_min = par_min.min(t.elapsed().as_micros());

        // 両者が同じ仕事をしたことの検算（塊の切り方で取りこぼしが出ていないか）。
        assert_eq!(seq_bytes, par_bytes, "逐次と並列で組み立てたバイト数が違う");
    }

    println!(
        "[raw_rebuild] entries={n}, 逐次 {:.1}ms / 並列 {:.1}ms（各 {iters} 回の最小値・刻み {CHUNK}）",
        seq_min as f64 / 1000.0,
        par_min as f64 / 1000.0,
    );
}
