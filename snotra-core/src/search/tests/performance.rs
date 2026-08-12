//! パフォーマンス計測（`#[ignore]`: kana/lower_name メモリ実体・検索/構築ベンチ）。
//!
//! `cargo test -p snotra-core <name> -- --ignored --nocapture` で実行する。

use super::common::empty_history;
// `NO_PARENT` / `TreeNodes` は #1059 の spike（本ファイル末尾）だけが使う。撤去時に一緒に消す。
use crate::index_tree::{NO_PARENT, TreeNodes};
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

// ---------------------------------------------------------------------------
// issue #1059 の spike（**足場**）
//
// **撤去条件**: 本 issue の判定を記録した PR がマージされたら、下の 5 つ
// （`kmp_failure` / `advance_over` / `forward_pass` / `parallel_sweep` / `sequential_sweep` と
// 計測関数 `spike_forward_pass_vs_parallel_sweep_over_real_index`）をまとめて消す。
// issue 番号を撤去の合図にしない——閉じるのが撤去 PR 自身のとき自己参照して発火しない。
//
// 測るのは「逐次前向き 1 パス（KMP 状態の親→子伝播）が、現行の並列走査（正規化キーの
// 組み立て + `find`）に単価で勝てるか」の 1 点だけである。**製品コードには 1 行も入れない。**
// ---------------------------------------------------------------------------

/// KMP の失敗関数。`pat` は正規化済み（ASCII 小文字・区切りは `\`）のバイト列。
fn kmp_failure(pat: &[u8]) -> Vec<u32> {
    let mut fail = vec![0u32; pat.len()];
    let mut k = 0usize;
    for i in 1..pat.len() {
        while k > 0 && pat[i] != pat[k] {
            k = fail[k - 1] as usize;
        }
        if pat[i] == pat[k] {
            k += 1;
        }
        fail[i] = k as u32;
    }
    fail
}

/// 1 セグメントぶんを食わせて照合状態を進める。到達したら `matched` を立て、
/// 失敗関数で状態を戻して**走査を続ける**（子孫のために状態が要る）。
///
/// 正規化はここで当てる（`/` → `\` と ASCII 小文字化）。**非 ASCII は素通しする**——
/// spike が測るのは単価の**下限**であり、`char::to_lowercase()` を通せば現行側と同じ費用が
/// 乗って比較の切れ味が鈍る。下限で負けるなら正しく作っても負ける。実データの非 ASCII は
/// 1.7%（`PERFORMANCE.md`）で、件数の突き合わせにわずかな差として現れうる。
#[inline]
fn advance_over(bytes: &[u8], pat: &[u8], fail: &[u32], state: &mut usize, matched: &mut bool) {
    for &raw in bytes {
        let b = if raw == b'/' {
            b'\\'
        } else {
            raw.to_ascii_lowercase()
        };
        while *state > 0 && b != pat[*state] {
            *state = fail[*state - 1] as usize;
        }
        if b == pat[*state] {
            *state += 1;
            if *state == pat.len() {
                *matched = true;
                *state = fail[*state - 1] as usize;
            }
        }
    }
}

/// B 側: 添字昇順の前向き 1 パス。`parent < i` ゆえ親は必ず先に確定している。
///
/// `matched[i] = matched[parent[i]] || (i のセグメントで末尾へ到達)`——親のパスは自分のパスの
/// 接頭辞なので、親が一致していれば自分も一致する。
///
/// **マッチ不能な部分木を飛ばす最適化は入れない**（issue #1059 の未確定 2）。部分文字列検索
/// ゆえ親で状態が 0 に戻っても子孫は自分のセグメントから一致を始められ、飛ばせない。
fn forward_pass(
    store: &PathStore,
    pat: &[u8],
    fail: &[u32],
    state: &mut [u32],
    matched: &mut [bool],
) -> usize {
    let mut hits = 0usize;
    for i in 0..store.len() {
        let p = store.parent_of(i);
        let (mut st, mut m) = if p == NO_PARENT {
            (0usize, false)
        } else {
            (state[p as usize] as usize, matched[p as usize])
        };
        if p == NO_PARENT {
            // 根はフルパスが `table` に居る（`CompactEntry::aux` の doc）。
            let full = store.table_str(store.aux_of(i));
            advance_over(full.trim().as_bytes(), pat, fail, &mut st, &mut m);
        } else {
            // 非根は「区切り + 表示名 + 拡張子」。`PathCursor::append` と同じ 3 つ。
            advance_over(b"\\", pat, fail, &mut st, &mut m);
            advance_over(store.name_of(i).as_bytes(), pat, fail, &mut st, &mut m);
            advance_over(
                store.table_str(store.aux_of(i)).as_bytes(),
                pat,
                fail,
                &mut st,
                &mut m,
            );
        }
        state[i] = st as u32;
        matched[i] = m;
        if m {
            hits += 1;
        }
    }
    hits
}

