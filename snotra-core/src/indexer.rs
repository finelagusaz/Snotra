//! スキャン対象の列挙・重複排除、インデックスキャッシュ（`index.bin`）の入出力、そして
//! **索引の材料を組のまま運ぶ器**（[`IndexMaterial`]）。
//!
//! `index.bin` への書き込みは `INDEX_WRITE_LOCK` で単一書き手に直列化し、tmp→rename の
//! 食い合いによる破損を防ぐ。**走査の契機は明示操作だけである**——初回構築・`/s` による
//! 手動再構築・設定変更による再構築の 3 つで、キャッシュヒットの起動は読むだけで終わる
//! （判断の記録は `docs/adr/ADR-rescan-explicit-only.md`）。
//!
//! 木と派生データを 1 つの型へ束ねるのはこのモジュールの責務である——**両者の長さが揃うこと**
//! を、消費側の規約ではなく型で持つ（`index.bin` から来た組は `IndexMaterial::from_untrusted`
//! が検証し、揃わなければ全走査へ落とす）。索引を建てる側は `search` にあり、そちらは組を
//! ほどかずに受け取る（理由は [`IndexMaterial`] の doc）。

use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::fs::Metadata;
use std::hash::{Hash, Hasher};
use std::os::windows::fs::MetadataExt;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM};

use crate::binfmt::{BinFile, try_deserialize_with_header};
use crate::config::{Config, ScanPath};
use crate::index_tree::{IndexTree, NameArena};
use crate::query::{
    file_char_mask, lower_file_name, measure_derived_sharing, name_char_mask, to_lower_folded,
};
use crate::str_arena::{LowerFileColumn, LowerFileSlot, LowerNameColumn};

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
/// 出所は `index.bin` から読んだもの（キャッシュヒット）と、`save_cache_sorted_in` が書いたその足で返したもの（反復 11 以降）である。**出所を数え上げてはならない**——かつて「**出所は 2 つある**」と書きながら**同じ段落の次の文で数え上げを禁じていた**（`docs/comment-guidelines.md`「第一原則: コメントは「なぜ」を書く」が経路の数を書かないよう定めているのに、自分で反した形）。正本は `save_cache_sorted` と `load_cache_in` の分岐であり、数えた散文は枝が増えるたびに腐る。**出所によって表現は変わらない**——潰し方の判定は `query::measure_derived_sharing` の 1 か所を通るので、消費側は出所を区別しない（区別するのは `lower` の variant だけである）。
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

impl LowerFileName {
    /// 列（[`LowerFileColumn`]）へ積むための借用の形。
    ///
    /// **この対応が 1 対 1 であることが、線上表現が変わっていないことの前提である**
    /// ——3 状態のどれかを別の状態へ写すと、`entry_view` の読み替えが静かにずれる。
    pub(crate) fn as_slot(&self) -> LowerFileSlot<'_> {
        match self {
            Self::Absent => LowerFileSlot::Absent,
            Self::SameAsLowerName => LowerFileSlot::SameAsLowerName,
            Self::Text(s) => LowerFileSlot::Text(s),
        }
    }
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
        ///
        /// **型は `Vec<Option<String>>` ではなく [`LowerNameColumn`] だが、線上のバイト列は
        /// 変わっていない**（正本は `crate::str_arena` の doc）——`lower_file_names` も同じ。
        lower_names: LowerNameColumn,
        lower_file_names: LowerFileColumn,
    },
    /// v5 / v4。全件が実体を持つ未測定の列。`assemble` が測って潰す。
    Raw {
        lower_names: Vec<String>,
        lower_file_names: Vec<Option<String>>,
    },
}

/// 2 つの**正規化済み**の根について、`a` が `b` の祖先か同一か。
///
/// **境界の 2 枝を 1 本にまとめてはならない。** [`crate::config::normalize_scan_path_key`] は
/// ドライブ根だけ末尾 `\` を残す（`c:\` に対し `c:\tools`）。ドライブ根にも境界チェックを
/// 課すと `c:\\tools` を探して偽になり、非ドライブ根から外すと `c:\tools` が `c:\toolsextra`
/// を入れ子だと誤判定する。
fn is_ancestor_or_same(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    b.len() > a.len() && b.starts_with(a) && (a.ends_with('\\') || b.as_bytes()[a.len()] == b'\\')
}

/// 2 つの正規化済みの根が重なるか（どちらが祖先でもよい）。
fn roots_overlap_pair(a: &str, b: &str) -> bool {
    is_ancestor_or_same(a, b) || is_ancestor_or_same(b, a)
}

/// 走査中の根の役割。**重複排除に払う代金を根ごとに決める。**
#[derive(Clone, Copy)]
struct RootRole {
    /// 先行する根と重なる → 既出かを照合する。
    check: bool,
    /// 後続の根と重なる → 自分のキーを積む。
    record: bool,
}

/// 根ごとに [`RootRole`] を決める。
///
/// **積むのは「後続の根と重なる」側だけである。** 木の走査は同じディレクトリを二度読まない
/// ので、あるエントリが二度現れるのは「根 `i` に入ったものが、後続の根 `j > i` の走査でも
/// 現れる」ときに限る。ゆえに根 `i` のキーを保持する必要があるのは後続に重なる根があるとき
/// だけで、**最後の根は（先行と重なっていても）照合するだけでよい**。
///
/// **額は根の順序に依存する。** 実運用点では最大の根 `C:\` が最後に来るため、その 30 万件が
/// 丸ごと「照合のみ」になる。**これは判定の欠陥ではない**——順序に対して述語は正しく、額だけが
/// 構成に依存する。**順序を並べ替えて額を取りに行ってはならない**: 重複排除は先勝ちであり
/// （[`Dedup::accept`]）、勝つ根が変われば索引の中身そのものが変わる——根ごとに
/// `extensions` / `include_folders` が違い、`target_path` は根の文字列へ連ねて組むので
/// 字面もその根のものになる。
///
/// **完全一致も重なりとして拾う。** `scan_all` は `dedup_scan_paths` を通さない配列も受け取る
/// （`src-tauri` の `icon_pipeline_cost_probe` が `Config::default_scan_paths()` を直接渡す）。
///
/// 根は一桁ゆえ全ペア走査で無料である。
fn root_roles(scan_paths: &[ScanPath]) -> Vec<RootRole> {
    let keys: Vec<String> = scan_paths
        .iter()
        .map(|sp| crate::config::normalize_scan_path_key(&sp.path))
        .collect();
    (0..keys.len())
        .map(|i| RootRole {
            check: keys[..i].iter().any(|h| roots_overlap_pair(h, &keys[i])),
            record: keys[i + 1..]
                .iter()
                .any(|j| roots_overlap_pair(&keys[i], j)),
        })
        .collect()
}

/// 走査中の重複排除の状態。**集合・バッファ・根の役割を 1 つに束ねる**——別々の引数で
/// 並べると、再帰の呼び出し点で組を崩せてしまう。
struct Dedup {
    /// 重なる根が 1 つも無ければ `None`（この走査は重複排除を必要としない）。
    set: Option<std::collections::HashSet<String>>,
    /// 照合だけの根で使い回す正規化キーのバッファ。**確保を走査あたり 1 回に抑える。**
    buf: String,
    /// いま走査している根の役割。[`scan_all`] のループが根ごとに差し替える。
    role: RootRole,
}

impl Dedup {
    /// このエントリを採用してよいか。
    ///
    /// **`record` が偽の根で `normalize_entry_key` を呼ばないことが本設計の全部である**
    /// ——実運用点では最大の根がこちらへ回り、30 万件ぶんの `String` 確保が消える。
    /// 照合は [`normalize_entry_key_into`] で 1 本のバッファへ詰め直し、`HashSet<String>` を
    /// `&str` で引く（`Borrow<str>`）。**記録側と照合側が同じ関数を通ることがバイト一致の
    /// 根拠である**——別実装を書き起こしてはならない。
    fn accept(&mut self, path: &str) -> bool {
        let Some(set) = self.set.as_mut() else {
            return true;
        };
        match (self.role.check, self.role.record) {
            // 積む根は insert が照合を兼ねる(既出なら false が返る)。
            (_, true) => set.insert(normalize_entry_key(path)),
            (true, false) => {
                normalize_entry_key_into(&mut self.buf, path);
                !set.contains(self.buf.as_str())
            }
            (false, false) => true,
        }
    }
}

