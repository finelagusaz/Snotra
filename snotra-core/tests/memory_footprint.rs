//! メモリ常駐量の実測ハーネス（計測専用・`#[ignore]`）。
//!
//! `SearchEngine` の常駐ヒープを**アロケータ実測**で取る。構造体の算術見積もりは
//! アロケータのブロックヘッダ・サイズクラス丸めを取りこぼすため、live バイト数と
//! **確保回数**の両方を測る（1 エントリあたり複数の小 `Box<str>` を持つ設計では、
//! 回数由来のオーバーヘッドが無視できない）。
//!
//! 計測は環境依存ゆえ CI では回さない。手元で release 実行する（コマンドの SSOT は
//! `docs/build-commands.md`）。**`--test-threads=1` を外さないこと**——上の計数器は
//! プロセス大域ゆえ、並列実行すると Phase A/B が奪い合い、失敗ではなく
//! **もっともらしい数値**を出す（実測: 規模に対する単調性の破れ・`live 0.00 MiB`）。
//!
//! 実運用点（`%APPDATA%\Snotra\index.bin` の実インデックス）と合成ラダーの両方を測る。
//! 実インデックスが無い環境では Phase A は自動スキップし、合成のみ報告する。
//!
//! **A/B を取るときは Phase B だけを名指しで走らせない。** rayon のグローバルスレッドプールは
//! 初回の並列構築で立ち上がり、その常駐（実測 0.11 MiB）は最初に走った区間へ計上される。
//! Phase A を飛ばすと Phase B の最小規模（N = 10,000）がその額を丸ごと被り、**削減した側が
//! 増えて見える**（実測で踏んだ: `live +0.48` が単独実行では `+0.59` になり、変更前の
//! `+0.57` を上回った）。両 Phase を同じ順で走らせた出力どうしを比べること。
//!
//! **内訳は `SearchEngine::footprint_rows`（`src/search/footprint.rs`）が数える。**
//! ここが数えるのは合計、あちらが数えるのはその割り付けであり、両者の差が「未帰属」になる。
//! 構築**前**の `Vec<AppEntry>` を走査して代用してはならない——`target_path` は構築時に
//! `PathStore` へ組み替えられて解放されるので、その走査は**もう存在しない物体を測る**
//! （反復 3 で実際にそうなり、内訳 122.52 MiB 対 常駐 54.92 MiB を出していた）。
//!
//! `tests/*.rs` は独立したクレートルートゆえ、ここで宣言する `#[global_allocator]` は
//! 製品バイナリに一切入らない（`tests/search_frame_cost.rs` と同じ隔離）。

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use snotra_core::config::Config;
use snotra_core::indexer::{self, AppEntry, LoadOrScanResult};
use snotra_core::search::{FootprintRow, SearchEngine};

// ---------------------------------------------------------------------------
// 計数アロケータ
// ---------------------------------------------------------------------------

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static LIVE_BLOCKS: AtomicUsize = AtomicUsize::new(0);

/// `System` を包み、live / peak バイト数と確保回数を数えるだけのアロケータ。
/// 並行スレッド（rayon の Wave 1/2 構築）からの確保も合算される。
struct Counting;

impl Counting {
    fn on_alloc(size: usize) {
        let live = LIVE.fetch_add(size, Ordering::Relaxed) + size;
        PEAK.fetch_max(live, Ordering::Relaxed);
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        LIVE_BLOCKS.fetch_add(1, Ordering::Relaxed);
    }

