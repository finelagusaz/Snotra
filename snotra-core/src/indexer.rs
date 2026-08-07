//! スキャン対象の列挙・重複排除と、インデックスキャッシュ（`index.bin`）の入出力。
//!
//! `index.bin` への書き込みは `INDEX_WRITE_LOCK` で単一書き手に直列化し、tmp→rename の
//! 食い合いによる破損を防ぐ。キャッシュヒット時の背景再スキャンは `BackgroundRescanTask`
//! として返し、spawn とアイコン無効化は所有者（`src-tauri`）へ委ねる。

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::fs::Metadata;
use std::hash::{Hash, Hasher};
use std::os::windows::fs::MetadataExt;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM};
#[cfg(windows)]
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
};

use crate::binfmt::{BinFile, try_deserialize_with_header};
use crate::config::{Config, ScanPath};
use crate::query::{file_char_mask, lower_file_name, name_char_mask, to_lower_folded};

const INDEX_MAGIC: [u8; 4] = *b"INDX";
const INDEX_CACHE_VERSION: u32 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppEntry {
    pub name: String,
    pub target_path: String,
    pub is_folder: bool,
}

/// キャッシュから読み込んだ事前計算データ。SearchEngine の構築時に渡すことで
/// 起動時の計算をスキップし、起動時間を短縮する。
///
/// - `char_masks` / `file_name_char_masks`: v3+ キャッシュヒット時に常に存在
/// - `lower_names` / `lower_file_names`: v4+ ヒット時のみ存在
///   (v3 フォールバック時は None → Wave 1 計算が走る)
///
/// `normalized_keys` は持たない——`target_path` からの導出へ移して索引・オンディスクの
/// 双方から外した（`PERFORMANCE.md`「パスクエリ全走査のコスト — `normalized_keys` を
/// 保持するか導出するか」）。
#[derive(Debug)]
pub struct CachedMasks {
    pub char_masks: Vec<u64>,
    pub file_name_char_masks: Vec<u64>,
    /// A-3: v4+ キャッシュ時のみ Some。存在すれば SearchEngine の Wave 1 をスキップ。
    pub lower_names: Option<Vec<String>>,
    pub lower_file_names: Option<Vec<Option<String>>>,
}

pub fn scan_all(scan_paths: &[ScanPath], show_hidden_system: bool) -> Vec<AppEntry> {
    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for sp in scan_paths {
        let ext_set = build_extension_list(&sp.extensions);
        scan_directory_with_extensions(
            Path::new(&sp.path),
            &ext_set,
            sp.include_folders,
            show_hidden_system,
            &mut entries,
            &mut seen,
        );
    }

    entries
}

/// Recursively scan for files matching given extensions, optionally including folders
fn scan_directory_with_extensions(
    dir: &Path,
    extensions: &[String],
    include_folders: bool,
    show_hidden_system: bool,
    entries: &mut Vec<AppEntry>,
    seen: &mut std::collections::HashSet<String>,
) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };

        if !show_hidden_system && is_hidden_or_system(&meta) {
            continue;
        }

        if meta.is_dir() {
            if include_folders {
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                if !name.is_empty() {
                    let path_str = path.to_string_lossy();
                    let key = normalize_entry_key(path_str.as_ref());
                    if seen.insert(key) {
                        entries.push(AppEntry {
                            name,
                            target_path: path_str.into_owned(),
                            is_folder: true,
                        });
                    }
                }
            }
            scan_directory_with_extensions(
                &path,
                extensions,
                include_folders,
                show_hidden_system,
                entries,
                seen,
            );
        } else {
            let ext = path.extension().and_then(|e| e.to_str());
            let matches = ext.is_some_and(|e| matches_extension(extensions, e));
            if matches {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                let path_str = path.to_string_lossy();
                let key = normalize_entry_key(path_str.as_ref());
                if !name.is_empty() && seen.insert(key) {
                    entries.push(AppEntry {
                        name,
                        target_path: path_str.into_owned(),
                        is_folder: false,
                    });
                }
            }
        }
    }
}

/// エントリが hidden または system 属性を持つか判定する。
/// `folder.rs` の同名判定と逆極性の2重定義があったため、単一定義に統一（issue #437）。
/// `folder.rs::read_dir_entries` はこの関数をそのまま import して使う。
pub(crate) fn is_hidden_or_system(meta: &Metadata) -> bool {
    let attrs = meta.file_attributes();
    let hidden = (attrs & FILE_ATTRIBUTE_HIDDEN.0) != 0;
    let system = (attrs & FILE_ATTRIBUTE_SYSTEM.0) != 0;
    hidden || system
}

pub fn normalize_entry_key(path: &str) -> String {
    let mut normalized = String::with_capacity(path.len());
    normalize_entry_key_into(&mut normalized, path);
    normalized
}

/// [`normalize_entry_key`] を**確保済みバッファへ**書き出す。`buf` の中身は捨てられる。
///
/// 検索ホットパスは `normalized_key` を索引に持たず、ここで毎回導出する。呼び出し側は
/// スレッドローカルのバッファを使い回すので、暖まったあとの確保は起きない。
///
/// **規則の定義はこの関数 1 つである。** `normalize_entry_key` はこれの薄い包みであり、
/// 記録時（`indexer` の重複排除キー・`history` の全キー）と照合時（`search`）が同じ規則を
/// 通ることがバイト一致の根拠になる。**畳み込み比較を別実装で書き起こしてはならない**
/// ——1 バイトずれると履歴照合が沈黙で外れる（クラッシュせず検索結果も返り、
/// ブーストだけが効かなくなるので気づく手段が無い）。
///
/// ASCII 高速路を持つ。**ASCII 範囲では Unicode 小文字化と ASCII 小文字化の結果が一致する**
/// ため分岐しても結果は変わらず、実運用点（312,377 パス・非 ASCII 混じりは 1.7%）で
/// 全件一致を実測してある。支配項は `char::to_lowercase()` のテーブル参照で、
/// 高速路の有無でパスクエリ全走査が 9.2-11.7 倍から 1.7-2.3 倍へ変わる
/// （`PERFORMANCE.md`「パスクエリ全走査のコスト — `normalized_keys` を保持するか導出するか」）。
pub fn normalize_entry_key_into(buf: &mut String, path: &str) {
    buf.clear();
    let trimmed = path.trim();
    buf.reserve(trimmed.len());
    if trimmed.is_ascii() {
        // 一括で動かす。1 文字ずつ `push` すると `String: Extend<char>` が毎回 UTF-8 符号化の
        // 分岐を通り、実測で 2.5-3 倍遅くなる（`unsafe` なしでこの速度を出すのが要点）。
        // `/` は Windows パスにまず現れず（スキャナが `\` で組む）、`find` は即 `None` を返して
        // `push_str` 1 回の memcpy に落ちる。
        let mut rest = trimmed;
        while let Some(pos) = rest.find('/') {
            buf.push_str(&rest[..pos]);
            buf.push('\\');
            rest = &rest[pos + 1..];
        }
        buf.push_str(rest);
        // `buf` は先頭で空にしてあるので全体が今回の書き込みぶんである。
        buf.make_ascii_lowercase();
        return;
    }
    for ch in trimmed.chars() {
        if ch == '/' {
            buf.push('\\');
        } else {
            buf.extend(ch.to_lowercase());
        }
    }
}

fn build_extension_list(extensions: &[String]) -> Vec<String> {
    let mut normalized: Vec<String> = extensions
        .iter()
        .map(|ext| ext.trim_start_matches('.'))
        .filter(|ext| !ext.is_empty())
        .map(|ext| ext.to_ascii_lowercase())
        .collect();
    normalized.sort_unstable();
    normalized.dedup();
    normalized
}

fn matches_extension(extensions: &[String], ext: &str) -> bool {
    extensions
        .binary_search_by(|candidate| compare_ascii_lower(candidate.as_str(), ext))
        .is_ok()
}

fn compare_ascii_lower(lower: &str, raw: &str) -> std::cmp::Ordering {
    for (a, b) in lower.bytes().zip(raw.bytes()) {
        let b_lower = b.to_ascii_lowercase();
        match a.cmp(&b_lower) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
    }
    lower.len().cmp(&raw.len())
}

#[derive(Debug, Clone, Copy)]
pub struct LoadOrScanStats {
    pub cache_hit: bool,
    pub hash_ms: u128,
    pub cache_load_ms: u128,
    /// 背景再スキャン用の digest を取る時間（キャッシュヒット時のみ非ゼロ）。
    ///
    /// **この項目が無かったために、ここに居た全エントリ複製がどのフェーズにも現れなかった。**
    /// `cache_load_ms` は複製の前で止まり、複製は `total_ms` にしか効かない——差を読む者が
    /// いなければ、起動段の live ブロックの 1/3 を占める処理が計測上は存在しないままになる。
    /// **`cache_load_ms` と `total_ms` の間に処理を足すときは、必ずここに並ぶ項目を作ること。**
    pub digest_ms: u128,
    pub scan_ms: u128,
    pub sort_ms: u128,
    pub cache_save_ms: u128,
    pub total_ms: u128,
}