pub fn scan_all(scan_paths: &[ScanPath], show_hidden_system: bool) -> Vec<AppEntry> {
    let mut entries = Vec::new();
    let roles = root_roles(scan_paths);
    // **どの根も集合に触れないなら建てない**（根拠は [`root_roles`] の doc）。
    let needs_set = roles.iter().any(|r| r.check || r.record);
    let mut dedup = Dedup {
        set: needs_set.then(std::collections::HashSet::new),
        buf: String::new(),
        role: RootRole {
            check: false,
            record: false,
        },
    };

    for (sp, role) in scan_paths.iter().zip(&roles) {
        dedup.role = *role;
        let ext_set = build_extension_list(&sp.extensions);
        scan_directory_with_extensions(
            Path::new(&sp.path),
            &ext_set,
            sp.include_folders,
            show_hidden_system,
            &mut entries,
            &mut dedup,
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
    dedup: &mut Dedup,
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
                    if dedup.accept(path_str.as_ref()) {
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
                dedup,
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
                if !name.is_empty() && dedup.accept(path_str.as_ref()) {
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

/// `load_or_scan_with_stats` の各フェーズ所要時間。
///
/// **`cache_load_ms` と `total_ms` の間に処理を足すときは、必ずここに並ぶ項目を作ること。**
/// 項目が無い処理は `total_ms` にしか効かず、差を読む者がいなければ計測上は存在しないままに
/// なる。反復 6 で実際にそうなった——ロード直後に全エントリを複製する処理がここに居たが、
/// `cache_load_ms` は複製の前で止まるため、起動段の live ブロックの 1/3 を占めたまま
/// どのフェーズにも現れなかった（項目を足して初めて見えた）。
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
    pub scan_ms: u128,
    pub sort_ms: u128,
    /// キャッシュ保存にかかった時間。**枝によって和への足し方が違う。**
    ///
    /// cache-miss 枝（scan して save する）では scan_ms / sort_ms に続く独立フェーズであり、
    /// フェーズの和に足す。**cache-hit 枝で旧版昇格（`upgrade_legacy_cache_in`）が走った
    /// 場合はここに昇格の save 時間が入るが、`cache_load_ms` の内数である**
    /// （`cache_read_ms` と同じ扱い——足すと二重計上になり、`total_ms` から差し引く残余計算が
    /// 負に振れて `saturating_sub` に黙って潰される）。save は `load_cache_in` 呼び出しの
    /// 内側（`LegacyUpgrade::Write` で旧版を読んだとき）で起きるため、cache_load_ms の外に
    /// 出しようがない。cache-hit かつ現行版を読んだときは 0。**この 0 は「昇格が走らなかった」
    /// と「走ったが 1 ms を切った」を区別しない**——区別が要る読み手は
    /// `LoadCacheResult::upgrade_save_ms` の variant を見る（#1054 / #1063）。
    ///
    /// **`load_or_scan_with_stats`（この struct の生成元）は常に `LegacyUpgrade::Write` で
    /// 呼ぶ**——`LegacyUpgrade::Skip` は corpus テストの入口（`load_cached_entries`）専用で、
    /// `LoadOrScanStats` を生成しない。ゆえにここでは `Skip` は考慮しなくてよい
    /// （`LoadCacheResult::upgrade_save_ms` の doc は `Skip` 経由の `None` も併記しているが、
    /// それは `LoadCacheResult` 自体が両方の呼び出し元を持つため）。
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
    ///
    /// **型は `Vec<String>` ではなく [`NameArena`] だが、線上のバイト列は変わっていない**
    /// ——アリーナは `seq of str` として読み書きし、要素ごとの `String` を作らずに 1 本の
    /// バッファへ流し込む（正本は [`NameArena`] の doc）。ゆえにこの変更は
    /// `INDEX_CACHE_VERSION` のバンプを伴わず、旧版フォールバックも増えない。
    /// **検知器は `index_tree` 側の `arena_wire_format_is_identical_to_vec_of_string` である**
    /// ——下の golden は名前の形を混ぜて持たないので、この一致だけを守る役には立たない
    /// （射程は [`NameArena`] の doc）。
    names: Cow<'a, NameArena>,
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
    ///
    /// **型は `[Option<String>]` ではなく [`LowerNameColumn`] だが、線上のバイト列は
    /// 変わっていない**——`names` が [`NameArena`] へ移ったのと同じ理屈で、要素ごとの
    /// `String` を作らずに 1 本のバッファへ流し込む（正本は `crate::str_arena` の doc）。
    /// **検知器は `str_arena` 側の `lower_name_column_wire_format_is_identical_to_vec_of_option_string`
    /// である**——下の golden は名前の形を混ぜて持たないので、この一致を守る役には立たない。
    lower_names: Cow<'a, LowerNameColumn>,
    /// 3 状態（`LowerFileName`）。「無い」と「`lower_name` と同一」を別の値で表す。
    ///
    /// 線上表現は `Vec<LowerFileName>` のままである（`lower_names` と同じ理屈。検知器は
    /// `lower_file_column_wire_format_is_identical_to_vec_of_lower_file_name`）。
    lower_file_names: Cow<'a, LowerFileColumn>,
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
    /// 昇格が持ち越す（[`BuiltAt::Carried`]）。**旧版のこの値は死んでいない。**
    built_at: u64,
    entries: Vec<AppEntry>,
    config_hash: u64,
    char_masks: Vec<u64>,
    file_name_char_masks: Vec<u64>,
    /// **v7 と線上表現が同一なので、同じ列の型で読む。** ここを `Vec<Option<String>>` の
    /// ままにすると、旧版枝でだけ per-entry の `String` が 41,994 個復活し、しかも
    /// `CachedLower::Collapsed` へ渡すために詰め替えが要る（正本は `crate::str_arena` の doc）。
    lower_names: LowerNameColumn,
    lower_file_names: LowerFileColumn,
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
    /// 昇格が持ち越す（[`BuiltAt::Carried`]）。**旧版のこの値は死んでいない。**
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
    /// 昇格が持ち越す（[`BuiltAt::Carried`]）。**旧版のこの値は死んでいない。**
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
    /// 昇格が持ち越す（[`BuiltAt::Carried`]）。**旧版のこの値は死んでいない。**
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
    /// 昇格が持ち越す（[`BuiltAt::Carried`]）。**旧版のこの値は死んでいない。**
    built_at: u64,
    entries: Vec<AppEntry>,
    config_hash: u64,
}

/// `index.bin` の有効性を判定する鍵。**走査対象そのものの同一性だけを表す**（入力の内訳と、
/// `include_path_env` / `migemo_enabled` を含めない理由は [`scan_identity_hash`] の doc）。
///
/// **この値は `index.bin` に焼き込まれ、次の起動が同じ計算で照合する**（[`load_cache_in`] の
/// 各枝の `config_hash != config_hash` 判定）。ゆえに**入力・順序・ハッシュ関数のどれを変えても
/// 全ユーザーのキャッシュが一斉に無効になる**——不一致は破損ではなく「設定が変わった」と
/// 読まれるので、次の起動は cache-miss 枝へ落ちて 22〜30 秒の全走査を払う（索引の更新は
/// 明示操作だけという設計上、これが自動で走る唯一の場所である）。
///
/// **[`DefaultHasher`] の出力は Rust のリリース間で安定しない**（std の契約——値ではなく
/// アルゴリズムが未規定である）。ツールチェインを上げた版を配ると、設定を何も変えていない
/// ユーザーが一度だけ全走査を払いうる。**症状は「その日の起動だけ遅い」で、検索結果は
/// 正しいまま**ゆえ挙動テストでは捕まらない。安定を要求するなら std ではないハッシュへ
/// 移すことになり、それ自体が同じ一斉無効化を 1 回払う。
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

/// `compute_config_hash`（private）を crate 外から呼ぶための薄い入口。**走査対象そのものの
/// 同一性だけを表す**——入力は `scan` と `show_hidden_system` の 2 つだけで、
/// `include_path_env` / `migemo_enabled` は含まない（それらは走査ではなく索引の**構築**を
/// 左右する入力であり、`engine::IndexInputs` が別に持つ）。
///
/// **呼び出し元（`src-tauri` の `apply_rescanned_index`）は #1001 で撤去済みで、
/// この関数は現在どこからも呼ばれない**——背景再スキャンが返した材料について、
/// それを走査した時点の対象と差し替え時点の現在 config が同じかをこの入口経由で照合して
/// いた。**内部を丸ごと公開する代わりに、比較に要る計算だけをここへ閉じ込めてある**という
/// 設計は、消費者が戻る日のために残す。
pub fn scan_identity_hash(scan: &[ScanPath], show_hidden_system: bool) -> u64 {
    compute_config_hash(scan, show_hidden_system)
}

fn cache_bin_file_in(dir: &Path) -> BinFile {
    BinFile::new_in(dir, INDEX_MAGIC, INDEX_CACHE_VERSION, "index.bin")
}

/// `index.bin` が名乗る最終構築時刻（UNIX 秒）を読む。読めなければ `None`。
///
/// **索引本体を読まない。** 設定アプリがこれを呼ぶので、17 MiB の確保を持ち込まない。
///
/// **`built_at` が全版で先頭フィールドであることは、観測された性質であって契約ではない**
/// （v2〜v7 の 6 版で確認した）。新しい版を足すときも先頭へ置くこと——この依存は
/// `index_cache_on_disk_format_is_stable` の assertion 1 本が固定している。
pub fn index_built_at_in(dir: &Path) -> Option<u64> {
    // u64 の postcard varint は最大 10 バイト。
    cache_bin_file_in(dir).peek_first_field::<u64>(10)
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
    //
    // **`LegacyUpgrade::Skip` を渡す。** ここは開発者の実 `%APPDATA%\Snotra\index.bin` を
    // 読む corpus テストの入口であり（`search/tests/common.rs`）、`Write` にすると
    // テストを走らせるだけで実データを書き換える（`LegacyUpgrade` の doc）。
    Some(
        load_cache(hash, LegacyUpgrade::Skip)?
            .material
            .tree()
            .materialize(),
    )
}

/// 走査して正準の並びへ整列するまで（保存はしない）と、その 2 段の所要時間。
struct Scanned {
    entries: Vec<AppEntry>,
    scan_ms: u128,
    sort_ms: u128,
}

/// 全走査して [`sort_entries_canonical`] を通し、2 段を測って返す。
///
/// **保存する枝と保存しない枝（`Config::config_dir` が引けないとき）が同じここを通る。**
/// 書き起こすと計器が 2 部出荷になり、片方だけが段を足したときに `scan_ms` / `sort_ms` の
/// 意味が枝ごとにずれる——**どちらの枝を測ったのかは `LoadOrScanStats` の値からは
/// 区別できない**ので、ずれても数字はもっともらしいまま残る。
///
/// **`INDEX_WRITE_LOCK` は取らない。** 走査は共有資源に触れないが、保存する枝は
/// 「走査から保存までを 1 回のロック取得で覆う」ことに依存している（→
/// [`upgrade_legacy_cache_in`] だけがその例外である理由は `LoadCacheResult::upgrade_save_ms`
/// の doc）。ゆえにロックの範囲は呼び出し側が決める。
fn scan_and_sort_timed(scan: &[ScanPath], show_hidden_system: bool) -> Scanned {
    let scan_started = Instant::now();
    let mut entries = scan_all(scan, show_hidden_system);
    let scan_ms = scan_started.elapsed().as_millis();

    let sort_started = Instant::now();
    sort_entries_canonical(&mut entries);
    let sort_ms = sort_started.elapsed().as_millis();

    Scanned {
        entries,
        scan_ms,
        sort_ms,
    }
}

/// `load_or_scan_with_stats` と同じ手順を `dir` 注入で行う（統合テスト用）。
///
/// **製品の入口（`Config::config_dir()` を解決する側）でテストを書かないこと。**
/// 実 `%APPDATA%\Snotra` を読み書きし、テスト実行が実運用のデータを動かす（#1013 の Gotcha）。
///
/// **「`load_or_scan` と同じで、ただし〜」と書いてはならない**（かつてそう書いていた）。その関数は #984 で削除され、この doc だけが実在しない名前を基準に自分を説明する状態になっていた。
fn load_or_scan_with_stats_in(
    dir: &Path,
    scan: &[ScanPath],
    show_hidden_system: bool,
) -> LoadOrScanResult {
    let total_started = Instant::now();

    let hash_started = Instant::now();
    let current_hash = compute_config_hash(scan, show_hidden_system);
    let hash_ms = hash_started.elapsed().as_millis();

    let cache_load_started = Instant::now();
    // **キャッシュが読めたらそこで終わりである。** 走査は明示操作の契機でしか走らない
    // （`docs/adr/ADR-rescan-explicit-only.md`）。
    if let Some(result) = load_cache_in(dir, current_hash, LegacyUpgrade::Write) {
        let cache_load_ms = cache_load_started.elapsed().as_millis();
        let stats = LoadOrScanStats {
            cache_hit: true,
            hash_ms,
            cache_load_ms,
            cache_read_ms: result.read_ms,
            scan_ms: 0,
            sort_ms: 0,
            // 昇格が走らなかった枝は `None` ゆえ 0（`LoadCacheResult::upgrade_save_ms` の doc）。
            // **ミリ秒へ戻したこの値は計器であって、昇格の有無の判定には使えない**——0 は
            // 「昇格が走らなかった」と「走ったが 1 ms を切った」の両方を意味しうる。この値で
            // 配線を見る `load_or_scan_with_stats_reports_upgrade_save_ms_in_cache_save_ms` は、
            // 20,000 件の治具で仕事量を跨がせることでその曖昧さを外している（#1054 / #1063）。
            // `cache_load_ms` の内数——フェーズの和には足さない
            // （`LoadOrScanStats::cache_save_ms` の doc）。
            cache_save_ms: result.upgrade_save_ms.unwrap_or(0),
            total_ms: total_started.elapsed().as_millis(),
        };
        return LoadOrScanResult {
            material: result.material,
            cache_changed: false,
            stats,
        };
    }
    let cache_load_ms = cache_load_started.elapsed().as_millis();

    // 権威的書き手: scan + sort + save を書き込みロック保持下で行い、
    // 別ビルドとの index.bin 同時書き込みを防ぐ。
    // フェーズ計測はクロージャの戻り値として持ち出す。
    let (material, scan_ms, sort_ms, cache_save_ms) = with_index_write_lock(|| {
        let Scanned {
            entries,
            scan_ms,
            sort_ms,
        } = scan_and_sort_timed(scan, show_hidden_system);

        let cache_save_started = Instant::now();
        // **保存が返す木と派生データをそのまま使う。** 走査結果を保存のために建て直させると、
        // 同じ木を 2 回建てることになる（親解決は実測 23 ms）。派生データも同じ理屈で、
        // 保存側が計算して書いたものをここで受け取らないと、下流が全件を実体化してから
        // 建て直すことになる。
        let (tree, masks) = save_cache_sorted_in(dir, entries, current_hash, BuiltAt::Scanned);
        let material = IndexMaterial::derived(tree, masks);
        let cache_save_ms = cache_save_started.elapsed().as_millis();

        (material, scan_ms, sort_ms, cache_save_ms)
    });

    let stats = LoadOrScanStats {
        cache_hit: false,
        hash_ms,
        cache_load_ms,
        // cache-miss の枝は `index.bin` を読み切れていない（不在・stale・破損のいずれか）。
        cache_read_ms: 0,
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
    }
}

/// キャッシュを読む、無ければ全走査して保存する。返すのは [`LoadOrScanResult`]——索引の材料（[`IndexMaterial`]）とフェーズ計測である。
///
/// 保存先（`Config::config_dir()`）を解決してから [`load_or_scan_with_stats_in`] へ委譲する薄い
/// 包みである。解決できない環境では保存できないが索引は建てられる——`save_cache_sorted` の
/// `None` 枝と同じ方針で、走査はして `index.bin` へは書かない。
///
/// **cache-hit でも書きうる。** 常に `LegacyUpgrade::Write` で読むので、置かれているのが
/// 旧版なら**その場で現行版へ書き戻す**（`upgrade_legacy_cache_in`）。ゆえに `#[ignore]` の
/// 計測ハーネスがこれを呼ぶと、**1 回目の実行が開発者の実 `index.bin` を現行版へ変える**
/// ——旧版のロードを測りたければ**先に退避すること**。2 回目以降は昇格後の姿しか測れず、
/// しかも出力は成功時とまったく同じに見える。実データを**書き換えずに**読む口は
/// [`load_cached_entries`]（`LegacyUpgrade::Skip`）で、既定のスイートはそちらを通る。
pub fn load_or_scan_with_stats(scan: &[ScanPath], show_hidden_system: bool) -> LoadOrScanResult {
    match Config::config_dir() {
        Some(dir) => load_or_scan_with_stats_in(&dir, scan, show_hidden_system),
        None => {
            // **ここだけ `with_index_write_lock` を経由しない。** 保存先が無いので
            // `index.bin` を書かず、保護すべき共有資源（tmp→rename の書き込み対象）が
            // そもそも存在しない——「index.bin を書く経路は排他を経由する」契約
            // （→「index.bin 書き込みの排他」）が対象を持たないだけで、免除ではない。
            let total_started = Instant::now();

            let Scanned {
                entries,
                scan_ms,
                sort_ms,
            } = scan_and_sort_timed(scan, show_hidden_system);

            LoadOrScanResult {
                material: IndexMaterial::from_tree(IndexTree::build(entries)),
                cache_changed: true,
                stats: LoadOrScanStats {
                    cache_hit: false,
                    // **照合する相手が居ないので計算しない。** `config_hash` は `index.bin` へ
                    // 焼き込んで次の起動と突き合わせるための値であり、書かない枝では
                    // 消費者が居ない（かつては捨てる前提で計算し、この項目を埋めていた）。
                    hash_ms: 0,
                    cache_load_ms: 0,
                    cache_read_ms: 0,
                    scan_ms,
                    sort_ms,
                    cache_save_ms: 0,
                    total_ms: total_started.elapsed().as_millis(),
                },
            }
        }
    }
}

/// 正準の並び。**木の形をこの順序が決める**ので、木を建てる者は必ずここを通すこと
/// （写しを書くと、写した側だけが旧い並びで木を建てて緑を返す）。
///
/// **`save_cache_sorted(_in)` を呼ぶ側は直前にここを通す契約である。** 親の解決は整列済みを
/// 前提に `target_path` を二分探索するので（[`crate::index_tree`] の `resolve_one`）、崩すと
/// 親が解決されないまま木が平たくなり、接頭辞共有で削ったフルパスが `table` へ実体で戻る。
/// **症状は「`index.bin` が太る」だけで検索結果は正しいまま**ゆえ、挙動テストでは捕まらない。
pub(crate) fn sort_entries_canonical(entries: &mut [AppEntry]) {
    entries.sort_by(|a, b| {
        a.target_path
            .cmp(&b.target_path)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.is_folder.cmp(&b.is_folder))
    });
}

/// `index.bin` へ書く `built_at` の出どころ。
///
/// **「走査した時刻」を名乗る値なので、走査していない書き手が現在時刻を打ってはならない。**
/// 形式昇格（[`upgrade_legacy_cache_in`]）は旧版のバイト列を現行版へ詰め替えるだけで
/// 走査を伴わないため、ここで現在時刻を打つと**索引の中身は何日も前のまま、名乗る時刻だけが
/// 今**になる。表示（設定アプリの最終構築日時・[`index_built_at_in`]）はそれを唯一の手がかりに
/// 「再構築が要ることに気づく」ためにあるので、嘘をつく相手は**最も索引が古い層**——旧版のまま
/// 放置していたユーザー——に限られる。
///
/// **引数にしてあるのは、書き手に選ばせるためではなく選ばざるを得なくするためである。**
/// 既定を「現在時刻」にすると、走査しない書き手を足した日に何も書かなくても通ってしまう。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuiltAt {
    /// いま走査した結果を書く。
    Scanned,
    /// 走査していない（形式昇格）。読めた値をそのまま持ち越す。
    Carried(u64),
}

impl BuiltAt {
    fn resolve(self) -> u64 {
        match self {
            BuiltAt::Scanned => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            BuiltAt::Carried(secs) => secs,
        }
    }
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
    let (tree, masks) = save_cache_sorted_in(&dir, entries, config_hash, BuiltAt::Scanned);
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
    pub(crate) lower_names: LowerNameColumn,
    pub(crate) lower_file_names: LowerFileColumn,
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
    //
    // **派生文字列 2 本はアリーナ（`crate::str_arena`）へ直に積む。** `derive_entry_collapsed`
    // が返す `String` はここで写して落ちるので、記録側に per-entry の spine は残らない
    // （`String` そのものの一時確保は残る——それを消すのは別反復である）。
    let len = entries.len();
    let mut char_masks = Vec::with_capacity(len);
    let mut file_name_char_masks = Vec::with_capacity(len);
    let mut collapsed_lower_names = LowerNameColumn::with_capacity(len);
    let mut collapsed_lower_file_names = LowerFileColumn::with_capacity(len);
    for entry in &entries {
        let (char_mask, file_mask, lower_name, lower_file) = derive_entry_collapsed(entry);
        char_masks.push(char_mask);
        file_name_char_masks.push(file_mask);
        collapsed_lower_names.push(lower_name.as_deref());
        collapsed_lower_file_names.push(lower_file.as_slot());
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
    built_at: BuiltAt,
) -> (IndexTree, CachedMasks) {
    let bf = cache_bin_file_in(dir);
    let derived = derive_columns(entries);

    // Cow::Borrowed で木の列と派生 Vec の全件 clone を避ける。
    // 出力バイト列は Owned 版と同一（golden テストで保証）。
    let derived_cols = derived.tree.columns();
    let cache = IndexCache {
        built_at: built_at.resolve(),
        names: Cow::Borrowed(derived_cols.names),
        is_folder: Cow::Borrowed(derived_cols.is_folder),
        parent: Cow::Borrowed(derived_cols.parent),
        aux: Cow::Borrowed(derived_cols.aux),
        table: Cow::Borrowed(derived_cols.table),
        sorted_by_path: derived_cols.sorted_by_path,
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
    // 別の書き手との index.bin 同時書き込みを防ぐ。
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
    /// **通る構築子は `upgrade`（[`LegacyUpgrade`]）で分かれる。** `Write` はどの版でも
    /// `upgrade_legacy_cache_in` 経由で [`IndexMaterial::derived`]（自分で導出した組・
    /// 長さを検証しない）を通る。`Skip` はディスクの生データをそのまま使うため、
    /// マスクを持つ版（v3〜v7）は [`IndexMaterial::from_untrusted`]（列長を検証する）、
    /// マスクを持たない v2 だけ `from_tree` を通る。
    material: IndexMaterial,
    /// `index.bin` をバイト列として読み終えるまでの時間（`LoadOrScanStats::cache_read_ms` へ運ぶ）。
    read_ms: u128,
    /// 旧版昇格（`upgrade_legacy_cache_in`）が走った場合の save 所要時間
    /// （`LoadOrScanStats::cache_save_ms` へ運ぶ）。昇格が走らなかった枝（現行版 v7・
    /// `LegacyUpgrade::Skip`）では `None`。
    ///
    /// **`Some` を作れるのは [`upgrade_legacy_cache_in`] の内側だけである。** ゆえに
    /// variant が「昇格 save を通ったか」そのものであり、**時間の値は判定に使わない**
    /// ——`Some(0)` は「通ったが 1 ms を切った」を表す正当な値である。壁時計のミリ秒を
    /// 「通った」の代理に使っていた頃は、1 件の治具で区間が時計の量子化に載り、
    /// 検知器が確率的に落ちた（#1054 / #1063 実測）。判定を variant へ移したので、
    /// 代理は残っていない。
    ///
    /// **`INDEX_WRITE_LOCK` の取得待ちを含む。** 昇格は読み終えてからロックを取りに行くので、
    /// 計測の始点がロックの外にある——**cache-miss 枝の `cache_save_ms` とは非対称で**、
    /// あちらは scan ごとロックの内側なので待ちが save の数に乗らない。**今のところ待ちは
    /// 立たない**: 製品の呼び出し元は `main` の起動段の 1 つだけで、もう一方の書き手
    /// （索引ビルドのスレッド）は `AppHandle` を要求するためその時点でまだ存在しない。
    /// 待ちが立ちうる書き手を足す日には、この値が「save が遅い」と読める形で嘘をつく。
    upgrade_save_ms: Option<u128>,
    /// 実際に読めた形式のバージョン。**現行版とは限らない**——フォールバック経路で読めた
    /// ときは旧版であり、`Write` のときは旧版枝（`upgrade_legacy_cache_in`）がその場で
    /// 現行版へ書き戻す（[`LegacyUpgrade`] の doc）。
    ///
    /// **読み手はテストだけである**（`allow(dead_code)` はそのためである）。かつては背景
    /// 再スキャンの昇格判定の入力だったが、昇格がロードの旧版枝へ移り、判定は枝そのものに
    /// なった。**残すのは、フォールバックの鎖のどの枝で読めたのかを外から見る手段がこれ
    /// しか無いからである**——材料だけを見ても v5 の枝で読めたのか v6 の枝で読めたのかは
    /// 区別できず、鎖の枝選択の退行は静かに通る。
    #[allow(dead_code)]
    version: u32,
}

/// 旧版を読んだとき、その場で現行版へ書き戻すか。
///
/// **`Skip` が要るのは、製品の起動経路以外にも `load_cache_in` を通る入口があるからである。**
/// `load_cached_entries` は corpus テストの入口（`search/tests/common.rs`）であり、
/// 開発者の実 `%APPDATA%\Snotra\index.bin` を読む。ここで書き戻すと、テストを走らせる
/// だけで実データを書き換える（#1013 と同型）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LegacyUpgrade {
    Write,
    Skip,
}

fn load_cache(config_hash: u64, upgrade: LegacyUpgrade) -> Option<LoadCacheResult> {
    let dir = Config::config_dir()?;
    load_cache_in(&dir, config_hash, upgrade)
}

/// 旧版を読んだ枝の共通処理: **走査結果を既に手に持っているので、その場で現行版へ書き戻す。**
///
/// **返す材料は書き戻しの副産物である**（`save_cache_sorted_in` が返す木とマスクをそのまま
/// 使う）。旧版が持っていたマスクは捨てて derive し直すが、これは**昇格する起動でだけ払う
/// 一回性の代価**であり、以後は現行版の枝に入る。
///
/// **保存に失敗してもロードは成功させる。** 昇格は最適化であって、失敗が索引の可用性を
/// 落としてはならない——落とすと、書けない環境（読み取り専用・ディスク満杯）で
/// 旧版ユーザーだけが索引を失う。
///
/// **`IndexMaterial::derived` を使う（`from_untrusted` ではない）。** `save_cache_sorted_in` →
/// `derive_columns` がこの場で導出した組であり、ディスクから読んだ検証すべき組ではない
/// （`IndexMaterial` の型 doc）。列長は `derive_columns` が 1 周で 4 本を埋める構成上の
/// 保証であり、`Option` で失敗を返す理由が無い。
///
/// **`built_at` は旧版が名乗っていた値を持ち越す**（[`BuiltAt::Carried`]）。ここは走査しない
/// 書き手なので、現在時刻を打つと「最後に走査した時刻」の意味が壊れる（理由の正本は
/// [`BuiltAt`] の doc）。検知器は `upgrade_carries_the_built_at_it_read`。
fn upgrade_legacy_cache_in(
    dir: &Path,
    mut entries: Vec<AppEntry>,
    config_hash: u64,
    read_ms: u128,
    version: u32,
    built_at: u64,
) -> LoadCacheResult {
    // **他の全呼び出し元と同じく、保存の直前に整列する。** `save_cache_sorted_in` は
    // 名前どおり整列済みの入力を前提とする——ここだけ怠ると、旧版ファイルが現在の
    // canon と違う順序で書かれていた場合に親の解決が当てにならなくなり（規約と帰結は
    // `sort_entries_canonical` の doc）、平たい木と `sorted_by_path=false` が現行版へ
    // そのまま焼き込まれる。
    sort_entries_canonical(&mut entries);
    // **`index.bin` を書く経路はすべて書き込みロックを経由する契約である。**
    //
    // **ここで測る save_ms はロック取得の待ちを含む**（`Instant::now()` が
    // `with_index_write_lock` より前にある）。**cache-miss 枝の `cache_save_ms` とは
    // 非対称である**——あちらは scan + sort + save をまとめて包むロックの内側で計測を
    // 始めるので、待ちは `scan_ms` より前に落ちて save の数には乗らない。直前の
    // `sort_entries_canonical` を含めない点だけが両者で揃っている。
    //
    // この区間全体は呼び出し元の `cache_load_started` の計測区間の内側で起きるため、
    // `LoadOrScanStats::cache_save_ms` へ運んだ値は `cache_load_ms` の**内数**になる
    // （フェーズの和には足さない——doc を参照）。
    let save_started = Instant::now();
    let (tree, masks) = with_index_write_lock(|| {
        save_cache_sorted_in(dir, entries, config_hash, BuiltAt::Carried(built_at))
    });
    // **`Some` を作るのはこの 1 行だけである**（`LoadCacheResult::upgrade_save_ms` の doc）。
    // 昇格 save を通ったことは、時間の値ではなくこの variant が表す。
    let upgrade_save_ms = Some(save_started.elapsed().as_millis());
    LoadCacheResult {
        material: IndexMaterial::derived(tree, masks),
        read_ms,
        upgrade_save_ms,
        // **`version` は「読めた」版のままにする。** 呼び出し側はこれで「旧版だった」を
        // 知る。書き戻した後の版を入れると、その事実が消える。
        version,
    }
}

/// 旧版枝が読み終えた生の材料。**判定（`LegacyUpgrade`）はまだ載っていない。**
///
/// `masks` は v2 だけ `None`（マスクを持たない版）。それ以外の旧版は必ず `Some`。
struct LegacyRead {
    entries: Vec<AppEntry>,
    masks: Option<CachedMasks>,
    version: u32,
    /// 旧版が名乗っていた最終構築時刻。**昇格はこれを持ち越す**（理由は [`BuiltAt`] の doc）。
    built_at: u64,
}

/// 旧版枝の出口を一本化する。**`match upgrade` を書くのはここ 1 か所だけである。**
///
/// 旧版枝を 1 本足すときは、その版のバイト列を [`LegacyRead`] へ組み立ててここへ渡すだけでよい
/// ——`Write`/`Skip` の 2 腕を枝ごとに書き起こす義務は生まれない（判定の写しを作らない）。
fn finish_legacy_read(
    dir: &Path,
    config_hash: u64,
    read_ms: u128,
    upgrade: LegacyUpgrade,
    read: LegacyRead,
) -> Option<LoadCacheResult> {
    match upgrade {
        LegacyUpgrade::Write => Some(upgrade_legacy_cache_in(
            dir,
            read.entries,
            config_hash,
            read_ms,
            read.version,
            read.built_at,
        )),
        LegacyUpgrade::Skip => {
            let tree = IndexTree::build(read.entries);
            let material = match read.masks {
                Some(masks) => IndexMaterial::from_untrusted(tree, masks)?,
                // マスクを持たない版（v2）ゆえ検証する列が無い（木の整合は `IndexTree::build` が持つ）。
                None => IndexMaterial::from_tree(tree),
            };
            Some(LoadCacheResult {
                material,
                read_ms,
                // **`Skip` は書き戻さないので save は起きない。**
                upgrade_save_ms: None,
                version: read.version,
            })
        }
    }
}

/// `load_cache` と同じ読み込みを `dir` 注入で行う（統合テスト用、issue #429）。
///
/// **`INDEX_WRITE_LOCK` を保持したまま呼んではならない。** `LegacyUpgrade::Write` で旧版を
/// 読むと、この関数は [`upgrade_legacy_cache_in`] 経由で同じロックを取りに行く——
/// `std::sync::Mutex` は再入できないので、その場で自己デッドロックする（**索引ロードが
/// 永久に返らない**形なので、テストが落ちるのではなくハングする）。
///
/// **ロードは「読むだけ」に見えるが、旧版枝は書き手である**——`load_cache_in` を「読み取り
/// なのでロックの内側でも安全」と読んだ瞬間にこの罠へ落ちる。実際の呼び出し順は
/// [`load_or_scan_with_stats_in`] が正本で、そこはロックの**外**でこれを呼び、cache-miss と
/// 判ってから初めてロックを取る。
fn load_cache_in(dir: &Path, config_hash: u64, upgrade: LegacyUpgrade) -> Option<LoadCacheResult> {
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
            // **現行版は昇格しないので save は起きない。**
            upgrade_save_ms: None,
            version: INDEX_CACHE_VERSION,
        });
    }

    // v6 フォールバック: `target_path` を全件そのまま持つ形式。**読めるが確保が 312,691 回
    // 余分にかかる**——フルパスの `String` を作り、木へ組み替えた段で即座に捨てる。
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
        return finish_legacy_read(
            dir,
            config_hash,
            read_ms,
            upgrade,
            LegacyRead {
                entries: cache.entries,
                masks: Some(masks),
                version: 6,
                built_at: cache.built_at,
            },
        );
    }

    // v5 フォールバック: 派生文字列を全件そのまま持つ形式。**読めるが確保が倍以上かかる**
    // ——625,380 個の `String` を作り、うち約 527,000 個は `assemble` が測って即座に捨てる。
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
        return finish_legacy_read(
            dir,
            config_hash,
            read_ms,
            upgrade,
            LegacyRead {
                entries: cache.entries,
                masks: Some(masks),
                version: 5,
                built_at: cache.built_at,
            },
        );
    }

    // v4 フォールバック: 末尾の normalized_keys を**読んで捨てる**。`Skip`（corpus テストの
    // 入口のみが通る）のときは v5 と同じ 4 本を復元し、どれも v4 に揃っているため
    // **Wave 1 はスキップされたまま**（v4 ユーザーの初回起動は遅くならない）。
    //
    // **`Write` のときは `finish_legacy_read` がその場で現行版へ書き戻す。** かつては
    // 「次回の save」に賭けていたが、当時 save が来る契機は索引の中身が変わったときだけで
    // あり、変わらない限りその「次回」は来なかった——実運用点で v4 が残り続け、毎起動
    // 35.98 MiB を読んで捨てていた（2026-08-07 実測）。
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
        return finish_legacy_read(
            dir,
            config_hash,
            read_ms,
            upgrade,
            LegacyRead {
                entries: cache.entries,
                masks: Some(masks),
                version: 4,
                built_at: cache.built_at,
            },
        );
    }

    // v3 フォールバック: ビットマスクのみ。`Skip` のときは lower names を持たないため
    // Wave 1 が実行される。**`Write` のときは `finish_legacy_read` 経由で derive し直すため、
    // v3 ユーザーも Wave 1 がスキップされるようになる**（昇格する起動 1 回ぶんの代価は
    // `upgrade_legacy_cache_in` の doc を参照）。
    if let Ok(cache) = try_deserialize_with_header::<IndexCacheV3>(&bytes, INDEX_MAGIC, 3) {
        if cache.config_hash != config_hash {
            return None;
        }
        let masks = CachedMasks {
            char_masks: cache.char_masks,
            file_name_char_masks: cache.file_name_char_masks,
            lower: None,
        };
        return finish_legacy_read(
            dir,
            config_hash,
            read_ms,
            upgrade,
            LegacyRead {
                entries: cache.entries,
                masks: Some(masks),
                version: 3,
                built_at: cache.built_at,
            },
        );
    }

    // v2 フォールバック (マスクなし)。`masks: None` を渡すと `finish_legacy_read` の `Skip` 腕が
    // `from_tree` へ落とす（検証する列が無いため）。
    if let Ok(cache) = try_deserialize_with_header::<IndexCacheV2>(&bytes, INDEX_MAGIC, 2) {
        if cache.config_hash != config_hash {
            return None;
        }
        return finish_legacy_read(
            dir,
            config_hash,
            read_ms,
            upgrade,
            LegacyRead {
                entries: cache.entries,
                masks: None,
                version: 2,
                built_at: cache.built_at,
            },
        );
    }

    None
}