/// A 側: 現行の形。正規化キーを組み立てて `find` する走査を rayon 並列で回す。
///
/// **`with_normalized_key` を通すのが要点である**——製品と同じ組み立て経路（thread-local
/// `CURSOR` の鎖の持ち回り）に乗せないと、A 側が実際より高く出て B の勝ちに見える。
fn parallel_sweep(store: &PathStore, pq: &str) -> usize {
    use rayon::prelude::*;
    (0..store.len())
        .into_par_iter()
        .filter(|&i| crate::search::scoring::with_normalized_key(store, i, |key| key.contains(pq)))
        .count()
}

/// A 側を**単スレッドで**回す変種。B との比較にだけ使う。
///
/// **これが無いと機序が決まらない。** 並列 A に B が負けても、それが「単価を削れなかった」
/// せいなのか「単価は削れたが並列度 16 → 1 の転落に負けた」せいなのかは区別できない。
/// 逐次同士を並べれば単価の比がそのまま出る。
fn sequential_sweep(store: &PathStore, pq: &str) -> usize {
    (0..store.len())
        .filter(|&i| crate::search::scoring::with_normalized_key(store, i, |key| key.contains(pq)))
        .count()
}

/// #1059 の判定: 前向き 1 パス（逐次）が現行走査（並列 16）に勝てるか。
///
/// **A/B を同一プロセス・同一セッションで並べる**（別実行の数字を突き合わせない）。
/// 判定基準は 2 つあり、両方を満たさないと採用にならない——`workspace/plan.md`
/// 「判定基準」を正本とする（0 件クエリでの単価と、全件マッチ側で `c:\` の p50 余裕
/// 約 1.2 ms に収まるか）。
#[test]
#[ignore = "計測専用。release + --nocapture で手動実行する"]
fn spike_forward_pass_vs_parallel_sweep_over_real_index() {
    use std::time::Instant;

    let Some(entries) = super::common::real_index_entries() else {
        println!("実 index.bin が無いためスキップします。");
        return;
    };
    // migemo は kana 列を建てるだけで PathStore に影響しない。spike の対象外ゆえ切る。
    let engine = SearchEngine::new_with_migemo(entries, false);
    let store = &engine.entries;
    let n = store.len();

    println!("\n=== #1059 spike: 前向き 1 パス（逐次） vs 現行走査 / {n} 件 ===");
    println!(
        "  query                 A並列 min/p50   A逐次 min/p50    B:1パス min/p50   hits(A/B)"
    );
    for pq in ["\\zzz-no-such-path\\", "c:\\users\\"] {
        let pat = pq.as_bytes();
        let fail = kmp_failure(pat);
        let mut state = vec![0u32; n];
        let mut matched = vec![false; n];

        let _ = parallel_sweep(store, pq);
        let _ = sequential_sweep(store, pq);
        let _ = forward_pass(store, pat, &fail, &mut state, &mut matched);

        let (mut par_us, mut seq_us, mut b_us) = (Vec::new(), Vec::new(), Vec::new());
        let (mut a_hits, mut b_hits) = (0usize, 0usize);
        for _ in 0..3 {
            let t = Instant::now();
            a_hits = parallel_sweep(store, pq);
            par_us.push(t.elapsed().as_micros() as u64);

            let t = Instant::now();
            let seq_hits = sequential_sweep(store, pq);
            seq_us.push(t.elapsed().as_micros() as u64);
            assert_eq!(
                seq_hits, a_hits,
                "並列と逐次で件数が違う（A 側の写しがずれている）"
            );

            let t = Instant::now();
            b_hits = forward_pass(store, pat, &fail, &mut state, &mut matched);
            b_us.push(t.elapsed().as_micros() as u64);
        }
        par_us.sort_unstable();
        seq_us.sort_unstable();
        b_us.sort_unstable();
        println!(
            "  {pq:<20}{:>7}/{:<8}{:>7}/{:<9}{:>7}/{:<9}{a_hits:>7}/{b_hits}",
            par_us[0], par_us[1], seq_us[0], seq_us[1], b_us[0], b_us[1],
        );
    }
    println!("  （µs。3 回の min と中央値。A並列は 16 コア・A逐次と B は 1 コア）");
    println!("  A逐次 ÷ B = 単価の削減倍率 / A逐次 ÷ A並列 = 並列度の利き");
}
