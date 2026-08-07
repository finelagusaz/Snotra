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
//! `tests/*.rs` は独立したクレートルートゆえ、ここで宣言する `#[global_allocator]` は
//! 製品バイナリに一切入らない（`tests/search_frame_cost.rs` と同じ隔離）。

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use snotra_core::config::Config;
use snotra_core::indexer::{self, AppEntry, LoadOrScanResult};
use snotra_core::search::SearchEngine;

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
// 内訳の分離計測（`docs/superpowers/specs/2026-08-07-index-memory-footprint-design.md` §5）
// ---------------------------------------------------------------------------

/// 文字列群の内訳。**`len` と `cap` を分けて持つのが要点**——`CachedMasks` は `Vec<String>`
/// で届き `SearchEngine` は `Vec<Box<str>>` で持つため、両者の差が「文字列そのもの」と
/// 「容量の遊び」のどちらを削るべきかを分ける。同一視すると候補の選択を誤る。
#[derive(Default, Clone, Copy)]
struct StrStat {
    len: usize,
    cap: usize,
    count: usize,
}

impl StrStat {
    fn add(&mut self, s: &String) {
        self.len += s.len();
        self.cap += s.capacity();
        self.count += 1;
    }

    fn of<'a>(it: impl Iterator<Item = &'a String>) -> Self {
        let mut stat = Self::default();
        for s in it {
            stat.add(s);
        }
        stat
    }
}

/// 文字列 Vec 1 本の内訳を 1 行で報告し、**帰属できたバイト数**を返す。
/// アロケータが数えるのは `capacity`（`layout.size()`）のほうであり、`len` は
/// 「削れば消える文字列そのもの」の量を示す別軸である。
fn breakdown_row(label: &str, stat: StrStat, n: usize) -> usize {
    println!(
        "  {label:<26} len {:>7.2} MiB  cap {:>7.2} MiB  要素 {:>8}  {:>6.1} B/entry",
        mib(stat.len),
        mib(stat.cap),
        stat.count,
        stat.cap as f64 / n as f64,
    );
    stat.cap
}

/// Vec 本体（ヒープ上の連続領域）を 1 行で報告し、帰属バイト数を返す。文字列の中身とは別勘定。
/// **`len` ではなく `capacity` × 要素サイズで数える**——アロケータが見るのは確保量である。
fn stride_row<T>(label: &str, capacity: usize, n: usize) -> usize {
    let bytes = capacity * std::mem::size_of::<T>();
    println!(
        "  {label:<26}                    stride {:>7.2} MiB              {:>6.1} B/entry",
        mib(bytes),
        bytes as f64 / n as f64,
    );
    bytes
}

/// `SearchEngine` を構築する**前**に、手元の `entries` と `cached_masks` を直接走査して
/// 常駐の内訳を出し、**帰属できた合計バイト数**を返す。engine 構築後は private フィールドゆえ
/// 外から測れず、また `new_with_cached_masks` は Vec を move で受け取るため、
/// この時点が唯一の観測点である。
///
/// この関数自身の確保は呼び出し側の計測区間の**外**（`t1` と `t2` の間）に置くこと。
///
/// 返り値をアロケータ実測に**合わせにいかない**。差はそのまま「未帰属」として出す
/// ——差を埋める項を推測で足すと、内訳が実測ではなく辻褄合わせになる。
fn report_breakdown(
    entries: &[AppEntry],
    entries_cap: usize,
    masks: Option<&indexer::CachedMasks>,
    n: usize,
) -> usize {
    if n == 0 {
        return 0;
    }
    println!("\n  --- 常駐の内訳（SearchEngine 構築前・cached_masks 由来）---");

    let names = StrStat::of(entries.iter().map(|e| &e.name));
    let paths = StrStat::of(entries.iter().map(|e| &e.target_path));
    let mut total = breakdown_row("entries[].name", names, n);
    total += breakdown_row("entries[].target_path", paths, n);
    total += stride_row::<AppEntry>("entries（Vec 本体）", entries_cap, n);

    let Some(masks) = masks else {
        println!("  cached_masks = None（v3 以前のキャッシュ。派生 Vec は Wave 1 で再計算される）");
        return total;
    };

    total += stride_row::<u64>("char_masks", masks.char_masks.capacity(), n);
    total += stride_row::<u64>(
        "file_name_char_masks",
        masks.file_name_char_masks.capacity(),
        n,
    );

    if let Some(v) = masks.lower_names.as_ref() {
        total += breakdown_row("lower_names", StrStat::of(v.iter()), n);
        total += stride_row::<String>("lower_names（Vec 本体）", v.capacity(), n);
    }
    if let Some(v) = masks.lower_file_names.as_ref() {
        total += breakdown_row("lower_file_names", StrStat::of(v.iter().flatten()), n);
        total += stride_row::<Option<String>>("lower_file_names（Vec 本体）", v.capacity(), n);
    }
    report_duplication(entries, masks, n);
    report_tree_feasibility(entries, n);
    total
}