/// `index.bin` の scan + save 区間を直列化する書き込みロック。
/// 書き手（`rebuild_and_save` / cache-miss save / ロードの旧版枝の昇格）が共有する。
/// `save_cache_sorted` 自体はロックを取らない（呼び出し側が保持する契約）。
static INDEX_WRITE_LOCK: Mutex<()> = Mutex::new(());

/// 書き込みロックをブロッキングで取得し、クロージャを実行して結果を返す。
fn with_index_write_lock<R>(f: impl FnOnce() -> R) -> R {
    // Mutex<()> は保持する状態を持たないため、poison しても into_inner で回復して継続する。
    // （`.unwrap()` だと一度の panic 以降、全 index 書き込みが永久に panic する）
    let _guard = INDEX_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    f()
}

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
pub(crate) fn extend_cached_masks(masks: &mut CachedMasks, new_entries: &[AppEntry]) {
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
                lower_names.push(name.as_deref());
                lower_file_names.push(file.as_slot());
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
    /// この計器はファイルを直接読むだけで、旧版を現行版へ書き戻さない（それをするのは
    /// `load_cache_in` の旧版枝である・[`LegacyUpgrade`] の doc）。ゆえに製品がまだ一度も
    /// ロードしていない `index.bin` はここでは旧版のまま現れる。かつては書き戻しの契機が
    /// 索引の中身の変化に括り付いており、**旧版が何日でも残った**——2026-08-07 に実測した
    /// （v5 導入後の実 `index.bin` が v4 のままで、`normalized_keys` を毎起動で読んで捨てて
    /// いた）。**この値を読まずに内訳を解釈しないこと。**
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
        /// **アリーナだが、線上のバイト列は `seq of str` のままである**（[`NameArena`]）。
        /// ゆえに `serialized_len` の勘定も帰属の割り方も v7 の頃と変わらない。
        names: &'a NameArena,
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
                    bytes: (0..names.len())
                        .map(|i| postcard_str_len(names.get(i)))
                        .sum(),
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
    /// v6 以降: 潰し済み。**列の型で受ける**（線上表現は `Vec<Option<String>>` /
    /// `Vec<LowerFileName>` のままだが、手に持っている物体は列である）。
    Collapsed {
        names: &'a LowerNameColumn,
        files: &'a LowerFileColumn,
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
                items: names.count_present(),
            },
            CacheByteRow {
                label: "lower_file_names（3 状態）",
                bytes: serialized_len(&files)?,
                items: files.count_text(),
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

    // ---- root_roles tests ----

    /// 述語のテスト用に最小の `ScanPath` を作る。拡張子と `include_folders` は
    /// **判定に関与しない**（設計書 §2.2 の過剰近似）。
    fn root(path: &str) -> ScanPath {
        ScanPath {
            path: path.to_string(),
            extensions: vec![".exe".to_string()],
            include_folders: false,
        }
    }

    /// **積むのは「後続の根と重なる」側である。** 重複が起きるのは先に入ったエントリが
    /// 後の走査で再び現れるときだけなので、向きはこの 1 通りしかない。
    #[test]
    fn root_roles_records_on_the_earlier_root_and_checks_on_the_later() {
        let roles = root_roles(&[root("C:\\X"), root("C:\\X\\sub")]);
        assert_eq!((roles[0].check, roles[0].record), (false, true));
        assert_eq!((roles[1].check, roles[1].record), (true, false));
    }

    /// **順序が逆でも役割が入れ替わるだけで、重複排除は成立する。**
    #[test]
    fn root_roles_follow_the_order_not_the_depth() {
        let roles = root_roles(&[root("C:\\X\\sub"), root("C:\\X")]);
        assert_eq!((roles[0].check, roles[0].record), (false, true));
        assert_eq!((roles[1].check, roles[1].record), (true, false));
    }

    /// 実運用点の形（最大の根が最後に来る）。**ここで `C:\` が「照合のみ」になることが
    /// この設計の全部である**——積まないので 30 万件ぶんの `String` 確保が消える。
    #[test]
    fn root_roles_over_the_real_shape_leave_the_largest_root_inert() {
        let roles = root_roles(&[
            root("C:\\ProgramData\\Microsoft\\Windows\\Start Menu\\Programs"),
            root("C:\\Users\\User\\Desktop"),
            root("C:\\"),
        ]);
        assert_eq!((roles[0].check, roles[0].record), (false, true));
        assert_eq!((roles[1].check, roles[1].record), (false, true));
        assert_eq!(
            (roles[2].check, roles[2].record),
            (true, false),
            "最大の根が積む側に回ると削減が消える"
        );
    }

    #[test]
    fn root_roles_are_all_inert_when_nothing_overlaps() {
        let roles = root_roles(&[root("C:\\A"), root("D:\\B")]);
        assert!(roles.iter().all(|r| !r.check && !r.record));
    }

    #[test]
    fn root_roles_treat_exact_duplicates_as_overlap() {
        let roles = root_roles(&[root("C:\\Tools"), root("c:/tools/")]);
        assert_eq!((roles[0].check, roles[0].record), (false, true));
        assert_eq!((roles[1].check, roles[1].record), (true, false));
    }

    /// **境界の 2 枝を 1 本にまとめると、ここが落ちる**（`c:\tools` は `c:\toolsextra` の
    /// 接頭辞だが、次の 1 バイトが `\` ではないので入れ子ではない）。
    #[test]
    fn root_roles_ignore_siblings_sharing_a_prefix() {
        let roles = root_roles(&[root("C:\\Tools"), root("C:\\ToolsExtra")]);
        assert!(roles.iter().all(|r| !r.check && !r.record));
    }

    #[test]
    fn root_roles_empty_for_no_paths() {
        assert!(root_roles(&[]).is_empty());
    }

    /// **入れ子の根では重複排除が要る。** `dedup_scan_paths` は完全一致マージのみゆえ、
    /// `X` と `X\sub` は両方とも残る（設計書 §1）。
    #[test]
    fn scan_all_dedups_when_roots_are_nested() {
        let dir = temp_dir("nested_roots");
        let sub = dir.join("sub");
        fs::create_dir_all(&sub).expect("create sub dir");
        fs::write(sub.join("tool.exe"), b"x").expect("write fixture");

        let scan = vec![
            ScanPath {
                path: dir.to_string_lossy().into_owned(),
                extensions: vec![".exe".to_string()],
                include_folders: false,
            },
            ScanPath {
                path: sub.to_string_lossy().into_owned(),
                extensions: vec![".exe".to_string()],
                include_folders: false,
            },
        ];
        let entries = scan_all(&scan, true);

        assert_eq!(
            entries.len(),
            1,
            "入れ子の根で同じファイルが二度入っている（重複排除が効いていない）"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// **子の根が先に来る順序でも重複が出ない。** 役割が入れ替わるだけで成立することを、
    /// 述語の単体テストではなく走査の結果で固定する。
    #[test]
    fn scan_all_dedups_when_the_child_root_comes_first() {
        let dir = temp_dir("nested_roots_child_first");
        let sub = dir.join("sub");
        fs::create_dir_all(&sub).expect("create sub dir");
        fs::write(sub.join("tool.exe"), b"x").expect("write fixture");

        let scan = vec![
            ScanPath {
                path: sub.to_string_lossy().into_owned(),
                extensions: vec![".exe".to_string()],
                include_folders: false,
            },
            ScanPath {
                path: dir.to_string_lossy().into_owned(),
                extensions: vec![".exe".to_string()],
                include_folders: false,
            },
        ];
        let entries = scan_all(&scan, true);

        assert_eq!(
            entries.len(),
            1,
            "子の根が先に来る順序で同じファイルが二度入っている"
        );

        let _ = fs::remove_dir_all(&dir);
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
        let mut dedup = Dedup {
            set: Some(std::collections::HashSet::new()),
            buf: String::new(),
            role: RootRole {
                check: false,
                record: true,
            },
        };
        let exts = build_extension_list(&["exe".to_string(), "bat".to_string()]);
        scan_directory_with_extensions(&dir, &exts, false, true, &mut entries, &mut dedup);

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
        let mut dedup = Dedup {
            set: Some(std::collections::HashSet::new()),
            buf: String::new(),
            role: RootRole {
                check: false,
                record: true,
            },
        };
        let exts = build_extension_list(&["exe".to_string()]);
        scan_directory_with_extensions(&dir, &exts, true, true, &mut entries, &mut dedup);

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
        let mut dedup = Dedup {
            set: Some(std::collections::HashSet::new()),
            buf: String::new(),
            role: RootRole {
                check: false,
                record: true,
            },
        };
        let exts = build_extension_list(&["exe".to_string()]);
        scan_directory_with_extensions(&dir, &exts, false, true, &mut entries, &mut dedup);

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
        let mut dedup = Dedup {
            set: Some(std::collections::HashSet::new()),
            buf: String::new(),
            role: RootRole {
                check: false,
                record: true,
            },
        };
        let exts = build_extension_list(&["exe".to_string()]);
        scan_directory_with_extensions(&dir, &exts, false, true, &mut entries, &mut dedup);

        let tools: Vec<&AppEntry> = entries.iter().filter(|e| e.name == "tool").collect();
        assert_eq!(tools.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_extensions_case_insensitive() {
        let dir = temp_dir("ext_case");
        fs::write(dir.join("app.EXE"), "").unwrap();

        let mut entries = Vec::new();
        let mut dedup = Dedup {
            set: Some(std::collections::HashSet::new()),
            buf: String::new(),
            role: RootRole {
                check: false,
                record: true,
            },
        };
        let exts = build_extension_list(&["exe".to_string()]);
        scan_directory_with_extensions(&dir, &exts, false, true, &mut entries, &mut dedup);

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
        let tree_cols = tree.columns();
        let cache = IndexCache {
            built_at: 1700000000,
            names: Cow::Owned(tree_cols.names.clone()),
            is_folder: Cow::Owned(tree_cols.is_folder.to_vec()),
            parent: Cow::Owned(tree_cols.parent.to_vec()),
            aux: Cow::Owned(tree_cols.aux.to_vec()),
            table: Cow::Owned(tree_cols.table.to_vec()),
            sorted_by_path: tree_cols.sorted_by_path,
            config_hash: 12345,
            char_masks: Cow::Owned(vec![0xAB, 0xCD]),
            file_name_char_masks: Cow::Owned(vec![0x12, 0x34]),
            // v6: `None` = name と同一。ここでは 2 件目がそれに当たる形にしてある。
            lower_names: Cow::Owned([Some("firefox"), None].into_iter().collect()),
            lower_file_names: Cow::Owned(
                [
                    LowerFileSlot::Text("firefox.lnk"),
                    LowerFileSlot::SameAsLowerName,
                ]
                .into_iter()
                .collect(),
            ),
        };

        let bytes =
            try_serialize_with_header(INDEX_MAGIC, INDEX_CACHE_VERSION, &cache).expect("serialize");
        let restored: IndexCache<'static> =
            try_deserialize_with_header(&bytes, INDEX_MAGIC, INDEX_CACHE_VERSION)
                .expect("deserialize");

        assert_eq!(restored.built_at, 1700000000);
        assert_eq!(restored.names.len(), 2);
        assert_eq!(restored.names.get(0), "Firefox");
        assert!(!restored.is_folder[0]);
        assert_eq!(restored.names.get(1), "Projects");
        assert!(restored.is_folder[1]);
        assert_eq!(restored.config_hash, 12345);
        // Cow フィールドは into_owned() で Vec に戻して比較（deserialize は Owned ゆえ move）。
        assert_eq!(restored.char_masks.into_owned(), vec![0xABu64, 0xCD]);
        assert_eq!(
            restored.file_name_char_masks.into_owned(),
            vec![0x12u64, 0x34]
        );
        assert_eq!(
            restored.lower_names.iter().collect::<Vec<_>>(),
            vec![Some("firefox"), None]
        );
        assert_eq!(
            restored.lower_file_names.iter().collect::<Vec<_>>(),
            vec![
                LowerFileSlot::Text("firefox.lnk"),
                LowerFileSlot::SameAsLowerName,
            ]
        );
    }

    /// `golden_fixture` の戻り値（entries / 2 本のマスク / 潰し済みの派生文字列 2 本）。
    type GoldenFixture = (
        Vec<AppEntry>,
        Vec<u64>,
        Vec<u64>,
        LowerNameColumn,
        LowerFileColumn,
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
            [Some("projects"), None, Some("firefox"), None]
                .into_iter()
                .collect(),
            [
                LowerFileSlot::Absent,
                LowerFileSlot::Text("app.exe"),
                LowerFileSlot::Text("firefox.lnk"),
                LowerFileSlot::SameAsLowerName,
            ]
            .into_iter()
            .collect(),
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
        let tree_cols = tree.columns();
        let cache = IndexCache {
            built_at: 1_700_000_000,
            names: Cow::Borrowed(tree_cols.names),
            is_folder: Cow::Borrowed(tree_cols.is_folder),
            parent: Cow::Borrowed(tree_cols.parent),
            aux: Cow::Borrowed(tree_cols.aux),
            table: Cow::Borrowed(tree_cols.table),
            sorted_by_path: tree_cols.sorted_by_path,
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

        // **`index_built_at_in` はヘッダー直後の最初のフィールドが `built_at` である
        // ことに依存している。** フィールドを並べ替えると golden も落ちるが、落ちた側が
        // 「並べ替えた」だけを報せて依存の所在を報せない。ここで名指ししておく。
        assert_eq!(
            crate::binfmt::peek_first_field_from_bytes::<u64>(&bytes, INDEX_MAGIC),
            Some(1_700_000_000),
            "ヘッダー直後の最初のフィールドが built_at でなくなった。index_built_at_in が\
             黙って別の値を返すようになる（表示だけが壊れ、テストは他が全部通る）"
        );

        let restored: IndexCache<'static> =
            try_deserialize_with_header(GOLDEN_V7, INDEX_MAGIC, INDEX_CACHE_VERSION)
                .expect("凍結 v7 バイトがロードできること");
        assert!(matches!(restored.names, Cow::Owned(_)));
        assert_eq!(restored.names.len(), 4);
        assert_eq!(restored.names.get(0), "Projects");
        assert!(restored.is_folder[0]);
        assert_eq!(restored.names.get(1), "app");
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

        let result =
            load_cache_in(&dir, 12345, LegacyUpgrade::Skip).expect("v6 の index.bin が読めること");
        assert_eq!(
            result.version, 6,
            "「読めた版」を運ぶ（`Write` のときは昇格の判断材料にもなる。`LegacyUpgrade` の doc）"
        );
        let (tree, masks) = result.material.into_parts();
        assert_eq!(tree.len(), 3);
        assert_eq!(tree.name_at(0), "Firefox");

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
                    lower_names.iter().collect::<Vec<_>>(),
                    vec![Some("firefox"), Some("projects"), None]
                );
                assert_eq!(
                    lower_file_names.iter().collect::<Vec<_>>(),
                    vec![
                        LowerFileSlot::Text("firefox.lnk"),
                        LowerFileSlot::Absent,
                        LowerFileSlot::SameAsLowerName,
                    ]
                );
            }
            other => panic!("v6 は Collapsed で返らなければならない（実際: {other:?}）"),
        }

        // config_hash が違えば stale 扱いで None（他の版の枝と同じ規律）。
        assert!(load_cache_in(&dir, 12346, LegacyUpgrade::Skip).is_none());

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

        let result =
            load_cache_in(&dir, 12345, LegacyUpgrade::Skip).expect("v5 の index.bin が読めること");
        assert_eq!(
            result.version, 5,
            "「読めた版」を運ぶ（`Write` のときは昇格の判断材料にもなる。`LegacyUpgrade` の doc）"
        );
        let (tree, masks) = result.material.into_parts();
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.name_at(0), "Firefox");

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
        assert!(load_cache_in(&dir, 12346, LegacyUpgrade::Skip).is_none());

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

        let result =
            load_cache_in(&dir, 12345, LegacyUpgrade::Skip).expect("v4 の index.bin が読めること");
        let (tree, masks) = result.material.into_parts();
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.name_at(0), "Firefox");
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
        assert!(load_cache_in(&dir, 12346, LegacyUpgrade::Skip).is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    /// v4 形式の `index.bin` を `dir` へ書く（テスト専用の治具）。
    ///
    /// **凍結バイト列（`GOLDEN_V4`）では代用できない。** あちらのエントリは固定であり、
    /// `config_hash` も治具の走査対象と噛み合わない——「その `dir` を走査した結果が
    /// v4 で載っている」状況を作るには、エントリそのものを v4 で書く必要がある。
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

        let (_, returned) =
            save_cache_sorted_in(&dir, entries.clone(), config_hash, BuiltAt::Scanned);

        let result = load_cache_in(&dir, config_hash, LegacyUpgrade::Skip)
            .expect("load cache written to dir");
        let (tree, masks) = result.material.into_parts();
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.name_at(0), "Firefox");
        assert_eq!(tree.name_at(1), "Projects");
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
                    lower_names.iter().collect::<Vec<_>>(),
                    vec![Some("firefox"), Some("projects")]
                );
                assert_eq!(
                    lower_file_names.iter().collect::<Vec<_>>(),
                    vec![
                        LowerFileSlot::Text("firefox.lnk"),
                        // "C:\\Projects" の file name 成分は "Projects" → "projects" で
                        // `lower_name` と一致する。
                        LowerFileSlot::SameAsLowerName,
                    ]
                );
            }
            other => panic!("v6 は Collapsed で返らなければならない（実際: {other:?}）"),
        }

        // config_hash が異なると stale 扱いで None
        assert!(load_cache_in(&dir, config_hash.wrapping_add(1), LegacyUpgrade::Skip).is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    /// **キャッシュヒットの起動は走査しない。** #1001 の受け入れの本体である。
    ///
    /// 「走査していない」を、時間ではなく**結果**で測る——キャッシュを保存した後で
    /// 走査対象へファイルを 1 つ足し、それが返る材料に**現れない**ことを見る。
    /// 走査が 1 回でも走れば現れるので、時計や環境に依存せず決定論的である。
    ///
    /// **残る死角**: この検知器が守るのは「走査という副作用が起きないこと」ではなく
    /// 「cache-hit の材料がキャッシュ由来であること」である。cache-hit 枝へ
    /// `let _ = scan_all(...)` のように結果を捨てる走査を足す退行は、材料が変わらない
    /// ので**この検知器では捕まらない**（変異確認で実測。詳細はコミットの報告に残す）。
    #[test]
    fn a_cache_hit_startup_does_not_scan() {
        let dir = temp_dir("cache_hit_no_scan");
        let scan_root = temp_dir("cache_hit_no_scan_root");
        std::fs::write(scan_root.join("first.txt"), b"x").expect("write");

        let scan = vec![ScanPath {
            path: scan_root.display().to_string(),
            extensions: vec![".txt".into()],
            include_folders: false,
        }];

        // 1 回目: cache-miss → 走査して保存する。
        let first = load_or_scan_with_stats_in(&dir, &scan, false);
        assert!(!first.stats.cache_hit, "1 回目は cache-miss であること");
        assert_eq!(first.material.tree().len(), 1);

        // キャッシュを書いた後で対象を増やす。
        std::fs::write(scan_root.join("second.txt"), b"y").expect("write");

        // 2 回目: cache-hit → 走査しないので、増えたファイルは見えない。
        let second = load_or_scan_with_stats_in(&dir, &scan, false);
        assert!(second.stats.cache_hit, "2 回目は cache-hit であること");
        assert_eq!(
            second.stats.scan_ms, 0,
            "cache-hit で走査時間が立ってはならない"
        );
        assert_eq!(
            second.material.tree().len(),
            1,
            "cache-hit の起動が走査している（増えたファイルが見えてしまった）"
        );

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&scan_root);
    }

    /// **旧版を読んだ起動が、その場で現行版へ書き戻す。** 移す前はここが背景再スキャンの
    /// 責務だった（#1001 で再スキャンごと撤去した）。書き戻さないと、索引の中身が
    /// 変わらないユーザーの `index.bin` は旧版のまま何日でも残り、新形式の削減を
    /// 永久に受け取らない（2026-08-07 実測。症状は「遅い」だけで検索結果は正しいまま）。
    #[test]
    fn load_cache_upgrades_a_legacy_format_in_place() {
        // `Write` は `upgrade_legacy_cache_in` 経由で `INDEX_WRITE_LOCK` を取る
        // （`upgrade_legacy_cache_in` の doc）。`INDEX_WRITE_LOCK` に触れるテストは
        // このガードで直列化する契約（`INDEX_LOCK_TEST_GUARD` の doc）。
        let _guard = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("load_upgrade");
        let entries = vec![
            AppEntry {
                name: "a".into(),
                target_path: "C:\\a".into(),
                is_folder: false,
            },
            AppEntry {
                name: "b".into(),
                target_path: "C:\\b".into(),
                is_folder: true,
            },
        ];
        let bytes = try_serialize_with_header(
            INDEX_MAGIC,
            4,
            &IndexCacheV4 {
                built_at: 1_700_000_000,
                entries: entries.clone(),
                config_hash: 42,
                char_masks: vec![0; entries.len()],
                file_name_char_masks: vec![0; entries.len()],
                lower_names: vec!["a".into(), "b".into()],
                lower_file_names: vec![None, None],
                normalized_keys: vec![],
            },
        )
        .expect("serialize");
        assert!(cache_bin_file_in(&dir).save_bytes(&bytes), "save");

        let result = load_cache_in(&dir, 42, LegacyUpgrade::Write).expect("v4 が読めること");
        assert_eq!(result.version, 4, "`version` は**読めた**版のままである");
        assert_eq!(result.material.tree().len(), 2, "材料が正しいこと");

        // ディスクは現行版になっていること。
        let raw = cache_bin_file_in(&dir)
            .load_bytes()
            .expect("読み直せること");
        assert_eq!(
            crate::binfmt::peek_version(&raw),
            Some(INDEX_CACHE_VERSION),
            "旧版を読んだ後、ディスクは現行版で書き戻されていること"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// **昇格は保存の直前に整列する**（`sort_entries_canonical` の契約）。
    ///
    /// **3 つの書き手のうち、入力の並びが自分の制御下に無いのはここだけである**——他の 2 つ
    /// （cache-miss 枝・`rebuild_and_save`）は自分で走査した結果を数行上で整列させるが、昇格が
    /// 受け取るのは**過去の版が過去の canon で書いたファイル**であり、その並びを今の canon が
    /// 保証する理由は無い。ゆえに「契約を守り忘れる」以外に「そもそも整列していない入力が
    /// 来る」経路がここにだけ在る。
    ///
    /// **測るのは正しさではなくサイズである。** [`crate::index_tree::IndexTree::build`] は
    /// 未整列を許容し（親の二分探索が空振りするだけで別の親を返さない）、取りこぼした
    /// エントリは根になって**自分のフルパスを `table` へ実体で置く**。検索結果は正しいまま
    /// `index.bin` が太るので、挙動テストでは捕まらない。
    #[test]
    fn legacy_upgrade_sorts_before_saving_so_the_tree_stays_shared() {
        let _guard = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("upgrade_sort");

        // **正準の並びの逆順で置く。** 旧版ファイルが今の canon と違う順序で書かれていた
        // 場合を治具にする（昇格が整列を怠ると、この並びのまま木を建てることになる）。
        let entries = vec![
            AppEntry {
                name: "c".into(),
                target_path: "C:\\d\\c.txt".into(),
                is_folder: false,
            },
            AppEntry {
                name: "b".into(),
                target_path: "C:\\d\\b.txt".into(),
                is_folder: false,
            },
            AppEntry {
                name: "a".into(),
                target_path: "C:\\d\\a.txt".into(),
                is_folder: false,
            },
            AppEntry {
                name: "d".into(),
                target_path: "C:\\d".into(),
                is_folder: true,
            },
        ];
        let bytes = try_serialize_with_header(
            INDEX_MAGIC,
            2,
            &IndexCacheV2 {
                built_at: 1_700_000_000,
                entries: entries.clone(),
                config_hash: 7,
            },
        )
        .expect("serialize");
        assert!(cache_bin_file_in(&dir).save_bytes(&bytes), "save");

        load_cache_in(&dir, 7, LegacyUpgrade::Write).expect("v2 が読めること");

        let raw = cache_bin_file_in(&dir)
            .load_bytes()
            .expect("読み直せること");
        let written = try_deserialize_with_header::<IndexCache<'static>>(
            &raw,
            INDEX_MAGIC,
            INDEX_CACHE_VERSION,
        )
        .expect("現行版で書き戻されていること");

        assert!(
            written.sorted_by_path,
            "昇格が整列せずに保存した（`sorted_by_path` が下りている）"
        );
        for child in ["C:\\d\\a.txt", "C:\\d\\b.txt", "C:\\d\\c.txt"] {
            assert!(
                !written.table.iter().any(|s| s == child),
                "親が解決されず {child} のフルパスが `table` へ実体で戻った\
                 ——木が平たくなり `index.bin` が太る（`sort_entries_canonical` の doc）"
            );
        }

        let _ = fs::remove_dir_all(&dir);
    }

    /// **昇格は走査していないので、`built_at` を打ち直さない**（[`BuiltAt`] の doc）。
    ///
    /// 打ち直すと、設定アプリが唯一の手がかりにしている「最終構築日時」が、走査していない
    /// 起動で現在時刻へ進む。嘘をつく相手は**最も索引が古い層**——旧版のまま放置していた
    /// ユーザー——に限られ、しかも表示はその層に「たった今構築した」と告げる。
    ///
    /// **両方向を固定する。** 持ち越し側だけを見ると、`built_at` を定数へ潰す変異
    /// （走査した書き手も打ち直さなくなる）が素通りする。
    #[test]
    fn upgrade_carries_the_built_at_it_read() {
        // `Write` は `INDEX_WRITE_LOCK` を取る（`upgrade_legacy_cache_in` の doc）。
        let _guard = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("upgrade_built_at");
        let entries = vec![AppEntry {
            name: "a".into(),
            target_path: "C:\\a".into(),
            is_folder: false,
        }];
        const LEGACY_BUILT_AT: u64 = 1_700_000_000;
        let bytes = try_serialize_with_header(
            INDEX_MAGIC,
            4,
            &IndexCacheV4 {
                built_at: LEGACY_BUILT_AT,
                entries: entries.clone(),
                config_hash: 42,
                char_masks: vec![0; entries.len()],
                file_name_char_masks: vec![0; entries.len()],
                lower_names: vec!["a".into()],
                lower_file_names: vec![None],
                normalized_keys: vec![],
            },
        )
        .expect("serialize");
        assert!(cache_bin_file_in(&dir).save_bytes(&bytes), "save");

        load_cache_in(&dir, 42, LegacyUpgrade::Write).expect("v4 が読めること");

        assert_eq!(
            index_built_at_in(&dir),
            Some(LEGACY_BUILT_AT),
            "昇格が `built_at` を打ち直している——走査していない起動で\
             「最終構築日時」が現在時刻へ進む（`BuiltAt` の doc）"
        );

        // 逆向き: 走査して書く側は現在時刻を打つ。
        let before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("UNIX_EPOCH より後")
            .as_secs();
        let scanned_dir = temp_dir("scanned_built_at");
        save_cache_sorted_in(&scanned_dir, entries, 42, BuiltAt::Scanned);
        let scanned = index_built_at_in(&scanned_dir).expect("書けていること");
        assert!(
            scanned >= before,
            "走査した書き手が `built_at` を進めていない（{scanned} < {before}）"
        );

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&scanned_dir);
    }

    /// **旧版昇格の save 時間は `LoadCacheResult::upgrade_save_ms` として見える化されている。**
    ///
    /// 昇格 save（`upgrade_legacy_cache_in` → `save_cache_sorted_in`。旧版起動 1 回だけ発生する
    /// `derive_columns` の再導出 + postcard シリアライズ + 数百 ms 級の書き込み）は、呼び出し元の
    /// `cache_load_ms` 計測区間の内側で起きる。運ばずに `None` を返すと、実際に save が起きた
    /// 起動で「保存していない」という**偽の測定値**を `LoadOrScanStats::cache_save_ms` が報告する
    /// ことになる（`LoadOrScanStats` の doc「`cache_load_ms` と `total_ms` の間に処理を足すときは
    /// 項目を作ること」が守るべき対象そのもの）。
    ///
    /// **両方向とも variant で見る**（旧版を `Write` で読んだら `Some(_)`、現行版は `None`）。
    /// **時間の値は判定に使わない**——1 件の治具では `derive_columns` の再導出 + postcard +
    /// tmp→rename がサブミリ秒で終わり、壁時計の `> 0` は時計の量子化に載って確率的に落ちた
    /// （#1054 で main の全体実行 6 回中 1 回・#1063 で別実行の 1 回）。**`Some(0)` はここでは
    /// 合格である**——通ったこと自体は variant が持ち、速さは判定に関わらない。
    ///
    /// **`Some` は「実際に書けた」ではなく「昇格の枝を通った」である**（`upgrade_legacy_cache_in`
    /// は save の失敗を飲む——理由はその doc）。書き戻しの成否を固定するのは
    /// `load_cache_upgrades_a_legacy_format_in_place` /
    /// `load_cache_does_not_rewrite_when_the_format_is_current` の対であり、ここの射程は
    /// 「計器がその枝を通ったことを報告するか」だけである。
    #[test]
    fn load_cache_reports_upgrade_save_ms_only_when_it_upgrades_a_legacy_format() {
        // `Write` は `INDEX_WRITE_LOCK` を取る（上のテストと同じ理由）。
        let _guard = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        // 旧版（v4）: 昇格が走るので save 時間が乗る。
        let legacy_dir = temp_dir("upgrade_save_ms_legacy");
        let entries = vec![AppEntry {
            name: "a".into(),
            target_path: "C:\\a".into(),
            is_folder: false,
        }];
        let bytes = try_serialize_with_header(
            INDEX_MAGIC,
            4,
            &IndexCacheV4 {
                built_at: 1_700_000_000,
                entries: entries.clone(),
                config_hash: 42,
                char_masks: vec![0; entries.len()],
                file_name_char_masks: vec![0; entries.len()],
                lower_names: vec!["a".into()],
                lower_file_names: vec![None],
                normalized_keys: vec![],
            },
        )
        .expect("serialize");
        assert!(cache_bin_file_in(&legacy_dir).save_bytes(&bytes), "save");

        let legacy_result =
            load_cache_in(&legacy_dir, 42, LegacyUpgrade::Write).expect("v4 が読めること");
        assert_eq!(legacy_result.material.tree().len(), 1, "材料が正しいこと");
        // **速さではなく通ったかを見る**（`Some(0)` も合格。理由はこのテストの doc）。
        assert!(
            legacy_result.upgrade_save_ms.is_some(),
            "旧版を Write で読んだら昇格 save の枝を通ること（`None` は\
             `upgrade_legacy_cache_in` のクロージャを一度も通っていないことを意味する）"
        );

        // 現行版（v7）: 昇格しないので save 時間は乗らない。
        let current_dir = temp_dir("upgrade_save_ms_current");
        let config_hash = 42u64;
        let derived = derive_columns(entries);
        let derived_cols = derived.tree.columns();
        let cache = IndexCache {
            built_at: 1_700_000_000,
            names: Cow::Borrowed(derived_cols.names),
            is_folder: Cow::Borrowed(derived_cols.is_folder),
            parent: Cow::Borrowed(derived_cols.parent),
            aux: Cow::Borrowed(derived_cols.aux),
            table: Cow::Borrowed(derived_cols.table),
            sorted_by_path: derived_cols.sorted_by_path,
            config_hash,
            char_masks: Cow::Borrowed(&derived.char_masks),
            file_name_char_masks: Cow::Borrowed(&derived.file_name_char_masks),
            lower_names: Cow::Borrowed(&derived.lower_names),
            lower_file_names: Cow::Borrowed(&derived.lower_file_names),
        };
        let bytes =
            try_serialize_with_header(INDEX_MAGIC, INDEX_CACHE_VERSION, &cache).expect("serialize");
        assert!(cache_bin_file_in(&current_dir).save_bytes(&bytes), "save");

        let current_result = load_cache_in(&current_dir, config_hash, LegacyUpgrade::Write)
            .expect("v7 が読めること");
        assert_eq!(
            current_result.upgrade_save_ms, None,
            "現行版は昇格しないので枝を通らないこと（`Some` は速さに関わらず\
             `upgrade_legacy_cache_in` を通ったことを意味する）"
        );

        let _ = fs::remove_dir_all(&legacy_dir);
        let _ = fs::remove_dir_all(&current_dir);
    }

    /// **`LoadOrScanStats::cache_save_ms` レベルで固定する。** 上のテストは
    /// `LoadCacheResult::upgrade_save_ms`（`load_cache_in` の返り値）までしか見ておらず、
    /// それを呼び出し元の `cache_save_ms` へ運ぶ配線（`load_or_scan_with_stats_in` の
    /// cache-hit 枝、`cache_save_ms: result.upgrade_save_ms`）自体は固定していない
    /// ——**最終レビュー Important 1 の実際の欠陥はこの配線が `cache_save_ms: 0` を
    /// 焼き込んでいたことであり**、`upgrade_save_ms` 単体の検知器はこの配線を落とす
    /// 退行（`result.upgrade_save_ms` を使わず `0` を書く）では落ちない。
    #[test]
    fn load_or_scan_with_stats_reports_upgrade_save_ms_in_cache_save_ms() {
        // `Write` は `INDEX_WRITE_LOCK` を取る（上のテストと同じ理由）。
        let _guard = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let scan = vec![ScanPath {
            path: "C:\\nonexistent-for-hash-only".into(),
            extensions: vec![".txt".into()],
            include_folders: false,
        }];
        let config_hash = compute_config_hash(&scan, false);

        // 旧版（v4）: cache-hit しつつ昇格が走るので cache_save_ms が非 0 になること。
        //
        // **母集団を 1 件にしてはならない。** 判定は `as_millis()` の整数値なので、昇格 save が
        // 1 ms を切ると「配線は生きているのに 0」で落ちる——実際に 1 件の治具では 8 回中 3 回
        // 落ちた（近傍のテストが先に同じ経路を通って温めた実行だけが 0 になる）。**時計を
        // 跨がせるのは閾値ではなく仕事量である。**
        //
        // **ここを `LoadCacheResult` 側と同じ variant 判定へ替えることはできない**（#1054 /
        // #1063 で替えたのは向こうだけである）——`LoadOrScanStats::cache_save_ms` は `u128` の
        // 外向き計器で覗く variant を持たず、しかもこの assert が「配線が `result.upgrade_save_ms`
        // を捨てて 0 を焼き込む」退行を捕まえる唯一の検知器である。時間を見るのをやめると
        // 検知器が 1 つ減る。
        let legacy_dir = temp_dir("stats_upgrade_save_ms_legacy");
        let entries: Vec<AppEntry> = (0..20_000)
            .map(|i| AppEntry {
                name: format!("entry{i:05}"),
                target_path: format!("C:\\dir{:03}\\entry{i:05}.txt", i / 100),
                is_folder: false,
            })
            .collect();
        let bytes = try_serialize_with_header(
            INDEX_MAGIC,
            4,
            &IndexCacheV4 {
                built_at: 1_700_000_000,
                entries: entries.clone(),
                config_hash,
                char_masks: vec![0; entries.len()],
                file_name_char_masks: vec![0; entries.len()],
                lower_names: entries.iter().map(|e| e.name.clone()).collect(),
                lower_file_names: vec![None; entries.len()],
                normalized_keys: vec![],
            },
        )
        .expect("serialize");
        assert!(cache_bin_file_in(&legacy_dir).save_bytes(&bytes), "save");

        let result = load_or_scan_with_stats_in(&legacy_dir, &scan, false);
        // **`cache_hit` を先に確かめる。** hash が合わず miss 枝へ落ちた場合も
        // `cache_save_ms > 0` にはなりうるが、それは cache-miss 枝の独立フェーズとしての
        // save であって、昇格 save の配線を固定したことにはならない——この assert が
        // 検知器の前提を保証する。
        assert!(
            result.stats.cache_hit,
            "config_hash を揃えたので cache-hit になること（miss だとこの検知器は無意味になる）"
        );
        assert!(
            result.stats.cache_save_ms > 0,
            "cache-hit 枝で旧版昇格が走ったら LoadOrScanStats::cache_save_ms が非 0 になること\
             （load_or_scan_with_stats_in の配線 result.upgrade_save_ms を落とす退行の検知器）"
        );

        // 現行版（v7）: cache-hit だが昇格しないので cache_save_ms は 0 のまま。
        let current_dir = temp_dir("stats_upgrade_save_ms_current");
        let derived = derive_columns(entries);
        let derived_cols = derived.tree.columns();
        let cache = IndexCache {
            built_at: 1_700_000_000,
            names: Cow::Borrowed(derived_cols.names),
            is_folder: Cow::Borrowed(derived_cols.is_folder),
            parent: Cow::Borrowed(derived_cols.parent),
            aux: Cow::Borrowed(derived_cols.aux),
            table: Cow::Borrowed(derived_cols.table),
            sorted_by_path: derived_cols.sorted_by_path,
            config_hash,
            char_masks: Cow::Borrowed(&derived.char_masks),
            file_name_char_masks: Cow::Borrowed(&derived.file_name_char_masks),
            lower_names: Cow::Borrowed(&derived.lower_names),
            lower_file_names: Cow::Borrowed(&derived.lower_file_names),
        };
        let bytes =
            try_serialize_with_header(INDEX_MAGIC, INDEX_CACHE_VERSION, &cache).expect("serialize");
        assert!(cache_bin_file_in(&current_dir).save_bytes(&bytes), "save");

        let result = load_or_scan_with_stats_in(&current_dir, &scan, false);
        assert!(result.stats.cache_hit, "現行版も cache-hit であること");
        assert_eq!(
            result.stats.cache_save_ms, 0,
            "現行版は昇格しないので cache_save_ms は 0 のままであること"
        );

        let _ = fs::remove_dir_all(&legacy_dir);
        let _ = fs::remove_dir_all(&current_dir);
    }

    /// **v2 の `Write` 枝を独立に固定する。** v2 はマスクを持たない唯一の版であり、
    /// `finish_legacy_read` の `LegacyRead { masks: None, .. }` を通る構造的に他と違う枝
    /// （`Skip` なら `from_tree`、`Write` なら `upgrade_legacy_cache_in` 経由で `derived`）。
    /// v4〜v6 の `Write` テストだけでは、`masks: None` を渡したときも `upgrade_legacy_cache_in`
    /// が正しく呼ばれることまでは固定されない（レビューの ⚠️ 指摘）。
    #[test]
    fn load_cache_upgrades_a_legacy_v2_format_in_place() {
        // `Write` は `INDEX_WRITE_LOCK` を取る（上のテストと同じ理由）。
        let _guard = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("load_upgrade_v2");
        let entries = vec![AppEntry {
            name: "a".into(),
            target_path: "C:\\a".into(),
            is_folder: false,
        }];
        let bytes = try_serialize_with_header(
            INDEX_MAGIC,
            2,
            &IndexCacheV2 {
                built_at: 1_700_000_000,
                entries: entries.clone(),
                config_hash: 42,
            },
        )
        .expect("serialize");
        assert!(cache_bin_file_in(&dir).save_bytes(&bytes), "save");

        let result = load_cache_in(&dir, 42, LegacyUpgrade::Write).expect("v2 が読めること");
        assert_eq!(result.version, 2, "`version` は**読めた**版のままである");
        assert_eq!(result.material.tree().len(), 1, "材料が正しいこと");
        // v2 は本来マスクを持たないが、`Write` で昇格した後は現行版として derive し直され、
        // 必ずマスクを持つ（`Skip` の `!has_masks()` と対になる非対称——`load_cache_in`
        // の doc を参照）。
        assert!(
            result.material.has_masks(),
            "昇格後は derive し直したマスクを持つこと"
        );

        // ディスクは現行版になっていること。
        let raw = cache_bin_file_in(&dir)
            .load_bytes()
            .expect("読み直せること");
        assert_eq!(
            crate::binfmt::peek_version(&raw),
            Some(INDEX_CACHE_VERSION),
            "旧版を読んだ後、ディスクは現行版で書き戻されていること"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// **現行版を読んだときは書き直さない。** ここが退行すると毎起動 17 MiB を書く
    /// （結果は正しいまま静かに遅くなるので挙動テストでは捕まらない）。
    ///
    /// **`built_at` を事前に過去へ固定した現行版を fixture として直接ディスクへ置く。**
    /// `save_cache_sorted_in` で作ってから読む形（旧版）だと save→load が同一プロセス内で
    /// マイクロ秒差に収まり、`built_at`（`SystemTime::now()...as_secs()`・秒粒度）が同じ秒の
    /// 値になるため、「現行版でも無条件に書き直す」退行が入っても差が出ず**原理的に発火しない**
    /// （レビューで指摘・2026-08-10）。固定値を仕込めば、書き直しが起きた瞬間に必ず
    /// 現在時刻へ動くので粒度に依存しない。
    #[test]
    fn load_cache_does_not_rewrite_when_the_format_is_current() {
        // v7 の `Write` 枝は分岐しないためロックは取らないが、`Write` を渡す以上は将来の
        // 退行（v7 判定漏れで旧版枝へ落ちる等）に備えて直列化しておく（Minor 7 の指摘）。
        let _guard = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("load_no_rewrite");
        let entries = vec![AppEntry {
            name: "a".into(),
            target_path: "C:\\a".into(),
            is_folder: false,
        }];
        let config_hash = 42u64;
        let derived = derive_columns(entries);
        let derived_cols = derived.tree.columns();
        let cache = IndexCache {
            built_at: 1_700_000_000,
            names: Cow::Borrowed(derived_cols.names),
            is_folder: Cow::Borrowed(derived_cols.is_folder),
            parent: Cow::Borrowed(derived_cols.parent),
            aux: Cow::Borrowed(derived_cols.aux),
            table: Cow::Borrowed(derived_cols.table),
            sorted_by_path: derived_cols.sorted_by_path,
            config_hash,
            char_masks: Cow::Borrowed(&derived.char_masks),
            file_name_char_masks: Cow::Borrowed(&derived.file_name_char_masks),
            lower_names: Cow::Borrowed(&derived.lower_names),
            lower_file_names: Cow::Borrowed(&derived.lower_file_names),
        };
        let bytes =
            try_serialize_with_header(INDEX_MAGIC, INDEX_CACHE_VERSION, &cache).expect("serialize");
        assert!(cache_bin_file_in(&dir).save_bytes(&bytes), "save");

        let result =
            load_cache_in(&dir, config_hash, LegacyUpgrade::Write).expect("v7 が読めること");
        assert_eq!(result.version, INDEX_CACHE_VERSION);
        assert_eq!(
            index_built_at_in(&dir),
            Some(1_700_000_000),
            "現行版のロードで index.bin を書き直してはならない"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// **`Skip` は書き戻さない。** `load_cached_entries`（corpus テストの入口）が通す枝で、
    /// ここが `Write` へ退行すると、開発者の実 `%APPDATA%\Snotra\index.bin` を読むだけの
    /// テスト実行が実データを書き換えてしまう（#1013 と同型）。`built_at` が動かないことで
    /// 書き戻しが起きていないことを測る。
    #[test]
    fn load_cache_skip_does_not_upgrade_a_legacy_format() {
        let dir = temp_dir("load_skip_no_upgrade");
        let entries = vec![AppEntry {
            name: "a".into(),
            target_path: "C:\\a".into(),
            is_folder: false,
        }];
        let bytes = try_serialize_with_header(
            INDEX_MAGIC,
            4,
            &IndexCacheV4 {
                built_at: 1_700_000_000,
                entries: entries.clone(),
                config_hash: 42,
                char_masks: vec![0; entries.len()],
                file_name_char_masks: vec![0; entries.len()],
                lower_names: vec!["a".into()],
                lower_file_names: vec![None],
                normalized_keys: vec![],
            },
        )
        .expect("serialize");
        assert!(cache_bin_file_in(&dir).save_bytes(&bytes), "save");

        let result = load_cache_in(&dir, 42, LegacyUpgrade::Skip).expect("v4 が読めること");
        assert_eq!(result.version, 4);

        // ディスクは v4 のまま、書き戻しは起きていない
        // （`index_built_at_in` は現行版の `IndexCache::built_at` を読める全版共通の口——
        // `LegacyUpgrade::Write` が走っていればここが現在時刻へ動く）。
        assert_eq!(
            index_built_at_in(&dir),
            Some(1_700_000_000),
            "`Skip` は index.bin を書き戻してはならない"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// 設定アプリが最終構築日時を出すための口。**17 MiB を読まない**ことが要点で、
    /// 読めない・無いときは黙って `None` を返す（表示は「未構築」へ倒れる）。
    #[test]
    fn index_built_at_reads_the_timestamp_without_loading_the_index() {
        let dir = temp_dir("built_at_read");
        assert_eq!(index_built_at_in(&dir), None, "不在は None");

        let entries = vec![AppEntry {
            name: "a".into(),
            target_path: "C:\\a".into(),
            is_folder: false,
        }];
        let _ = save_cache_sorted_in(&dir, entries, 42, BuiltAt::Scanned);

        let built_at = index_built_at_in(&dir).expect("保存した直後は読めること");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // 保存は今なので、未来ではなく、かつ極端に古くもない。
        assert!(
            built_at <= now,
            "未来の値を返してはならない: {built_at} > {now}"
        );
        assert!(
            now - built_at < 300,
            "保存直後の値とかけ離れている: {built_at}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// **旧版でも読める**（`built_at` は全版で先頭フィールドである）。
    #[test]
    fn index_built_at_reads_a_legacy_version_too() {
        let dir = temp_dir("built_at_legacy");
        let bytes = try_serialize_with_header(
            INDEX_MAGIC,
            4,
            &IndexCacheV4 {
                built_at: 1_700_000_000,
                entries: vec![],
                config_hash: 1,
                char_masks: vec![],
                file_name_char_masks: vec![],
                lower_names: vec![],
                lower_file_names: vec![],
                normalized_keys: vec![],
            },
        )
        .expect("serialize");
        assert!(cache_bin_file_in(&dir).save_bytes(&bytes), "save");
        assert_eq!(index_built_at_in(&dir), Some(1_700_000_000));

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
    /// **フォールバックの鎖のどの枝で読めたのかを外から見る手段はこれしか無い**（材料だけを
    /// 見ても v5 の枝と v6 の枝は区別できない）。しかも**取り違えても検索結果は正しいまま**で
    /// ある——枝選択の退行は「読めてはいるが想定と違う形式で読んでいる」形で静かに残る。
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
        save_cache_sorted_in(&dir, entries.clone(), config_hash, BuiltAt::Scanned);
        assert_eq!(
            load_cache_in(&dir, config_hash, LegacyUpgrade::Skip)
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
            lower_names: lower_names.iter().map(|s| Some(s.as_str())).collect(),
            lower_file_names: lower_file_names
                .iter()
                .map(|f| match f {
                    Some(s) => LowerFileSlot::Text(s),
                    None => LowerFileSlot::Absent,
                })
                .collect(),
        };
        fs::write(
            dir.join("index.bin"),
            try_serialize_with_header(INDEX_MAGIC, 6, &v6).expect("serialize v6"),
        )
        .expect("write v6");
        assert_eq!(
            load_cache_in(&dir, config_hash, LegacyUpgrade::Skip)
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
            load_cache_in(&dir, config_hash, LegacyUpgrade::Skip)
                .expect("v5 が読めること")
                .version,
            5
        );
        let _ = fs::remove_dir_all(&dir);

        // v4: 末尾に normalized_keys を持つ形式。
        let dir = temp_dir("version_reported_v4");
        write_v4_cache_in(&dir, &entries, config_hash);
        assert_eq!(
            load_cache_in(&dir, config_hash, LegacyUpgrade::Skip)
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
            load_cache_in(&dir, config_hash, LegacyUpgrade::Skip)
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
        let v2_result =
            load_cache_in(&dir, config_hash, LegacyUpgrade::Skip).expect("v2 が読めること");
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

    /// **木の節点数は索引のエントリ件数である。** `save_cache_sorted_in` は走査結果を木へ
    /// 組み替えて返すので、件数が食い違えば下流の並列 Vec と木で長さがずれる。
    #[test]
    fn tree_len_is_the_entry_count() {
        let entries = vec![
            AppEntry {
                name: "A".into(),
                target_path: "C:\\a.txt".into(),
                is_folder: false,
            },
            AppEntry {
                name: "B".into(),
                target_path: "C:\\dir\\b.txt".into(),
                is_folder: false,
            },
        ];
        let n = entries.len();
        let dir = temp_dir("tree_len_is_entry_count");
        let (tree, _) = save_cache_sorted_in(&dir, entries, 0, BuiltAt::Scanned);
        assert_eq!(tree.len(), n, "木の len は索引のエントリ件数と一致する");
        let _ = fs::remove_dir_all(&dir);
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
                lower_names: [None].into_iter().collect(),
                lower_file_names: [LowerFileSlot::SameAsLowerName].into_iter().collect(),
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
                    lower_names.iter().collect::<Vec<_>>(),
                    vec![None, None, Some("docs")],
                    "`name` と同一なら追記側でも落とす"
                );
                assert_eq!(
                    lower_file_names.iter().collect::<Vec<_>>(),
                    vec![
                        LowerFileSlot::SameAsLowerName,
                        LowerFileSlot::Text("tool.exe"),
                        LowerFileSlot::SameAsLowerName,
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
            derived.lower_names.iter().collect::<Vec<_>>(),
            vec![Some("tool"), Some("docs"), None, Some("root")]
        );
        assert_eq!(
            derived.lower_file_names.iter().collect::<Vec<_>>(),
            vec![
                LowerFileSlot::Text("tool.exe"),
                LowerFileSlot::SameAsLowerName,
                LowerFileSlot::Text("notes.txt"),
                LowerFileSlot::Absent,
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

    /// 2 件の木を建てる（下の検証テスト群の材料）。
    fn two_entry_tree() -> IndexTree {
        IndexTree::build(vec![
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
        ])
    }

    /// **`from_untrusted` の拒否経路そのものを走らせる。**
    ///
    /// これが無いと、`index.bin` から来た組を検証する**唯一の**機構に検知器が 1 本も無い状態になる——条件の向きを書き換えても（`!=` を `<` へ、`lower_ok` の腕を 1 本落とす等）既存テストは全数緑のまま通り、壊れた `index.bin` は「木より短いマスク」として起動経路へ入る。`assemble` の長さ検証は `debug_assert` ゆえ release では消えるので、帰結は起動後の初回検索での添字外アクセス → `panic = "abort"` である（全走査による自動復旧も起きない）。
    ///
    /// **長い側も弾くことまで見る。** 等値（`!=`）で書いてあるので短い列と長い列の両方が落ちるが、`<` へ書き換えると長い側だけが素通りする——列が余分に長い `index.bin` は添字外にはならないが、木と対応しないマスクで検索することになり**スコアが静かにずれる**。
    #[test]
    fn from_untrusted_rejects_masks_whose_len_disagrees_with_the_tree() {
        let full = |n: usize| CachedMasks {
            char_masks: vec![0; n],
            file_name_char_masks: vec![0; n],
            lower: None,
        };

        // 揃っている組は受け取る（受理側を測らないと、下の拒否が「常に None」でも緑になる）。
        assert!(
            IndexMaterial::from_untrusted(two_entry_tree(), full(2)).is_some(),
            "長さの揃った組は受理されなければならない"
        );

        for n in [1usize, 3] {
            assert!(
                IndexMaterial::from_untrusted(two_entry_tree(), full(n)).is_none(),
                "木が 2 件なのにマスクが {n} 件の組を受理している"
            );
        }

        // 列ごとに独立して見ていること（片方だけずれた組も弾く）。
        let mut only_file_short = full(2);
        only_file_short.file_name_char_masks.pop();
        assert!(
            IndexMaterial::from_untrusted(two_entry_tree(), only_file_short).is_none(),
            "file_name_char_masks だけがずれた組を受理している"
        );

        // `lower` の 2 variant も見ていること。
        let collapsed_short = CachedMasks {
            char_masks: vec![0; 2],
            file_name_char_masks: vec![0; 2],
            lower: Some(CachedLower::Collapsed {
                lower_names: [None].into_iter().collect(),
                lower_file_names: [LowerFileSlot::Absent].into_iter().collect(),
            }),
        };
        assert!(
            IndexMaterial::from_untrusted(two_entry_tree(), collapsed_short).is_none(),
            "Collapsed の列がずれた組を受理している"
        );
        let raw_short = CachedMasks {
            char_masks: vec![0; 2],
            file_name_char_masks: vec![0; 2],
            lower: Some(CachedLower::Raw {
                lower_names: vec!["firefox".to_string()],
                lower_file_names: vec![None],
            }),
        };
        assert!(
            IndexMaterial::from_untrusted(two_entry_tree(), raw_short).is_none(),
            "Raw の列がずれた組を受理している"
        );
    }
}