/// `load_or_scan_with_stats` の戻り値。
pub struct LoadOrScanResult {
    /// ロード or スキャンされたエントリ集合。
    pub entries: Vec<AppEntry>,
    /// キャッシュが無く（または stale で）フルスキャンが走った場合 true。
    pub cache_changed: bool,
    /// 各フェーズの所要時間。
    pub stats: LoadOrScanStats,
    /// v3/v4 キャッシュヒット時の事前計算データ。
    pub cached_masks: Option<CachedMasks>,
    /// キャッシュヒット時のみ `Some`。`src-tauri` が低優先度スレッドで `run()` し、
    /// `RescanOutcome::Changed` ならアイコンキャッシュを無効化する。
    pub rescan_task: Option<BackgroundRescanTask>,
}

/// v5 フォーマット: ビットマスクに加えて lower_names / lower_file_names を保存。
/// 起動時に SearchEngine の Wave 1（to_lower_folded）を完全スキップできる。
///
/// **v4 との差は `normalized_keys` を持たないことだけである**（実測 35.56 MiB / 312,377 件）。
/// `target_path` から `normalize_entry_key_into` で導出できる純粋な派生であり、検索時に
/// 必要な候補についてだけ詰め直す形へ移した（`PERFORMANCE.md`「パスクエリ全走査のコスト —
/// `normalized_keys` を保持するか導出するか」）。
///
/// **owned/borrowed を単一 struct に統合する（`Cow<'a, [T]>`）**。save は `Cow::Borrowed` で
/// `entries` の全件 clone を避けてシリアライズし、load は `Cow::Owned` で deserialize する
/// （`IndexCache<'static>`）。単一 struct ゆえ「owned 版と borrowed 版でフィールド順がズレて
/// `index.bin` を無言破損する」footgun は型として起こり得ない。`Cow<[T]>` は Borrowed/Owned とも
/// 内側スライスの `serialize_seq` に委譲し `Vec<T>`/`&[T]` とバイト列が一致するため、
/// Cow 化そのものはバイト形式を変えない。形式の絶対安定は
/// `index_cache_on_disk_format_is_stable`（golden bytes）でガードする。
#[derive(Serialize, Deserialize)]
struct IndexCache<'a> {
    built_at: u64,
    entries: Cow<'a, [AppEntry]>,
    config_hash: u64,
    char_masks: Cow<'a, [u64]>,
    file_name_char_masks: Cow<'a, [u64]>,
    lower_names: Cow<'a, [String]>,
    lower_file_names: Cow<'a, [Option<String>]>,
}

/// v4 フォールバック用スキーマ（末尾に `normalized_keys` を持つ旧形式）。
///
/// **読むだけで捨てる。** 導出可能な派生を持たない形へ移したため、v4 バイト列から
/// 復元するのは v5 と同じ 4 本（マスク 2 本 + lower 2 本）である。**それらは v4 にも
/// 揃っているので Wave 1 はスキップされたまま**で、v4 ユーザーの初回起動は遅くならない。
#[derive(Deserialize)]
struct IndexCacheV4 {
    #[allow(dead_code)]
    built_at: u64,
    entries: Vec<AppEntry>,
    config_hash: u64,
    char_masks: Vec<u64>,
    file_name_char_masks: Vec<u64>,
    lower_names: Vec<String>,
    lower_file_names: Vec<Option<String>>,
    #[allow(dead_code)]
    normalized_keys: Vec<String>,
}

/// v3 フォールバック用スキーマ（ビットマスクのみ、lower names なし）。
#[derive(Serialize, Deserialize)]
struct IndexCacheV3 {
    #[allow(dead_code)]
    built_at: u64,
    entries: Vec<AppEntry>,
    config_hash: u64,
    char_masks: Vec<u64>,
    file_name_char_masks: Vec<u64>,
}

/// v2 フォールバック用スキーマ（ビットマスクフィールドなし）。
/// v2 キャッシュをヒットした場合はマスクなし（None）で返し、
/// SearchEngine::new() が通常通りマスクを計算する。
#[derive(Serialize, Deserialize)]
struct IndexCacheV2 {
    #[allow(dead_code)]
    built_at: u64,
    entries: Vec<AppEntry>,
    config_hash: u64,
}

fn compute_config_hash(scan: &[ScanPath], show_hidden_system: bool) -> u64 {
    let mut hasher = DefaultHasher::new();
    for sp in scan {
        sp.path.hash(&mut hasher);
        sp.extensions.hash(&mut hasher);
        sp.include_folders.hash(&mut hasher);
    }
    show_hidden_system.hash(&mut hasher);
    hasher.finish()
}

fn cache_bin_file_in(dir: &Path) -> BinFile {
    BinFile::new_in(dir, INDEX_MAGIC, INDEX_CACHE_VERSION, "index.bin")
}

/// Load cached entries or scan the filesystem. Returns `(entries, cache_changed)`
/// where `cache_changed = true` means the cache was missing/stale and a full scan ran.
pub fn load_or_scan(scan: &[ScanPath], show_hidden_system: bool) -> (Vec<AppEntry>, bool) {
    let result = load_or_scan_with_stats(scan, show_hidden_system);
    (result.entries, result.cache_changed)
}

/// `index.bin` に載っているときだけエントリを返す（**走査は絶対にしない**）。実データを
/// corpus として使うテスト専用。
///
/// **`load_or_scan_with_stats` で代用してはならない。** あちらの cache-miss の枝は全走査を
/// 走らせたうえで `index.bin` を書く。`INDEX_WRITE_LOCK` はプロセス内の `static` ゆえ、
/// **テストバイナリが複数プロセスに分かれると排他が効かず**、固定 tmp 名（`index.bin.tmp`）の
/// 食い合いでキャッシュが壊れうる（`snotra-core/CLAUDE.md`「index.bin 書き込みの排他」）。
/// 開発者の `cargo test` が黙って C ドライブを全走査する副作用も同時に消える。
///
/// **既定のスイートで実データを読むテストはすべてここを通すこと**（`search/tests/path.rs` の
/// corpus 2 件と `tests/path_query_cost.rs` の `derives_same_bytes_as_normalize_entry_key`）。
/// `#[ignore]` の計測ハーネスのうち `cached_masks` を要するものだけは
/// [`load_or_scan_with_stats`] のままでよい——手元で 1 つずつ意図して走らせるものであり、
/// 実運用の起動経路を測ることが目的だからである。
#[doc(hidden)]
pub fn load_cached_entries(scan: &[ScanPath], show_hidden_system: bool) -> Option<Vec<AppEntry>> {
    let hash = compute_config_hash(scan, show_hidden_system);
    Some(load_cache(hash)?.entries)
}

/// Same as `load_or_scan`, but returns the full `LoadOrScanResult`: timing stats,
/// cached bitmasks, and—on cache hit—a `BackgroundRescanTask` for the caller to
/// run on a background thread.
pub fn load_or_scan_with_stats(scan: &[ScanPath], show_hidden_system: bool) -> LoadOrScanResult {
    let total_started = Instant::now();

    let hash_started = Instant::now();
    let current_hash = compute_config_hash(scan, show_hidden_system);
    let hash_ms = hash_started.elapsed().as_millis();

    let cache_load_started = Instant::now();
    if let Some((result, rescan_generation)) =
        load_with_index_generation(|| load_cache(current_hash))
    {
        let cache_load_ms = cache_load_started.elapsed().as_millis();
        let return_entries = result.entries;
        let cached_masks = result.cached_masks;
        // 背景再スキャンはここでは spawn しない。タスクとして返し、`src-tauri` が
        // `AppHandle` を持った状態で spawn する（`Changed` 時のアイコン無効化のため）。
        //
        // **エントリを複製せず digest を取る。** 複製していた頃は起動のたびに 624,754 個の
        // `String` を確保しており（実測・ロード段の live ブロックの 1/3）、しかも
        // `cache_load_ms` が**この行の前で止まる**ため、どのフェーズ計測にも現れなかった。
        let digest_started = Instant::now();
        let rescan_task = BackgroundRescanTask {
            scan: scan.to_vec(),
            show_hidden_system,
            config_hash: current_hash,
            cached_digest: entries_digest(&return_entries),
            generation: rescan_generation,
        };
        let digest_ms = digest_started.elapsed().as_millis();
        let stats = LoadOrScanStats {
            cache_hit: true,
            hash_ms,
            cache_load_ms,
            digest_ms,
            scan_ms: 0,
            sort_ms: 0,
            cache_save_ms: 0,
            total_ms: total_started.elapsed().as_millis(),
        };
        return LoadOrScanResult {
            entries: return_entries,
            cache_changed: false,
            stats,
            cached_masks,
            rescan_task: Some(rescan_task),
        };
    }
    let cache_load_ms = cache_load_started.elapsed().as_millis();

    // 権威的書き手: scan + sort + save を書き込みロック保持下で行い、
    // 背景再スキャン / 別ビルドとの index.bin 同時書き込みを防ぐ。
    // フェーズ計測はクロージャの戻り値として持ち出す。
    let (entries, scan_ms, sort_ms, cache_save_ms) = with_index_write_lock(|| {
        let scan_started = Instant::now();
        let mut entries = scan_all(scan, show_hidden_system);
        let scan_ms = scan_started.elapsed().as_millis();

        let sort_started = Instant::now();
        sort_entries_canonical(&mut entries);
        let sort_ms = sort_started.elapsed().as_millis();

        let cache_save_started = Instant::now();
        save_cache_sorted(&entries, current_hash);
        let cache_save_ms = cache_save_started.elapsed().as_millis();

        (entries, scan_ms, sort_ms, cache_save_ms)
    });

    let stats = LoadOrScanStats {
        cache_hit: false,
        hash_ms,
        cache_load_ms,
        // cache-miss の枝は背景再スキャンを返さない（走査したてが最新である）。
        digest_ms: 0,
        scan_ms,
        sort_ms,
        cache_save_ms,
        total_ms: total_started.elapsed().as_millis(),
    };

    LoadOrScanResult {
        entries,
        cache_changed: true,
        stats,
        cached_masks: None,
        rescan_task: None,
    }
}

