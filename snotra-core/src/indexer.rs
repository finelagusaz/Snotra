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
use crate::index_tree::IndexTree;
use crate::query::{
    file_char_mask, lower_file_name, measure_derived_sharing, name_char_mask, to_lower_folded,
};

const INDEX_MAGIC: [u8; 4] = *b"INDX";
/// `index.bin` の現行フォーマット版。
///
/// 計測ハーネス（`tests/memory_footprint.rs`）が「読めた版が現行版か」を判定するために読む。
/// **版のリテラルを他所へ焼き込まないこと**——反復 8 で v6 へ上げたとき、ハーネスの注記だけ
/// が `5` のまま取り残され、「現行は v5。実運用点は v6 のまま」という**それ自体が矛盾した**
/// 文を出し続けた（現行が v5 なら v6 は存在しえない）。
#[doc(hidden)]
pub const INDEX_CACHE_VERSION: u32 = 7;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppEntry {
    pub name: String,
    pub target_path: String,
    pub is_folder: bool,
}

/// 事前計算済みの派生データ。SearchEngine の構築時に渡すことで起動時の計算をスキップする。
///
/// **出所は 2 つある。** `index.bin` から読んだもの（キャッシュヒット）と、
/// `save_cache_sorted_in` が書いたその足で返したもの（反復 11 以降）。**後者を受け取る経路を数え上げてはならない**——正本は `save_cache_sorted` を呼ぶ側であり、数えた散文は経路が増えるたびに腐る。**どちらも同じ表現で返る**——潰し方の判定は `query::measure_derived_sharing` の 1 か所を通るので、消費側は出所を区別しない（区別するのは `lower` の variant だけである）。
///
/// - `char_masks` / `file_name_char_masks`: この型が在るなら必ず在る
/// - `lower`: 派生文字列を持たない古い版を読んだときは `None` → Wave 1 計算が走る。
///   **版の番号を書かない**（`Engine::from_material` の doc と同じ理由で、番号を書くと版を上げるたびにこの散文だけが腐る）
///
/// `normalized_keys` は持たない——`target_path` からの導出へ移して索引・オンディスクの
/// 双方から外した（`PERFORMANCE.md`「パスクエリ全走査のコスト — `normalized_keys` を
/// 保持するか導出するか」）。
/// `PartialEq` は**テストのときだけ**持つ。「返した組と `index.bin` へ書いた組が同一である」
/// を 1 行で言うためであり、製品はこの型を比べない。
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct CachedMasks {
    pub char_masks: Vec<u64>,
    pub file_name_char_masks: Vec<u64>,
    /// v4+ キャッシュ時のみ `Some`。存在すれば `SearchEngine` の Wave 1 をスキップする。
    pub lower: Option<CachedLower>,
}

/// `lower_file_names` のオンディスク表現（v6）。
///
/// **`Option<String>` では足りない。** `None` には「file name 成分が無い」という先客がおり、
/// そこへ「`lower_names[i]` と同一」を重ねると 2 つの意味が同じ表現に乗る。メモリ側は
/// `CompactEntry::file_name_is_lower_name`（構造体の空きパディング）で解いたが、**ディスクに
/// 空きパディングは無い**——旗を別の `Vec<bool>` で持つと 0.30 MiB 余分にかかり、しかも
/// 2 本の Vec の対応がずれても型は何も言わない。3 状態を 1 つの enum に閉じると、
/// postcard のタグ 1 バイトだけで済み**意味も型に載る**（実測 1.11 MiB 対 1.41 MiB）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LowerFileName {
    /// file name 成分が無い（`query::lower_file_name` が `None` を返した）。
    Absent,
    /// 解決後の `lower_name` とバイト一致する。
    SameAsLowerName,
    /// 独自の文字列。
    Text(String),
}

/// キャッシュから復元した派生文字列。**潰し済みか未測定かを型で区別する。**
///
/// **分ける理由は「測り直しが無駄だから」である**（`search/build.rs` の `DerivedStrings` の doc
/// が機序の正本）。潰し済みの列を測定経路へ流しても結果は変わらないが、312,690 回の比較が
/// 丸ごと無駄になる。variant を分けることで、その取り違えはコンパイルを通らない。
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub enum CachedLower {
    /// v6 以降。記録時に `query::measure_derived_sharing` で測って潰してある。
    /// `assemble` は測り直さず、この潰し方をそのまま索引の表現として使う。
    Collapsed {
        /// `None` = `entries[i].name` と同一。
        lower_names: Vec<Option<String>>,
        lower_file_names: Vec<LowerFileName>,
    },
    /// v5 / v4。全件が実体を持つ未測定の列。`assemble` が測って潰す。
    Raw {
        lower_names: Vec<String>,
        lower_file_names: Vec<Option<String>>,
    },
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
    /// `index.bin` をバイト列として読む時間。**`cache_load_ms` の内数である**
    /// （他の項目と違い、フェーズの和には足さない）。
    ///
    /// `cache_load_ms` は「読む」と「deserialize する」の 2 つを 1 つの数にしており、
    /// **両者はオンディスク形式の変更に対して逆向きに振る舞う**——読むバイトを減らせば前者は
    /// 減るが、形式を圧縮すれば後者は増えうる。分けずに測ると、どちらが効いたのか原理的に
    /// 区別できない。cache-miss の枝では 0（読む対象が無い）。
    pub cache_read_ms: u128,
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
    /// ロード or スキャンされた索引の材料（木と派生データの組）。
    ///
    /// **`Vec<AppEntry>` ではない**——v7 は `target_path` をディスクに持たないので、
    /// 実体へ戻すと削った 312,691 回の確保がその場で復活する。
    ///
    /// **組のまま渡す。** ほどいた 2 値を持ち回せると、木を伸ばしてマスクを追記し忘れる形が書けるようになる（正本は [`IndexMaterial`] の doc）。
    pub material: IndexMaterial,
    /// キャッシュが無く（または stale で）フルスキャンが走った場合 true。
    pub cache_changed: bool,
    /// 各フェーズの所要時間。
    pub stats: LoadOrScanStats,
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
    /// 表示名（`AppEntry.name`）。
    names: Cow<'a, [String]>,
    is_folder: Cow<'a, [bool]>,
    /// 木の親（`crate::index_tree::TreeNodes::parent_of`）。
    parent: Cow<'a, [u32]>,
    /// 木の `table` 添字（`crate::index_tree::TreeNodes::aux_of`）。
    aux: Cow<'a, [u32]>,
    /// 拡張子と、親を持たないエントリのフルパスの intern 表。
    table: Cow<'a, [String]>,
    /// 保存時に測った整列の旗。**5 列と違い、読む側は検証せずそのまま信じる**——
    /// 理由と被害範囲は [`crate::index_tree::IndexTree::from_parts`] の doc に書いてある
    /// （壊れた値の帰結は同スコアの tie-break の順序に閉じる）。
    sorted_by_path: bool,
    config_hash: u64,
    char_masks: Cow<'a, [u64]>,
    file_name_char_masks: Cow<'a, [u64]>,
    /// `None` = `names[i]` とバイト一致（実データで 86.6%）。
    lower_names: Cow<'a, [Option<String>]>,
    /// 3 状態（`LowerFileName`）。「無い」と「`lower_name` と同一」を別の値で表す。
    lower_file_names: Cow<'a, [LowerFileName]>,
}

/// v6 フォールバック用スキーマ（`target_path` を全件そのまま持つ旧形式）。
///
/// **v7 との差は `target_path` の表現だけである。** v6 は 312,691 件のフルパスを実体で持ち
/// （実測 36.01 MiB・ディスクの 70%）、v7 は木の親と拡張子 id に置き換えて持たない。
/// 読み込みはどちらも成功し、**違うのは確保の回数**——v6 を読むと `String` を 312,691 個
/// 余分に作り、`PathStore` へ組み替えた `assemble` が即座に捨てる。
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct IndexCacheV6 {
    #[allow(dead_code)]
    built_at: u64,
    entries: Vec<AppEntry>,
    config_hash: u64,
    char_masks: Vec<u64>,
    file_name_char_masks: Vec<u64>,
    lower_names: Vec<Option<String>>,
    lower_file_names: Vec<LowerFileName>,
}

/// v5 フォールバック用スキーマ（派生文字列を全件そのまま持つ旧形式）。
///
/// **v6 との差は `lower_names` / `lower_file_names` の表現だけである。** v5 は全 312,690 件を
/// 実体で持ち（実測 21.63 MiB）、v6 は共有を測って潰した形で持つ（1.90 MiB）。読み込みは
/// どちらも成功し、**違うのは確保の回数**——v5 を読むと 625,380 個の `String` を作り、
/// うち 527,000 個は `assemble` が即座に捨てる。
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct IndexCacheV5 {
    #[allow(dead_code)]
    built_at: u64,
    entries: Vec<AppEntry>,
    config_hash: u64,
    char_masks: Vec<u64>,
    file_name_char_masks: Vec<u64>,
    lower_names: Vec<String>,
    lower_file_names: Vec<Option<String>>,
}

/// v4 フォールバック用スキーマ（末尾に `normalized_keys` を持つ旧形式）。
///
/// **読むだけで捨てる。** 導出可能な派生を持たない形へ移したため、v4 バイト列から
/// 復元するのは v5 と同じ 4 本（マスク 2 本 + lower 2 本）である。**それらは v4 にも
/// 揃っているので Wave 1 はスキップされたまま**で、v4 ユーザーの初回起動は遅くならない。
/// `Serialize` は**テストのときだけ**derive する。製品が v4 を書く経路はもう無い
/// （読むだけ・書くのは常に現行版）が、「中身は同じだが版だけ古い」状況を作る治具には要る。
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
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
///
/// **返すのは木を実体へ戻したものであり、ディスクに在った文字列ではない**（v7 は
/// `target_path` を持たない）。ゆえに「組み直しが原文と一致するか」をこの返り値と
/// 突き合わせても、**組み直しの結果どうしを比べることになる**。その照合の接地は
/// `index_tree_raw_matches_frozen_v6_specimen`（旧形式の凍結バイト列が唯一の原文）が持つ。
#[doc(hidden)]
pub fn load_cached_entries(scan: &[ScanPath], show_hidden_system: bool) -> Option<Vec<AppEntry>> {
    let hash = compute_config_hash(scan, show_hidden_system);
    // **実体化の規則は `IndexTree::materialize` の 1 つである。** ここに写しを置いていた
    // ときは、木 →`Vec<AppEntry>` の規則が 2 部出荷されていた——片方に触れば corpus テストは
    // 製品が決して見ないデータを検証することになる（`index_tree.rs` の `//!` が「辿る規則は
    // 1 つ」と定めているのと同じ理屈で、戻す規則も 1 つでなければならない）。
    Some(load_cache(hash)?.material.tree().materialize())
}

/// キャッシュを読む、無ければ全走査して保存する。返すのは [`LoadOrScanResult`]——索引の材料（[`IndexMaterial`]）とフェーズ計測、そしてキャッシュヒット時だけ背景再スキャンのタスクである。
///
/// **「`load_or_scan` と同じで、ただし〜」と書いてはならない**（かつてそう書いていた）。その関数は #984 で削除され、この doc だけが実在しない名前を基準に自分を説明する状態になっていた。
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
        let cache_read_ms = result.read_ms;
        let cached_version = result.version;
        let material = result.material;
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
            cached_digest: digest_over(material.tree()),
            generation: rescan_generation,
            cached_version,
        };
        let digest_ms = digest_started.elapsed().as_millis();
        let stats = LoadOrScanStats {
            cache_hit: true,
            hash_ms,
            cache_load_ms,
            cache_read_ms,
            digest_ms,
            scan_ms: 0,
            sort_ms: 0,
            cache_save_ms: 0,
            total_ms: total_started.elapsed().as_millis(),
        };
        return LoadOrScanResult {
            material,
            cache_changed: false,
            stats,
            rescan_task: Some(rescan_task),
        };
    }
    let cache_load_ms = cache_load_started.elapsed().as_millis();

    // 権威的書き手: scan + sort + save を書き込みロック保持下で行い、
    // 背景再スキャン / 別ビルドとの index.bin 同時書き込みを防ぐ。
    // フェーズ計測はクロージャの戻り値として持ち出す。
    let (material, scan_ms, sort_ms, cache_save_ms) = with_index_write_lock(|| {
        let scan_started = Instant::now();
        let mut entries = scan_all(scan, show_hidden_system);
        let scan_ms = scan_started.elapsed().as_millis();

        let sort_started = Instant::now();
        sort_entries_canonical(&mut entries);
        let sort_ms = sort_started.elapsed().as_millis();

        let cache_save_started = Instant::now();
        // **保存が返す木と派生データをそのまま使う。** 走査結果を保存のために建て直させると、
        // 同じ木を 2 回建てることになる（親解決は実測 23 ms）。派生データも同じ理屈で、
        // 保存側が計算して書いたものをここで受け取らないと、下流が全件を実体化してから
        // 建て直すことになる。
        let material = save_cache_sorted(entries, current_hash);
        let cache_save_ms = cache_save_started.elapsed().as_millis();

        (material, scan_ms, sort_ms, cache_save_ms)
    });

    let stats = LoadOrScanStats {
        cache_hit: false,
        hash_ms,
        cache_load_ms,
        // cache-miss の枝は `index.bin` を読み切れていない（不在・stale・破損のいずれか）。
        cache_read_ms: 0,
        // cache-miss の枝は背景再スキャンを返さない（走査したてが最新である）。
        digest_ms: 0,
        scan_ms,
        sort_ms,
        cache_save_ms,
        total_ms: total_started.elapsed().as_millis(),
    };

    LoadOrScanResult {
        // **cache-miss でも派生データを持って返る。** 保存側が `index.bin` へ書いたのと同じ 4 本であり、キャッシュヒット時に `load_cache_in` が返すものと**表現まで同じ**である（どちらも `collapse_lower_pair` = `measure_derived_sharing` を通した `Collapsed`）。ゆえに `cache_changed` は「どのコンストラクタが選ばれるか」を決めない。
        material,
        cache_changed: true,
        stats,
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
/// **この値を `index.bin` へ書いてはならない。** 突き合わせるのはロード直後に取った値と、
/// 背景スレッドが走査後に取った値——どちらも同じプロセス・同じバイナリである。ゆえに
/// `DefaultHasher` の出力が Rust のバージョン間で変わっても影響せず、下の `CHUNK` を
/// 変えてもよい。
///
/// **永続化は「12 ms を消せる」最適化に見えるが、その瞬間にオンディスク互換の制約へ化ける。**
/// ディスクへ書く `compute_config_hash` は、版が変わるとキャッシュが 1 度無効になるという
/// 性質を既に背負っている。**契約と受容する残余（衝突の帰結）の正本は [`digest_over`] の
/// doc である**——こちらは 2 つの source のうち `&[AppEntry]` 専用の入口にすぎない。
fn entries_digest(entries: &[AppEntry]) -> u64 {
    digest_over(&entries)
}

/// digest が 1 件から読むもの。**比べる 2 辺が別の形で索引を持つために要る。**
///
/// 背景再スキャンは走査結果（`Vec<AppEntry>`・フルパスを実体で持つ）を、ロード側は
/// [`IndexTree`]（フルパスを持たず組み直す）を差し出す。**畳み込みの形を 2 回書いては
/// ならない**——塊の切り方が digest の値そのものを決めるので、別実装にした瞬間に
/// 両辺が永久に食い違い、再スキャンが毎起動走り続ける（結果は正しいまま静かに遅くなる）。
/// ここを通す限り、混ぜる値も順序も塊の境界も 1 つの実装が決める。
trait DigestSource: Sync {
    fn count(&self) -> usize;
    /// `i` 番の `name` / `target_path` / `is_folder` を**この順で**混ぜる。
    ///
    /// `scratch` はフルパスを組み直す側だけが使う作業用バッファで、中身は捨てられる。
    fn mix(&self, i: usize, scratch: &mut String, hasher: &mut DefaultHasher);
}

impl DigestSource for &[AppEntry] {
    fn count(&self) -> usize {
        self.len()
    }
    fn mix(&self, i: usize, _scratch: &mut String, hasher: &mut DefaultHasher) {
        let e = &self[i];
        e.name.hash(hasher);
        e.target_path.hash(hasher);
        e.is_folder.hash(hasher);
    }
}

impl DigestSource for IndexTree {
    fn count(&self) -> usize {
        self.len()
    }
    fn mix(&self, i: usize, scratch: &mut String, hasher: &mut DefaultHasher) {
        self.names[i].hash(hasher);
        // **組み直した結果は原文とバイト一致する**（`raw_path_into` の doc が根拠の所在を
        // 持つ）。ゆえに走査結果と同じバイト列が混ざる。
        self.path_into(scratch, i);
        scratch.hash(hasher);
        self.is_folder[i].hash(hasher);
    }
}

/// 塊の大きさ。**値を変えると digest が変わる**（分割が畳み込みの形を決めるため）。
/// プロセス内でしか比較しないので問題にならない——契約の正本は [`digest_over`] の doc である
/// （[`entries_digest`] は 2 つの source のうち片方専用の入口にすぎないので、そちらを指すと
/// 一般側を読むために特殊側へ跳ぶことになる）。
///
/// **`index_tree::resolve_all` の刻みとは独立である**（片方は親の解決、片方は畳み込みの境界）。
/// 値が 8192 で一致しているのは偶然であり、共通化しない。
const DIGEST_CHUNK: usize = 8192;

/// [`DigestSource`] を畳み込んで 1 つの u64 にする。**畳み込みの形はここが唯一持つ。**
///
/// **契約: この値はプロセス内でしか比較しない。** `DefaultHasher`（SipHash 1-3）の出力は
/// Rust のバージョン間で安定である保証が無く、[`DIGEST_CHUNK`] も畳み込みの形を決めるので、
/// どちらも今は自由に動かせる——ディスクへ出した瞬間に動かせなくなる。書くと決めるなら、
/// 安定なハッシュ関数を明示的に選び、刻みを形式の一部として固定し、`IndexCache` の
/// バージョンを上げること（手順は `snotra-core/CLAUDE.md`「IndexCache バージョン変更
/// チェックリスト」）。
///
/// **受容する残余: 衝突（2⁻⁶⁴）は「変わったのに Unchanged と報告する」形で現れる。**
/// 索引は次にファイルシステムが変わるまで古いままになり、アイコンキャッシュも無効化されない。
/// **一時的ではなく、その状態が続く限り持続する**——同じ組み合わせなら毎回同じ衝突になる。
/// `compute_config_hash` は同じ u64 でキャッシュの有効性そのものを判定しており（衝突すれば
/// **別の config の索引をそのまま使う**）、こちらの帰結はそれより軽い。
fn digest_over<S: DigestSource + ?Sized>(src: &S) -> u64 {
    let n = src.count();
    // 塊ごとに並列でハッシュし、**塊の順に**畳む。長さを先に混ぜるので分割は一意に決まり、
    // 境界の曖昧さは生じない。**解放と違ってハッシュは並列化が効く**——実測 43 → 12 ms
    // （直列版と各 3 回の最小値で比較。`search/build.rs` の共有の潰しは同じ手が効かなかった）。
    let chunk_digests: Vec<u64> = (0..n)
        .step_by(DIGEST_CHUNK)
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|start| {
            let end = (start + DIGEST_CHUNK).min(n);
            let mut hasher = DefaultHasher::new();
            // 組み直す側のためのバッファは**塊ごとに 1 本**（1 件ごとに確保しない）。
            let mut scratch = String::new();
            for i in start..end {
                src.mix(i, &mut scratch, &mut hasher);
            }
            hasher.finish()
        })
        .collect();

    let mut hasher = DefaultHasher::new();
    n.hash(&mut hasher);
    for d in chunk_digests {
        d.hash(&mut hasher);
    }
    hasher.finish()
}

