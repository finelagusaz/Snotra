//! パフォーマンス計測（`#[ignore]`: kana/lower_name メモリ実体・検索/構築ベンチ）。
//!
//! `cargo test -p snotra-core <name> -- --ignored --nocapture` で実行する。

use super::common::empty_history;
use crate::indexer::AppEntry;
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

// `measure_kana_footprint` / `measure_kana_memory_footprint`（issue #337 の ROI ゲート）は
// 撤去した（#1056）。**製品が捨てた表現を測る計器になっていたためである**——あれは
// `entries` から自前で `Vec<Box<str>>` を組んで「kana_lower_names のメモリ実体」と名乗って
// いたが、`kana_lower_names` は `NameArena` になり、あの物体は索引のどこにも存在しない。
// `snotra-core/CLAUDE.md` の footprint 節が明文で禁じている型（「構築前の `Vec<AppEntry>` を
// 走査して代用しない——その走査は存在しない物体を測る」）の再発であり、`#[ignore]` ゆえ
// CI は黙る。答えていた問い（migemo の限界費用）は `tests/memory_footprint.rs` の Phase B
// ラダー（on − off）が現に答えており、`PERFORMANCE.md` の表もその数字で書かれている。

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

/// migemo on/off の構築コスト差を計測する（issue #337）。kana 構築（to_kana の全エントリ map）をスキップした分の差を可視化する。構築はロック外（`PrebuiltIndex`）で行われ、ロック保持時間は `apply_prebuilt_index` の O(1) ムーブのみ（migemo 状態に依存しない）。**コンストラクタを名指ししない**——製品が通るのは `from_material` の 1 つであり、`PrebuiltIndex::new` は `#[cfg(test)]` ゆえ製品から呼べない（同関数の doc）。**この一文は 2 度腐った**（`new` → `from_cache`/`from_tree` → `from_material`）ので、次に触るときは名前を書かず「入口は 1 つ」だけを残すことを検討する。
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