/// ソート済みエントリ列の digest。背景再スキャンが「索引が変わったか」を判定するためだけに
/// 使う（かつては全エントリの複製を抱えて逐一比較していた）。
///
/// **`entries_equal` が見ていた 3 フィールドと長さをすべて混ぜること。** 1 つでも落とすと、
/// その種の変更を**確率ではなく確定で**永久に見逃す（`is_folder` を落とせば folder↔file の
/// 反転が永遠に検出されない）。列はどちらも `sort_entries_canonical` を通っており
/// （保存 3 経路と `try_background_rescan_in` のすべてで直前に呼ぶ）、順序は canonical である。
///
/// **`index.bin` へは書かない。** 突き合わせるのはロード直後に取った値と、背景スレッドが
/// 走査後に取った値——どちらも同じプロセス・同じバイナリである。ゆえに `DefaultHasher` の
/// 出力が Rust のバージョン間で変わっても影響しない（ディスクへ書く `compute_config_hash` は
/// 版が変わるとキャッシュが 1 度無効になる、という性質を持つ。こちらはそれを背負わない）。
///
/// **受容する残余: 衝突（2⁻⁶⁴）は「変わったのに Unchanged と報告する」形で現れる。**
/// 索引は次にファイルシステムが変わるまで古いままになり、アイコンキャッシュも無効化されない。
/// **一時的ではなく、その状態が続く限り持続する**——同じ組み合わせなら毎回同じ衝突になる。
/// `compute_config_hash` は同じ u64 でキャッシュの有効性そのものを判定しており（衝突すれば
/// **別の config の索引をそのまま使う**）、こちらの帰結はそれより軽い。
fn entries_digest(entries: &[AppEntry]) -> u64 {
    /// 塊の大きさ。**値を変えると digest が変わる**（分割が畳み込みの形を決めるため）。
    /// プロセス内でしか比較しないので問題にならない——上の doc がその契約である。
    const CHUNK: usize = 8192;

    // 塊ごとに並列でハッシュし、**塊の順に**畳む。長さを先に混ぜるので分割は一意に決まり、
    // 境界の曖昧さは生じない。**解放と違ってハッシュは並列化が効く**——実測 43 → 12 ms
    // （直列版と各 3 回の最小値で比較。`search/build.rs` の共有の潰しは同じ手が効かなかった）。
    let chunk_digests: Vec<u64> = entries
        .par_chunks(CHUNK)
        .map(|chunk| {
            let mut hasher = DefaultHasher::new();
            for e in chunk {
                e.name.hash(&mut hasher);
                e.target_path.hash(&mut hasher);
                e.is_folder.hash(&mut hasher);
            }
            hasher.finish()
        })
        .collect();

    let mut hasher = DefaultHasher::new();
    entries.len().hash(&mut hasher);
    for d in chunk_digests {
        d.hash(&mut hasher);
    }
    hasher.finish()
}

fn sort_entries_canonical(entries: &mut [AppEntry]) {
    entries.sort_by(|a, b| {
        a.target_path
            .cmp(&b.target_path)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.is_folder.cmp(&b.is_folder))
    });
}

fn save_cache_sorted(entries: &[AppEntry], config_hash: u64) {
    let Some(dir) = Config::config_dir() else {
        return;
    };
    save_cache_sorted_in(&dir, entries, config_hash);
}

/// `save_cache_sorted` と同じ保存処理を `dir` 注入で行う（統合テスト用、issue #429）。
fn save_cache_sorted_in(dir: &Path, entries: &[AppEntry], config_hash: u64) {
    let bf = cache_bin_file_in(dir);

    // マスクを計算してキャッシュに含める。起動時に SearchEngine::new_with_cached_masks()
    // がマスク再計算をスキップできるようにする。
    let lower_names: Vec<String> = entries.iter().map(|e| to_lower_folded(&e.name)).collect();
    let lower_file_names: Vec<Option<String>> = entries
        .iter()
        .map(|e| lower_file_name(&e.target_path))
        .collect();
    let char_masks: Vec<u64> = lower_names.iter().map(|n| name_char_mask(n)).collect();
    let file_name_char_masks: Vec<u64> = lower_file_names
        .iter()
        .map(|n| file_char_mask(n.as_deref()))
        .collect();
    // Cow::Borrowed で entries の全件 clone を避ける（派生 Vec も参照で渡す）。
    // 出力バイト列は Owned 版と同一（golden テストで保証）。
    let cache = IndexCache {
        built_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        entries: Cow::Borrowed(entries),
        config_hash,
        char_masks: Cow::Borrowed(&char_masks),
        file_name_char_masks: Cow::Borrowed(&file_name_char_masks),
        lower_names: Cow::Borrowed(&lower_names),
        lower_file_names: Cow::Borrowed(&lower_file_names),
    };
    if !bf.save(&cache) {
        eprintln!("[indexer] failed to save {}", bf.path().display());
    }
}

/// Force rebuild: scan and save cache, regardless of existing cache.
/// Called from settings dialog (Phase 5).
pub fn rebuild_and_save(scan: &[ScanPath], show_hidden_system: bool) -> Vec<AppEntry> {
    // 権威的書き手: scan + sort + save を書き込みロック保持下で行い、
    // 背景再スキャン / 別の rebuild との index.bin 同時書き込みを防ぐ。
    with_index_write_lock(|| {
        let mut entries = scan_all(scan, show_hidden_system);
        sort_entries_canonical(&mut entries);
        let config_hash = compute_config_hash(scan, show_hidden_system);
        save_cache_sorted(&entries, config_hash);
        entries
    })
}

/// キャッシュ読み込み結果。v3 ヒット時はマスク付き、v2 ヒット時はマスクなし。
struct LoadCacheResult {
    entries: Vec<AppEntry>,
    cached_masks: Option<CachedMasks>,
}

fn load_cache(config_hash: u64) -> Option<LoadCacheResult> {
    let dir = Config::config_dir()?;
    load_cache_in(&dir, config_hash)
}

/// `load_cache` と同じ読み込みを `dir` 注入で行う（統合テスト用、issue #429）。
fn load_cache_in(dir: &Path, config_hash: u64) -> Option<LoadCacheResult> {
    let bf = cache_bin_file_in(dir);
    let bytes = bf.load_bytes()?;

    // v5 (現行): ビットマスク + lower names。
    // deserialize は Cow::Owned を返すため .into_owned() は clone なしの move。
    if let Ok(cache) =
        try_deserialize_with_header::<IndexCache<'static>>(&bytes, INDEX_MAGIC, INDEX_CACHE_VERSION)
    {
        if cache.config_hash != config_hash {
            return None;
        }
        let masks = CachedMasks {
            char_masks: cache.char_masks.into_owned(),
            file_name_char_masks: cache.file_name_char_masks.into_owned(),
            lower_names: Some(cache.lower_names.into_owned()),
            lower_file_names: Some(cache.lower_file_names.into_owned()),
        };
        return Some(LoadCacheResult {
            entries: cache.entries.into_owned(),
            cached_masks: Some(masks),
        });
    }

    // v4 フォールバック: 末尾の normalized_keys を**読んで捨てる**。復元する 4 本は v5 と同じで
    // どれも v4 に揃っているため、**Wave 1 はスキップされたまま**（v4 ユーザーの初回起動は
    // 遅くならない）。次回の save で v5 へ昇格する。
    if let Ok(cache) = try_deserialize_with_header::<IndexCacheV4>(&bytes, INDEX_MAGIC, 4) {
        if cache.config_hash != config_hash {
            return None;
        }
        let masks = CachedMasks {
            char_masks: cache.char_masks,
            file_name_char_masks: cache.file_name_char_masks,
            lower_names: Some(cache.lower_names),
            lower_file_names: Some(cache.lower_file_names),
        };
        return Some(LoadCacheResult {
            entries: cache.entries,
            cached_masks: Some(masks),
        });
    }

    // v3 フォールバック: ビットマスクのみ（lower names なし → Wave 1 は実行）
    if let Ok(cache) = try_deserialize_with_header::<IndexCacheV3>(&bytes, INDEX_MAGIC, 3) {
        if cache.config_hash != config_hash {
            return None;
        }
        let masks = CachedMasks {
            char_masks: cache.char_masks,
            file_name_char_masks: cache.file_name_char_masks,
            lower_names: None,
            lower_file_names: None,
        };
        return Some(LoadCacheResult {
            entries: cache.entries,
            cached_masks: Some(masks),
        });
    }

    // v2 フォールバック (マスクなし)
    if let Ok(cache) = try_deserialize_with_header::<IndexCacheV2>(&bytes, INDEX_MAGIC, 2) {
        if cache.config_hash != config_hash {
            return None;
        }
        return Some(LoadCacheResult {
            entries: cache.entries,
            cached_masks: None,
        });
    }

    None
}

