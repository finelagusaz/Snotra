//! 検索 1 回あたりの一時確保を数える検知器と計器（打鍵ごとに払う確保であって常駐ではない）。
//!
//! **挙動テストはこの性質を原理的に見ない。** 候補ごとの確保を戻しても検索結果は 1 件も
//! 変わらず、退行は「遅い」としてしか現れない。ゆえに検知器は時間ではなく**確保の回数と
//! バイト数**を数える——どちらも入力だけで決まるので共有ランナーでも揺れず、`#[ignore]` を
//! 付けずに CI で走らせられる。
//!
//! **数えるのは「このスレッドで起きた確保」だけである。** 計数器はスレッドローカルで、検索は
//! worker 1 本の rayon プールの中で走らせる（[`on_single_worker`]）。fold も reduce もその
//! worker が担うので検索の確保は全部そこへ計上され、libtest が並行に走らせる他のテストの確保は
//! 混ざらない。**1 本で数えてよいのは、候補ごとの確保がスレッド数に依らないからである。**
//! 分割の数に比例する確保（fold ごとの top-k と一致の収集）は規模とスレッド数の両方で変わるが、
//! 候補 1 件ごとの確保とは桁が違う——検知器の閾値はその間に置いてある。
//!
//! **壁時計は測らない。** 計数器は確保のたびに働くので、確保の多い側ほど遅く見せる。
//! A/B の壁時計は既定のアロケータで走るベンチ（`bench_fuzzy_search_scaling` 等）が測る。
//!
//! **手元の計器（`measure_search_allocations`）は実 `index.bin` を書き換えうる。**
//! `load_or_scan_with_stats` は旧版を読めば現行版へ昇格し、キャッシュが外れれば走査して
//! 書き出す（契約は同関数の doc）。CI で走る検知器は実データに触れない。
//!
//! 撤去条件を持たない恒久の検知器である（足場ではない）。実測値は
//! `PERFORMANCE.md`「採用: 検索の候補ごとの確保をやめる」が正本。

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::Instant;

use snotra_core::config::Config;
use snotra_core::engine::Engine;
use snotra_core::history::HistoryStore;
use snotra_core::indexer::{self, AppEntry};
use snotra_core::search::{SearchEngine, SearchMode};
use snotra_core::ui_types::SearchResult;

// ---------------------------------------------------------------------------
// 計数アロケータ（スレッドローカル）
// ---------------------------------------------------------------------------

thread_local! {
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
    static BYTES: Cell<usize> = const { Cell::new(0) };
}

/// `System` を包み、このスレッドの確保回数と確保バイト数を数える。解放は数えない——問いは
/// 「1 回の検索がアロケータを何回呼ぶか」である。
///
/// **`alloc_zeroed` / `realloc` は実装しない**——`GlobalAlloc` の既定実装がどちらも `alloc` を
/// 1 回通る（`realloc` は新しい大きさで）ので、数える量は変わらない。
struct Counting;