/// 正準の並び。**digest の値と木の形の両方をこの順序が決める**ので、比べる両辺・木を建てる
/// 両者は必ずここを通すこと（写しを書くと、写した側だけが旧い並びで木を建てて緑を返す）。
pub(crate) fn sort_entries_canonical(entries: &mut [AppEntry]) {
    entries.sort_by(|a, b| {
        a.target_path
            .cmp(&b.target_path)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.is_folder.cmp(&b.is_folder))
    });
}

/// エントリを木へ組み替えて保存し、**その木と、書いたばかりの派生データを返す**。
///
/// **`&[AppEntry]` を借りる形にしてはならない。** v7 が書くのは木であり、木を建てる段は
/// `target_path` の `String` を move で吸い上げる。借りる形にすると保存のたびに全件を
/// clone することになり、削ったはずの 36.01 MiB が保存経路で復活する。返り値にしてあるのは、
/// 呼び出し側がそのまま索引の材料に使えるようにするためである（`rebuild_and_save` と
/// cache-miss の枝が実際にそうする）。
///
/// **派生データを持たない材料を返す枝がある。** `config_dir` が引けないときは `index.bin` を書かないので派生データも計算しない——常に計算する形にすると、誰も読まない列を全件ぶん組み立てることになる。**この分岐がこの性質の正本である**（数え上げは他へ書かない）。
fn save_cache_sorted(entries: Vec<AppEntry>, config_hash: u64) -> IndexMaterial {
    let Some(dir) = Config::config_dir() else {
        return IndexMaterial::from_tree(IndexTree::build(entries));
    };
    let (tree, masks) = save_cache_sorted_in(&dir, entries, config_hash);
    IndexMaterial::derived(tree, masks)
}

/// 木と派生 4 本を導出しただけの中間の姿。**I/O もロック契約も持たない。**
///
/// 分けてあるのは、`index.bin` を書く関数と「潰しの導出」を突き合わせたい検知器が別物だから
/// である——書き込みごと公開すると、検知器がファイルシステムと、型に無いロック契約
/// （→「index.bin 書き込みの排他」）を巻き込む。
///
/// **タプルにしてはならない。** `Vec<u64>` が 2 本隣接するので、名前の無い並びでは取り違えて
/// も型検査を通る（同じ理由で [`CachedMasks`] は組のまま渡す）。
pub(crate) struct DerivedColumns {
    pub(crate) tree: IndexTree,
    pub(crate) char_masks: Vec<u64>,
    pub(crate) file_name_char_masks: Vec<u64>,
    pub(crate) lower_names: Vec<Option<String>>,
    pub(crate) lower_file_names: Vec<LowerFileName>,
}

impl DerivedColumns {
    /// 列を [`CachedMasks`] へ畳む。
    ///
    /// **`index.bin` へ書き終えた後に呼ぶ。** 書く側は素の列を `Cow::Borrowed` で借りるので、
    /// enum（[`CachedLower`]）へ包むのは借用が終わってからでなければならない——逆順にすると
    /// 包んだ中身を取り出すために `unreachable!()` 付きの `match` が要る。
    pub(crate) fn into_cached_masks(self) -> (IndexTree, CachedMasks) {
        let masks = CachedMasks {
            char_masks: self.char_masks,
            file_name_char_masks: self.file_name_char_masks,
            // **`Collapsed` で渡す。** 列は `collapse_lower_pair`（= `measure_derived_sharing`）
            // を通してあり、`assemble` はこれを測り直さない（variant の意味は [`CachedLower`]）。
            lower: Some(CachedLower::Collapsed {
                lower_names: self.lower_names,
                lower_file_names: self.lower_file_names,
            }),
        };
        (self.tree, masks)
    }
}

/// エントリから木と派生 4 本を導出する。**I/O を持たない。**
pub(crate) fn derive_columns(entries: Vec<AppEntry>) -> DerivedColumns {
    // マスクをここで計算するのは、受け取った側が再計算せずに索引の表現へそのまま使うためである（運ぶ器は [`IndexMaterial`] であり、受け取る経路をここで数えない）。
    //
    // **per-entry の導出そのものは [`derive_entry_collapsed`] が持つ。** 追記側
    // （`extend_cached_masks`）と同じ関数を通ることだけが、ディスクとメモリで潰れ方と
    // マスクが一致する根拠である。ここに列ごとの別実装を書き起こしてはならない。
    //
    // 4 本を 1 周で埋める。列ごとの `collect` に分けると、潰す前の `Vec<String>` /
    // `Vec<Option<String>>` の spine が潰した後の spine と重なって生きる区間ができる。
    let len = entries.len();
    let mut char_masks = Vec::with_capacity(len);
    let mut file_name_char_masks = Vec::with_capacity(len);
    let mut collapsed_lower_names = Vec::with_capacity(len);
    let mut collapsed_lower_file_names = Vec::with_capacity(len);
    for entry in &entries {
        let (char_mask, file_mask, lower_name, lower_file) = derive_entry_collapsed(entry);
        char_masks.push(char_mask);
        file_name_char_masks.push(file_mask);
        collapsed_lower_names.push(lower_name);
        collapsed_lower_file_names.push(lower_file);
    }

    // **木を建てるのは派生文字列を導出し終えた後である。** 建てる段が `target_path` を
    // 吸い上げるので、順序を入れ替えると `lower_file_name(&e.target_path)` の材料が消える。
    let tree = IndexTree::build(entries);

    DerivedColumns {
        tree,
        char_masks,
        file_name_char_masks,
        lower_names: collapsed_lower_names,
        lower_file_names: collapsed_lower_file_names,
    }
}

/// `save_cache_sorted` と同じ保存処理を `dir` 注入で行う（統合テスト用、issue #429）。
///
/// **`INDEX_WRITE_LOCK` は取らない**（`save_cache_sorted` と同じ契約で、呼び出し側が保持する）。
/// この契約は型に無いので、**呼び出し元をこのモジュールの中に閉じておくこと**が唯一の担保で
/// ある（→「index.bin 書き込みの排他」）。導出だけを要する検知器は [`derive_columns`] を呼ぶ。
///
/// **書いた 4 本をそのまま返す。** かつては書いた直後に捨てており、cache-miss の枝は
/// `new_from_tree` が木を実体化して Wave 1/2 を建て直していた——**計算したものを捨ててから、
/// 同じものを作り直していた**（実測は `PERFORMANCE.md`「採用: 保存が返した派生データを
/// cache-miss がそのまま使う」）。
///
/// **書き込みの失敗は返り値に影響しない。** 返すのは今メモリに在る木と、その木に対して導出
/// した派生データであり、両者の整合はディスクへ届いたかとは独立である。書けなければ次回が
/// cache-miss になるだけで、この起動の索引は正しい。
fn save_cache_sorted_in(
    dir: &Path,
    entries: Vec<AppEntry>,
    config_hash: u64,
) -> (IndexTree, CachedMasks) {
    let bf = cache_bin_file_in(dir);
    let derived = derive_columns(entries);

    // Cow::Borrowed で木の列と派生 Vec の全件 clone を避ける。
    // 出力バイト列は Owned 版と同一（golden テストで保証）。
    let cache = IndexCache {
        built_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        names: Cow::Borrowed(&derived.tree.names),
        is_folder: Cow::Borrowed(&derived.tree.is_folder),
        parent: Cow::Borrowed(&derived.tree.parent),
        aux: Cow::Borrowed(&derived.tree.aux),
        table: Cow::Borrowed(&derived.tree.table),
        sorted_by_path: derived.tree.sorted_by_path,
        config_hash,
        char_masks: Cow::Borrowed(&derived.char_masks),
        file_name_char_masks: Cow::Borrowed(&derived.file_name_char_masks),
        lower_names: Cow::Borrowed(&derived.lower_names),
        lower_file_names: Cow::Borrowed(&derived.lower_file_names),
    };
    if !bf.save(&cache) {
        eprintln!("[indexer] failed to save {}", bf.path().display());
    }
    // **畳むのは書き終えた後である**（借用の順序・[`DerivedColumns::into_cached_masks`] の doc）。
    // `clone` は挟まない——上の `Cow::Borrowed` の借用は `bf.save` を最後に終わる（NLL）。
    derived.into_cached_masks()
}

/// Force rebuild: scan and save cache, regardless of existing cache.
/// Called from settings dialog (Phase 5).
///
/// **書いた派生データをそのまま返す。** かつては捨てており、呼び出し側は木しか受け取れず Wave 1/2 を建て直していた——**計算したものを捨ててから、同じものを作り直していた**（額は `PERFORMANCE.md`「採用: `PrebuiltIndex` を `CachedMasks` 込みで建てる」）。
///
/// **派生データを持つかは [`IndexMaterial`] の内側の話である**（保存先が引けない枝では計算しない）。**持たない場合を数え上げてはならない**——正本は `save_cache_sorted` の分岐である。
///
/// PATH エントリをマージするなら [`IndexMaterial::extend_with_path_entries`] を通すこと。
pub fn rebuild_and_save(scan: &[ScanPath], show_hidden_system: bool) -> IndexMaterial {
    // 権威的書き手: scan + sort + save を書き込みロック保持下で行い、
    // 背景再スキャン / 別の rebuild との index.bin 同時書き込みを防ぐ。
    with_index_write_lock(|| {
        let mut entries = scan_all(scan, show_hidden_system);
        sort_entries_canonical(&mut entries);
        let config_hash = compute_config_hash(scan, show_hidden_system);
        save_cache_sorted(entries, config_hash)
    })
}

/// キャッシュ読み込み結果。
struct LoadCacheResult {
    /// 索引の材料。**v7 は木をディスクから直接読み、旧版は `target_path` から建て直す。**
    ///
    /// **マスクを持つ版の枝はすべて [`IndexMaterial::from_untrusted`] を通す**（列長を検証する）。**「どの枝も」と書いてはならない**——マスクを持たない版は検証する列が無いので `from_tree` を通る。構成上の正しさで検証を省けるのは導出したその足の組だけである。
    material: IndexMaterial,
    /// `index.bin` をバイト列として読み終えるまでの時間（`LoadOrScanStats::cache_read_ms` へ運ぶ）。
    read_ms: u128,
    /// 実際に読めた形式のバージョン。**現行版とは限らない**——フォールバック経路で読めた
    /// ときは旧版であり、背景再スキャンがそれを見て現行版へ書き戻す。
    version: u32,
}

fn load_cache(config_hash: u64) -> Option<LoadCacheResult> {
    let dir = Config::config_dir()?;
    load_cache_in(&dir, config_hash)
}