/// `index.bin` の scan + save 区間を直列化する書き込みロック。
/// 権威的ビルド（`rebuild_and_save` / cache-miss save）と背景再スキャンが共有する。
/// `save_cache_sorted` 自体はロックを取らない（呼び出し側が保持する契約）。
static INDEX_WRITE_LOCK: Mutex<()> = Mutex::new(());
/// 権威的な index 書き込みの開始ごとに進む世代。
/// 背景タスクは生成時の値を保持し、ロック取得後に一致を検査する。
static INDEX_GENERATION: AtomicU64 = AtomicU64::new(0);

fn current_index_generation() -> u64 {
    INDEX_GENERATION.load(Ordering::Relaxed)
}

fn snapshot_index_generation() -> u64 {
    let _guard = INDEX_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    current_index_generation()
}

/// キャッシュ読み込み前の世代と読み込み結果を対にする。
/// この順序により、読み込み後に権威的ビルドが始まった場合、返却する背景タスクは
/// 必ず古い世代を保持し、新しい `index.bin` を上書きせず `Skipped` になる。
fn load_with_index_generation<T>(load: impl FnOnce() -> Option<T>) -> Option<(T, u64)> {
    let generation = snapshot_index_generation();
    load().map(|value| (value, generation))
}

/// 書き込みロックを非ブロッキングで取得し、取れたらクロージャを実行して `Some(結果)` を返す。
/// 取得できなければクロージャを実行せず `None` を返す。背景再スキャン等の日和見的書き手が使う。
fn try_with_index_write_lock<R>(f: impl FnOnce() -> R) -> Option<R> {
    let _guard = INDEX_WRITE_LOCK.try_lock().ok()?;
    Some(f())
}

/// 書き込みロックをブロッキングで取得し、クロージャを実行して結果を返す。
/// 権威的書き手（`rebuild_and_save` / cache-miss save）が使う。
fn with_index_write_lock<R>(f: impl FnOnce() -> R) -> R {
    // Mutex<()> は保持する状態を持たないため、poison しても into_inner で回復して継続する。
    // （`.unwrap()` だと一度の panic 以降、全 index 書き込みが永久に panic する）
    let _guard = INDEX_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    INDEX_GENERATION.fetch_add(1, Ordering::Relaxed);
    f()
}

/// 背景再スキャンの結果。`src-tauri` 側はこれを見てアイコン無効化等の後始末を行う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RescanOutcome {
    /// 権威的ビルドが書き込み中でロックを取得できず、再スキャンをスキップした。
    Skipped,
    /// 再スキャンしたが、エントリ集合はキャッシュと同一だった。
    Unchanged,
    /// エントリ集合がキャッシュと異なり、`index.bin` を更新した。
    /// 呼び出し側はアイコンキャッシュの無効化を行うこと。
    Changed,
}

/// 背景再スキャン本体。書き込みロックを取得できなければ（権威的ビルドが進行中）
/// スキャン・保存をせず `Skipped` を返す。再スキャンは日和見的な鮮度維持であり、
/// 本式ビルドが走っていれば不要。アイコンキャッシュには触れない（`icons.bin` は
/// `src-tauri` の資源 — 呼び出し側が `Changed` を見て無効化する）。
fn try_background_rescan(
    scan: &[ScanPath],
    show_hidden_system: bool,
    config_hash: u64,
    cached_digest: u64,
    generation: u64,
) -> RescanOutcome {
    let Some(dir) = Config::config_dir() else {
        return RescanOutcome::Skipped;
    };
    try_background_rescan_in(
        &dir,
        scan,
        show_hidden_system,
        config_hash,
        cached_digest,
        generation,
    )
}

fn try_background_rescan_in(
    dir: &Path,
    scan: &[ScanPath],
    show_hidden_system: bool,
    config_hash: u64,
    cached_digest: u64,
    generation: u64,
) -> RescanOutcome {
    // 権威的ビルド（rebuild_and_save / cache-miss save）が書き込み中なら Skipped。
    let changed = try_with_index_write_lock(|| {
        // 世代検査は書き込みロック取得後に行う。検査後に権威的ビルドが割り込む
        // TOCTOU を防ぎ、古い snapshot が新しい index.bin を巻き戻さないため。
        if current_index_generation() != generation {
            return None;
        }
        let mut scanned = scan_all(scan, show_hidden_system);
        sort_entries_canonical(&mut scanned);
        if entries_digest(&scanned) == cached_digest {
            Some(false)
        } else {
            save_cache_sorted_in(dir, &scanned, config_hash);
            Some(true)
        }
    });
    match changed {
        None | Some(None) => RescanOutcome::Skipped,
        Some(Some(false)) => RescanOutcome::Unchanged,
        Some(Some(true)) => RescanOutcome::Changed,
    }
}

/// 背景再スキャンのタスク。所有データを抱え、`src-tauri` 側のスレッドへ `move` できる。
/// `load_or_scan_with_stats` がキャッシュヒット時に `Some` で返す。
///
/// **全エントリの複製ではなく digest を持つ。** かつては `Vec<AppEntry>` を丸ごと抱えており、
/// 起動のたびに 312,377 件ぶんの `String` を 624,754 個確保していた（実測 62.5 MiB・ロード段の
/// live ブロックの 1/3）。比較の消費点は `entries_digest`（private）1 か所で真偽値しか要らない。
pub struct BackgroundRescanTask {
    scan: Vec<ScanPath>,
    show_hidden_system: bool,
    config_hash: u64,
    cached_digest: u64,
    generation: u64,
}

impl BackgroundRescanTask {
    /// 再スキャンを実行し、結果を返す。`Changed` のときは呼び出し側が
    /// アイコンキャッシュを無効化すること。
    pub fn run(self) -> RescanOutcome {
        try_background_rescan(
            &self.scan,
            self.show_hidden_system,
            self.config_hash,
            self.cached_digest,
            self.generation,
        )
    }
}

/// 呼び出し元スレッドの優先度を下げる。背景再スキャン等のバックグラウンドスレッドが
/// 起動直後のユーザー操作と CPU を奪い合わないようにする。`src-tauri` が rescan
/// スレッドの先頭で呼ぶ。
#[cfg(windows)]
pub fn lower_current_thread_priority() {
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

#[cfg(not(windows))]
pub fn lower_current_thread_priority() {}

/// レジストリキーの RAII ガード。Drop 時に自動で RegCloseKey を呼ぶ。
#[cfg(windows)]
struct RegKeyGuard(windows::Win32::System::Registry::HKEY);

#[cfg(windows)]
impl Drop for RegKeyGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::System::Registry::RegCloseKey(self.0);
        }
    }
}

/// ユーザー環境変数の PATH を読み取る（HKCU\Environment\Path）。
/// システム PATH（System32 等）は含まない。
/// REG_EXPAND_SZ の場合は環境変数を展開して返す。
#[cfg(windows)]
fn read_user_path() -> Option<String> {
    use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
    use windows::Win32::System::Registry::*;
    use windows::core::w;

    unsafe {
        let mut raw_key = HKEY::default();
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Environment"),
            Some(0),
            KEY_READ,
            &mut raw_key,
        )
        .ok()
        .ok()?;
        let key = RegKeyGuard(raw_key);

        let mut data_type = REG_VALUE_TYPE::default();
        let mut buf_size: u32 = 0;

        // サイズ取得
        let status = RegQueryValueExW(
            key.0,
            w!("Path"),
            None,
            Some(&mut data_type),
            None,
            Some(&mut buf_size),
        );
        if status.is_err() || buf_size == 0 {
            return None;
        }

        // 値取得
        let mut buf = vec![0u16; (buf_size as usize) / 2];
        RegQueryValueExW(
            key.0,
            w!("Path"),
            None,
            Some(&mut data_type),
            Some(buf.as_mut_ptr() as *mut u8),
            Some(&mut buf_size),
        )
        .ok()
        .ok()?;

        // null terminator を除去
        while buf.last() == Some(&0) {
            buf.pop();
        }

        // REG_EXPAND_SZ の場合は環境変数を展開
        if data_type == REG_EXPAND_SZ {
            // null terminator を付加して ExpandEnvironmentStringsW に渡す
            buf.push(0);
            let required =
                ExpandEnvironmentStringsW(windows::core::PCWSTR::from_raw(buf.as_ptr()), None);
            if required == 0 {
                buf.pop(); // remove null terminator
                return Some(String::from_utf16_lossy(&buf));
            }
            let mut expanded = vec![0u16; required as usize];
            ExpandEnvironmentStringsW(
                windows::core::PCWSTR::from_raw(buf.as_ptr()),
                Some(&mut expanded),
            );
            // null terminator を除去
            while expanded.last() == Some(&0) {
                expanded.pop();
            }
            Some(String::from_utf16_lossy(&expanded))
        } else {
            Some(String::from_utf16_lossy(&buf))
        }
    }
    // key は RegKeyGuard の Drop で自動クローズ
}

#[cfg(not(windows))]
fn read_user_path() -> Option<String> {
    None
}

