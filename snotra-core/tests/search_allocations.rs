//! 検索 1 回あたりの一時確保を数える検知器と計器（打鍵ごとに払う確保であって常駐ではない）。
//!
//! **挙動テストはこの性質を原理的に見ない。** 候補ごとの確保を戻しても検索結果は 1 件も
//! 変わらず、退行は「遅い」としてしか現れない。ゆえに検知器は時間ではなく**確保の回数と
//! バイト数**を数える——どちらも入力だけで決まるので共有ランナーでも揺れず、`#[ignore]` を
//! 付けずに CI で走らせられる。
//!
//! **計数器はスレッドローカルで、数える範囲は 2 通りある。**
//!
//! - **worker 1 本**（[`on_single_worker`]）: fold も reduce もその worker が担うので、検索の確保は
//!   全部そこへ計上される。候補ごとの確保と結果ごとの確保は、スレッド数に依らないのでこれで足りる
//! - **専用プールの全 worker**（[`allocations_on_all_workers`]）: rayon の葉ごとの確保（fold ごとの
//!   top-k と一致の収集・reduce の連結）は葉の数に比例し、worker 1 本では葉が 2 枚しかできず
//!   1/10〜1/20 にしか映らない（#1280）。各 worker の計数を `ThreadPool::broadcast` で読んで足す
//!
//! どちらも専用のプールで走らせるので、libtest が並行に走らせる他のテストの確保は混ざらない。
//!
//! **壁時計は測らない。** 計数器は確保のたびに働くので、確保の多い側ほど遅く見せる。
//! A/B の壁時計は既定のアロケータで走るベンチ（`bench_fuzzy_search_scaling` 等）が測る。
//!
//! **手元の計器（`measure_search_allocations`）は実 `index.bin` を書き換えうる。**
//! `load_or_scan_with_stats` は旧版を読めば現行版へ昇格し、キャッシュが外れれば走査して
//! 書き出す（契約は同関数の doc）。CI で走る検知器は実データに触れない。
//!
//! 撤去条件を持たない恒久の検知器である（足場ではない）。実測値は
//! `PERFORMANCE.md`「採用: 検索の候補ごとの確保をやめる」・「採用: 結果の組み立てでパスを
//! 長さちょうど 1 回で確保する（#1277）」・「採用: 検索の rayon の葉ごとの確保を減らす（#1280）」が正本。

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
    pool_of(1).install(f)
}

/// `pool` の全 worker と呼び出し側のスレッドの確保の（回数, バイト数）の合計。
fn counts_on_all_workers(pool: &rayon::ThreadPool) -> (usize, usize) {
    let workers = pool.broadcast(|_| (ALLOCS.with(Cell::get), BYTES.with(Cell::get)));
    let caller = (ALLOCS.with(Cell::get), BYTES.with(Cell::get));
    workers
        .into_iter()
        .fold(caller, |(a, b), (x, y)| (a + x, b + y))
}

/// `f` を `pool` の中で走らせ、その間に全 worker と呼び出し側で起きた確保の（回数, バイト数）と、
/// `f` の戻り値を返す。
///
/// **broadcast 自身も確保する**（結果の `Vec` など）。読みの 2 回を空打ちして、その差を引く。
fn allocations_on_all_workers<R: Send>(
    pool: &rayon::ThreadPool,
    f: impl FnOnce() -> R + Send,
) -> (usize, usize, R) {
    let before = counts_on_all_workers(pool);
    let out = pool.install(f);
    let after = counts_on_all_workers(pool);
    let idle_before = counts_on_all_workers(pool);
    let idle_after = counts_on_all_workers(pool);
    let overhead = (idle_after.0 - idle_before.0, idle_after.1 - idle_before.1);
    (
        after.0 - before.0 - overhead.0,
        after.1 - before.1 - overhead.1,
        out,
    )
}