/// `load_cache` と同じ読み込みを `dir` 注入で行う（統合テスト用、issue #429）。
fn load_cache_in(dir: &Path, config_hash: u64) -> Option<LoadCacheResult> {
    let bf = cache_bin_file_in(dir);
    let read_started = Instant::now();
    let bytes = bf.load_bytes()?;
    let read_ms = read_started.elapsed().as_millis();

    // v7 (現行): 木の列 + ビットマスク + **共有を潰した** lower names。
    // deserialize は Cow::Owned を返すため .into_owned() は clone なしの move。
    if let Ok(cache) =
        try_deserialize_with_header::<IndexCache<'static>>(&bytes, INDEX_MAGIC, INDEX_CACHE_VERSION)
    {
        if cache.config_hash != config_hash {
            return None;
        }
        // **検証を通してから木にする。** 5 本の列は独立に読まれるので、壊れたファイルは
        // 長さの揃わない列や範囲外の添字を与えうる（帰結は `IndexTree::from_parts` の doc）。
        // 通らなければ `None`＝全走査へ落とす——**版が読めたことは中身が健全であることを
        // 意味しない。**
        let tree = IndexTree::from_parts(
            cache.names.into_owned(),
            cache.is_folder.into_owned(),
            cache.parent.into_owned(),
            cache.aux.into_owned(),
            cache.table.into_owned(),
            cache.sorted_by_path,
        )?;
        let masks = CachedMasks {
            char_masks: cache.char_masks.into_owned(),
            file_name_char_masks: cache.file_name_char_masks.into_owned(),
            // **`Collapsed` で渡す。** ここを `Raw` にすると `assemble` が測り直し、
            // `None` どうしの一致で file name 成分の無いエントリに旗が立つ。
            lower: Some(CachedLower::Collapsed {
                lower_names: cache.lower_names.into_owned(),
                lower_file_names: cache.lower_file_names.into_owned(),
            }),
        };
        return Some(LoadCacheResult {
            material: IndexMaterial::from_untrusted(tree, masks)?,
            read_ms,
            version: INDEX_CACHE_VERSION,
        });
    }

    // v6 フォールバック: `target_path` を全件そのまま持つ形式。**読めるが確保が 312,691 回
    // 余分にかかる**——フルパスの `String` を作り、木へ組み替えた段で即座に捨てる。
    // 背景再スキャンが v7 へ昇格させるまでの 1 回だけ通る経路である（→「indexer.rs の
    // 背景再スキャン」）。
    if let Ok(cache) = try_deserialize_with_header::<IndexCacheV6>(&bytes, INDEX_MAGIC, 6) {
        if cache.config_hash != config_hash {
            return None;
        }
        let masks = CachedMasks {
            char_masks: cache.char_masks,
            file_name_char_masks: cache.file_name_char_masks,
            lower: Some(CachedLower::Collapsed {
                lower_names: cache.lower_names,
                lower_file_names: cache.lower_file_names,
            }),
        };
        return Some(LoadCacheResult {
            material: IndexMaterial::from_untrusted(IndexTree::build(cache.entries), masks)?,
            read_ms,
            version: 6,
        });
    }

    // v5 フォールバック: 派生文字列を全件そのまま持つ形式。**読めるが確保が倍以上かかる**
    // ——625,380 個の `String` を作り、うち約 527,000 個は `assemble` が測って即座に捨てる。
    // 背景再スキャンが v6 へ昇格させるまでの 1 回だけ通る経路である（→「indexer.rs の
    // 背景再スキャン」）。
    if let Ok(cache) = try_deserialize_with_header::<IndexCacheV5>(&bytes, INDEX_MAGIC, 5) {
        if cache.config_hash != config_hash {
            return None;
        }
        let masks = CachedMasks {
            char_masks: cache.char_masks,
            file_name_char_masks: cache.file_name_char_masks,
            lower: Some(CachedLower::Raw {
                lower_names: cache.lower_names,
                lower_file_names: cache.lower_file_names,
            }),
        };
        return Some(LoadCacheResult {
            material: IndexMaterial::from_untrusted(IndexTree::build(cache.entries), masks)?,
            read_ms,
            version: 5,
        });
    }

    // v4 フォールバック: 末尾の normalized_keys を**読んで捨てる**。復元する 4 本は v5 と同じで
    // どれも v4 に揃っているため、**Wave 1 はスキップされたまま**（v4 ユーザーの初回起動は
    // 遅くならない）。
    //
    // **昇格は「次回の save」では起きない。** ここはかつてそう書いていたが、save の契機は
    // cache-miss と背景再スキャンの `Changed` だけであり、索引の中身が変わらない限りその
    // 「次回」は来ない——実運用点で v4 が残り続け、毎起動 35.98 MiB を読んで捨てていた
    // （2026-08-07 実測）。昇格させるのは背景再スキャンで、`version` がその判断材料である。
    if let Ok(cache) = try_deserialize_with_header::<IndexCacheV4>(&bytes, INDEX_MAGIC, 4) {
        if cache.config_hash != config_hash {
            return None;
        }
        let masks = CachedMasks {
            char_masks: cache.char_masks,
            file_name_char_masks: cache.file_name_char_masks,
            lower: Some(CachedLower::Raw {
                lower_names: cache.lower_names,
                lower_file_names: cache.lower_file_names,
            }),
        };
        return Some(LoadCacheResult {
            material: IndexMaterial::from_untrusted(IndexTree::build(cache.entries), masks)?,
            read_ms,
            version: 4,
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
            lower: None,
        };
        return Some(LoadCacheResult {
            material: IndexMaterial::from_untrusted(IndexTree::build(cache.entries), masks)?,
            read_ms,
            version: 3,
        });
    }

    // v2 フォールバック (マスクなし)
    if let Ok(cache) = try_deserialize_with_header::<IndexCacheV2>(&bytes, INDEX_MAGIC, 2) {
        if cache.config_hash != config_hash {
            return None;
        }
        return Some(LoadCacheResult {
            // マスクを持たない版ゆえ検証する列が無い（木の整合は `IndexTree::build` が持つ）。
            material: IndexMaterial::from_tree(IndexTree::build(cache.entries)),
            read_ms,
            version: 2,
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
    cached_version: u32,
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
        cached_version,
    )
}

fn try_background_rescan_in(
    dir: &Path,
    scan: &[ScanPath],
    show_hidden_system: bool,
    config_hash: u64,
    cached_digest: u64,
    generation: u64,
    cached_version: u32,
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
        let changed = entries_digest(&scanned) != cached_digest;

        // **書く条件は 2 つある。** 中身が変わったとき（従来）と、**読めた形式が旧版のとき**。
        // 後者を欠くと、索引の中身が変わらない限り旧版が何日でも残り、そのユーザーは
        // 新形式の削減を**永久に受け取らない**（2026-08-07 に実運用点で実測。詳細は
        // `background_rescan_upgrades_stale_format_when_entries_are_unchanged`）。
        //
        // 昇格をこの経路に置くのは、ここが **`sort_entries_canonical` を通した自前の
        // 走査結果を既に持っている唯一の場所**だからである。ロード側で書こうとすると、
        // engine へ move する `entries` の複製が要り、反復 6 で消した 62.5 MiB が復活する。
        if changed || cached_version != INDEX_CACHE_VERSION {
            // **返る木と `CachedMasks` はここでは捨てる。** 背景再スキャンは索引を建てない
            // ——建てるのは呼び出し側（`Changed` を見た `src-tauri` が再構築を kick する）で
            // あり、ここが抱えると起動段の外に索引 1 本ぶんの常駐が生まれる。
            drop(save_cache_sorted_in(dir, scanned, config_hash));
        }
        Some(changed)
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
    /// ロードで実際に読めた形式のバージョン。**旧版なら、中身が変わっていなくても
    /// 現行版で書き戻す**（`try_background_rescan_in`）。ここを運ばないと、索引が変わらない
    /// ユーザーの `index.bin` は旧版のまま留まり、新形式の削減を永久に受け取らない。
    cached_version: u32,
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
            self.cached_version,
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

/// `target_path` の**末尾セグメントだけ**を [`normalize_entry_key_into`] で正規化して
/// `buf` へ書く。`buf` の中身は捨てられる。
///
/// [`scan_path_dirs`] の事前フィルタ専用。**照合する両辺は必ずこの 1 つを通すこと**——
/// `normalize_entry_key_into` と同じ理屈で、同じ手順を通ることだけが一致の根拠になる。
///
/// **これは篩であって判定ではない。** 正規化は「全体 `trim` → 文字単位の写像（小文字化と
/// `/` → `\`）」であり、写像は新たな `\` を生まないのでセグメント境界を保つ。ゆえに
/// `normalize_entry_key(a) == normalize_entry_key(b)` ならこのキーも必ず一致する
/// （＝**偽陰性を出さない**）。逆は成り立たない——別ディレクトリの同名ファイルが
/// 通り抜けるので、通した候補はフルパスの正規化キーで確かめること。
///
/// **論証が乗っている前提を 2 つ名指しする。どちらを触っても、この篩だけが静かに偽陰性を
/// 出す。**
///
/// 1. **ASCII 高速路の両分岐が ASCII 入力で一致すること。** [`normalize_entry_key_into`] は
///    高速路を持ち、フルパスとその末尾セグメントは**別の分岐を通りうる**（フルパスに非 ASCII
///    が混じり、ファイル名だけ ASCII の場合）。一致の根拠は同関数の doc が持ち、実インデックス
///    の全パスでの一致を `tests/path_query_cost.rs` の
///    `derives_same_bytes_as_normalize_entry_key` が固定する。
/// 2. **小文字化が空白を作らず消さないこと。** この関数はパス全体を `trim` してから
///    セグメントを切り出し、[`normalize_entry_key_into`] が**そのセグメントをもう一度
///    `trim` する**。区切りの直後に空白があるパス（`C:\dir\ tool.exe`）では、
///    「正規化してから切り出す」と「切り出してから正規化する」が一致するために、
///    写像と `trim` が可換であることが要る。
pub(crate) fn normalize_file_name_key_into(buf: &mut String, target_path: &str) {
    let trimmed = target_path.trim();
    let segment = match trimmed.rfind(['\\', '/']) {
        // 区切りはどちらも 1 バイトゆえ `i + 1` は char 境界。
        Some(i) => &trimmed[i + 1..],
        None => trimmed,
    };
    normalize_entry_key_into(buf, segment);
}

/// PATH 上で見つけた実行ファイル 1 件と、照合に使う 2 段のキー。
struct PathCandidate {
    entry: AppEntry,
    /// `normalize_entry_key(entry.target_path)`（判定に使う）。
    key: String,
    /// [`normalize_file_name_key_into`] の結果（篩に使う）。
    file_key: String,
}

/// `path_list` のディレクトリを平坦スキャンし、対象拡張子の実行ファイルを候補として返す。
/// PATH ディレクトリ間の重複はここで排除する（既存エントリとの照合は [`reject_existing`]）。
fn enumerate_path_candidates(path_list: &str, show_hidden_system: bool) -> Vec<PathCandidate> {
    let path_exts = ["exe", "bat", "cmd", "com"];
    let mut candidates: Vec<PathCandidate> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

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
            if !seen.insert(key.clone()) {
                continue;
            }
            let mut file_key = String::new();
            normalize_file_name_key_into(&mut file_key, &path_str);
            candidates.push(PathCandidate {
                entry: AppEntry {
                    name,
                    target_path: path_str,
                    is_folder: false,
                },
                key,
                file_key,
            });
        }
    }

    candidates
}

/// `existing_entries` に既にある候補を落とし、残りを**列挙順のまま**返す。
///
/// **問いを反転させてある**（反復 9）。かつては既存エントリ全件の正規化キーを `HashSet`
/// へ積んでから候補を引いていた——**全件を積んで少数を照合する**比率であり、区間のほとんど
/// がその積み上げだった。候補側に小さな索引を作り、既存エントリは**ファイル名だけ**を篩に
/// 掛けて素通しする形にすると、全件ぶんの `String` 確保が消える。篩を抜けた少数だけが
/// フルパスの正規化キーまで進む。
///
/// 実測値は `PERFORMANCE.md`「採用: PATH スキャンの問いを反転（確保 314,395 → 2,066・反復 9）」を
/// 正本とする（ここには写さない——数値は次の反復で動き、写しは片方だけ更新されて残る）。
/// 篩が偽陰性を出さない根拠は [`normalize_file_name_key_into`] の doc。
fn reject_existing(candidates: Vec<PathCandidate>, existing: &IndexTree) -> Vec<AppEntry> {
    // **候補が無ければ既存エントリを 1 件も見ない。** この関数のコストは丸ごと下の走査に
    // あるので、外すと PATH に実行ファイルを持たないユーザーで全件ぶんの篩が戻る。
    if candidates.is_empty() {
        return Vec::new();
    }

    // ファイル名キー → 候補の添字（同名の候補が別ディレクトリに並びうるので複数持つ）。
    let mut by_file_key: std::collections::HashMap<&str, Vec<usize>> =
        std::collections::HashMap::with_capacity(candidates.len());
    for (i, c) in candidates.iter().enumerate() {
        by_file_key.entry(c.file_key.as_str()).or_default().push(i);
    }

    let mut rejected = vec![false; candidates.len()];
    // 確保を暖まらせて使い回す（`normalize_entry_key_into` の doc と同じ形）。
    //
    // **生パスと正規化キーは別の変数に持つ。** 1 本を `mem::take` で貸し借りする形も書けるが、
    // `take` は容量 0 の `String` を置き去りにするので `normalize_entry_key_into` が篩を通る
    // たびに確保し直し、**この行が意図している使い回しがまさに消える**。読む側も、1 つの名前が
    // 生パスと正規化キーのどちらを持つ瞬間なのかを 3 文追わされる。
    let mut file_buf = String::new();
    let mut seg_buf = String::new();
    let mut raw_buf = String::new();
    let mut norm_buf = String::new();
    for i in 0..existing.len() {
        // **篩はフルパスを組み立てない。** 木では末尾成分が `name` + 拡張子で直接取れるので、
        // ここが 312,691 回走っても根まで辿る必要が無い（`IndexTree::file_key_into`）。
        existing.file_key_into(&mut file_buf, &mut seg_buf, i);
        let Some(idxs) = by_file_key.get(file_buf.as_str()) else {
            continue;
        };
        // **篩を通った分だけフルパスを組み立てる。** 通るのは PATH 上の実行ファイルと
        // 同名のエントリだけなので、組み立ては全件ではなくその数に比例する。
        existing.path_into(&mut raw_buf, i);
        normalize_entry_key_into(&mut norm_buf, &raw_buf);
        for &i in idxs {
            if candidates[i].key == norm_buf {
                rejected[i] = true;
            }
        }
    }

    // **`zip` で対応を構造にする。** 外部イテレータで駆動する `retain` でも同じ結果になるが、
    // それは「`retain` が要素を元の順に 1 回ずつ訪れる」という約束への暗黙の依存になる。
    //
    // **長さの一致を保証しているのは `zip` ではない**——`Zip` は短い方で黙って止まる。
    // 保証しているのは上の `vec![false; candidates.len()]` が同じ関数の同じ式で長さを
    // 決めていることと、`by_file_key` が `candidates` を `&str` で借りているために
    // **その借用が死ぬ（＝この `into_iter()`）まで `candidates` を変える経路が構築できない**
    // ことである。ずれを借用検査が作れなくしているので、実行時の assert は置かない
    // ——コンパイラが証明することを実行時に主張し直しても、守るものが増えない。
    candidates
        .into_iter()
        .zip(rejected)
        .filter(|(_, rejected)| !rejected)
        .map(|(c, _)| c.entry)
        .collect()
}

/// セミコロン区切りのパスリストからディレクトリを平坦スキャンし、
/// 既存エントリにない実行ファイルを返す。
///
/// `read_user_path` から分離することでテスト可能性を確保。列挙と篩の分担は
/// [`enumerate_path_candidates`] と [`reject_existing`] の doc を見ること。
fn scan_path_dirs(
    path_list: &str,
    existing: &IndexTree,
    show_hidden_system: bool,
) -> Vec<AppEntry> {
    reject_existing(
        enumerate_path_candidates(path_list, show_hidden_system),
        existing,
    )
}

/// ユーザー PATH のディレクトリを平坦スキャンし、既存エントリにない実行ファイルを返す。
///
/// - レジストリ `HKCU\Environment\Path` から読み取る（システム PATH を含まない）
/// - `REG_EXPAND_SZ` の環境変数は展開済み
/// - 再帰スキャンなし（PATH ディレクトリの直下のみ）
/// - 対象拡張子: .exe / .bat / .cmd / .com（正本は `enumerate_path_candidates` の `path_exts`）
/// - `existing_entries` に同一パスがあるものは返さない（normalize_entry_key で判定）
/// - PATH ディレクトリ間での重複も排除する
pub fn scan_path_env(existing: &IndexTree, show_hidden_system: bool) -> Vec<AppEntry> {
    let user_path = match read_user_path() {
        Some(p) if !p.is_empty() => p,
        _ => return Vec::new(),
    };
    scan_path_dirs(&user_path, existing, show_hidden_system)
}

/// エントリ 1 件の派生（**潰す前**）。マスク 2 本と、潰す前の派生文字列 1 組を返す。
///
/// **マスクは潰す前の完全な文字列から導出する。** 潰した後に取ると
/// `file_char_mask(None) == 0` になり、pre-filter が false negative を出す
/// （`search/build.rs` の `compute_wave2` と同じ不変条件——あちらは「ビットマスクより後に
/// 潰す」という順序で守っており、ここでは「潰す前に導出する」という順序で守る）。
///
/// 潰さない列（`CachedLower::Raw`）へ追記する経路と、マスクだけを要する経路（`lower` が
/// `None`）がこれを直接呼ぶ。潰した形が要るなら [`derive_entry_collapsed`] を呼ぶ。
fn derive_entry_lowers(entry: &AppEntry) -> (u64, u64, String, Option<String>) {
    let lower_name = to_lower_folded(&entry.name);
    let lower_file = lower_file_name(&entry.target_path);
    let char_mask = name_char_mask(&lower_name);
    let file_mask = file_char_mask(lower_file.as_deref());
    (char_mask, file_mask, lower_name, lower_file)
}

/// エントリ 1 件の派生（**潰した形**）。マスク 2 本と、畳んだ派生文字列 1 組を返す。
///
/// **潰す前にマスクを取るという順序は、この 1 か所にしかない。**（`derive_entry_lowers` が
/// 返した後でだけ `collapse_lower_pair` を当てる。）記録側（[`derive_columns`]）と追記側
/// （`extend_cached_masks` の `Collapsed` 枝）が同じここを通ることが、ディスクとメモリで
/// 潰れ方とマスクが一致する根拠である。検知器は `derived_masks_come_from_the_uncollapsed_strings`。
fn derive_entry_collapsed(entry: &AppEntry) -> (u64, u64, Option<String>, LowerFileName) {
    let (char_mask, file_mask, lower_name, lower_file) = derive_entry_lowers(entry);
    let (lower_name, lower_file) = collapse_lower_pair(&entry.name, lower_name, lower_file);
    (char_mask, file_mask, lower_name, lower_file)
}

/// 派生文字列 1 組を、`measure_derived_sharing` の判定に従って潰した形へ畳む。
///
/// **唯一の呼び出し元は [`derive_entry_collapsed`] である**（記録側・追記側はそこを通る）。
/// 別実装で書き起こすと、その経路の分だけが索引の読み替えとずれる——`assemble` は
/// `Collapsed` を測り直さないので、ずれは**検索結果のスコアという形で静かに現れる**。
fn collapse_lower_pair(
    name: &str,
    lower_name: String,
    lower_file: Option<String>,
) -> (Option<String>, LowerFileName) {
    let sharing = measure_derived_sharing(name, &lower_name, lower_file.as_deref());
    let file = match (sharing.file_name_is_lower_name, lower_file) {
        (true, _) => LowerFileName::SameAsLowerName,
        (false, None) => LowerFileName::Absent,
        (false, Some(s)) => LowerFileName::Text(s),
    };
    let name = if sharing.lower_name_is_name {
        None
    } else {
        Some(lower_name)
    };
    (name, file)
}

/// CachedMasks の各 Vec に新しいエントリの分を追記する。
/// インデックスキャッシュの恩恵を維持しつつ、PATH エントリ等の追加分を補完する。
///
/// `char_masks` / `file_name_char_masks` は常に追記。派生文字列は `lower` が `Some` の場合のみ、
/// **その variant が持つ表現に合わせて**追記する。`kana_lower_names` は SearchEngine 側で
/// entries から直接計算されるためここでは扱わない。
pub fn extend_cached_masks(masks: &mut CachedMasks, new_entries: &[AppEntry]) {
    for entry in new_entries {
        // per-entry の導出は記録側（`derive_columns`）と同じ [`derive_entry_lowers`] /
        // [`derive_entry_collapsed`] を通す。ここに関数列を書き起こしてはならない。
        match masks.lower {
            // v3 以下: 派生文字列を持たない（Wave 1 が全件を計算する）。マスクだけ足す。
            None => {
                let (char_mask, file_mask, _, _) = derive_entry_lowers(entry);
                masks.char_masks.push(char_mask);
                masks.file_name_char_masks.push(file_mask);
            }
            // 潰さない列。**この枝は潰す段を持たないので順序の不変条件も持たない**
            // （`assemble` が後で測る）。
            Some(CachedLower::Raw {
                ref mut lower_names,
                ref mut lower_file_names,
            }) => {
                let (char_mask, file_mask, lower, lower_file) = derive_entry_lowers(entry);
                masks.char_masks.push(char_mask);
                masks.file_name_char_masks.push(file_mask);
                lower_names.push(lower);
                lower_file_names.push(lower_file);
            }
            // **潰し済みの列へは、同じ判定を通した値だけを足す。**
            Some(CachedLower::Collapsed {
                ref mut lower_names,
                ref mut lower_file_names,
            }) => {
                let (char_mask, file_mask, name, file) = derive_entry_collapsed(entry);
                masks.char_masks.push(char_mask);
                masks.file_name_char_masks.push(file_mask);
                lower_names.push(name);
                lower_file_names.push(file);
            }
        }
    }
}

/// 索引の材料。**木と、その木に対して導出した派生データを組のまま運ぶ。**
///
/// **ほどいて 2 つの値として持ち回してはならない。** 束ねる理由は [`CachedMasks`] が 4 本を束ねるのと同じ（`SearchEngine::new_with_cached_masks` の doc「境界を跨ぐ手前でほどかない」）で、こちらが消すのは「**木を伸ばしたのにマスクへ追記し忘れる**」誤りである。フィールドが private ゆえ木だけを伸ばす経路は書けない——その誤りは `SearchEngine` の長さ検証が `debug_assert` ゆえ **release で沈黙し**、再構築を終えた後の初回検索で並列 Vec の添字外 panic として出る（`panic = "abort"` ゆえプロセスごと終了）。
///
/// **`Option` を消費側へ配らない。** かつては呼び出し点が `match` で `Some` / `None` を捌き、同じ分岐が `PrebuiltIndex` / `Engine` / `SearchEngine` の **3 層 5 か所**へ写っていた。`PrebuiltIndex::from_parts` を足すだけの案は**そのうち 1 か所しか直せない**（根は最下層の 2 コンストラクタで、上の 2 層はその写しだから）ので採らなかった。
///
/// **ディスクから来た組は `from_untrusted` を通す**（列長を検証する）。構成上正しいのは `derive_columns` の出力だけである。
pub struct IndexMaterial {
    tree: IndexTree,
    masks: Option<CachedMasks>,
}

impl IndexMaterial {
    /// 派生データを持たない材料（初回起動と、保存側がマスクを返さなかったとき）。
    pub fn from_tree(tree: IndexTree) -> Self {
        Self { tree, masks: None }
    }

    /// 導出したその足の組。**長さを検証しない**——[`derive_columns`] が 1 周で 4 本を埋めるので構成上一致する。
    pub(crate) fn derived(tree: IndexTree, masks: CachedMasks) -> Self {
        Self {
            tree,
            masks: Some(masks),
        }
    }

    /// **ディスクから読んだ組を検証してから受け取る。** 列長が木と揃わなければ `None` を返し、呼び出し側は全走査へ落ちる。
    ///
    /// **見るのは列の長さだけである。** マスクの中身が正しいかは検証できない（正しさの定義が「その木から導出した値と一致する」であり、それを確かめるには導出し直すことになる——検証のために削減を捨てる形になる）。検証をここに置くのは、`load_cache_in` が [`IndexTree::from_parts`] で**木の整合しか**見ていなかったためである——切り詰められた `index.bin` は「木より短いマスク」として起動経路へ入り、`SearchEngine` の長さ検証は `debug_assert` ゆえ release で消えて初回検索の添字外 panic になっていた。**版が読めたことは中身が健全であることを意味しない**（`from_parts` の doc と同じ理屈）。
    pub(crate) fn from_untrusted(tree: IndexTree, masks: CachedMasks) -> Option<Self> {
        let n = tree.len();
        if masks.char_masks.len() != n || masks.file_name_char_masks.len() != n {
            return None;
        }
        let lower_ok = match &masks.lower {
            None => true,
            Some(CachedLower::Collapsed {
                lower_names,
                lower_file_names,
            }) => lower_names.len() == n && lower_file_names.len() == n,
            Some(CachedLower::Raw {
                lower_names,
                lower_file_names,
            }) => lower_names.len() == n && lower_file_names.len() == n,
        };
        lower_ok.then_some(Self {
            tree,
            masks: Some(masks),
        })
    }

    /// PATH スキャンが見つけたエントリをマージする。**マスクへの追記と木への追加を対で行う唯一の場所である。**
    ///
    /// 起動経路（`src-tauri` の `main`）と背景の再構築（同 `drain_index`）が同じここを通る。片方だけ書く形は**この型の外からは書けない**——それがフィールドを private にしてある理由である。検知器は `search/tests/build.rs` の `path_merge_after_cache_miss_agrees_with_deriving_over_the_extended_tree` と `path_merge_extends_the_tree_even_without_derived_data`（**どちらも変異を注入して落ちることを実測してある**）。
    ///
    /// **スキャンは呼び出し側に残してある。** 実 PATH 環境変数を読む [`scan_path_env`] を内側へ入れると、この操作が決定的なユニットテストに乗らない。スキャンの呼び忘れは `entries` が手元に無いという形で目に見えるので、閉じる価値があるのはマージの側だけである。
    pub fn extend_with_path_entries(&mut self, entries: Vec<AppEntry>) {
        // **追記が先に来るのは所有権の帰結であって規約ではない。** `extend_with_roots` は `entries` を move で取るため、逆順に書くと clone が要る——順序の取り違えは「うっかり」では書けない。ゆえにこの順序に検知器を置いていない。
        if let Some(masks) = self.masks.as_mut() {
            extend_cached_masks(masks, &entries);
        }
        self.tree.extend_with_roots(entries);
    }

    /// 木の読み取り（件数・パスの組み直し）。**伸ばす操作は公開しない。**
    pub fn tree(&self) -> &IndexTree {
        &self.tree
    }

    /// 索引を建てる側だけがほどく。**crate 外へ出さない**——ほどいた 2 値を持ち回せるようになると、この型の存在理由が消える。
    pub(crate) fn into_parts(self) -> (IndexTree, Option<CachedMasks>) {
        (self.tree, self.masks)
    }

    /// 派生データを持っているか。**検知器と計測ハーネスのためだけに在る**（`SearchEngine::footprint_rows` と同じ `#[doc(hidden)] pub` の扱い）。
    ///
    /// **製品コードはこれを読んで分岐してはならない。** 建て方の分岐は `SearchEngine::from_material` の 1 か所に閉じており、そこへ戻すために `Option` を外へ出していない。ここが要るのは 2 つの理由による: (1) `extend_with_path_entries` が**マスクを取り落とさない**ことを検知器が名指しで測れるようにする——落とすと `from_material` が黙って木からの導出へ切り替わり、A/B 一致は**成立したまま**削減だけが消える（挙動テストでは捕まらない類の退行）。(2) `tests/memory_footprint.rs` が「どちらの枝を測ったか」を出力に添える——添えないとその数字は読めない。
    #[doc(hidden)]
    pub fn has_masks(&self) -> bool {
        self.masks.is_some()
    }
}

// ---------------------------------------------------------------------------
// 計測専用: `index.bin` のオンディスク内訳
// ---------------------------------------------------------------------------

/// `index.bin` の 1 項目が占めるオンディスクのバイト数。
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct CacheByteRow {
    pub label: &'static str,
    pub bytes: usize,
    pub items: usize,
}

/// `index.bin` のバイト内訳。
///
/// **`residual` が 0 でなければ帰属が誤っている。** postcard は struct に枠を持たず、
/// フィールドの連結がそのまま payload になる——ゆえに項目別の長さの和は payload 長と
/// **一致しなければならない**。この検算が無い内訳は、正しい帰属と誤った帰属を区別できない
/// （`tests/memory_footprint.rs` のフェーズ内訳が残余を出すのと同じ理由）。
#[doc(hidden)]
#[derive(Debug)]
pub struct CacheByteBreakdown {
    /// 実際に読めた形式のバージョン。**現行版とは限らない。**
    ///
    /// 実運用点のファイルが旧版のまま留まることは実際に起きる——`index.bin` を書き直す契機は
    /// cache-miss と背景再スキャンの `Changed` だけであり、索引の中身が変わらない限り**旧版が
    /// 何日でも残る**（2026-08-07 に実測: v5 導入後の実 `index.bin` が v4 のままで、
    /// `normalized_keys` を毎起動で読んで捨てていた）。**この値を読まずに内訳を解釈しないこと。**
    pub version: u32,
    /// `index.bin` のファイル長（ヘッダ 8 バイトを含む）。
    pub file_len: usize,
    /// postcard payload の長さ（`file_len` − ヘッダ）。
    pub payload_len: usize,
    /// `IndexCache` のフィールド別バイト数。
    pub rows: Vec<CacheByteRow>,
    /// `payload_len` − Σ`rows`。**0 でなければ帰属が誤っている。**
    pub residual: i64,
    /// `entries` の内部内訳（`name` / `target_path` / `is_folder`）。
    pub entry_rows: Vec<CacheByteRow>,
    /// `entries` のバイト数 − Σ`entry_rows`。**0 でなければ帰属が誤っている。**
    pub entry_residual: i64,
}

/// 内訳計器から見た「エントリの持ち方」。版によって形が違うので、両方を数えられる形にする。
///
/// **v7 が来た瞬間に `None` を返す作りにしてはならない。** この計器は形式を変える判断の一次
/// 証拠であり、新形式で黙れば**削減した直後にだけ測れなくなる**（同じ形の失敗が
/// `PERFORMANCE.md` に記録されている）。
enum EntryRepr<'a> {
    /// v6 以下: `AppEntry` の列を丸ごと持つ（`target_path` が実体で入っている）。
    Flat(&'a [AppEntry]),
    /// v7: 木の列。`target_path` は親と拡張子 id に置き換わり、実体は根のぶんだけ `table` に残る。
    ///
    /// **`sorted_by_path` も持つ。** 1 バイトしか無いが、`IndexCache` に居る以上
    /// 帰属しなければ残余が 0 にならない——そして残余の検算は、列を 1 本落とした誤りと
    /// 旗を落とした誤りを区別しない（実際に 5 列だけを数えて +1 B で落ちた）。
    Tree {
        names: &'a [String],
        is_folder: &'a [bool],
        parent: &'a [u32],
        aux: &'a [u32],
        table: &'a [String],
        sorted_by_path: bool,
    },
}

impl EntryRepr<'_> {
    fn count(&self) -> usize {
        match self {
            Self::Flat(e) => e.len(),
            Self::Tree { names, .. } => names.len(),
        }
    }

    fn top_row(&self) -> Option<CacheByteRow> {
        // 版ごとに違うラベル。**リテラルを他所へ書き写さない**——照合に使うと、片方の版でだけ
        // 一致しなくなる形の不変条件が生まれる（呼び出し側はバイト数を move の前に読むので、
        // ラベルで探し直す必要は無い）。
        let label = match self {
            Self::Flat(_) => "entries",
            Self::Tree { .. } => "木（5 列 + 整列の旗）",
        };
        let bytes = match self {
            Self::Flat(e) => serialized_len(e)?,
            Self::Tree {
                names,
                is_folder,
                parent,
                aux,
                table,
                sorted_by_path,
            } => {
                serialized_len(names)?
                    + serialized_len(is_folder)?
                    + serialized_len(parent)?
                    + serialized_len(aux)?
                    + serialized_len(table)?
                    + serialized_len(sorted_by_path)?
            }
        };
        Some(CacheByteRow {
            label,
            bytes,
            items: self.count(),
        })
    }

    /// 上の 1 行をさらに割った内訳。**算術で出すが、和が上の実測値と一致することで
    /// 裏打ちされる**（一致しなければ `entry_residual` に現れる）。
    fn sub_rows(&self) -> Vec<CacheByteRow> {
        let n = self.count();
        let strs = |v: &[String]| -> usize { v.iter().map(|s| postcard_str_len(s)).sum() };
        match self {
            Self::Flat(entries) => vec![
                CacheByteRow {
                    label: "entries: 長さプレフィックス",
                    bytes: varint_len(n),
                    items: 1,
                },
                CacheByteRow {
                    label: "entries[].name",
                    bytes: entries.iter().map(|e| postcard_str_len(&e.name)).sum(),
                    items: n,
                },
                CacheByteRow {
                    label: "entries[].target_path",
                    bytes: entries
                        .iter()
                        .map(|e| postcard_str_len(&e.target_path))
                        .sum(),
                    items: n,
                },
                CacheByteRow {
                    label: "entries[].is_folder",
                    bytes: n,
                    items: n,
                },
            ],
            Self::Tree {
                names,
                is_folder,
                parent,
                aux,
                table,
                // **`..` を書かない。** 値は読まない（旗は真偽によらず 1 バイト）が、列を
                // 足したときにここを触り忘れたらコンパイルを止めるのが網羅的分解の役目
                // である——`footprint_rows` と同じ規律（`snotra-core/CLAUDE.md` の
                // search.rs 節）。残余の検算は `#[ignore]` の計器でしか走らないので、
                // 落とすとコンパイラの検出が手作業へ格下げされる。
                sorted_by_path: _,
            } => vec![
                CacheByteRow {
                    label: "木: 長さプレフィックス（5 列）",
                    bytes: varint_len(n) * 4 + varint_len(table.len()),
                    items: 5,
                },
                CacheByteRow {
                    label: "sorted_by_path（整列の旗）",
                    bytes: 1,
                    items: 1,
                },
                CacheByteRow {
                    label: "is_folder",
                    bytes: is_folder.len(),
                    items: n,
                },
                CacheByteRow {
                    label: "names",
                    bytes: strs(names),
                    items: n,
                },
                CacheByteRow {
                    label: "parent",
                    bytes: parent.iter().map(|v| varint_len(*v as usize)).sum(),
                    items: n,
                },
                CacheByteRow {
                    label: "aux",
                    bytes: aux.iter().map(|v| varint_len(*v as usize)).sum(),
                    items: n,
                },
                CacheByteRow {
                    label: "table（拡張子 + 根のフルパス）",
                    bytes: strs(table),
                    items: table.len(),
                },
            ],
        }
    }
}

/// postcard の LEB128 varint が `v` を表すのに使うバイト数。
fn varint_len(mut v: usize) -> usize {
    let mut n = 1;
    while v >= 0x80 {
        v >>= 7;
        n += 1;
    }
    n
}

/// 文字列 1 件を postcard へ書いたときの長さ（長さの varint + 本体）。
///
/// 括り出してあるのは、`sub_rows` の中だけで同じ 2 項式が 3 回書かれていたためである
/// ——新しい表現を足すたびに 4 回目・5 回目と増える形だった。
#[inline]
fn postcard_str_len(s: &str) -> usize {
    varint_len(s.len()) + s.len()
}

/// 値を postcard へ書いたときの長さ（バッファは即座に捨てる）。
fn serialized_len<T: Serialize>(value: &T) -> Option<usize> {
    postcard::to_allocvec(value).ok().map(|v| v.len())
}

/// `dir` の `index.bin` を読み、フィールド別のバイト内訳を返す。
///
/// **オンディスク形式を変える判断の唯一の一次証拠である。** 常駐の内訳
/// （`SearchEngine::footprint_rows`）はメモリが「持たない」ことを学んだ後の姿を映すので、
/// ディスクが何を持ち続けているかは**そちらからは原理的に見えない**（`target_path` は
/// 常駐 0.01 MiB に対しディスクは全文を持つ）。
///
/// **現行版だけでなく旧版も読む。ただし製品のフォールバック鎖（`load_cache_in`）より
/// 狭い**——最古の版まではたどらない（読める版の一覧はこの関数の分岐が正本。書き写すと
/// 版を足したときに片方だけ腐る）。現行版だけを読む形にしてはならない——実運用点の
/// ファイルが旧版のまま留まることは実際に起きるので、そこで `None` を返す計器は
/// **一番測りたい相手にだけ黙る**。
///
/// **この関数が読めないほど古い版では、今もそう黙る。** 製品が読めて計器が読めない版の
/// 幅がその盲点であり、`None` は「壊れている」ではなく「この計器の射程の外」を意味する。
/// 読める版を増やすときは `load_cache_in` の鎖と揃えること。読めた版は
/// [`CacheByteBreakdown::version`] が返す。
///
/// **撤去条件**: オンディスク形式の削減を打ち切ったとき（＝`INDEX_CACHE_VERSION` をこれ以上
/// 形式縮小のために上げないと決めたとき）。それまでは各反復の前後で天井と実績を突き合わせる。
#[doc(hidden)]
pub fn cache_byte_breakdown_in(dir: &Path) -> Option<CacheByteBreakdown> {
    let bf = cache_bin_file_in(dir);
    let bytes = bf.load_bytes()?;
    let file_len = bytes.len();

    if let Ok(c) =
        try_deserialize_with_header::<IndexCache<'static>>(&bytes, INDEX_MAGIC, INDEX_CACHE_VERSION)
    {
        drop(bytes);
        return build_breakdown(
            INDEX_CACHE_VERSION,
            file_len,
            c.built_at,
            c.config_hash,
            EntryRepr::Tree {
                names: &c.names,
                is_folder: &c.is_folder,
                parent: &c.parent,
                aux: &c.aux,
                table: &c.table,
                sorted_by_path: c.sorted_by_path,
            },
            &c.char_masks,
            &c.file_name_char_masks,
            LowerRepr::Collapsed {
                names: &c.lower_names,
                files: &c.lower_file_names,
            },
            None,
        );
    }

    // **v6 を落とさない。** v7 が現行になった瞬間、v6 は「実運用点に実際に置かれている版」に
    // なった——ここを飛ばすと、置き換えようとしている当の形式でだけ計器が黙る
    // （実測で踏んだ: 実 `index.bin` が v6 のとき「読めなかったためスキップ」と出た）。
    if let Ok(c) = try_deserialize_with_header::<IndexCacheV6>(&bytes, INDEX_MAGIC, 6) {
        drop(bytes);
        return build_breakdown(
            6,
            file_len,
            c.built_at,
            c.config_hash,
            EntryRepr::Flat(&c.entries),
            &c.char_masks,
            &c.file_name_char_masks,
            LowerRepr::Collapsed {
                names: &c.lower_names,
                files: &c.lower_file_names,
            },
            None,
        );
    }

    if let Ok(c) = try_deserialize_with_header::<IndexCacheV5>(&bytes, INDEX_MAGIC, 5) {
        drop(bytes);
        return build_breakdown(
            5,
            file_len,
            c.built_at,
            c.config_hash,
            EntryRepr::Flat(&c.entries),
            &c.char_masks,
            &c.file_name_char_masks,
            LowerRepr::Raw {
                names: &c.lower_names,
                files: &c.lower_file_names,
            },
            None,
        );
    }

    if let Ok(c) = try_deserialize_with_header::<IndexCacheV4>(&bytes, INDEX_MAGIC, 4) {
        drop(bytes);
        return build_breakdown(
            4,
            file_len,
            c.built_at,
            c.config_hash,
            EntryRepr::Flat(&c.entries),
            &c.char_masks,
            &c.file_name_char_masks,
            LowerRepr::Raw {
                names: &c.lower_names,
                files: &c.lower_file_names,
            },
            Some(&c.normalized_keys),
        );
    }

    None
}

/// 派生文字列 2 本の、版ごとの表現。**`build_breakdown` へは表現だけを渡す**——行の生成を
/// 呼び出し側へ出すと、フィールドの並び順が文書の申し合わせに落ちる。postcard は struct に
/// 枠を持たないので、**2 行を入れ替えても長さの和は変わり**、残余 0 の検算はその誤りを
/// 捕まえない（捕まえるのは項目の欠落と重複だけである）。
enum LowerRepr<'a> {
    /// v6: 潰し済み。
    Collapsed {
        names: &'a [Option<String>],
        files: &'a [LowerFileName],
    },
    /// v5 / v4: 全件が実体を持つ。**両版で型も数え方も同一**ゆえ 1 つの variant で足りる
    /// （版そのものは [`CacheByteBreakdown::version`] が正本として持つ）。
    Raw {
        names: &'a [String],
        files: &'a [Option<String>],
    },
}