/// セミコロン区切りのパスリストからディレクトリを平坦スキャンし、
/// 既存エントリにない実行ファイルを返す。
///
/// `read_user_path` から分離することでテスト可能性を確保。
fn scan_path_dirs(
    path_list: &str,
    existing_entries: &[AppEntry],
    show_hidden_system: bool,
) -> Vec<AppEntry> {
    let mut seen: std::collections::HashSet<String> = existing_entries
        .iter()
        .map(|e| normalize_entry_key(&e.target_path))
        .collect();

    let path_exts = ["exe", "bat", "cmd", "com"];
    let mut new_entries = Vec::new();

    for dir_str in path_list.split(';') {
        let dir_str = dir_str.trim();
        if dir_str.is_empty() {
            continue;
        }
        let dir = Path::new(dir_str);
        let Ok(read_dir) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_dir() {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if !show_hidden_system && is_hidden_or_system(&meta) {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase());
            let Some(ref ext) = ext else { continue };
            if !path_exts.contains(&ext.as_str()) {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                continue;
            }
            let path_str = path.to_string_lossy().into_owned();
            let key = normalize_entry_key(&path_str);
            if seen.insert(key) {
                new_entries.push(AppEntry {
                    name,
                    target_path: path_str,
                    is_folder: false,
                });
            }
        }
    }

    new_entries
}

/// ユーザー PATH のディレクトリを平坦スキャンし、既存エントリにない実行ファイルを返す。
///
/// - レジストリ `HKCU\Environment\Path` から読み取る（システム PATH を含まない）
/// - `REG_EXPAND_SZ` の環境変数は展開済み
/// - 再帰スキャンなし（PATH ディレクトリの直下のみ）
/// - 対象拡張子: .exe / .bat / .cmd
/// - `existing_entries` に同一パスがあるものは返さない（normalize_entry_key で判定）
/// - PATH ディレクトリ間での重複も排除する
pub fn scan_path_env(existing_entries: &[AppEntry], show_hidden_system: bool) -> Vec<AppEntry> {
    let user_path = match read_user_path() {
        Some(p) if !p.is_empty() => p,
        _ => return Vec::new(),
    };
    scan_path_dirs(&user_path, existing_entries, show_hidden_system)
}