/// 設計書 §2 の「4 重保持」が実データで何件に当たるかを測る。§2 は indexer の
/// 名前導出規則（folder は `file_name()` / file は `file_stem()`）からの**機構上の導出**で
/// あり、率は測っていない。ここで測る率が候補 A の削減量の係数になる。
fn report_duplication(entries: &[AppEntry], masks: &indexer::CachedMasks, n: usize) {
    let folders = entries.iter().filter(|e| e.is_folder).count();
    println!(
        "  is_folder = {folders} / {n}（{:.1}%）",
        folders as f64 * 100.0 / n as f64
    );

    // 長さが揃わないキャッシュは索引がずれている（`assemble` の debug_assert 相当）。
    // 一致率を出す前に弾く——ずれたまま比較した率は無意味である。
    let (Some(lower), Some(files)) = (masks.lower_names.as_ref(), masks.lower_file_names.as_ref())
    else {
        return;
    };
    if lower.len() != n || files.len() != n {
        println!(
            "  警告: 派生 Vec の長さが entries と揃いません（lower {} / file {} / entries {n}）。\
             一致率の計測をスキップします。",
            lower.len(),
            files.len()
        );
        return;
    }

    let mut folder_same = 0usize;
    let mut file_same = 0usize;
    for (i, entry) in entries.iter().enumerate() {
        if files[i].as_deref() == Some(lower[i].as_str()) {
            if entry.is_folder {
                folder_same += 1;
            } else {
                file_same += 1;
            }
        }
    }
    let files_total = n - folders;
    println!(
        "  lower_file_names == lower_names: folder {folder_same}/{folders}（{:.1}%）\
         / file {file_same}/{files_total}",
        folder_same as f64 * 100.0 / folders.max(1) as f64,
    );
}