fn pool_of(threads: usize) -> rayon::ThreadPool {
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .expect("rayon のプールを作れない")
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

/// 親フォルダの鎖の下にファイルを並べた索引。ファイルのフルパスは根から
/// `DEEP_FOLDERS` 段を足して組み立てることになる（親が索引に無いと、パスは根 1 段で
/// 組み上がり、組み立ての成長が起きない）。
///
/// 親として解決されるには 2 つ要る: `target_path` 順に並んでいること（`IndexTree::build` は
/// 親を二分探索で引く）と、ファイルの `name` が末尾成分の接頭辞であること（indexer が
/// `file_stem()` を名前にする規則の写し）。後者を外すとファイルは根になり、この fixture は
/// 何も測らなくなる。フォルダの名前は `alpha` の文字を含まないので、ファイルだけが一致する。
fn engine_with_deep_paths(files: usize) -> SearchEngine {
    const DEEP_FOLDERS: usize = 8;
    let mut dir = String::from("C:\\directory-segment-root");
    let mut entries = vec![AppEntry {
        name: "directory-segment-root".into(),
        target_path: dir.clone(),
        is_folder: true,
    }];
    for d in 0..DEEP_FOLDERS {
        let name = format!("directory-segment-{d:02}");
        dir = format!("{dir}\\{name}");
        entries.push(AppEntry {
            name,
            target_path: dir.clone(),
            is_folder: true,
        });
    }
    entries.extend((0..files).map(|i| AppEntry {
        name: format!("alpha{i:05}"),
        target_path: format!("{dir}\\alpha{i:05}.exe"),
        is_folder: false,
    }));
    entries.sort_by(|a, b| a.target_path.cmp(&b.target_path));
    SearchEngine::new(entries)
}

/// 結果 1 件ごとの確保が「表示名 1 回 + フルパス 1 回」を超えない。
///
/// 回帰テスト: `heap_into_results` はフルパスを `String::new()` から段ごとに書き足しており、
/// 容量が倍々に伸びるたびに確保していた（#1277。額は `//!` が指す `PERFORMANCE.md` の節）。
/// 同じ索引・同じクエリで結果の上限だけを 2 点変え、増えた確保を増えた結果で割る——
/// 上限に依らない確保（top-k のヒープ・一致の収集）は差で消える。閾値 2.5 は、使い回しの
/// バッファから長さちょうどで写す形（2 回）と、段ごとに伸ばす形（この fixture で 3 回以上）の間に置く。
#[test]
fn result_assembly_allocates_once_per_path() {
    const FEW: usize = 10;
    const MANY: usize = 40;
    on_single_worker(|| {
        let history = empty_history();
        let mut engine = engine_with_deep_paths(200);
        let mut measure = |limit: usize| {
            let _ = engine.search("alpha", limit, &history, SearchMode::Fuzzy);
            let _ = engine.search(RESET_QUERY, limit, &history, SearchMode::Fuzzy);
            let (allocs, _, results) =
                allocations_during(|| engine.search("alpha", limit, &history, SearchMode::Fuzzy));
            // 上限まで埋まらなければ、差は結果の増分を測っていない。
            assert_eq!(results.len(), limit, "fixture が上限まで一致していない");
            allocs
        };
        let few = measure(FEW);
        let many = measure(MANY);
        let per_result = many.saturating_sub(few) as f64 / (MANY - FEW) as f64;
        assert!(
            per_result < 2.5,
            "結果 1 件あたり {per_result:.2} 回確保した（上限 {FEW} 件: {few} 回 → {MANY} 件: {many} 回）"
        );
    });
}

/// rayon の葉 1 枚あたりの確保が、葉の中の一致の数に比例して増えない。
///
/// 回帰テスト: fold の一致の収集は容量 0 から倍々で伸び、reduce の identity は葉ごとに
/// `TopK::new` と、空の `Vec` への写しを確保していた（#1280）。同じ索引・同じクエリを worker
/// 1 本と 4 本のプールで走らせ、差（葉が増えた分の確保）を見る——候補ごと・結果ごとの確保は
/// 差で消える。
///
/// **件数とスレッド数が検出力を決める。**
/// - 8,000 件は、葉の数を `with_min_len` が上から抑える規模である（4 本と 8 本で値が一致した）
/// - **4 本に固定する。** 2 本では、ランナーが負荷を受けると変更前の差が 26〜29 に落ち、閾値を割った
/// - 閾値 32 は、4 本での採る形（20・busy-loop 32 本の負荷下で最大 23）と変更前（74）の間に置く
///   （実測の出所は `PERFORMANCE.md`「採用: 検索の rayon の葉ごとの確保を減らす（#1280）」）
#[test]
fn split_search_allocations_do_not_grow_with_leaf_matches() {
    const N: usize = 8_000;
    const THRESHOLD: usize = 32;
    let history = empty_history();
    let mut engine = engine_where_every_entry_matches(
        N,
        |i| format!("alpha tool {i:05}"),
        |i| format!("bin{i:05}"),
    );
    let mut measure = |pool: &rayon::ThreadPool| {
        // 暖機は各 worker のスレッドローカル（照合器・組み立てバッファ）を立ち上げる。
        for _ in 0..2 {
            pool.install(|| {
                let _ = engine.search("alpha", MAX_RESULTS, &history, SearchMode::Fuzzy);
                let _ = engine.search(RESET_QUERY, MAX_RESULTS, &history, SearchMode::Fuzzy);
            });
        }
        let (allocs, _, results) = allocations_on_all_workers(pool, || {
            engine.search("alpha", MAX_RESULTS, &history, SearchMode::Fuzzy)
        });
        assert_eq!(
            results.len(),
            MAX_RESULTS,
            "fixture が上限まで一致していない"
        );
        allocs
    };
    let single = measure(&pool_of(1));
    let split = measure(&pool_of(4));
    let grew = split.saturating_sub(single);
    assert!(
        grew < THRESHOLD,
        "worker 4 本で確保が {grew} 回増えた（1 本: {single} 回 → 4 本: {split} 回）"
    );
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
    const SEQUENCES: [&[&str]; 3] = [
        &["c", "co", "cod", "code"],
        &["s", "se", "set"],
        &["c:\\", "c:\\u", "c:\\users\\"],
    ];
    println!("--- worker 1 本 ---");
    report_header();
    for sequence in SEQUENCES {
        // 暖機（スレッドローカルの立ち上げ）→ 再利用を外す → 並びを打つ。
        let _ = engine.search(sequence[0]);
        let _ = engine.search(RESET_QUERY);
        for query in sequence {
            report_measured(query, || engine.search(query));
        }
    }

    // 葉に比例する確保は worker 1 本では葉が 2 枚しかできず縮んで映る（`//!`）。製品と同じ
    // 既定の本数のプールで、全 worker の分を数える。
    let pool = rayon::ThreadPoolBuilder::new()
        .build()
        .expect("rayon の既定プールを作れない");
    println!(
        "--- 既定のプールの全 worker（{} 本）---",
        pool.current_num_threads()
    );
    println!(
        "  {:<22}{:>8}{:>10}{:>12}",
        "query", "results", "allocs", "KiB"
    );
    for sequence in SEQUENCES {
        pool.install(|| {
            let _ = engine.search(sequence[0]);
            let _ = engine.search(RESET_QUERY);
        });
        for query in sequence {
            let (allocs, bytes, results) =
                allocations_on_all_workers(&pool, || engine.search(query));
            println!(
                "  {query:<22}{:>8}{allocs:>10}{:>12.1}",
                results.len(),
                bytes as f64 / 1024.0
            );
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