impl Counting {
    fn on_alloc(size: usize) {
        // `try_with`: スレッドの終了処理（TLS 破棄後）に走る確保でも panic させない。
        let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
        let _ = BYTES.try_with(|c| c.set(c.get() + size));
    }
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        Self::on_alloc(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// `f` の実行中にこのスレッドで起きた確保の（回数, バイト数）と、`f` の戻り値。
fn allocations_during<R>(f: impl FnOnce() -> R) -> (usize, usize, R) {
    let (allocs, bytes) = (ALLOCS.with(Cell::get), BYTES.with(Cell::get));
    let out = f();
    (
        ALLOCS.with(Cell::get) - allocs,
        BYTES.with(Cell::get) - bytes,
        out,
    )
}

/// worker 1 本の rayon プールの中で `f` を走らせる（理由は `//!`）。
fn on_single_worker<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .expect("rayon の 1 スレッドプールを作れない")
        .install(f)
}

/// 空の履歴。存在しないパスを渡すと `load_in` は空で返す（実 `history.bin` を読まない）。
fn empty_history() -> HistoryStore {
    HistoryStore::load_in(&std::env::temp_dir().join(format!(
        "snotra-search-allocations-{}-unused",
        std::process::id()
    )))
}

// ---------------------------------------------------------------------------
// 検知器（CI で走る）
// ---------------------------------------------------------------------------

const SMALL: usize = 1_000;
const LARGE: usize = 4_000;
const MAX_RESULTS: usize = 10;

/// incremental の再利用を外すためのクエリ。測るクエリの接頭辞にならず、fixture の名前に
/// 含まれない文字だけで書く（再利用は前回クエリの単調拡張に限られ、前回候補が空なら起きない）。
const RESET_QUERY: &str = "zzzz";

/// 全エントリが bitmask pre-filter を通り、Fuzzy でも一致する索引。
/// `file_stem` はファイル名の拡張子より前で、表示名とは別の文字列にしてある
/// （`lower_file_name` が `lower_name` と共有されると file name 側の照合が見えなくなる）。
fn engine_where_every_entry_matches(
    n: usize,
    name: fn(usize) -> String,
    file_stem: fn(usize) -> String,
) -> SearchEngine {
    let entries = (0..n)
        .map(|i| AppEntry {
            name: name(i),
            target_path: format!("C:\\apps\\{}.exe", file_stem(i)),
            is_folder: false,
        })
        .collect();
    SearchEngine::new(entries)
}

/// 全件走査になる検索 1 回ぶんの確保（回数, バイト数）。
///
/// 1 回目は暖機である——スレッドローカルの照合器・組み立てバッファは最初の検索で立ち上がり、
/// その確保は打鍵ごとの費用ではない。続く [`RESET_QUERY`] が incremental の再利用を外す。
fn full_scan_allocations(
    engine: &mut SearchEngine,
    query: &str,
    history: &HistoryStore,
) -> (usize, usize) {
    let _ = engine.search(query, MAX_RESULTS, history, SearchMode::Fuzzy);
    let _ = engine.search(RESET_QUERY, MAX_RESULTS, history, SearchMode::Fuzzy);
    let (allocs, bytes, results) =
        allocations_during(|| engine.search(query, MAX_RESULTS, history, SearchMode::Fuzzy));
    // 全件が一致する fixture ゆえ結果は上限まで埋まる。埋まらないなら照合経路を通っていない
    // ——その場合の確保の少なさは「確保しなかった」ではなく「測っていない」である。
    assert_eq!(
        results.len(),
        MAX_RESULTS,
        "query={query:?} が全件に一致していない（検知器の前提が崩れている）"
    );
    (allocs, bytes)
}

struct Case {
    label: &'static str,
    query: &'static str,
    name: fn(usize) -> String,
    file_stem: fn(usize) -> String,
}

/// 照合器へ渡す haystack の作り方が違う fixture（何を通すかは各 `Case` のコメント）。
const FUZZY_CASES: &[Case] = &[
    Case {
        // ASCII の名前は UTF-32 へ展開せず、借用のまま照合器へ渡る側の経路を通る。
        label: "ASCII の表示名",
        query: "alpha",
        name: |i| format!("alpha tool {i:05}"),
        file_stem: |i| format!("bin{i:05}"),
    },
    Case {
        // 表示名に `.` が無いので name の照合は不成立になり、file name の照合まで進む。
        label: "`.` を含むクエリ（file name の照合）",
        query: "beta.exe",
        name: |i| format!("beta tool {i:05}"),
        file_stem: |i| format!("beta{i:05}"),
    },
    Case {
        // 非 ASCII の名前は bitmask が全ビットを立てるので必ず照合まで進み、UTF-32 への
        // 展開（書記素ごとの先頭の文字）を要する側の経路を通る。
        label: "非 ASCII の表示名",
        query: "アルファ",
        name: |i| format!("アルファ{i:05}"),
        file_stem: |i| format!("bin{i:05}"),
    },
];

/// Fuzzy の全件走査で、照合へ進んだ候補 1 件ごとに確保しない。
///
/// 回帰テスト: `score_one_entry` は照合器へ渡す haystack を候補ごとに `Utf32String` で
/// 確保していた（ASCII でも複製・非 ASCII は `Vec<char>`）。確保の数が規模に比例して増えれば
/// 赤になる——閾値は「候補 10 件につき 1 回」で、比例する形（候補 1 件につき 1 回以上）とは
/// 1 桁離してある。
#[test]
fn fuzzy_full_scan_does_not_allocate_per_candidate() {
    on_single_worker(|| {
        let history = empty_history();
        for case in FUZZY_CASES {
            let mut small = engine_where_every_entry_matches(SMALL, case.name, case.file_stem);
            let mut large = engine_where_every_entry_matches(LARGE, case.name, case.file_stem);
            let (small_allocs, _) = full_scan_allocations(&mut small, case.query, &history);
            let (large_allocs, _) = full_scan_allocations(&mut large, case.query, &history);
            let grew = large_allocs.saturating_sub(small_allocs);
            assert!(
                grew < (LARGE - SMALL) / 10,
                "{}: 候補 {} 件の増加に対して確保が {grew} 回増えた（{small_allocs} → {large_allocs}）",
                case.label,
                LARGE - SMALL,
            );
        }
    });
}

/// パスクエリの全件走査で、エントリ数に比例するバイト数を確保しない。
///
/// 回帰テスト: `search_with_options` は incremental を再利用しない検索のたびに
/// 全エントリの添字を `Vec<usize>` へ集めてから走査していた。パスクエリは前回候補を
/// 再利用しないので（`IncrementalCache::caches_candidates`）、打鍵のたびにこの形になる。
/// 確保の**回数**は 1 回のままなので回数では捕まらず、バイト数で見る。
#[test]
fn path_query_full_scan_does_not_allocate_per_entry() {
    on_single_worker(|| {
        let history = empty_history();
        let name: fn(usize) -> String = |i| format!("gamma tool {i:05}");
        let file_stem: fn(usize) -> String = |i| format!("gamma{i:05}");
        let mut small = engine_where_every_entry_matches(SMALL, name, file_stem);
        let mut large = engine_where_every_entry_matches(LARGE, name, file_stem);
        let (_, small_bytes) = full_scan_allocations(&mut small, "c:\\apps\\", &history);
        let (_, large_bytes) = full_scan_allocations(&mut large, "c:\\apps\\", &history);
        let grew = large_bytes.saturating_sub(small_bytes);
        let per_entry_index = (LARGE - SMALL) * std::mem::size_of::<usize>();
        assert!(
            grew < per_entry_index / 2,
            "エントリ {} 件の増加に対して確保が {grew} バイト増えた（{small_bytes} → {large_bytes}）。\
             添字を全件ぶん集めると {per_entry_index} バイト増える",
            LARGE - SMALL,
        );
    });
}

// ---------------------------------------------------------------------------
// 計器（手元で release 実行する）
// ---------------------------------------------------------------------------

/// 実 `index.bin` と合成ラダーで、検索 1 回あたりの確保を表にする（合否は言わない）。
///
/// 実 `index.bin` は製品の呼び出し形（`Engine::search`・実 config の mode / limit / migemo・
/// 実履歴）で測る。**PATH マージは通さない**ので、これを「実運用点」と呼ばない（実起動との違いと、
/// それを通す計器は `tests/path_query_cost.rs` の `measure_path_query_frame_cost_at_operating_point`
/// の doc）。**打鍵の並びのまま測る**——2 打鍵目以降は前回候補を再利用するので、全件走査とは
/// 候補の数が違う。並びの先頭で [`RESET_QUERY`] を打ち、前の並びの候補を持ち越さない。
#[test]
#[ignore = "計測専用。release + --nocapture で手動実行する"]
fn measure_search_allocations() {
    on_single_worker(|| {
        measure_real_index();
        measure_synthetic_ladder();
    });
    println!("  * µs は計数器込み・worker 1 本の値であり、製品の壁時計ではない");
}

/// 1 回の検索を計数と計時の区間で囲み、表の 1 行を出す。
fn report_measured(query: &str, search: impl FnOnce() -> Vec<SearchResult>) {
    let started = Instant::now();
    let (allocs, bytes, results) = allocations_during(search);
    println!(
        "  {query:<22}{:>8}{allocs:>10}{:>12.1}{:>10}",
        results.len(),
        bytes as f64 / 1024.0,
        started.elapsed().as_micros()
    );
}

fn report_header() {
    println!(
        "  {:<22}{:>8}{:>10}{:>12}{:>10}",
        "query", "results", "allocs", "KiB", "µs*"
    );
}

fn measure_real_index() {
    let config = Config::load();
    if config.paths.scan.is_empty() {
        println!("実 config に scan パスが無いため実 index.bin の計測をスキップします。");
        return;
    }
    let loaded =
        indexer::load_or_scan_with_stats(&config.paths.scan, config.search.show_hidden_system);
    let n = loaded.material.tree().len();
    println!(
        "\n=== 実 index.bin（PATH マージなし・{n} 件・cache_hit = {}・migemo = {}・mode = {:?}）===",
        loaded.stats.cache_hit, config.search.migemo_enabled, config.search.normal_mode
    );
    let mut engine = Engine::from_material(loaded.material, HistoryStore::load(), config);
    report_header();
    for sequence in [
        &["c", "co", "cod", "code"][..],
        &["s", "se", "set"][..],
        &["c:\\", "c:\\u", "c:\\users\\"][..],
    ] {
        // 暖機（スレッドローカルの立ち上げ）→ 再利用を外す → 並びを打つ。
        let _ = engine.search(sequence[0]);
        let _ = engine.search(RESET_QUERY);
        for query in sequence {
            report_measured(query, || engine.search(query));
        }
    }
}

/// ベンチ（`search/tests/performance.rs` の `make_bench_entries`）と同じ形の名前——
/// bitmask を通る率がクエリごとにばらつく、実在するアプリ名に近い分布。
/// **別 crate のため写している**（統合テストから crate 内の fixture へは届かない）。
fn bench_like_entries(n: usize) -> Vec<AppEntry> {
    const PREFIXES: [&str; 14] = [
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
    const SUFFIXES: [&str; 13] = [
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
        .map(|i| AppEntry {
            name: format!(
                "{} {} {i}",
                PREFIXES[i % PREFIXES.len()],
                SUFFIXES[i % SUFFIXES.len()]
            ),
            target_path: format!("C:\\Program Files\\App{i}\\app{i}.lnk"),
            is_folder: false,
        })
        .collect()
}

fn measure_synthetic_ladder() {
    let history = empty_history();
    for n in [10_000usize, 100_000, 300_000] {
        println!("\n=== 合成（ベンチと同じ名前の分布・{n} 件・全件走査）===");
        let mut engine = SearchEngine::new(bench_like_entries(n));
        report_header();
        for query in [
            "vis",
            "code",
            "micro",
            "app",
            "sett",
            "app.lnk",
            "fake\\vis",
        ] {
            let _ = engine.search(query, MAX_RESULTS, &history, SearchMode::Fuzzy);
            let _ = engine.search(RESET_QUERY, MAX_RESULTS, &history, SearchMode::Fuzzy);
            report_measured(query, || {
                engine.search(query, MAX_RESULTS, &history, SearchMode::Fuzzy)
            });
        }
    }
}