/// フォルダ木の接頭辞共有（`target_path` を「親 index + 末尾成分」で持つ案）の
/// **構造前提**を測る。削減量ではなく前提そのものが対象である——「親が索引に居る」
/// 「親 + 区切り + 末尾がバイト一致する」「親 index が自分より小さい」はどれも機構からは
/// 導けない。scan の順序・`show_hidden_system` フィルタ・ドライブ直下・UNC 共有・
/// 区切り文字の揺れがいずれも例外を作りうるため、実データに当てる以外に確かめようがない。
///
/// `parent_index < self_index` を測るのは、成り立つなら**循環を表現不能にできる**からである
/// （文書化した契約ではなく構造で担保する形になり、`index.bin` の破損時に無限ループ＝
/// フォールバック鎖が捕まえないハングではなく、load 時 1 比較の検証で弾ける）。
fn report_tree_feasibility(entries: &[AppEntry], n: usize) {
    use std::collections::HashMap;

    println!("  --- フォルダ木の接頭辞共有の構造前提 ---");

    let by_path: HashMap<&str, usize> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| (e.target_path.as_str(), i))
        .collect();
    if by_path.len() != n {
        // 重複パスがあると親の解決先が一意でなくなる。率を出す前に事実として報告する。
        println!(
            "  警告: target_path が一意ではありません（distinct {} / entries {n}）",
            by_path.len()
        );
    }

    // バイト内訳は件数比とは別物である。folder が件数の 81.9% でも、深さ分布が違えば
    // バイト比は一致しない——削減量の係数になるのは件数ではなくこちら。
    let (mut bytes_folder, mut bytes_file) = (0usize, 0usize);
    for e in entries {
        if e.is_folder {
            bytes_folder += e.target_path.len();
        } else {
            bytes_file += e.target_path.len();
        }
    }
    let bytes_total = bytes_folder + bytes_file;
    println!(
        "  target_path のバイト内訳: folder {:.2} MiB（{:.1}%）/ file {:.2} MiB（{:.1}%）",
        mib(bytes_folder),
        bytes_folder as f64 * 100.0 / bytes_total.max(1) as f64,
        mib(bytes_file),
        bytes_file as f64 * 100.0 / bytes_total.max(1) as f64,
    );

    let mut parent_of: Vec<Option<usize>> = vec![None; n];
    let (mut roots, mut orphan, mut mismatch, mut order_violation) =
        (0usize, 0usize, 0usize, 0usize);
    let (mut ok_folder, mut ok_file) = (0usize, 0usize);
    let (mut tail_is_name_folder, mut tail_has_name_prefix_file) = (0usize, 0usize);
    // 木表現で「それでも保持が要る文字列」のバイト数を 2 案ぶん積む。
    let (mut keep_full, mut keep_tail_file, mut keep_ext_file) = (0usize, 0usize, 0usize);
    let mut exts: HashMap<&str, usize> = HashMap::new();

    for (i, e) in entries.iter().enumerate() {
        let path = std::path::Path::new(&e.target_path);
        let (Some(parent), Some(tail)) = (path.parent(), path.file_name()) else {
            roots += 1;
            keep_full += e.target_path.len();
            continue;
        };
        let (Some(parent), Some(tail)) = (parent.to_str(), tail.to_str()) else {
            orphan += 1;
            keep_full += e.target_path.len();
            continue;
        };

        // 再構築規則: 親が区切りで終わる（`C:\` 等のドライブ直下）なら区切りを足さない。
        let mut rebuilt = String::with_capacity(parent.len() + 1 + tail.len());
        rebuilt.push_str(parent);
        if !parent.ends_with(['\\', '/']) {
            rebuilt.push('\\');
        }
        rebuilt.push_str(tail);

        let Some(&pi) = by_path.get(parent) else {
            orphan += 1;
            keep_full += e.target_path.len();
            continue;
        };
        if rebuilt != e.target_path {
            mismatch += 1;
            keep_full += e.target_path.len();
            continue;
        }

        parent_of[i] = Some(pi);
        if pi >= i {
            order_violation += 1;
        }
        if e.is_folder {
            ok_folder += 1;
            // folder は indexer が `file_name()` を name に使うため、末尾成分が name と
            // 一致すれば追加のバイトは 0 になる（`name` を再利用できる）。
            if tail == e.name {
                tail_is_name_folder += 1;
            } else {
                keep_full += tail.len();
            }
        } else {
            ok_file += 1;
            // file の name は `file_stem()`（拡張子なし）ゆえ末尾成分とは一致しない。
            // name が接頭辞なら差分＝拡張子だけを持てばよく、拡張子は強く intern する。
            keep_tail_file += tail.len();
            if let Some(ext) = tail.strip_prefix(e.name.as_str()) {
                tail_has_name_prefix_file += 1;
                keep_ext_file += ext.len();
                *exts.entry(ext).or_insert(0) += 1;
            } else {
                keep_ext_file += tail.len();
            }
        }
    }

    let folders = entries.iter().filter(|e| e.is_folder).count();
    let files = n - folders;
    println!(
        "  親の解決: 解決 {}（folder {ok_folder} / file {ok_file}）/ ルート {roots} / \
         索引に親が不在 {orphan} / 再構築の不一致 {mismatch}",
        ok_folder + ok_file
    );
    println!(
        "  末尾成分 == name: folder {tail_is_name_folder}/{ok_folder}（{:.1}%）/ \
         file の name が末尾成分の接頭辞 {tail_has_name_prefix_file}/{ok_file}",
        tail_is_name_folder as f64 * 100.0 / ok_folder.max(1) as f64,
    );
    println!("  親 index < 自 index の違反: {order_violation} 件（0 なら循環が表現不能にできる）");

    // 深さ = 根まで辿る段数。再構築コストは 1 エントリあたりこの段数ぶんのランダム読みになる。
    // 循環があっても止まるよう上限で打ち切り、打ち切り件数を隠さず出す。
    let (mut depth_sum, mut depth_max, mut truncated) = (0usize, 0usize, 0usize);
    for i in 0..n {
        let mut d = 0usize;
        let mut cur = i;
        while let Some(p) = parent_of[cur] {
            d += 1;
            cur = p;
            if d > 4096 {
                truncated += 1;
                break;
            }
        }
        depth_sum += d;
        depth_max = depth_max.max(d);
    }
    println!(
        "  深さ: 平均 {:.2} 段 / 最大 {depth_max} 段（打ち切り {truncated} 件）",
        depth_sum as f64 / n.max(1) as f64,
    );

    // 投影。文字列の中身だけを比べる（Vec 本体・`Box<str>` のポインタ分は別勘定）。
    let projected_tail = keep_full + keep_tail_file;
    let projected_ext = keep_full + keep_ext_file;
    println!(
        "  投影（文字列の中身のみ）: 現在 {:.2} MiB → 末尾成分を持つ案 {:.2} MiB / \
         file は拡張子だけ持つ案 {:.2} MiB（distinct 拡張子 {}）",
        mib(bytes_total),
        mib(projected_tail),
        mib(projected_ext),
        exts.len(),
    );
    println!(
        "  参考: folder {folders} / file {files}、親 index の追加分 {:.2} MiB（u32 × {n}）",
        mib(4 * n),
    );
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

    // 実起動と同じ経路を辿る（main.rs:203 → 246）: load_or_scan_with_stats が返す
    // cached_masks を new_from_cache へ渡し、Wave 1 をスキップする。ここを
    // load_or_scan（masks 破棄）で代用すると再計算が走り、ピークも構築コストも別物になる。
    reset_peak();
    let t0 = snap();
    let load_start = std::time::Instant::now();
    let result = indexer::load_or_scan_with_stats(scan, config.search.show_hidden_system);
    let load_ms = load_start.elapsed().as_secs_f64() * 1000.0;
    let t1 = snap();
    let n = result.entries.len();

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
    report("index.bin ロード（entries + masks）", t0, t1, n);

    let LoadOrScanResult {
        entries,
        cached_masks,
        rescan_task,
        ..
    } = result;
    // 背景再スキャンタスクは src-tauri 側で別スレッドが消費する。常駐計測の対象外。
    drop(rescan_task);

    // 内訳は engine 構築の**前**に取る（構築後は private フィールドで、かつ Vec は
    // move で吸われる）。計測区間の外＝`t2` の snapshot より前に置くこと。
    let accounted = report_breakdown(&entries, entries.capacity(), cached_masks.as_ref(), n);

    // 実 config どおりの migemo 設定で構築（= 実運用の常駐形）。
    reset_peak();
    let t2 = snap();
    let build_start = std::time::Instant::now();
    let engine = match cached_masks {
        Some(masks) => SearchEngine::new_with_cached_masks(
            entries,
            masks.char_masks,
            masks.file_name_char_masks,
            masks.lower_names,
            masks.lower_file_names,
            config.search.migemo_enabled,
        ),
        None => SearchEngine::new_with_migemo(entries, config.search.migemo_enabled),
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
    // 内訳は**構築前**の走査、常駐は**構築後**の実測である。両者の差は「未帰属」ではなく
    // 「構築が正味で増減させた分」を含む——`assemble` の `shrink_to_fit` は実際に減らす。
    // **差を埋める項を推測で足さない**。差が説明できないなら、それ自体が所見である。
    println!(
        "      構築前の内訳 {:.2} MiB → 構築後の常駐 {:.2} MiB（差 {:+.2} MiB・{:+.1} B/entry）",
        mib(accounted),
        mib(resident),
        delta_mib(resident, accounted),
        (resident as f64 - accounted as f64) / n as f64,
    );

    drop(engine);
    let t4 = snap();
    println!(
        "  drop 後の残留: {:.2} MiB（0 に近ければリークなし）",
        mib(t4.live.saturating_sub(t0.live))
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