    fn on_dealloc(size: usize) {
        LIVE.fetch_sub(size, Ordering::Relaxed);
        LIVE_BLOCKS.fetch_sub(1, Ordering::Relaxed);
    }
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            Self::on_alloc(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        Self::on_dealloc(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            // realloc はブロック数を変えない（1 ブロックのサイズ差し替え）。
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
            let live = LIVE.fetch_add(new_size, Ordering::Relaxed) + new_size;
            PEAK.fetch_max(live, Ordering::Relaxed);
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        p
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// 計測区間のスナップショット。
#[derive(Clone, Copy)]
struct Snap {
    live: usize,
    peak: usize,
    allocs: usize,
    blocks: usize,
}

fn snap() -> Snap {
    Snap {
        live: LIVE.load(Ordering::Relaxed),
        peak: PEAK.load(Ordering::Relaxed),
        allocs: ALLOCS.load(Ordering::Relaxed),
        blocks: LIVE_BLOCKS.load(Ordering::Relaxed),
    }
}

/// peak を現在の live まで引き下げ、次の区間の peak を独立に測れるようにする。
fn reset_peak() {
    PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
}

fn mib(bytes: usize) -> f64 {
    bytes as f64 / 1024.0 / 1024.0
}

/// 符号つきの差分（MiB）。**`saturating_sub` で丸めない**——区間が正味で解放する
/// （`shrink_to_fit` 等）ことは実際に起きる。飽和させると減少が `0.00 MiB` に化け、
/// 「何も起きなかった」と読める嘘になる（実測で踏んだ）。
fn delta_mib(after: usize, before: usize) -> f64 {
    (after as f64 - before as f64) / 1024.0 / 1024.0
}

/// 区間の差分を 1 行で報告する。`n` はエントリ数（0 なら per-entry を出さない）。
fn report(label: &str, before: Snap, after: Snap, n: usize) {
    let live = after.live as f64 - before.live as f64;
    let blocks = after.blocks as f64 - before.blocks as f64;
    let allocs = after.allocs.saturating_sub(before.allocs);
    println!(
        "  {label:<34} live {:>+8.2} MiB  peak {:>8.2} MiB  blocks {blocks:>+9.0}  allocs {allocs:>9}",
        live / 1024.0 / 1024.0,
        mib(after.peak.saturating_sub(before.live)),
    );
    if n > 0 {
        println!(
            "  {:<34} = {:>+6.1} B/entry, {:>+5.2} blocks/entry",
            "",
            live / n as f64,
            blocks / n as f64,
        );
    }
}

// ---------------------------------------------------------------------------
// 内訳（`docs/superpowers/specs/2026-08-07-index-memory-footprint-design.md` §5）
// ---------------------------------------------------------------------------

/// 構築**後**の `SearchEngine` の内訳を表にし、帰属できた（バイト, ブロック）を返す。
///
/// 数えるのは `SearchEngine::footprint_rows`（`src/search/footprint.rs`）であり、ここは
/// 出力と突き合わせだけを持つ。**帰属を実測に合わせにいかない**——差はそのまま「未帰属」と
/// して出す。差を埋める項を推測で足すと、内訳が実測ではなく辻褄合わせになる。
fn report_breakdown(rows: &[FootprintRow], n: usize) -> (usize, usize) {
    println!("\n  --- 常駐の内訳（構築後の SearchEngine を走査）---");
    println!(
        "  {:<44}{:>10}{:>11}{:>10}{:>10}",
        "項目", "確保 MiB", "blocks", "要素", "B/entry"
    );
    let (mut bytes, mut blocks) = (0usize, 0usize);
    for row in rows {
        println!(
            "  {:<44}{:>10.2}{:>11}{:>10}{:>10.1}",
            row.label,
            mib(row.bytes),
            row.blocks,
            row.count,
            row.bytes as f64 / n as f64,
        );
        bytes += row.bytes;
        blocks += row.blocks;
    }
    (bytes, blocks)
}

// ---------------------------------------------------------------------------
// 合成インデックス（実運用点の文字列長分布に合わせる）
// ---------------------------------------------------------------------------

/// 実測分布（`index.bin` デコード: name 平均 10.4B / path 平均 66.4B / folder 率 98.9%）に
/// 寄せた合成エントリを作る。文字列長がメモリ量を支配するため、長さの再現を優先する。
fn synthetic_entries(n: usize) -> Vec<AppEntry> {
    (0..n)
        .map(|i| {
            let name = format!("entry{i:05}");
            let target_path = format!(
                "C:\\workspace\\project{:03}\\src\\module{:04}\\{name}",
                i % 512,
                i % 997
            );
            AppEntry {
                name,
                target_path,
                is_folder: i % 100 != 0,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Phase A: 実運用点（実 index.bin）
// ---------------------------------------------------------------------------

/// `index.bin` のヘッダーが名乗る形式バージョン（ファイルが無ければ `None`）。
///
/// **ロード側に問わずファイルを直接読む。** 知りたいのは「どの形式が実運用点に置かれて
/// いるか」であって、ロードがどの枝を選んだかではない——両者は一致するとは限らず
/// （config_hash 不一致なら版が現行でも全走査へ落ちる）、食い違ったときに区別が付くのは
/// 独立に読んだ側だけである。保存先の導出は `Config::config_dir()` の 1 点を通す。
///
/// **ヘッダーの 8 バイトだけを読む。** 全体を読むと 51 MiB の一時確保がこのハーネスの
/// 計数器に乗り、測りに来た当の数字を汚す。
fn on_disk_index_version() -> Option<u32> {
    use std::io::Read;
    let path = Config::config_dir()?.join("index.bin");
    let mut header = [0u8; 8];
    std::fs::File::open(path)
        .ok()?
        .read_exact(&mut header)
        .ok()?;
    // **ヘッダーの配置を書き写さない。** 正本は `binfmt` で、そこと同じ 1 つを通す。
    snotra_core::binfmt::peek_version(&header)
}

#[test]
#[ignore = "計測専用。release + --nocapture で手動実行する"]
fn measure_real_index_footprint() {
    let config = Config::load();
    let scan = &config.paths.scan;
    if scan.is_empty() {
        println!("実 config に scan パスが無いため Phase A をスキップします。");
        return;
    }

    println!("\n=== Phase A: 実運用点（実 index.bin / 実 config）===");
    println!(
        "  migemo_enabled = {}, show_hidden_system = {}",
        config.search.migemo_enabled, config.search.show_hidden_system
    );

    // **ロードより前に読む。** cache-miss の枝はこの区間の中で現行版を書き出すので、
    // あとから読むと「測る前に何が置かれていたか」ではなく「測ったあと何が残ったか」に
    // なってしまう——旧版を測った実行が v7 と自称する。
    let on_disk_version = on_disk_index_version();

    // 実起動と同じ経路を辿る（main.rs:203 → 246）: load_or_scan_with_stats が返す
    // cached_masks を new_from_cache へ渡し、Wave 1 をスキップする。ここを
    // load_or_scan（masks 破棄）で代用すると再計算が走り、ピークも構築コストも別物になる。
    reset_peak();
    let t0 = snap();
    let load_start = std::time::Instant::now();
    let result = indexer::load_or_scan_with_stats(scan, config.search.show_hidden_system);
    let load_ms = load_start.elapsed().as_secs_f64() * 1000.0;
    let t1 = snap();
    let n = result.tree.len();

    if result.cache_changed {
        println!(
            "  警告: index.bin がヒットせず全走査が走りました（entries={n}）。\
             以下の load 区間は走査コスト込みで、常駐量の解釈には使えません。"
        );
    }
    println!(
        "  entries = {n}, cache_hit = {}, cached_masks = {}",
        result.stats.cache_hit,
        result.cached_masks.is_some()
    );
    // **どの版を測ったかを出さないと、この数字は読めない。** 旧版の `index.bin` は
    // フォールバック枝で読まれ、木は `target_path` を実体化してから建て直される
    // ——現行版の削減はそこには現れないのに、出力の見た目は成功時とまったく同じである
    // （実運用点が旧版のまま残る機序は `snotra-core/CLAUDE.md`「indexer.rs の背景再スキャン」）。
    match on_disk_version {
        Some(v) if v == indexer::INDEX_CACHE_VERSION => {
            println!("  on-disk 形式: v{v}（現行）");
        }
        Some(v) => {
            println!(
                "  on-disk 形式: v{v} — 旧版である。**以下はフォールバック枝の数字であり、\
                 現行 v{} の常駐ではない。** 昇格させてから測り直すこと（アプリを 1 回起動して\
                 背景再スキャンを走らせるか、index.bin を退避して全走査させる）",
                indexer::INDEX_CACHE_VERSION
            );
        }
        None => println!("  on-disk 形式: index.bin を読めなかった（全走査の枝である）"),
    }
    report("index.bin ロード（entries + masks）", t0, t1, n);
    // **フェーズ内訳を出す。** 製品が既に測っている（`LoadOrScanStats`）のに、ここが壁時計だけを
    // 出していたせいで「ロードのどこに時間が居るか」が見えなかった——全エントリ複製が
    // `cache_load_ms` の外に居て、どのフェーズにも現れないまま起動段の live ブロックの 1/3 を
    // 占めていたのはそれが理由である
    // （`PERFORMANCE.md`「採用: 背景再スキャンの比較を digest へ（ロード 395 → 312 ms・反復 6）」）。
    // **合計が `total` に届かない分は、まだ名前の付いていない処理がそこに居るということである。**
    let s = &result.stats;
    // `cache_read_ms` は `cache_load_ms` の**内数**ゆえ、残余の計算には足さない
    // （足すと二重計上で残余が負に振れる）。分けて出すのは、読むバイト数と deserialize が
    // オンディスク形式の変更に対して**逆向きに振る舞う**ためである。
    println!(
        "  フェーズ: total {}ms = hash {}ms + cache_load {}ms（うち read {}ms）+ digest {}ms + scan {}ms + sort {}ms + cache_save {}ms（残余 {}ms）",
        s.total_ms,
        s.hash_ms,
        s.cache_load_ms,
        s.cache_read_ms,
        s.digest_ms,
        s.scan_ms,
        s.sort_ms,
        s.cache_save_ms,
        s.total_ms.saturating_sub(
            s.hash_ms + s.cache_load_ms + s.digest_ms + s.scan_ms + s.sort_ms + s.cache_save_ms
        ),
    );

    let LoadOrScanResult {
        tree,
        cached_masks,
        rescan_task,
        ..
    } = result;
    // 背景再スキャンタスクは src-tauri 側で別スレッドが消費する。常駐計測の対象外。
    drop(rescan_task);

    // **実運用の起動経路は、ロードと構築の間に PATH スキャンを挟む**（`main.rs` の
    // `load_or_scan_with_stats` → `scan_path_env` → `Engine::new_from_cache`）。この区間は
    // **かつて**全 `entries` の `target_path` を正規化して `HashSet` へ積んでおり、
    // **エントリ数に比例する確保がロードと構築のどちらの計測にも入っていなかった**
    // （反復 6 の digest がどのフェーズにも現れなかったのと同じ形）。反復 9 でその積み上げ
    // は消えたが、**計器はそのまま残す**——エントリ数に比例する仕事がここへ戻る退行を
    // 捕まえられるのは、この区間を名指しで測るこの行だけである。
    //
    // **返り値は entries へ混ぜない。** PATH スキャンはファイルシステムを読むため実行ごとに
    // ぶれ、混ぜると常駐がバイト単位で再現しなくなる（決定性は内訳と実測を突き合わせる
    // 前提そのもの）。ここで測るのは区間のコストであって、PATH エントリの常駐ではない。
    if config.search.include_path_env {
        reset_peak();
        let tp0 = snap();
        let path_start = std::time::Instant::now();
        let path_entries = indexer::scan_path_env(&tree, config.search.show_hidden_system);
        let path_ms = path_start.elapsed().as_secs_f64() * 1000.0;
        let tp1 = snap();
        let added = path_entries.len();
        drop(path_entries);
        report("PATH スキャン（起動経路・常駐外）", tp0, tp1, n);
        println!("  壁時計: PATH スキャン {path_ms:.0} ms（+{added} 件・返り値は捨てる）");
    }

    // 実 config どおりの migemo 設定で構築（= 実運用の常駐形）。
    reset_peak();
    let t2 = snap();
    let build_start = std::time::Instant::now();
    let engine = match cached_masks {
        Some(masks) => {
            SearchEngine::new_with_cached_masks(tree, masks, config.search.migemo_enabled)
        }
        None => SearchEngine::new_from_tree(tree, config.search.migemo_enabled),
    };
    let build_ms = build_start.elapsed().as_secs_f64() * 1000.0;
    let t3 = snap();
    report("SearchEngine 構築（実 config）", t2, t3, n);
    // 起動経路の壁時計。**メモリ削減 1 件につきレイテンシ 1 件を対にする**ための計器
    // （設計書 §4.2・前例は #110）。1 回きりの起動経路ゆえ標本は 1 つで、
    // `bench_new_scaling` とは別物である（あちらは合成 Vec からの構築で index.bin を通らない）。
    println!(
        "  壁時計: ロード {load_ms:.0} ms / 構築 {build_ms:.0} ms（各 1 標本・実行間で数十%ぶれる）"
    );

    let resident = t3.live.saturating_sub(t0.live);
    let blocks = t3.blocks.saturating_sub(t0.blocks);
    println!(
        "  --- 合計常駐（entries + 派生）: {:.2} MiB / peak {:.2} MiB / blocks {blocks}（{:.2} blocks/entry）---",
        mib(resident),
        mib(t3.peak.saturating_sub(t0.live)),
        blocks as f64 / n as f64,
    );

    // 内訳は**構築後**に取る。走査そのものの確保（`rows`）は `t3` より後ゆえ常駐には入らないが、
    // 生かしたままだと下の「drop 後の残留」に混じるので、engine より先に落とす。
    let rows = engine.footprint_rows();
    let (accounted, accounted_blocks) = report_breakdown(&rows, n);
    drop(rows);

    // **帰属は実測を超えてはならない。** 超えたなら丸めではなく二重計上である。
    // 残る差の内訳は下の「drop 後の残留」が測る——engine を落としても残る額（rayon の
    // ワーカープール等、索引ではないもの）がその内側にある。
    println!(
        "      帰属 {:.2} MiB / {accounted_blocks} blocks → 未帰属 {:+.2} MiB / {:+} blocks（{:+.1} B/entry）",
        mib(accounted),
        delta_mib(resident, accounted),
        blocks as i64 - accounted_blocks as i64,
        (resident as f64 - accounted as f64) / n as f64,
    );

    drop(engine);
    let t4 = snap();
    println!(
        "  drop 後の残留: {:.2} MiB（索引ではないものの額。未帰属はこれを下回れない）",
        mib(t4.live.saturating_sub(t0.live))
    );

    // オンディスクの内訳は**最後に取る**。この計器自身が形式全体を postcard へ書き直して
    // 長さを測る（＝100 MiB 級の一時確保をする）ので、上の常駐・残留の測定より前に置くと
    // peak を汚す。
    report_cache_bytes(n);
}

/// `index.bin` のバイト内訳を表にする。
///
/// **常駐の内訳とは測っている物体が違う。** あちらは構築後の `SearchEngine`——メモリが
/// 反復 2〜5 で「持たない」ことを学んだ後の姿である。こちらはディスクが今も持ち続けている
/// 姿で、両者の差が「読んで確保して `assemble` が即座に捨てているもの」になる。
/// **オンディスク形式を変える判断は、常駐の内訳からは原理的にできない**——`target_path` は
/// 常駐 0.01 MiB（フォルダ木の接頭辞共有）に対し、ディスクは全文を持つ。
fn report_cache_bytes(n: usize) {
    let Some(b) = indexer::cache_byte_breakdown_in(&Config::config_dir().expect("config dir"))
    else {
        // **「どの版としても読めない」と書いてはならない。** この計器の射程は製品の
        // フォールバック鎖より狭く、製品は読めるがここでは読めない古い版が在りうる
        // （`cache_byte_breakdown_in` の doc）。版の一覧もここへ書き写さない——焼き込むと
        // 版を足したときにこの 1 行だけが腐る。
        println!("\n  --- index.bin の内訳: この計器が読める版では読めなかったためスキップ ---");
        return;
    };

    // **版を必ず出す。** 実運用点のファイルが現行版とは限らない——`index.bin` を書き直す契機は
    // cache-miss と背景再スキャンの `Changed` だけなので、索引の中身が変わらなければ旧版が
    // 残り続ける。版を見ずに内訳を読むと、既に消したはずのフィールドを「まだある」と誤読する。
    println!(
        "\n  --- index.bin の内訳（v{} / {:.2} MiB / payload {:.2} MiB）---",
        b.version,
        mib(b.file_len),
        mib(b.payload_len)
    );
    // 現行版はリテラルで書かず定数を読む（理由は `INDEX_CACHE_VERSION` の doc）。
    if b.version != snotra_core::indexer::INDEX_CACHE_VERSION {
        println!(
            "  ※ **実運用点は v{} のまま**で、現行 v{} が消したフィールドをまだ読んでいる。",
            b.version,
            snotra_core::indexer::INDEX_CACHE_VERSION
        );
    }
    println!(
        "  {:<44}{:>10}{:>11}{:>10}",
        "項目", "MiB", "要素", "B/entry"
    );
    for row in b.rows.iter().chain(b.entry_rows.iter()) {
        println!(
            "  {:<44}{:>10.2}{:>11}{:>10.1}",
            row.label,
            mib(row.bytes),
            row.items,
            row.bytes as f64 / n as f64,
        );
    }
    // **残余が 0 でなければ帰属が誤っている。** postcard は struct に枠を持たず、フィールドの
    // 連結がそのまま payload になる——項目別の和は payload 長と一致しなければならない。
    // 一致しない内訳は、正しい帰属と誤った帰属を区別できない（このセッションで最も高くついた
    // 失敗が「計器が製品でないものを測っていた」ことであり、その検知器がこの 1 行である）。
    println!(
        "      残余: フィールド {:+} B / entries 内訳 {:+} B（**どちらも 0 でなければ帰属が誤っている**）",
        b.residual, b.entry_residual
    );
    assert_eq!(
        b.residual, 0,
        "index.bin のフィールド帰属が payload 長と一致しない"
    );
    assert_eq!(
        b.entry_residual, 0,
        "entries の内訳が entries のバイト数と一致しない"
    );
}

// ---------------------------------------------------------------------------
// Phase B: migemo の限界費用と規模スケーリング
// ---------------------------------------------------------------------------

#[test]
#[ignore = "計測専用。release + --nocapture で手動実行する"]
fn measure_synthetic_scaling() {
    println!("\n=== Phase B: 合成インデックスのスケーリング ===");

    for n in [10_000usize, 38_847, 100_000] {
        println!("\n  [N = {n}]");

        reset_peak();
        let t0 = snap();
        let entries = synthetic_entries(n);
        let t1 = snap();
        report("Vec<AppEntry> のみ", t0, t1, n);

        // migemo 無効（kana 2 本が空 Vec）。
        let cloned = entries.clone();
        reset_peak();
        let t2 = snap();
        let engine_off = SearchEngine::new_with_migemo(cloned, false);
        let t3 = snap();
        report("SearchEngine（migemo=off）", t2, t3, n);
        drop(engine_off);

        // migemo 有効（kana_lower_names + kana_char_masks を追加構築）。
        let cloned = entries.clone();
        reset_peak();
        let t4 = snap();
        let engine_on = SearchEngine::new_with_migemo(cloned, true);
        let t5 = snap();
        report("SearchEngine（migemo=on）", t4, t5, n);
        drop(engine_on);

        let off = t3.live.saturating_sub(t2.live);
        let on = t5.live.saturating_sub(t4.live);
        println!(
            "  {:<34} +{:.2} MiB（migemo の限界費用）",
            "kana 2 本の追加分",
            mib(on.saturating_sub(off))
        );

        drop(entries);
    }
}