/// CachedMasks の各 Vec に新しいエントリの分を追記する。
/// インデックスキャッシュの恩恵を維持しつつ、PATH エントリ等の追加分を補完する。
///
/// `char_masks` / `file_name_char_masks` は常に追記。
/// `lower_names` / `lower_file_names` / `normalized_keys` は Some の場合のみ追記。
/// `kana_lower_names` は SearchEngine 側で entries から直接計算されるためここでは扱わない。
pub fn extend_cached_masks(masks: &mut CachedMasks, new_entries: &[AppEntry]) {
    for entry in new_entries {
        let lower = to_lower_folded(&entry.name);
        let lower_file = lower_file_name(&entry.target_path);

        let mask = name_char_mask(&lower);
        let file_mask = file_char_mask(lower_file.as_deref());

        masks.char_masks.push(mask);
        masks.file_name_char_masks.push(file_mask);

        if let Some(ref mut ln) = masks.lower_names {
            ln.push(lower);
        }
        if let Some(ref mut lfn) = masks.lower_file_names {
            lfn.push(lower_file);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binfmt::{try_deserialize_with_header, try_serialize_with_header};
    use std::fs;

    /// `INDEX_WRITE_LOCK` に触れるテストを直列化するガード。
    /// `cargo test` は同一ファイル内のテストを並列実行するため、「ロック空き」を
    /// 期待するテストと「ロック保持中」を作るテストが食い合わないよう、
    /// これらのテストは先頭でこのガードを取得する。
    static INDEX_LOCK_TEST_GUARD: Mutex<()> = Mutex::new(());

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("snotra_idx_test_{}", tag));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn scan_with_extensions_filters_by_ext() {
        let dir = temp_dir("ext_filter");
        fs::write(dir.join("app.exe"), "").unwrap();
        fs::write(dir.join("script.bat"), "").unwrap();
        fs::write(dir.join("readme.txt"), "").unwrap();

        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let exts = build_extension_list(&["exe".to_string(), "bat".to_string()]);
        scan_directory_with_extensions(&dir, &exts, false, true, &mut entries, &mut seen);

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"app"));
        assert!(names.contains(&"script"));
        assert!(!names.contains(&"readme"));
        assert!(entries.iter().all(|e| !e.is_folder));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_with_extensions_includes_folders() {
        let dir = temp_dir("ext_folders");
        fs::write(dir.join("app.exe"), "").unwrap();
        fs::create_dir(dir.join("subdir")).unwrap();

        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let exts = build_extension_list(&["exe".to_string()]);
        scan_directory_with_extensions(&dir, &exts, true, true, &mut entries, &mut seen);

        let folder_entries: Vec<&AppEntry> = entries.iter().filter(|e| e.is_folder).collect();
        assert_eq!(folder_entries.len(), 1);
        assert_eq!(folder_entries[0].name, "subdir");
        assert!(folder_entries[0].is_folder);

        let file_entries: Vec<&AppEntry> = entries.iter().filter(|e| !e.is_folder).collect();
        assert_eq!(file_entries.len(), 1);
        assert_eq!(file_entries[0].name, "app");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_with_extensions_no_folders_when_disabled() {
        let dir = temp_dir("ext_no_folders");
        fs::write(dir.join("app.exe"), "").unwrap();
        fs::create_dir(dir.join("subdir")).unwrap();

        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let exts = build_extension_list(&["exe".to_string()]);
        scan_directory_with_extensions(&dir, &exts, false, true, &mut entries, &mut seen);

        assert!(entries.iter().all(|e| !e.is_folder));
        assert_eq!(entries.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_keeps_same_name_different_paths() {
        let dir = temp_dir("ext_dedup");
        let sub1 = dir.join("a");
        let sub2 = dir.join("b");
        fs::create_dir_all(&sub1).unwrap();
        fs::create_dir_all(&sub2).unwrap();
        fs::write(sub1.join("tool.exe"), "").unwrap();
        fs::write(sub2.join("tool.exe"), "").unwrap();

        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let exts = build_extension_list(&["exe".to_string()]);
        scan_directory_with_extensions(&dir, &exts, false, true, &mut entries, &mut seen);

        let tools: Vec<&AppEntry> = entries.iter().filter(|e| e.name == "tool").collect();
        assert_eq!(tools.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_extensions_case_insensitive() {
        let dir = temp_dir("ext_case");
        fs::write(dir.join("app.EXE"), "").unwrap();

        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let exts = build_extension_list(&["exe".to_string()]);
        scan_directory_with_extensions(&dir, &exts, false, true, &mut entries, &mut seen);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "app");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn index_cache_binary_roundtrip() {
        let entries = vec![
            AppEntry {
                name: "Firefox".to_string(),
                target_path: "C:\\apps\\firefox.lnk".to_string(),
                is_folder: false,
            },
            AppEntry {
                name: "Projects".to_string(),
                target_path: "C:\\Projects".to_string(),
                is_folder: true,
            },
        ];

        let cache = IndexCache {
            built_at: 1700000000,
            entries: Cow::Owned(entries.clone()),
            config_hash: 12345,
            char_masks: Cow::Owned(vec![0xAB, 0xCD]),
            file_name_char_masks: Cow::Owned(vec![0x12, 0x34]),
            lower_names: Cow::Owned(vec!["firefox".to_string(), "projects".to_string()]),
            lower_file_names: Cow::Owned(vec![Some("firefox.lnk".to_string()), None]),
        };

        let bytes =
            try_serialize_with_header(INDEX_MAGIC, INDEX_CACHE_VERSION, &cache).expect("serialize");
        let restored: IndexCache<'static> =
            try_deserialize_with_header(&bytes, INDEX_MAGIC, INDEX_CACHE_VERSION)
                .expect("deserialize");

        assert_eq!(restored.built_at, 1700000000);
        assert_eq!(restored.entries.len(), 2);
        assert_eq!(restored.entries[0].name, "Firefox");
        assert!(!restored.entries[0].is_folder);
        assert_eq!(restored.entries[1].name, "Projects");
        assert!(restored.entries[1].is_folder);
        assert_eq!(restored.config_hash, 12345);
        // Cow フィールドは into_owned() で Vec に戻して比較（deserialize は Owned ゆえ move）。
        assert_eq!(restored.char_masks.into_owned(), vec![0xABu64, 0xCD]);
        assert_eq!(
            restored.file_name_char_masks.into_owned(),
            vec![0x12u64, 0x34]
        );
        assert_eq!(
            restored.lower_names.into_owned(),
            vec!["firefox", "projects"]
        );
        assert_eq!(
            restored.lower_file_names.into_owned(),
            vec![Some("firefox.lnk".to_string()), None]
        );
    }

    #[test]
    fn index_cache_on_disk_format_is_stable() {
        // on-disk バイト形式の絶対安定を守る golden テスト。
        // IndexCache のフィールド順・型を変えると（= 既存 index.bin を無言破損）バイト列が変化し
        // このテストが落ちる。save/load が単一 struct を共有する統合後、フィールド reorder は
        // roundtrip テストを素通りするため、この golden が唯一の検出器（version 非バンプでも検出）。
        // 意図的な形式変更（INDEX_CACHE_VERSION バンプ）時は golden を更新すること。
        let entries = vec![
            AppEntry {
                name: "Firefox".to_string(),
                target_path: "C:\\apps\\firefox.lnk".to_string(),
                is_folder: false,
            },
            AppEntry {
                name: "Projects".to_string(),
                target_path: "C:\\Projects".to_string(),
                is_folder: true,
            },
        ];
        let char_masks = vec![0xABu64, 0xCD];
        let file_name_char_masks = vec![0x12u64, 0x34];
        let lower_names = vec!["firefox".to_string(), "projects".to_string()];
        let lower_file_names = vec![Some("firefox.lnk".to_string()), None];

        // save 経路と同じ Cow::Borrowed で構築する。
        let cache = IndexCache {
            built_at: 1_700_000_000,
            entries: Cow::Borrowed(&entries),
            config_hash: 12345,
            char_masks: Cow::Borrowed(&char_masks),
            file_name_char_masks: Cow::Borrowed(&file_name_char_masks),
            lower_names: Cow::Borrowed(&lower_names),
            lower_file_names: Cow::Borrowed(&lower_file_names),
        };
        let bytes =
            try_serialize_with_header(INDEX_MAGIC, INDEX_CACHE_VERSION, &cache).expect("serialize");

        // 凍結 golden（固定 fixture の serialize 出力・INDX magic + version 5 ヘッダー込み）。
        // 形式変更時のみ更新する。
        // **v4 golden との差は先頭のバージョンバイトと、末尾の `normalized_keys` 列の欠落だけ**
        // （下の `frozen_v4_bytes_still_load_with_lower_names` と並べて読める形にしてある）。
        const GOLDEN_V5: &[u8] = &[
            73, 78, 68, 88, 5, 0, 0, 0, 128, 226, 207, 170, 6, 2, 7, 70, 105, 114, 101, 102, 111,
            120, 19, 67, 58, 92, 97, 112, 112, 115, 92, 102, 105, 114, 101, 102, 111, 120, 46, 108,
            110, 107, 0, 8, 80, 114, 111, 106, 101, 99, 116, 115, 11, 67, 58, 92, 80, 114, 111,
            106, 101, 99, 116, 115, 1, 185, 96, 2, 171, 1, 205, 1, 2, 18, 52, 2, 7, 102, 105, 114,
            101, 102, 111, 120, 8, 112, 114, 111, 106, 101, 99, 116, 115, 2, 1, 11, 102, 105, 114,
            101, 102, 111, 120, 46, 108, 110, 107, 0,
        ];
        assert_eq!(
            bytes, GOLDEN_V5,
            "on-disk 形式が変化した。IndexCache のフィールド順/型変更は既存 index.bin を破損する。\
             意図的なら INDEX_CACHE_VERSION をバンプし golden を更新すること"
        );

        let restored: IndexCache<'static> =
            try_deserialize_with_header(GOLDEN_V5, INDEX_MAGIC, INDEX_CACHE_VERSION)
                .expect("凍結 v5 バイトがロードできること");
        assert!(matches!(restored.entries, Cow::Owned(_)));
        assert_eq!(restored.entries.len(), 2);
        assert_eq!(restored.entries[0].name, "Firefox");
        assert_eq!(restored.entries[0].target_path, "C:\\apps\\firefox.lnk");
        assert!(!restored.entries[0].is_folder);
        assert_eq!(restored.entries[1].name, "Projects");
        assert!(restored.entries[1].is_folder);
        assert_eq!(restored.char_masks.into_owned(), char_masks);
        assert_eq!(restored.lower_names.into_owned(), lower_names);
    }

    /// v5 化の前に実際に書かれていた v4 バイト列（同じ fixture の serialize 出力で、末尾に
    /// `normalized_keys` を持つ）。`config_hash` は 12345、entries は Firefox / Projects の 2 件。
    const GOLDEN_V4: &[u8] = &[
        73, 78, 68, 88, 4, 0, 0, 0, 128, 226, 207, 170, 6, 2, 7, 70, 105, 114, 101, 102, 111, 120,
        19, 67, 58, 92, 97, 112, 112, 115, 92, 102, 105, 114, 101, 102, 111, 120, 46, 108, 110,
        107, 0, 8, 80, 114, 111, 106, 101, 99, 116, 115, 11, 67, 58, 92, 80, 114, 111, 106, 101,
        99, 116, 115, 1, 185, 96, 2, 171, 1, 205, 1, 2, 18, 52, 2, 7, 102, 105, 114, 101, 102, 111,
        120, 8, 112, 114, 111, 106, 101, 99, 116, 115, 2, 1, 11, 102, 105, 114, 101, 102, 111, 120,
        46, 108, 110, 107, 0, 2, 19, 99, 58, 92, 97, 112, 112, 115, 92, 102, 105, 114, 101, 102,
        111, 120, 46, 108, 110, 107, 11, 99, 58, 92, 112, 114, 111, 106, 101, 99, 116, 115,
    ];

    /// **v4 の凍結バイト列**（v5 化の前に実際に書かれていた形式。同じ fixture の
    /// serialize 出力で、末尾に `normalized_keys` を持つ）から、新コードが
    /// `lower_names` / `lower_file_names` を復元できることを示す。
    ///
    /// 向きが要点である——新コードの出力を golden 化しても forward-stability しか
    /// 示せない。**旧形式の凍結バイトを入力にして初めて後方互換の証拠になる**
    /// （`snotra-core/CLAUDE.md`「データ永続化の注意」）。
    ///
    /// 対になる `v4_index_bin_loads_through_load_cache_in_with_wave1_skipped` が、同じ
    /// バイト列を **`load_cache_in` 経由で**読む（分岐の選択と戻り値まで含めて測る）。
    #[test]
    fn frozen_v4_bytes_still_load_with_lower_names() {
        // v5 として読もうとすると失敗する（末尾に余分な normalized_keys が残るため）。
        assert!(
            try_deserialize_with_header::<IndexCache>(GOLDEN_V4, INDEX_MAGIC, INDEX_CACHE_VERSION)
                .is_err(),
            "v4 バイトが v5 として読めてはならない"
        );

        let restored: IndexCacheV4 =
            try_deserialize_with_header(GOLDEN_V4, INDEX_MAGIC, 4).expect("v4 として読めること");
        assert_eq!(restored.entries.len(), 2);
        assert_eq!(restored.entries[0].name, "Firefox");
        assert_eq!(restored.char_masks, vec![0xABu64, 0xCD]);
        assert_eq!(restored.lower_names, vec!["firefox", "projects"]);
        assert_eq!(
            restored.lower_file_names,
            vec![Some("firefox.lnk".to_string()), None]
        );
        // 捨てる側も、読めていること自体は確かめておく（形式のずれを黙って通さない）。
        assert_eq!(
            restored.normalized_keys,
            vec!["c:\\apps\\firefox.lnk", "c:\\projects"]
        );
    }

    /// **v4 の `index.bin` を `load_cache_in` 経由で読む。** 上の struct 単体テストとは層が違う
    /// ——こちらは「どの分岐が選ばれ、`CachedMasks` に何が入って返るか」を測る。
    ///
    /// `lower_names` / `lower_file_names` が Some で返ることが、**v4 ユーザーの初回起動で
    /// Wave 1 が走らない**ことの根拠である（`new_with_cached_masks` のスキップ判定はこの
    /// 2 本が揃っているかで決まる）。v4 分岐を消すと struct 単体テストは通ったままここが落ちる。
    #[test]
    fn v4_index_bin_loads_through_load_cache_in_with_wave1_skipped() {
        let dir = temp_dir("v4_fallback_through_load_cache_in");
        fs::write(dir.join("index.bin"), GOLDEN_V4).expect("write v4 index.bin");

        let result = load_cache_in(&dir, 12345).expect("v4 の index.bin が読めること");
        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.entries[0].name, "Firefox");
        let masks = result.cached_masks.expect("v4 でもマスクは返る");
        assert_eq!(masks.char_masks, vec![0xABu64, 0xCD]);
        assert_eq!(
            masks.lower_names,
            Some(vec!["firefox".to_string(), "projects".to_string()]),
            "v4 から lower_names が復元されないと Wave 1 が走り、初回起動が遅くなる"
        );
        assert_eq!(
            masks.lower_file_names,
            Some(vec![Some("firefox.lnk".to_string()), None])
        );

        // config_hash が違えば stale 扱いで None（v5 経路と同じ規律）。
        assert!(load_cache_in(&dir, 12346).is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_cache_sorted_in_then_load_cache_in_roundtrip() {
        // issue #429: BinFile の dir 注入経路（save_cache_sorted_in / load_cache_in）が
        // 実ファイル I/O を通して往復することを検証する（旧来は config_dir 固定で統合テスト不可）。
        let dir = temp_dir("cache_dir_injection_roundtrip");
        let entries = vec![
            AppEntry {
                name: "Firefox".to_string(),
                target_path: "C:\\apps\\firefox.lnk".to_string(),
                is_folder: false,
            },
            AppEntry {
                name: "Projects".to_string(),
                target_path: "C:\\Projects".to_string(),
                is_folder: true,
            },
        ];
        let config_hash = 42u64;

        save_cache_sorted_in(&dir, &entries, config_hash);

        let result = load_cache_in(&dir, config_hash).expect("load cache written to dir");
        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.entries[0].name, "Firefox");
        assert_eq!(result.entries[1].name, "Projects");
        let masks = result.cached_masks.expect("v5 cache should include masks");
        assert_eq!(
            masks.lower_names,
            Some(vec!["firefox".to_string(), "projects".to_string()])
        );

        // config_hash が異なると stale 扱いで None
        assert!(load_cache_in(&dir, config_hash.wrapping_add(1)).is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_cache_v2_migrates_to_no_masks() {
        // v2 フォーマット（マスクなし）のキャッシュを読み込んだとき
        // cached_masks が None で返ることを確認する。
        let entries = vec![AppEntry {
            name: "Firefox".to_string(),
            target_path: "C:\\apps\\firefox.lnk".to_string(),
            is_folder: false,
        }];
        let config_hash = 999u64;

        let cache_v2 = IndexCacheV2 {
            built_at: 0,
            entries: entries.clone(),
            config_hash,
        };
        let bytes = try_serialize_with_header(INDEX_MAGIC, 2, &cache_v2).expect("serialize v2");

        // try_deserialize_with_header で v2 として読める
        let restored: IndexCacheV2 =
            try_deserialize_with_header(&bytes, INDEX_MAGIC, 2).expect("deserialize v2");
        assert_eq!(restored.entries[0].name, "Firefox");
        assert_eq!(restored.config_hash, config_hash);

        // v4 として読もうとすると失敗する（フィールドが足りない）
        let v4_result =
            try_deserialize_with_header::<IndexCache>(&bytes, INDEX_MAGIC, INDEX_CACHE_VERSION);
        assert!(v4_result.is_err(), "v2 bytes should not deserialize as v4");
    }

    #[test]
    fn load_cache_v3_fallback_yields_masks_without_lower_names() {
        // v3 フォーマット（ビットマスクあり、lower names なし）のキャッシュを読み込んだとき
        // CachedMasks に char_masks が入り、lower_names が None で返ることを確認する。
        let entries = vec![AppEntry {
            name: "Firefox".to_string(),
            target_path: "C:\\apps\\firefox.lnk".to_string(),
            is_folder: false,
        }];
        let config_hash = 42u64;

        let cache_v3 = IndexCacheV3 {
            built_at: 0,
            entries: entries.clone(),
            config_hash,
            char_masks: vec![0xAB],
            file_name_char_masks: vec![0xCD],
        };
        let bytes = try_serialize_with_header(INDEX_MAGIC, 3, &cache_v3).expect("serialize v3");

        let restored: IndexCacheV3 =
            try_deserialize_with_header(&bytes, INDEX_MAGIC, 3).expect("deserialize v3");
        assert_eq!(restored.char_masks, vec![0xAB]);

        // v4 として読もうとすると失敗する（lower_names フィールドがない）
        let v4_result =
            try_deserialize_with_header::<IndexCache>(&bytes, INDEX_MAGIC, INDEX_CACHE_VERSION);
        assert!(v4_result.is_err(), "v3 bytes should not deserialize as v4");
    }

    #[test]
    fn config_hash_changes_with_different_paths() {
        let scan1 = vec![ScanPath {
            path: "C:\\A".to_string(),
            extensions: vec![".lnk".to_string()],
            include_folders: false,
        }];
        let scan2 = vec![ScanPath {
            path: "C:\\B".to_string(),
            extensions: vec![".lnk".to_string()],
            include_folders: false,
        }];
        let hash1 = compute_config_hash(&scan1, false);
        let hash2 = compute_config_hash(&scan2, false);
        assert_ne!(hash1, hash2);
    }

    // digest の検査は「同じなら一致」と「1 フィールドずつ違えば不一致」を**フィールドごとに
    // 分けて**置く。まとめると、落としたフィールドがどれか失敗名から分からなくなる
    // ——digest が `is_folder` を混ぜ忘れる類の誤りは、確率ではなく**確定で**その種の変更を
    // 永久に見逃すため、名指しできることに意味がある（`entries_digest` の doc）。

    #[test]
    fn entries_digest_identical() {
        let a = vec![
            AppEntry {
                name: "A".into(),
                target_path: "C:\\a.exe".into(),
                is_folder: false,
            },
            AppEntry {
                name: "B".into(),
                target_path: "C:\\b".into(),
                is_folder: true,
            },
        ];
        let b = a.clone();
        assert_eq!(entries_digest(&a), entries_digest(&b));
    }

    #[test]
    fn entries_digest_different_length() {
        let a = vec![AppEntry {
            name: "A".into(),
            target_path: "C:\\a.exe".into(),
            is_folder: false,
        }];
        let b = vec![
            AppEntry {
                name: "A".into(),
                target_path: "C:\\a.exe".into(),
                is_folder: false,
            },
            AppEntry {
                name: "B".into(),
                target_path: "C:\\b.exe".into(),
                is_folder: false,
            },
        ];
        assert_ne!(entries_digest(&a), entries_digest(&b));
    }

    #[test]
    fn entries_digest_different_name() {
        let a = vec![AppEntry {
            name: "A".into(),
            target_path: "C:\\a.exe".into(),
            is_folder: false,
        }];
        let b = vec![AppEntry {
            name: "B".into(),
            target_path: "C:\\a.exe".into(),
            is_folder: false,
        }];
        assert_ne!(entries_digest(&a), entries_digest(&b));
    }

    #[test]
    fn entries_digest_different_target() {
        let a = vec![AppEntry {
            name: "A".into(),
            target_path: "C:\\a.exe".into(),
            is_folder: false,
        }];
        let b = vec![AppEntry {
            name: "A".into(),
            target_path: "C:\\b.exe".into(),
            is_folder: false,
        }];
        assert_ne!(entries_digest(&a), entries_digest(&b));
    }

    #[test]
    fn entries_digest_different_is_folder() {
        let a = vec![AppEntry {
            name: "A".into(),
            target_path: "C:\\a".into(),
            is_folder: false,
        }];
        let b = vec![AppEntry {
            name: "A".into(),
            target_path: "C:\\a".into(),
            is_folder: true,
        }];
        assert_ne!(entries_digest(&a), entries_digest(&b));
    }

    #[test]
    fn entries_digest_both_empty() {
        assert_eq!(entries_digest(&[]), entries_digest(&[]));
    }

    /// 隣り合うフィールドの境界が曖昧だと、`name` と `target_path` の切れ目が動いただけの
    /// 別物が同じ digest になる。`Hash for str` が長さを混ぜるため実際には起きないが、
    /// **手書きの `write` へ変えた瞬間に壊れる**種類の性質なのでここで固定する。
    #[test]
    fn entries_digest_field_boundary_is_not_ambiguous() {
        let a = vec![AppEntry {
            name: "AB".into(),
            target_path: "C".into(),
            is_folder: false,
        }];
        let b = vec![AppEntry {
            name: "A".into(),
            target_path: "BC".into(),
            is_folder: false,
        }];
        assert_ne!(entries_digest(&a), entries_digest(&b));
    }

    /// エントリの境界も同じ理由で曖昧であってはならない（1 件が 2 件に割れても気づく）。
    #[test]
    fn entries_digest_entry_boundary_is_not_ambiguous() {
        let a = vec![AppEntry {
            name: "AB".into(),
            target_path: "P".into(),
            is_folder: false,
        }];
        let b = vec![
            AppEntry {
                name: "A".into(),
                target_path: "P".into(),
                is_folder: false,
            },
            AppEntry {
                name: "B".into(),
                target_path: "P".into(),
                is_folder: false,
            },
        ];
        assert_ne!(entries_digest(&a), entries_digest(&b));
    }

    #[test]
    fn sorted_comparison_ignores_enumeration_order() {
        let mut a = vec![
            AppEntry {
                name: "B".into(),
                target_path: "C:\\b.exe".into(),
                is_folder: false,
            },
            AppEntry {
                name: "A".into(),
                target_path: "C:\\a.exe".into(),
                is_folder: false,
            },
        ];
        let mut b = vec![
            AppEntry {
                name: "A".into(),
                target_path: "C:\\a.exe".into(),
                is_folder: false,
            },
            AppEntry {
                name: "B".into(),
                target_path: "C:\\b.exe".into(),
                is_folder: false,
            },
        ];

        sort_entries_canonical(&mut a);
        sort_entries_canonical(&mut b);
        // **digest は順序に依存する**（列を順に混ぜるため）。列挙順の違いを吸収しているのは
        // `sort_entries_canonical` のほうであり、比較を digest へ替えたことでこの検査は
        // 「あれば望ましい」から**必須**になった——ソートを外した経路が 1 つでもできると、
        // 中身が同じでも「変わった」と判定して毎回 index.bin を書き直す。
        assert_eq!(entries_digest(&a), entries_digest(&b));
    }

    #[test]
    fn canonical_sort_orders_by_target_then_name_then_is_folder() {
        let mut entries = vec![
            AppEntry {
                name: "B".into(),
                target_path: "C:\\a.exe".into(),
                is_folder: true,
            },
            AppEntry {
                name: "A".into(),
                target_path: "C:\\b.exe".into(),
                is_folder: false,
            },
            AppEntry {
                name: "A".into(),
                target_path: "C:\\a.exe".into(),
                is_folder: false,
            },
            AppEntry {
                name: "A".into(),
                target_path: "C:\\a.exe".into(),
                is_folder: true,
            },
        ];

        sort_entries_canonical(&mut entries);

        assert_eq!(entries[0].target_path, "C:\\a.exe");
        assert_eq!(entries[0].name, "A");
        assert!(!entries[0].is_folder);

        assert_eq!(entries[1].target_path, "C:\\a.exe");
        assert_eq!(entries[1].name, "A");
        assert!(entries[1].is_folder);

        assert_eq!(entries[2].target_path, "C:\\a.exe");
        assert_eq!(entries[2].name, "B");
        assert!(entries[2].is_folder);

        assert_eq!(entries[3].target_path, "C:\\b.exe");
        assert_eq!(entries[3].name, "A");
        assert!(!entries[3].is_folder);
    }

    #[test]
    fn config_hash_changes_with_different_scan() {
        let scan1 = vec![ScanPath {
            path: "C:\\Tools".to_string(),
            extensions: vec![".exe".to_string()],
            include_folders: false,
        }];
        let scan2 = vec![ScanPath {
            path: "C:\\Tools".to_string(),
            extensions: vec![".exe".to_string(), ".bat".to_string()],
            include_folders: false,
        }];
        let hash1 = compute_config_hash(&scan1, false);
        let hash2 = compute_config_hash(&scan2, false);
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn scan_all_empty_when_no_paths() {
        let entries = scan_all(&[], false);
        assert!(
            entries.is_empty(),
            "scan_all with no paths should return empty"
        );
    }

    #[test]
    fn try_background_rescan_skips_when_write_lock_held() {
        let _serial = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // 権威的なインデックスビルドが書き込みロックを保持している状況を再現する。
        let _held = INDEX_WRITE_LOCK.lock().unwrap();
        // 背景再スキャンは書き込みロックを取得できないため、
        // スキャンも保存もせず Skipped を返さねばならない。
        let outcome = try_background_rescan(
            &[],
            false,
            0,
            entries_digest(&[]),
            current_index_generation(),
        );
        assert_eq!(
            outcome,
            RescanOutcome::Skipped,
            "background rescan must return Skipped when the index write lock is held"
        );
    }

    #[test]
    fn background_rescan_task_run_reports_unchanged_for_empty_inputs() {
        let _serial = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // 空のスキャン対象 → scan_all は空 → 空のキャッシュと一致 → Unchanged。
        let task = BackgroundRescanTask {
            scan: Vec::new(),
            show_hidden_system: false,
            config_hash: 0,
            cached_digest: entries_digest(&[]),
            generation: current_index_generation(),
        };
        assert_eq!(task.run(), RescanOutcome::Unchanged);
    }

    #[test]
    fn stale_background_rescan_cannot_overwrite_newer_index_generation() {
        let _serial = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("stale_background_rescan");
        let old_hash = 41;
        let new_hash = 42;
        let old_generation = current_index_generation();

        with_index_write_lock(|| {
            save_cache_sorted_in(&dir, &[], new_hash);
        });

        let outcome = try_background_rescan_in(
            &dir,
            &[],
            false,
            old_hash,
            entries_digest(&[AppEntry {
                name: "stale".to_string(),
                target_path: "C:\\stale.exe".to_string(),
                is_folder: false,
            }]),
            old_generation,
        );

        assert_eq!(outcome, RescanOutcome::Skipped);
        assert!(load_cache_in(&dir, new_hash).is_some());
        assert!(load_cache_in(&dir, old_hash).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rescan_generation_is_snapshotted_before_cache_load() {
        let _serial = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let generation_before_load = current_index_generation();

        let (_, task_generation) = load_with_index_generation(|| {
            with_index_write_lock(|| {});
            Some(())
        })
        .expect("the injected cache load should succeed");

        assert_eq!(task_generation, generation_before_load);
        assert_ne!(task_generation, current_index_generation());
    }

    #[test]
    fn try_with_index_write_lock_skips_closure_when_lock_held() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let _serial = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // 権威的なインデックスビルドが書き込みロックを保持している状況を再現する。
        let _held = INDEX_WRITE_LOCK.lock().unwrap();
        // ロックを取得できないので、クロージャは実行されず None が返らねばならない。
        let ran = AtomicBool::new(false);
        let result = try_with_index_write_lock(|| ran.store(true, Ordering::SeqCst));
        assert!(
            !ran.load(Ordering::SeqCst),
            "closure must not run while the index write lock is held"
        );
        assert!(
            result.is_none(),
            "try_with_index_write_lock must return None when the lock is held"
        );
    }

    #[test]
    fn with_index_write_lock_holds_lock_during_closure() {
        let _serial = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // with_index_write_lock がクロージャ実行中ずっとロックを保持していることを、
        // 「クロージャ内から try_lock すると失敗する」という形で決定論的に検証する。
        // ブロッキング取得なので、他テストがロック保持中でも待つだけで flaky にならない。
        let observed_locked = with_index_write_lock(|| INDEX_WRITE_LOCK.try_lock().is_err());
        assert!(
            observed_locked,
            "with_index_write_lock must hold INDEX_WRITE_LOCK while running the closure"
        );
    }

    #[test]
    fn try_with_index_write_lock_runs_closure_when_lock_free() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let _serial = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // ロックが空いていればクロージャを実行し、Some(結果) を返す。
        // 回帰テスト: 「スキップ」を通した最小実装が同じ2行でこの経路も満たすため即パスする。
        let ran = AtomicBool::new(false);
        let result = try_with_index_write_lock(|| {
            ran.store(true, Ordering::SeqCst);
            42
        });
        assert!(
            ran.load(Ordering::SeqCst),
            "closure must run when the index write lock is free"
        );
        assert_eq!(
            result,
            Some(42),
            "try_with_index_write_lock must return Some(closure result) when the lock is free"
        );
    }

    #[test]
    #[cfg(windows)]
    fn read_user_path_does_not_contain_unexpanded_vars() {
        // HKCU\Environment\Path は存在しない環境もあるため、
        // Some が返った場合のみ展開結果を検証する
        if let Some(path) = read_user_path() {
            assert!(!path.contains('%'), "環境変数が未展開: {path}");
        }
    }

    #[test]
    fn scan_path_dirs_adds_new_entries() {
        let dir = temp_dir("path_add");
        fs::write(dir.join("tool.exe"), "").unwrap();
        fs::write(dir.join("script.bat"), "").unwrap();

        let path_list = dir.to_string_lossy().to_string();
        let entries = scan_path_dirs(&path_list, &[], true);

        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.name == "tool"));
        assert!(entries.iter().any(|e| e.name == "script"));
        assert!(entries.iter().all(|e| !e.is_folder));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_path_dirs_skips_existing_paths() {
        let dir = temp_dir("path_skip");
        fs::write(dir.join("tool.exe"), "").unwrap();

        let existing = vec![AppEntry {
            name: "tool".to_string(),
            target_path: dir.join("tool.exe").to_string_lossy().into_owned(),
            is_folder: false,
        }];

        let path_list = dir.to_string_lossy().to_string();
        let entries = scan_path_dirs(&path_list, &existing, true);

        assert!(entries.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_path_dirs_ignores_non_executable_extensions() {
        let dir = temp_dir("path_exts");
        fs::write(dir.join("tool.exe"), "").unwrap();
        fs::write(dir.join("lib.dll"), "").unwrap();
        fs::write(dir.join("readme.txt"), "").unwrap();
        fs::write(dir.join("data.json"), "").unwrap();

        let path_list = dir.to_string_lossy().to_string();
        let entries = scan_path_dirs(&path_list, &[], true);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "tool");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_path_dirs_deduplicates_across_dirs() {
        let dir = temp_dir("path_dedup");
        fs::write(dir.join("tool.exe"), "").unwrap();

        // 同じディレクトリを2回指定
        let path_list = format!("{};{}", dir.display(), dir.display());
        let entries = scan_path_dirs(&path_list, &[], true);

        assert_eq!(entries.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_path_dirs_handles_nonexistent_dir() {
        let entries = scan_path_dirs("C:\\nonexistent_dir_12345", &[], true);
        assert!(entries.is_empty());
    }

    #[test]
    fn scan_path_dirs_handles_empty_path_list() {
        let entries = scan_path_dirs("", &[], true);
        assert!(entries.is_empty());
    }

    #[test]
    fn extend_cached_masks_grows_all_vecs() {
        let mut masks = CachedMasks {
            char_masks: vec![0xAB],
            file_name_char_masks: vec![0xCD],
            lower_names: Some(vec!["existing".to_string()]),
            lower_file_names: Some(vec![Some("existing.lnk".to_string())]),
        };

        let new_entries = vec![AppEntry {
            name: "tool".to_string(),
            target_path: "C:\\bin\\tool.exe".to_string(),
            is_folder: false,
        }];

        extend_cached_masks(&mut masks, &new_entries);

        assert_eq!(masks.char_masks.len(), 2);
        assert_eq!(masks.file_name_char_masks.len(), 2);
        assert_eq!(masks.lower_names.as_ref().unwrap().len(), 2);
        assert_eq!(masks.lower_file_names.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn extend_cached_masks_handles_none_optional_fields() {
        let mut masks = CachedMasks {
            char_masks: vec![0xAB],
            file_name_char_masks: vec![0xCD],
            lower_names: None,
            lower_file_names: None,
        };

        let new_entries = vec![AppEntry {
            name: "tool".to_string(),
            target_path: "C:\\bin\\tool.exe".to_string(),
            is_folder: false,
        }];

        extend_cached_masks(&mut masks, &new_entries);

        assert_eq!(masks.char_masks.len(), 2);
        assert_eq!(masks.file_name_char_masks.len(), 2);
        assert!(masks.lower_names.is_none());
        assert!(masks.lower_file_names.is_none());
    }
}