/// 読めた版によらず同じ帰属を組む。**スライスで受ける**ので、現行の `Cow<[T]>` も旧版の
/// `Vec<T>` も同じ経路を通る（postcard はどちらも `serialize_seq` へ委譲し、バイト列は
/// 一致する）。
#[allow(clippy::too_many_arguments)]
fn build_breakdown(
    version: u32,
    file_len: usize,
    built_at: u64,
    config_hash: u64,
    entries: EntryRepr<'_>,
    char_masks: &[u64],
    file_name_char_masks: &[u64],
    lower: LowerRepr<'_>,
    normalized_keys: Option<&[String]>,
) -> Option<CacheByteBreakdown> {
    let (lower_names_row, lower_file_names_row) = match lower {
        LowerRepr::Collapsed { names, files } => (
            CacheByteRow {
                label: "lower_names（潰し済み）",
                bytes: serialized_len(&names)?,
                // **実体を持つ件数を出す**（列の長さではない）。潰れた分は 1 バイトの
                // タグにしかならないので、件数と長さの比が共有の効きを表す。
                items: names.iter().filter(|s| s.is_some()).count(),
            },
            CacheByteRow {
                label: "lower_file_names（3 状態）",
                bytes: serialized_len(&files)?,
                items: files
                    .iter()
                    .filter(|f| matches!(f, LowerFileName::Text(_)))
                    .count(),
            },
        ),
        LowerRepr::Raw { names, files } => (
            CacheByteRow {
                label: "lower_names（全件実体）",
                bytes: serialized_len(&names)?,
                items: names.len(),
            },
            CacheByteRow {
                label: "lower_file_names（全件実体）",
                bytes: serialized_len(&files)?,
                items: files.iter().filter(|s| s.is_some()).count(),
            },
        ),
    };
    // **バイト数はここで読んでおく。** `rows` へ move した行を後からラベルの文字列照合で
    // 探し直す形も書けるが、それは「行を作る側と探す側で版ごとのラベルが一致し続ける」という
    // 不変条件を新設し、外したときは `?` の無言の `None` として出る。`usize` は `Copy` なので
    // move の前に読めば、その不変条件ごと要らなくなる。
    let top_row = entries.top_row()?;
    let entries_bytes = top_row.bytes;
    let mut rows = vec![
        CacheByteRow {
            label: "built_at",
            bytes: serialized_len(&built_at)?,
            items: 1,
        },
        top_row,
        CacheByteRow {
            label: "config_hash",
            bytes: serialized_len(&config_hash)?,
            items: 1,
        },
        CacheByteRow {
            label: "char_masks",
            bytes: serialized_len(&char_masks)?,
            items: char_masks.len(),
        },
        CacheByteRow {
            label: "file_name_char_masks",
            bytes: serialized_len(&file_name_char_masks)?,
            items: file_name_char_masks.len(),
        },
        lower_names_row,
        lower_file_names_row,
    ];
    if let Some(keys) = normalized_keys {
        rows.push(CacheByteRow {
            label: "normalized_keys（v4 のみ・読んで捨てる）",
            bytes: serialized_len(&keys)?,
            items: keys.len(),
        });
    }

    let payload_len = file_len - 8;
    let attributed: usize = rows.iter().map(|r| r.bytes).sum();
    let residual = payload_len as i64 - attributed as i64;

    let entry_rows = entries.sub_rows();
    let entry_attributed: usize = entry_rows.iter().map(|r| r.bytes).sum();
    let entry_residual = entries_bytes as i64 - entry_attributed as i64;

    Some(CacheByteBreakdown {
        version,
        file_len,
        payload_len,
        rows,
        residual,
        entry_rows,
        entry_residual,
    })
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

    /// テスト用の作業ディレクトリを作り直して返す。
    ///
    /// 名前には `tag`（プロセス内の一意性）に加えて **`std::process::id()`** を含める。
    /// `INDEX_WRITE_LOCK` はプロセス内の `static Mutex` ゆえ、テストバイナリが複数
    /// プロセスに分かれる状況（`cargo test` と `cargo test --release` の重なり・
    /// temp root を共有する 2 ジョブ・別 worktree での並行実行）では効かない。
    /// pid を落とすと、片方の `remove_dir_all` がもう片方の `create_dir_all` や
    /// `index.bin.tmp` の書き込みに割り込み、コード変更と無関係な panic になる（#978）。
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("snotra_idx_test_{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn temp_dir_name_contains_process_id() {
        let dir = temp_dir("process_unique");
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .expect("temp dir name");
        assert_eq!(
            name,
            format!("snotra_idx_test_process_unique-{}", std::process::id()),
            "作業ディレクトリ名に自プロセスの pid が入っていない（#978）"
        );
        let _ = fs::remove_dir_all(&dir);
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

        let tree = IndexTree::build(entries.clone());
        let cache = IndexCache {
            built_at: 1700000000,
            names: Cow::Owned(tree.names.clone()),
            is_folder: Cow::Owned(tree.is_folder.clone()),
            parent: Cow::Owned(tree.parent.clone()),
            aux: Cow::Owned(tree.aux.clone()),
            table: Cow::Owned(tree.table.clone()),
            sorted_by_path: tree.sorted_by_path,
            config_hash: 12345,
            char_masks: Cow::Owned(vec![0xAB, 0xCD]),
            file_name_char_masks: Cow::Owned(vec![0x12, 0x34]),
            // v6: `None` = name と同一。ここでは 2 件目がそれに当たる形にしてある。
            lower_names: Cow::Owned(vec![Some("firefox".to_string()), None]),
            lower_file_names: Cow::Owned(vec![
                LowerFileName::Text("firefox.lnk".to_string()),
                LowerFileName::SameAsLowerName,
            ]),
        };

        let bytes =
            try_serialize_with_header(INDEX_MAGIC, INDEX_CACHE_VERSION, &cache).expect("serialize");
        let restored: IndexCache<'static> =
            try_deserialize_with_header(&bytes, INDEX_MAGIC, INDEX_CACHE_VERSION)
                .expect("deserialize");

        assert_eq!(restored.built_at, 1700000000);
        assert_eq!(restored.names.len(), 2);
        assert_eq!(restored.names[0], "Firefox");
        assert!(!restored.is_folder[0]);
        assert_eq!(restored.names[1], "Projects");
        assert!(restored.is_folder[1]);
        assert_eq!(restored.config_hash, 12345);
        // Cow フィールドは into_owned() で Vec に戻して比較（deserialize は Owned ゆえ move）。
        assert_eq!(restored.char_masks.into_owned(), vec![0xABu64, 0xCD]);
        assert_eq!(
            restored.file_name_char_masks.into_owned(),
            vec![0x12u64, 0x34]
        );
        assert_eq!(
            restored.lower_names.into_owned(),
            vec![Some("firefox".to_string()), None]
        );
        assert_eq!(
            restored.lower_file_names.into_owned(),
            vec![
                LowerFileName::Text("firefox.lnk".to_string()),
                LowerFileName::SameAsLowerName,
            ]
        );
    }

    /// `golden_fixture` の戻り値（entries / 2 本のマスク / 潰し済みの派生文字列 2 本）。
    type GoldenFixture = (
        Vec<AppEntry>,
        Vec<u64>,
        Vec<u64>,
        Vec<Option<String>>,
        Vec<LowerFileName>,
    );

    /// 現行 golden の fixture。**版を名前に持たない**——凍結バイト列は版ごとに増えるが、
    /// それを生む入力は常に「現行版が凍結している 1 つ」だからである（旧版の定数は当時の
    /// 入力ではなく**自分が何を含むか**を doc に持つ）。
    ///
    /// 3 つの網羅をここで背負う。どれが欠けても、その表現のバイトが凍結されずに素通りする:
    ///
    /// - **`LowerFileName` の 3 状態すべて**（タグの値が変わっても golden が気づかない）
    /// - **木の根と非根の両方**（1..2 件目が親子）。根の `aux` はフルパスの id、非根の `aux` は
    ///   拡張子の id という**同じ列の 2 つの意味**が、これで初めて両方バイトに現れる
    /// - **`sorted_by_path` が真になる並び**（`target_path` のバイト昇順）。偽だけを凍結すると、
    ///   旗の位置や極性が変わっても 1 バイトも動かない
    fn golden_fixture() -> GoldenFixture {
        // **バイト昇順で並べる**（`C:\P` < `C:\a` < `C:\d`）。崩すと `sorted_by_path` が偽に
        // なるうえ、`IndexTree::build` の親の二分探索が取りこぼして 2 件目が根になる
        // ——どちらも「落ちない形での網羅の喪失」である。
        let entries = vec![
            AppEntry {
                name: "Projects".to_string(),
                target_path: "C:\\Projects".to_string(),
                is_folder: true,
            },
            // 唯一の非根。親は 0 番で、`aux` は拡張子 `.exe` の id を指す。
            AppEntry {
                name: "app".to_string(),
                target_path: "C:\\Projects\\app.exe".to_string(),
                is_folder: false,
            },
            AppEntry {
                name: "Firefox".to_string(),
                target_path: "C:\\apps\\firefox.lnk".to_string(),
                is_folder: false,
            },
            AppEntry {
                name: "docs".to_string(),
                target_path: "C:\\docs".to_string(),
                is_folder: true,
            },
        ];
        (
            entries,
            vec![0xABu64, 0xCD, 0xEF, 0x21],
            vec![0x12u64, 0x34, 0x56, 0x78],
            // 2・4 件目は `name` と同一（＝落とせる）。
            vec![
                Some("projects".to_string()),
                None,
                Some("firefox".to_string()),
                None,
            ],
            vec![
                LowerFileName::Absent,
                LowerFileName::Text("app.exe".to_string()),
                LowerFileName::Text("firefox.lnk".to_string()),
                LowerFileName::SameAsLowerName,
            ],
        )
    }

    #[test]
    fn index_cache_on_disk_format_is_stable() {
        // on-disk バイト形式の絶対安定を守る golden テスト。
        // IndexCache のフィールド順・型を変えると（= 既存 index.bin を無言破損）バイト列が変化し
        // このテストが落ちる。save/load が単一 struct を共有する統合後、フィールド reorder は
        // roundtrip テストを素通りするため、この golden が唯一の検出器（version 非バンプでも検出）。
        // 意図的な形式変更（INDEX_CACHE_VERSION バンプ）時は golden を更新すること。
        let (entries, char_masks, file_name_char_masks, lower_names, lower_file_names) =
            golden_fixture();

        // save 経路と同じ Cow::Borrowed で構築する。
        let tree = IndexTree::build(entries.clone());
        let cache = IndexCache {
            built_at: 1_700_000_000,
            names: Cow::Borrowed(&tree.names),
            is_folder: Cow::Borrowed(&tree.is_folder),
            parent: Cow::Borrowed(&tree.parent),
            aux: Cow::Borrowed(&tree.aux),
            table: Cow::Borrowed(&tree.table),
            sorted_by_path: tree.sorted_by_path,
            config_hash: 12345,
            char_masks: Cow::Borrowed(&char_masks),
            file_name_char_masks: Cow::Borrowed(&file_name_char_masks),
            lower_names: Cow::Borrowed(&lower_names),
            lower_file_names: Cow::Borrowed(&lower_file_names),
        };
        let bytes =
            try_serialize_with_header(INDEX_MAGIC, INDEX_CACHE_VERSION, &cache).expect("serialize");

        assert_eq!(
            bytes, GOLDEN_V7,
            "on-disk 形式が変化した。IndexCache のフィールド順/型変更は既存 index.bin を破損する。\
             意図的なら INDEX_CACHE_VERSION をバンプし golden を更新すること"
        );

        let restored: IndexCache<'static> =
            try_deserialize_with_header(GOLDEN_V7, INDEX_MAGIC, INDEX_CACHE_VERSION)
                .expect("凍結 v7 バイトがロードできること");
        assert!(matches!(restored.names, Cow::Owned(_)));
        assert_eq!(restored.names.len(), 4);
        assert_eq!(restored.names[0], "Projects");
        assert!(restored.is_folder[0]);
        assert_eq!(restored.names[1], "app");
        assert!(!restored.is_folder[1]);

        // **木の 3 列は、組み直したフルパスで検算する。** `names` / `is_folder` だけを見ると
        // `parent` / `aux` / `table` は「新コードの出力を新コードで読み返した」だけになり、
        // 親や拡張子の取り違えをそのまま凍結する。突き合わせる相手は fixture の
        // `target_path` リテラル——木を通っていない唯一の原文である。
        let restored_tree = IndexTree::from_parts(
            restored.names.into_owned(),
            restored.is_folder.into_owned(),
            restored.parent.into_owned(),
            restored.aux.into_owned(),
            restored.table.into_owned(),
            restored.sorted_by_path,
        )
        .expect("凍結 v7 の列が木の不変条件を満たすこと");
        assert!(
            restored.sorted_by_path,
            "fixture はバイト昇順ゆえ真である（偽だけを凍結すると旗が動いても気づかない）"
        );
        let mut buf = String::new();
        for (i, entry) in entries.iter().enumerate() {
            restored_tree.path_into(&mut buf, i);
            assert_eq!(
                buf, entry.target_path,
                "凍結 v7 から組み直したフルパスが原文とずれている（index {i}）"
            );
        }

        assert_eq!(restored.char_masks.into_owned(), char_masks);
        assert_eq!(restored.lower_names.into_owned(), lower_names);
        assert_eq!(restored.lower_file_names.into_owned(), lower_file_names);
    }

    /// **旧形式の凍結バイト列が、木の組み直しの唯一の接地である。**
    ///
    /// v7 は `target_path` を持たないので、v7 から実体化した値と組み直しを突き合わせても
    /// 「組み直しの結果どうし」を比べることにしかならない。ここでは **v6 の凍結バイト列**
    /// ——すなわち木を知らない時代に書かれた原文——から読み、木へ組み替えて組み直した結果が
    /// 1 バイトも違わないことを見る。
    ///
    /// 実データ規模の corpus は `search/tests/path.rs` が受け持つ（開発機限定）。ここは
    /// 版をまたいで CI でも走る側である。
    #[test]
    fn index_tree_raw_matches_frozen_v6_specimen() {
        let restored: IndexCacheV6 =
            try_deserialize_with_header(GOLDEN_V6, INDEX_MAGIC, 6).expect("凍結 v6 が読めること");
        let expected: Vec<String> = restored
            .entries
            .iter()
            .map(|e| e.target_path.clone())
            .collect();
        let tree = IndexTree::build(restored.entries);
        let mut buf = String::new();
        for (i, want) in expected.iter().enumerate() {
            tree.path_into(&mut buf, i);
            assert_eq!(&buf, want, "原文の組み直しがずれている（index {i}）");
        }
        assert!(!expected.is_empty(), "凍結 v6 が空では接地にならない");
    }

    /// 凍結 golden（`golden_fixture` の serialize 出力・INDX magic + version 7 ヘッダー込み）。
    ///
    /// **この定数が持つのは forward-stability だけである。** v7 は現行版ゆえ「v7 として実際に
    /// 書かれていた旧バイト列」が存在せず、新コードの出力を凍結する以外に採りようがない
    /// （`snotra-core/CLAUDE.md`「データ永続化の注意」が禁じている向きは、**旧形式の後方互換を
    /// 新出力の golden で代用すること**である）。後方互換はここではなく `GOLDEN_V6` /
    /// `GOLDEN_V5` / `GOLDEN_V4` からの load テストが持ち、木の組み直しの接地は
    /// `index_tree_raw_matches_frozen_v6_specimen` が持つ。
    ///
    /// **末尾の `lower_file_names` は `LowerFileName` のタグである**: `Absent` = 0、
    /// `SameAsLowerName` = 1、`Text` = 2 + 文字列。`lower_names` 側は `Option` の
    /// `None` = 0 / `Some` = 1 + 文字列。タグの割り当てを変えると（＝ variant の宣言順を
    /// 入れ替えると）既存の `index.bin` を無言で誤読するので、ここが落ちる。
    const GOLDEN_V7: &[u8] = &[
        73, 78, 68, 88, 7, 0, 0, 0, 128, 226, 207, 170, 6, 4, 8, 80, 114, 111, 106, 101, 99, 116,
        115, 3, 97, 112, 112, 7, 70, 105, 114, 101, 102, 111, 120, 4, 100, 111, 99, 115, 4, 1, 0,
        0, 1, 4, 255, 255, 255, 255, 15, 0, 255, 255, 255, 255, 15, 255, 255, 255, 255, 15, 4, 2,
        1, 3, 4, 5, 0, 4, 46, 101, 120, 101, 11, 67, 58, 92, 80, 114, 111, 106, 101, 99, 116, 115,
        19, 67, 58, 92, 97, 112, 112, 115, 92, 102, 105, 114, 101, 102, 111, 120, 46, 108, 110,
        107, 7, 67, 58, 92, 100, 111, 99, 115, 1, 185, 96, 4, 171, 1, 205, 1, 239, 1, 33, 4, 18,
        52, 86, 120, 4, 1, 8, 112, 114, 111, 106, 101, 99, 116, 115, 0, 1, 7, 102, 105, 114, 101,
        102, 111, 120, 0, 4, 0, 2, 7, 97, 112, 112, 46, 101, 120, 101, 2, 11, 102, 105, 114, 101,
        102, 111, 120, 46, 108, 110, 107, 1,
    ];

    /// **v7 化の前に実際に書かれていた v6 バイト列**（`target_path` を実体で全件持つ形式）。
    /// `config_hash` は 12345、entries は Firefox / Projects / docs の 3 件。
    ///
    /// 末尾 3 バイト `0, 1` の前後は `LowerFileName` のタグである（割り当ては `GOLDEN_V7` の
    /// doc が正本。v6 と v7 で同じであり、変えれば既存の `index.bin` を無言で誤読する）。
    const GOLDEN_V6: &[u8] = &[
        73, 78, 68, 88, 6, 0, 0, 0, 128, 226, 207, 170, 6, 3, 7, 70, 105, 114, 101, 102, 111, 120,
        19, 67, 58, 92, 97, 112, 112, 115, 92, 102, 105, 114, 101, 102, 111, 120, 46, 108, 110,
        107, 0, 8, 80, 114, 111, 106, 101, 99, 116, 115, 11, 67, 58, 92, 80, 114, 111, 106, 101,
        99, 116, 115, 1, 4, 100, 111, 99, 115, 7, 67, 58, 92, 100, 111, 99, 115, 1, 185, 96, 3,
        171, 1, 205, 1, 239, 1, 3, 18, 52, 86, 3, 1, 7, 102, 105, 114, 101, 102, 111, 120, 1, 8,
        112, 114, 111, 106, 101, 99, 116, 115, 0, 3, 2, 11, 102, 105, 114, 101, 102, 111, 120, 46,
        108, 110, 107, 0, 1,
    ];

    /// **v6 の凍結バイト列から `load_cache_in` が読めること。**
    ///
    /// **`index_tree_raw_matches_frozen_v6_specimen` では代用できない。** あちらは
    /// `try_deserialize_with_header` を直接呼ぶので、`load_cache_in` の枝選択・`config_hash` の
    /// 判定・`CachedLower` の variant・`version` の帰属を 1 つも通らない。
    ///
    /// **v6 は「全ユーザーの `index.bin` が今まさに置かれている版」である。** v7 が現行に
    /// なったことでフォールバック枝へ落ちた——つまりこの枝は新設であり、かつ**最初に
    /// 通る人が最も多い**枝でもある。
    ///
    /// **`CachedLower::Collapsed` で返らなければならない。** `Raw` で返すと `assemble` が
    /// 測り直し、`None` どうしの一致で file name 成分を持たないエントリに旗が立つ。
    #[test]
    fn frozen_v6_bytes_load_as_collapsed_through_load_cache_in() {
        let dir = temp_dir("v6_frozen_through_load_cache_in");
        fs::write(dir.join("index.bin"), GOLDEN_V6).expect("write v6 index.bin");

        let result = load_cache_in(&dir, 12345).expect("v6 の index.bin が読めること");
        assert_eq!(
            result.version, 6,
            "v6 として読めたことが昇格の判断材料になる"
        );
        let (tree, masks) = result.material.into_parts();
        assert_eq!(tree.len(), 3);
        assert_eq!(tree.names[0], "Firefox");

        // 木は `target_path` から建て直される。原文へ戻せることまで見る——v6 の実体を
        // 捨てて木にした段で取りこぼせば、以後この索引のパスは静かに壊れる。
        let mut buf = String::new();
        tree.path_into(&mut buf, 0);
        assert_eq!(buf, "C:\\apps\\firefox.lnk");

        let masks = masks.expect("v6 でもマスクは返る");
        match masks.lower {
            Some(CachedLower::Collapsed {
                lower_names,
                lower_file_names,
            }) => {
                assert_eq!(
                    lower_names,
                    vec![
                        Some("firefox".to_string()),
                        Some("projects".to_string()),
                        None
                    ]
                );
                assert_eq!(
                    lower_file_names,
                    vec![
                        LowerFileName::Text("firefox.lnk".to_string()),
                        LowerFileName::Absent,
                        LowerFileName::SameAsLowerName,
                    ]
                );
            }
            other => panic!("v6 は Collapsed で返らなければならない（実際: {other:?}）"),
        }

        // config_hash が違えば stale 扱いで None（他の版の枝と同じ規律）。
        assert!(load_cache_in(&dir, 12346).is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    /// **v6 化の前に実際に書かれていた v5 バイト列**（派生文字列を全件そのまま持つ形式）。
    /// `config_hash` は 12345、entries は Firefox / Projects の 2 件。
    const GOLDEN_V5: &[u8] = &[
        73, 78, 68, 88, 5, 0, 0, 0, 128, 226, 207, 170, 6, 2, 7, 70, 105, 114, 101, 102, 111, 120,
        19, 67, 58, 92, 97, 112, 112, 115, 92, 102, 105, 114, 101, 102, 111, 120, 46, 108, 110,
        107, 0, 8, 80, 114, 111, 106, 101, 99, 116, 115, 11, 67, 58, 92, 80, 114, 111, 106, 101,
        99, 116, 115, 1, 185, 96, 2, 171, 1, 205, 1, 2, 18, 52, 2, 7, 102, 105, 114, 101, 102, 111,
        120, 8, 112, 114, 111, 106, 101, 99, 116, 115, 2, 1, 11, 102, 105, 114, 101, 102, 111, 120,
        46, 108, 110, 107, 0,
    ];

    /// **v5 の凍結バイト列から `load_cache_in` が読めること。**
    ///
    /// 向きが要点である——v6 の往復（上の golden）が示すのは forward-stability だけで、
    /// 「既存ユーザーの `index.bin` が読めるか」は独立には証明しない
    /// （`snotra-core/CLAUDE.md`「データ永続化の注意」）。
    ///
    /// **`CachedLower::Raw` で返らなければならない。** `Collapsed` で返すと `assemble` が
    /// 測り直しをスキップし、全件実体の列を「潰し済み」と誤解して読み替える。
    #[test]
    fn frozen_v5_bytes_load_as_raw_through_load_cache_in() {
        let dir = temp_dir("v5_frozen_through_load_cache_in");
        fs::write(dir.join("index.bin"), GOLDEN_V5).expect("write v5 index.bin");

        let result = load_cache_in(&dir, 12345).expect("v5 の index.bin が読めること");
        assert_eq!(
            result.version, 5,
            "v5 として読めたことが昇格の判断材料になる"
        );
        let (tree, masks) = result.material.into_parts();
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.names[0], "Firefox");

        let masks = masks.expect("v5 でもマスクは返る");
        match masks.lower {
            Some(CachedLower::Raw {
                lower_names,
                lower_file_names,
            }) => {
                assert_eq!(
                    lower_names,
                    vec!["firefox".to_string(), "projects".to_string()]
                );
                assert_eq!(
                    lower_file_names,
                    vec![Some("firefox.lnk".to_string()), None]
                );
            }
            other => panic!("v5 は Raw で返らなければならない（実際: {other:?}）"),
        }

        // config_hash が違えば stale 扱いで None（v6 経路と同じ規律）。
        assert!(load_cache_in(&dir, 12346).is_none());

        let _ = fs::remove_dir_all(&dir);
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
        let (tree, masks) = result.material.into_parts();
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.names[0], "Firefox");
        let masks = masks.expect("v4 でもマスクは返る");
        assert_eq!(masks.char_masks, vec![0xABu64, 0xCD]);
        match masks.lower {
            Some(CachedLower::Raw {
                lower_names,
                lower_file_names,
            }) => {
                assert_eq!(
                    lower_names,
                    vec!["firefox".to_string(), "projects".to_string()],
                    "v4 から lower_names が復元されないと Wave 1 が走り、初回起動が遅くなる"
                );
                assert_eq!(
                    lower_file_names,
                    vec![Some("firefox.lnk".to_string()), None]
                );
            }
            other => panic!("v4 は Raw で返らなければならない（実際: {other:?}）"),
        }

        // config_hash が違えば stale 扱いで None（v6 経路と同じ規律）。
        assert!(load_cache_in(&dir, 12346).is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    /// v4 形式の `index.bin` を `dir` へ書く（テスト専用の治具）。
    ///
    /// **凍結バイト列（`GOLDEN_V4`）では代用できない。** あちらのエントリは固定であり、
    /// 背景再スキャンは実ファイルシステムを走査した結果と digest を突き合わせる——
    /// 「中身は同じだが版だけ古い」状況を作るには、走査で出てくるエントリそのものを
    /// v4 で書く必要がある。
    fn write_v4_cache_in(dir: &Path, entries: &[AppEntry], config_hash: u64) {
        let lower_names: Vec<String> = entries.iter().map(|e| to_lower_folded(&e.name)).collect();
        let lower_file_names: Vec<Option<String>> = entries
            .iter()
            .map(|e| lower_file_name(&e.target_path))
            .collect();
        let v4 = IndexCacheV4 {
            built_at: 0,
            entries: entries.to_vec(),
            config_hash,
            char_masks: lower_names.iter().map(|s| name_char_mask(s)).collect(),
            file_name_char_masks: lower_file_names
                .iter()
                .map(|s| file_char_mask(s.as_deref()))
                .collect(),
            lower_names,
            lower_file_names,
            // v5 が消したフィールド。**これを読んで捨てることが昇格の動機である。**
            normalized_keys: entries
                .iter()
                .map(|e| normalize_entry_key(&e.target_path))
                .collect(),
        };
        let bytes = try_serialize_with_header(INDEX_MAGIC, 4, &v4).expect("serialize v4");
        fs::write(dir.join("index.bin"), &bytes).expect("write v4 index.bin");
    }

    /// **旧版の `index.bin` は、索引の中身が変わらない限り旧版のまま残り続けていた。**
    ///
    /// 書き戻す契機は cache-miss と背景再スキャンの `Changed` だけであり、走査結果が
    /// キャッシュと一致する限り save は永久に来ない。`IndexCacheV4` の doc が書いていた
    /// 「次回の save で v5 へ昇格する」は、**その「次回」が来ない経路を勘定していなかった**。
    ///
    /// 実運用点で実測して踏んだ（2026-08-07）: v5 導入後の実 `index.bin` が v4 のまま残り、
    /// 毎起動で `normalized_keys` 35.98 MiB を読んでは捨てていた。**症状は「遅い」だけで、
    /// 検索結果は正しいまま**——挙動テストでは原理的に捕まらない類である。
    #[test]
    fn background_rescan_upgrades_stale_format_when_entries_are_unchanged() {
        let _guard = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("rescan_upgrades_stale_format");
        let apps = dir.join("apps");
        fs::create_dir_all(&apps).expect("create apps dir");
        fs::write(apps.join("firefox.lnk"), b"x").expect("write fixture");

        let scan = vec![ScanPath {
            path: apps.to_string_lossy().into_owned(),
            extensions: vec![".lnk".to_string()],
            include_folders: false,
        }];
        let config_hash = compute_config_hash(&scan, false);

        let mut entries = scan_all(&scan, false);
        sort_entries_canonical(&mut entries);
        assert_eq!(entries.len(), 1, "治具の走査が 1 件を返すこと");
        let digest = entries_digest(&entries);

        write_v4_cache_in(&dir, &entries, config_hash);
        assert_eq!(
            cache_byte_breakdown_in(&dir)
                .expect("v4 が読めること")
                .version,
            4,
            "治具が v4 を書けていること（ここが現行版だと以降の検査が自明に通る）"
        );

        let outcome = try_background_rescan_in(
            &dir,
            &scan,
            false,
            config_hash,
            digest,
            current_index_generation(),
            4,
        );

        // **エントリ集合は変わっていない。** 形式の昇格は中身を変えないので `Changed` を
        // 返してはならない——呼び出し側はそれを見てアイコンキャッシュを捨てる。
        assert_eq!(outcome, RescanOutcome::Unchanged);
        assert_eq!(
            cache_byte_breakdown_in(&dir)
                .expect("昇格後も読めること")
                .version,
            INDEX_CACHE_VERSION,
            "版が現行へ昇格していること"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// 対の検査: **版が現行なら書き直さない。** 昇格の条件を「旧版のとき」に絞れておらず
    /// 毎回書いていると、起動のたびに 71 MiB の書き込みが背景で走る（症状は同じく
    /// 「遅い」だけで結果は正しい）。`built_at` を見て、ファイルが据え置かれたことを測る。
    #[test]
    fn background_rescan_does_not_rewrite_when_format_is_current() {
        let _guard = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("rescan_keeps_current_format");
        let apps = dir.join("apps");
        fs::create_dir_all(&apps).expect("create apps dir");
        fs::write(apps.join("firefox.lnk"), b"x").expect("write fixture");

        let scan = vec![ScanPath {
            path: apps.to_string_lossy().into_owned(),
            extensions: vec![".lnk".to_string()],
            include_folders: false,
        }];
        let config_hash = compute_config_hash(&scan, false);

        let mut entries = scan_all(&scan, false);
        sort_entries_canonical(&mut entries);
        let digest = entries_digest(&entries);

        save_cache_sorted_in(&dir, entries.clone(), config_hash);
        let before = fs::read(dir.join("index.bin")).expect("現行版の index.bin が読めること");

        let outcome = try_background_rescan_in(
            &dir,
            &scan,
            false,
            config_hash,
            digest,
            current_index_generation(),
            INDEX_CACHE_VERSION,
        );

        assert_eq!(outcome, RescanOutcome::Unchanged);
        let after = fs::read(dir.join("index.bin")).expect("read index.bin again");
        assert_eq!(
            before, after,
            "版が現行で中身も同じなら、ファイルは 1 バイトも変わらないこと"
        );

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

        let (_, returned) = save_cache_sorted_in(&dir, entries.clone(), config_hash);

        let result = load_cache_in(&dir, config_hash).expect("load cache written to dir");
        let (tree, masks) = result.material.into_parts();
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.names[0], "Firefox");
        assert_eq!(tree.names[1], "Projects");
        let masks = masks.expect("v6 cache should include masks");

        // **書いたものと返したものが同一である。** cache-miss の枝はこの返り値をそのまま
        // 索引の材料にするので、ここがずれると「保存したキャッシュで次回起動したとき」と
        // 「保存した回の起動」で索引の姿が変わる——**どちらも結果は正しく出る**ので挙動
        // テストでは捕まらない。同じ値どうしの同一性ゆえ ⚠（save 側の潰し方が `assemble` の
        // 測り直しと一致するか）の証拠にはならない。捕まえるのは「返す側だけを別実装で
        // 計算する」退行である。
        //
        // **列ごとに分解しない。** 手で分解すると、`CachedMasks` に列が増えたとき**足し忘れ
        // てもコンパイルが通る**。`Collapsed` であることは直下の `match` が期待値つきで見て
        // おり、この等号がそれを返り値側へ運ぶのでカバレッジは減らない。
        assert_eq!(
            returned, masks,
            "返り値が index.bin へ書いたものとずれている"
        );

        // **`Collapsed` で返る。** save 側が `measure_derived_sharing` で潰して書いており、
        // "Firefox" → "firefox" は小文字化で変わるので実体が残り、file name は別物ゆえ `Text`。
        match masks.lower {
            Some(CachedLower::Collapsed {
                lower_names,
                lower_file_names,
            }) => {
                assert_eq!(
                    lower_names,
                    vec![Some("firefox".to_string()), Some("projects".to_string())]
                );
                assert_eq!(
                    lower_file_names,
                    vec![
                        LowerFileName::Text("firefox.lnk".to_string()),
                        // "C:\\Projects" の file name 成分は "Projects" → "projects" で
                        // `lower_name` と一致する。
                        LowerFileName::SameAsLowerName,
                    ]
                );
            }
            other => panic!("v6 は Collapsed で返らなければならない（実際: {other:?}）"),
        }

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

    /// **`load_cache_in` が返す `version` は、実際に読めた枝と一致しなければならない。**
    ///
    /// この値だけが背景再スキャンの昇格判定（`cached_version != INDEX_CACHE_VERSION`）の入力で
    /// あり、**取り違えても検索結果は正しいまま**である——枝に誤って現行版を書けば、その形式の
    /// ユーザーは永久に昇格せず旧形式を読み続ける（症状は「遅い」だけ・実運用点で 1 度踏んだ）。
    /// ゆえに **`load_cache_in` の全枝**の値をここで固定する（枝の数を書かない——版を足した
    /// ときにこの散文だけが腐り、しかも「揃っている」と読めてしまう。実際に v7 を足したとき
    /// 「5 枝すべて」のまま v6 が抜けた）。
    ///
    /// **既存の v2 / v3 テストでは代用できない。** あちらは `try_deserialize_with_header` を
    /// 直接呼んでおり `load_cache_in` の枝選択を通らないので、`version` の帰属を見ていない。
    #[test]
    fn load_cache_in_reports_the_version_it_actually_read() {
        let entries = vec![AppEntry {
            name: "Firefox".to_string(),
            target_path: "C:\\apps\\firefox.lnk".to_string(),
            is_folder: false,
        }];
        let config_hash = 4242u64;
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

        // **現行版**: 製品の save 経路そのものを通す（版のリテラルを書かない——比較相手は
        // `INDEX_CACHE_VERSION` であり、番号を書くとこのコメントだけが版を上げたとき腐る）。
        let dir = temp_dir("version_reported_current");
        save_cache_sorted_in(&dir, entries.clone(), config_hash);
        assert_eq!(
            load_cache_in(&dir, config_hash)
                .expect("現行版が読めること")
                .version,
            INDEX_CACHE_VERSION
        );
        let _ = fs::remove_dir_all(&dir);

        // v6: `target_path` を実体で全件持つ形式。**実運用点が今まさに置かれている版**であり、
        // ここを `INDEX_CACHE_VERSION` と取り違えると全ユーザーが永久に昇格しない。
        let dir = temp_dir("version_reported_v6");
        let v6 = IndexCacheV6 {
            built_at: 0,
            entries: entries.clone(),
            config_hash,
            char_masks: char_masks.clone(),
            file_name_char_masks: file_name_char_masks.clone(),
            lower_names: lower_names.iter().cloned().map(Some).collect(),
            lower_file_names: lower_file_names
                .iter()
                .map(|f| match f {
                    Some(s) => LowerFileName::Text(s.clone()),
                    None => LowerFileName::Absent,
                })
                .collect(),
        };
        fs::write(
            dir.join("index.bin"),
            try_serialize_with_header(INDEX_MAGIC, 6, &v6).expect("serialize v6"),
        )
        .expect("write v6");
        assert_eq!(
            load_cache_in(&dir, config_hash)
                .expect("v6 が読めること")
                .version,
            6
        );
        let _ = fs::remove_dir_all(&dir);

        // v5: 派生文字列を全件そのまま持つ形式。
        let dir = temp_dir("version_reported_v5");
        let v5 = IndexCacheV5 {
            built_at: 0,
            entries: entries.clone(),
            config_hash,
            char_masks: char_masks.clone(),
            file_name_char_masks: file_name_char_masks.clone(),
            lower_names: lower_names.clone(),
            lower_file_names: lower_file_names.clone(),
        };
        fs::write(
            dir.join("index.bin"),
            try_serialize_with_header(INDEX_MAGIC, 5, &v5).expect("serialize v5"),
        )
        .expect("write v5");
        assert_eq!(
            load_cache_in(&dir, config_hash)
                .expect("v5 が読めること")
                .version,
            5
        );
        let _ = fs::remove_dir_all(&dir);

        // v4: 末尾に normalized_keys を持つ形式。
        let dir = temp_dir("version_reported_v4");
        write_v4_cache_in(&dir, &entries, config_hash);
        assert_eq!(
            load_cache_in(&dir, config_hash)
                .expect("v4 が読めること")
                .version,
            4
        );
        let _ = fs::remove_dir_all(&dir);

        // v3: マスクのみ（lower names なし）。
        let dir = temp_dir("version_reported_v3");
        let v3 = IndexCacheV3 {
            built_at: 0,
            entries: entries.clone(),
            config_hash,
            char_masks: char_masks.clone(),
            file_name_char_masks: file_name_char_masks.clone(),
        };
        fs::write(
            dir.join("index.bin"),
            try_serialize_with_header(INDEX_MAGIC, 3, &v3).expect("serialize v3"),
        )
        .expect("write v3");
        assert_eq!(
            load_cache_in(&dir, config_hash)
                .expect("v3 が読めること")
                .version,
            3
        );
        let _ = fs::remove_dir_all(&dir);

        // v2: マスクなし。
        let dir = temp_dir("version_reported_v2");
        let v2 = IndexCacheV2 {
            built_at: 0,
            entries: entries.clone(),
            config_hash,
        };
        fs::write(
            dir.join("index.bin"),
            try_serialize_with_header(INDEX_MAGIC, 2, &v2).expect("serialize v2"),
        )
        .expect("write v2");
        let v2_result = load_cache_in(&dir, config_hash).expect("v2 が読めること");
        assert_eq!(v2_result.version, 2);
        assert!(
            !v2_result.material.has_masks(),
            "v2 はマスクを持たない（枝を取り違えていないことの裏取り）"
        );
        let _ = fs::remove_dir_all(&dir);
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

    /// **`DigestSource` の 2 実装が同じ値を出すこと。ここが背景再スキャンの心臓である。**
    ///
    /// 比べる 2 辺は別々の実装を通る——走査側は `&[AppEntry]`（`target_path` を実体で持つ）、
    /// ロード側は [`IndexTree`]（持たずに組み直す）。**main では実装が 1 つで、食い違いは
    /// 構造的に起こりえなかった**が、木を導入した時点でそれは失われた。1 件でも値がずれれば
    /// `changed` が毎起動 true になり、16.5 MiB の `index.bin` を書き直し、
    /// `RescanOutcome::Changed` がアイコンキャッシュを捨てる——**検索結果は正しいまま**なので
    /// 挙動テストは 1 本も落ちない。
    ///
    /// 木を通る形を fixture に全部載せる: 根 / 非根 / 拡張子あり・なし / folder / file /
    /// 非 ASCII / 空白。**根だけの fixture では意味が無い**——根は `table` のフルパスを
    /// そのまま返すので、組み直しの規則を 1 つも通らない。
    #[test]
    fn digest_over_tree_matches_digest_over_scanned_entries() {
        let entries: Vec<AppEntry> = [
            ("Projects", "C:\\Projects", true),
            ("My App", "C:\\Projects\\My App", true),
            ("run", "C:\\Projects\\My App\\run.exe", false),
            ("Ünïcode", "C:\\Projects\\Ünïcode.LNK", false),
            ("apps", "C:\\apps", true),
            ("tool", "C:\\apps\\tool.exe", false),
        ]
        .iter()
        .map(|(name, path, is_folder)| AppEntry {
            name: (*name).to_string(),
            target_path: (*path).to_string(),
            is_folder: *is_folder,
        })
        .collect();

        let tree = IndexTree::build(entries.clone());
        // fixture の前提: 木が実際に枝を持っていること（全件が根だと組み直しを検査しない）。
        assert!(
            tree.parent
                .iter()
                .any(|p| *p != crate::index_tree::NO_PARENT),
            "fixture が全件根になっている（親の解決が効いていない）"
        );
        assert_eq!(
            digest_over(&tree),
            entries_digest(&entries),
            "木とスキャン結果の digest がずれている——再スキャンが毎起動走り続ける"
        );
    }

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
            INDEX_CACHE_VERSION,
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
            // **現行版を渡す。** 旧版にすると昇格の枝へ入り、このテストが実 `config_dir` の
            // `index.bin` を空の内容で上書きする（開発者の索引が消える）。
            cached_version: INDEX_CACHE_VERSION,
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
            save_cache_sorted_in(&dir, Vec::new(), new_hash);
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
            INDEX_CACHE_VERSION,
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
        let entries = scan_path_dirs(&path_list, &IndexTree::empty(), true);

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
        let entries = scan_path_dirs(&path_list, &IndexTree::build(existing.clone()), true);

        assert!(entries.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_path_dirs_keeps_candidate_that_only_shares_a_file_name() {
        // 事前フィルタはファイル名しか見ないので、別ディレクトリの同名ファイルは必ず
        // 通り抜ける。**通り抜けた先のフルパス比較が効いていないと、起動できるはずの
        // exe が黙って消える**（返り値が減るだけで panic もテスト失敗も起きない）。
        let dir = temp_dir("path_same_name");
        fs::write(dir.join("tool.exe"), "").unwrap();

        let existing = vec![AppEntry {
            name: "tool".to_string(),
            target_path: "C:\\elsewhere\\tool.exe".to_string(),
            is_folder: false,
        }];

        let path_list = dir.to_string_lossy().to_string();
        let entries = scan_path_dirs(&path_list, &IndexTree::build(existing.clone()), true);

        assert_eq!(entries.len(), 1, "ディレクトリが違うので新規のはず");
        assert_eq!(entries[0].name, "tool");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_path_dirs_rejects_only_the_matching_candidate_among_several() {
        // **候補が複数あり、その一部だけが落ちる経路。** 旧実装は判定と採用が
        // `if seen.insert(key) { push }` の同一式にあり、どれを落とすかがずれることは
        // 原理的に起きなかった。反転で「候補の索引 → `rejected` → `zip`」の 3 段に
        // 分解したので、ずれうる箇所が新設されている。**ずれても件数は合いうる**ため、
        // 名前まで見ないと沈黙する（起動できるはずの exe が消え、既存が重複で入る）。
        let dir = temp_dir("path_partial");
        fs::write(dir.join("a.exe"), "").unwrap();
        fs::write(dir.join("b.exe"), "").unwrap();
        fs::write(dir.join("c.exe"), "").unwrap();

        let existing = vec![AppEntry {
            name: "b".to_string(),
            target_path: dir.join("b.exe").to_string_lossy().into_owned(),
            is_folder: false,
        }];

        let path_list = dir.to_string_lossy().to_string();
        let entries = scan_path_dirs(&path_list, &IndexTree::build(existing.clone()), true);

        // `read_dir` の順序は OS の保証を持たないので、ここは順序ではなく**集合**で見る。
        // 検出力は落ちない——添字がずれれば落ちるのは別の候補になるので、`b` が結果へ
        // 混ざって集合が変わる。**単一ディレクトリ内の順序はどのテストも固定していない**
        // （`read_dir` に順序保証が無いので原理的にできない。`..._preserves_enumeration_order`
        // が固定するのは PATH ディレクトリ**間**の順序である）。
        let mut names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, vec!["a", "c"], "真ん中の候補だけが落ちるはず");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_path_dirs_rejects_the_right_one_among_same_named_candidates() {
        // **篩の同じバケットに候補が 2 つ入る経路。** `by_file_key` の値が `Vec<usize>`
        // である理由そのものであり、同名 exe が別ディレクトリに並ぶのは実運用で珍しく
        // ない（多重インストール）。内側ループを `idxs.first()` へ縮めても他のテストは
        // 全部通る——そのとき落ちるのは先頭の候補だけなので、**既存にある方が残って
        // 索引へ重複で入る**。件数もパスも見ないと沈黙する。
        let dir_a = temp_dir("path_samename_a");
        let dir_b = temp_dir("path_samename_b");
        fs::write(dir_a.join("tool.exe"), "").unwrap();
        fs::write(dir_b.join("tool.exe"), "").unwrap();

        let existing = vec![AppEntry {
            name: "tool".to_string(),
            target_path: dir_b.join("tool.exe").to_string_lossy().into_owned(),
            is_folder: false,
        }];

        let path_list = format!("{};{}", dir_a.display(), dir_b.display());
        let entries = scan_path_dirs(&path_list, &IndexTree::build(existing.clone()), true);

        assert_eq!(entries.len(), 1, "既存にある dir_b 側だけが落ちるはず");
        assert_eq!(
            entries[0].target_path,
            dir_a.join("tool.exe").to_string_lossy(),
            "落とす相手を同名の別候補と取り違えている"
        );

        let _ = fs::remove_dir_all(&dir_a);
        let _ = fs::remove_dir_all(&dir_b);
    }

    #[test]
    fn scan_path_dirs_skips_existing_paths_written_in_other_notations() {
        // **事前フィルタが偽陰性を出さないことの検査。** 既存エントリ側の表記が違っても
        // （大文字・`/` 区切り・前後の空白）、正規化キーが一致するならファイル名キーも
        // 必ず一致して篩を通り、フルパス比較で落ちる。ここが破れると重複エントリが
        // 索引へ入る——これも結果が「それらしく」出るので挙動テストでは捕まらない。
        let dir = temp_dir("path_notation");
        fs::write(dir.join("tool.exe"), "").unwrap();

        let canonical = dir.join("tool.exe").to_string_lossy().into_owned();
        let path_list = dir.to_string_lossy().to_string();

        for variant in [
            canonical.to_ascii_uppercase(),
            canonical.replace('\\', "/"),
            format!("  {canonical}  "),
        ] {
            let existing = vec![AppEntry {
                name: "tool".to_string(),
                target_path: variant.clone(),
                is_folder: false,
            }];
            let entries = scan_path_dirs(&path_list, &IndexTree::build(existing.clone()), true);
            assert!(
                entries.is_empty(),
                "表記 {variant:?} で重複を落とせていない"
            );
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_path_dirs_preserves_enumeration_order() {
        // 返り値は呼び出し側で `entries.extend` されるだけでソートし直されない
        // （`main.rs` の起動経路・`indexing.rs` の背景ビルド経路とも）。反転で
        // 「積みながら返す」から「候補を作って落とす」へ変えたので、順序を固定する。
        let dir_a = temp_dir("path_order_a");
        let dir_b = temp_dir("path_order_b");
        fs::write(dir_a.join("first.exe"), "").unwrap();
        fs::write(dir_b.join("second.exe"), "").unwrap();

        let path_list = format!("{};{}", dir_a.display(), dir_b.display());
        let entries = scan_path_dirs(&path_list, &IndexTree::empty(), true);

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["first", "second"],
            "PATH ディレクトリの順序を保つ"
        );

        let _ = fs::remove_dir_all(&dir_a);
        let _ = fs::remove_dir_all(&dir_b);
    }

    #[test]
    fn scan_path_dirs_ignores_non_executable_extensions() {
        let dir = temp_dir("path_exts");
        fs::write(dir.join("tool.exe"), "").unwrap();
        fs::write(dir.join("lib.dll"), "").unwrap();
        fs::write(dir.join("readme.txt"), "").unwrap();
        fs::write(dir.join("data.json"), "").unwrap();

        let path_list = dir.to_string_lossy().to_string();
        let entries = scan_path_dirs(&path_list, &IndexTree::empty(), true);

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
        let entries = scan_path_dirs(&path_list, &IndexTree::empty(), true);

        assert_eq!(entries.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_path_dirs_handles_nonexistent_dir() {
        let entries = scan_path_dirs("C:\\nonexistent_dir_12345", &IndexTree::empty(), true);
        assert!(entries.is_empty());
    }

    #[test]
    fn scan_path_dirs_handles_empty_path_list() {
        let entries = scan_path_dirs("", &IndexTree::empty(), true);
        assert!(entries.is_empty());
    }

    #[test]
    fn extend_cached_masks_grows_raw_vecs() {
        let mut masks = CachedMasks {
            char_masks: vec![0xAB],
            file_name_char_masks: vec![0xCD],
            lower: Some(CachedLower::Raw {
                lower_names: vec!["existing".to_string()],
                lower_file_names: vec![Some("existing.lnk".to_string())],
            }),
        };

        let new_entries = vec![AppEntry {
            name: "tool".to_string(),
            target_path: "C:\\bin\\tool.exe".to_string(),
            is_folder: false,
        }];

        extend_cached_masks(&mut masks, &new_entries);

        assert_eq!(masks.char_masks.len(), 2);
        assert_eq!(masks.file_name_char_masks.len(), 2);
        match masks.lower {
            Some(CachedLower::Raw {
                lower_names,
                lower_file_names,
            }) => {
                assert_eq!(
                    lower_names,
                    vec!["existing".to_string(), "tool".to_string()]
                );
                assert_eq!(
                    lower_file_names,
                    vec![
                        Some("existing.lnk".to_string()),
                        Some("tool.exe".to_string())
                    ],
                    "Raw へは潰さずそのまま足す（`assemble` が後で測る）"
                );
            }
            other => panic!("variant は保たれなければならない（実際: {other:?}）"),
        }
    }

    /// **潰し済みの列へは、同じ判定を通した値だけを足す。**
    ///
    /// `assemble` は `Collapsed` を測り直さないので、ここで生の値を混ぜると PATH エントリの
    /// 分だけが索引の読み替えとずれる——`entry_view` はディスクの潰し方を信じるため、
    /// **クラッシュも検索の失敗も起こさず、スコアだけが変わる**。
    #[test]
    fn extend_cached_masks_collapses_before_appending_to_collapsed_vecs() {
        let mut masks = CachedMasks {
            char_masks: vec![0xAB],
            file_name_char_masks: vec![0xCD],
            lower: Some(CachedLower::Collapsed {
                lower_names: vec![None],
                lower_file_names: vec![LowerFileName::SameAsLowerName],
            }),
        };

        let new_entries = vec![
            // `name` が既に小文字 → `lower_name` は落とせる。file name は拡張子ぶん別物。
            AppEntry {
                name: "tool".to_string(),
                target_path: "C:\\bin\\tool.exe".to_string(),
                is_folder: false,
            },
            // 大文字を含む → `lower_name` は実体を持つ。file name 成分は `lower_name` と同一。
            AppEntry {
                name: "Docs".to_string(),
                target_path: "C:\\Docs".to_string(),
                is_folder: true,
            },
        ];

        extend_cached_masks(&mut masks, &new_entries);

        assert_eq!(masks.char_masks.len(), 3);
        match masks.lower {
            Some(CachedLower::Collapsed {
                lower_names,
                lower_file_names,
            }) => {
                assert_eq!(
                    lower_names,
                    vec![None, None, Some("docs".to_string())],
                    "`name` と同一なら追記側でも落とす"
                );
                assert_eq!(
                    lower_file_names,
                    vec![
                        LowerFileName::SameAsLowerName,
                        LowerFileName::Text("tool.exe".to_string()),
                        LowerFileName::SameAsLowerName,
                    ]
                );
            }
            other => panic!("variant は保たれなければならない（実際: {other:?}）"),
        }

        // **マスクは潰す前の文字列から取る**（記録側の
        // `derived_masks_come_from_the_uncollapsed_strings` と同じ不変条件を追記側でも見る）。
        // 潰した後に取ると `SameAsLowerName` の 3 件目が `file_char_mask(None) == 0` になる。
        assert_eq!(
            masks.file_name_char_masks[1],
            file_char_mask(Some("tool.exe"))
        );
        assert_eq!(
            masks.file_name_char_masks[2],
            file_char_mask(Some("docs")),
            "`SameAsLowerName` へ潰れる件でも、マスクは潰す前の \"docs\" から取る"
        );
        assert_ne!(masks.file_name_char_masks[2], 0);
        assert_eq!(masks.char_masks[1], name_char_mask("tool"));
        assert_eq!(masks.char_masks[2], name_char_mask("docs"));
    }

    /// **マスクは潰す前の完全な文字列から導出する**（順序の不変条件・[`derive_entry_collapsed`]）。
    ///
    /// 潰した後に取ると、`lower_name` が `None` へ潰れた件は `name` から、file name が
    /// `SameAsLowerName` / `Absent` へ潰れた件は `file_char_mask(None) == 0` から取ることに
    /// なり、**pre-filter が false negative を出してその経路のエントリだけが検索でヒット
    /// しなくなる**。結果は「それらしく」出るので挙動テストでは捕まらない——ここで潰れた件の
    /// マスクを、潰す前の文字列から取ったマスクと直接突き合わせる。
    ///
    /// `derive_columns` は I/O を持たないので temp dir を要さない。
    #[test]
    fn derived_masks_come_from_the_uncollapsed_strings() {
        let entries = vec![
            // 両方潰れない: `name` に大文字、file name は拡張子ぶん `lower_name` と別。
            AppEntry {
                name: "Tool".to_string(),
                target_path: "C:\\bin\\Tool.exe".to_string(),
                is_folder: false,
            },
            // file name が `lower_name` と同一 → `SameAsLowerName` へ潰れる。
            AppEntry {
                name: "Docs".to_string(),
                target_path: "C:\\Docs".to_string(),
                is_folder: true,
            },
            // `name` が既に小文字 → `lower_names[2]` は `None` へ潰れる。
            AppEntry {
                name: "notes".to_string(),
                target_path: "C:\\bin\\notes.txt".to_string(),
                is_folder: false,
            },
            // file name 成分が無い → `Absent`（マスクは 0 が正しい唯一の件）。
            AppEntry {
                name: "Root".to_string(),
                target_path: "C:\\".to_string(),
                is_folder: true,
            },
        ];

        let derived = derive_columns(entries);

        // 前提: 潰れることを先に固定する（潰れなくなればこのテストは自明に通ってしまう）。
        assert_eq!(
            derived.lower_names,
            vec![
                Some("tool".to_string()),
                Some("docs".to_string()),
                None,
                Some("root".to_string())
            ]
        );
        assert_eq!(
            derived.lower_file_names,
            vec![
                LowerFileName::Text("tool.exe".to_string()),
                LowerFileName::SameAsLowerName,
                LowerFileName::Text("notes.txt".to_string()),
                LowerFileName::Absent,
            ]
        );

        // 本題: マスクは潰す前の文字列に対応する。
        assert_eq!(derived.char_masks[0], name_char_mask("tool"));
        assert_eq!(derived.char_masks[1], name_char_mask("docs"));
        assert_eq!(
            derived.char_masks[2],
            name_char_mask("notes"),
            "`None` へ潰れた件も、マスクは潰す前の \"notes\" から取る"
        );
        assert_eq!(derived.char_masks[3], name_char_mask("root"));

        assert_eq!(
            derived.file_name_char_masks[0],
            file_char_mask(Some("tool.exe"))
        );
        assert_eq!(
            derived.file_name_char_masks[1],
            file_char_mask(Some("docs")),
            "`SameAsLowerName` へ潰れた件も、マスクは潰す前の \"docs\" から取る"
        );
        assert_ne!(
            derived.file_name_char_masks[1], 0,
            "潰した後に取ると `file_char_mask(None) == 0` になる"
        );
        assert_eq!(
            derived.file_name_char_masks[2],
            file_char_mask(Some("notes.txt"))
        );
        assert_eq!(
            derived.file_name_char_masks[3], 0,
            "file name 成分が無い件だけが 0 である"
        );
    }

    #[test]
    fn extend_cached_masks_handles_absent_lower() {
        let mut masks = CachedMasks {
            char_masks: vec![0xAB],
            file_name_char_masks: vec![0xCD],
            lower: None,
        };

        let new_entries = vec![AppEntry {
            name: "tool".to_string(),
            target_path: "C:\\bin\\tool.exe".to_string(),
            is_folder: false,
        }];

        extend_cached_masks(&mut masks, &new_entries);

        assert_eq!(masks.char_masks.len(), 2);
        assert_eq!(masks.file_name_char_masks.len(), 2);
        assert!(masks.lower.is_none());
    }
}
