//! スキャン対象の列挙・重複排除・走査結果の正準整列。
//!
//! **重複排除に払う代金は根ごとに決める**——役割を割り当てるのが [`root_roles`]、それを消費して
//! 採否を決めるのが [`Dedup::accept`] である。正規化キーの規則そのものは `super::keys` が持ち、
//! ここはその呼び出し側にすぎない（記録側と照合側が同じ関数を通ることがバイト一致の根拠）。
//!
//! **走査は専用のプールで並列に回し、重複排除は根を歩き終えてから、その根の前順の列へ順に当てる**
//! （[`scan_all_with_threads`]）。返す列は 1 スレッドで歩いたときと同じになる。

use rayon::prelude::*;
use std::fs::Metadata;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM};

use crate::config::ScanPath;

use super::AppEntry;
use super::keys::{normalize_entry_key, normalize_entry_key_into};

#[cfg(test)]
mod tests;

/// 2 つの**正規化済み**の根について、`a` が `b` の祖先か同一か。
///
/// **境界の 2 つの分岐を 1 本にまとめてはならない。** [`crate::config::normalize_scan_path_key`] は
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
/// **積むのは「後続の根と重なる」側だけである。** ツリーの走査は同じディレクトリを二度読まない
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
///
/// **役割はツリーの深さではなく走査順の関数である。** ゆえに重複排除の検知器は、親の根が先・子の根が
/// 先の両順序を固定する（`scan_all_dedups_when_roots_are_nested` /
/// `scan_all_dedups_when_the_child_root_comes_first`）。
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
    ///
    /// **この関数の照合分岐で確保が戻る退行は、CI では検出されない。** 走査結果が同じままなので
    /// 挙動テストにも `root_roles` の単体テストにも映らず、`tests/memory_footprint.rs` の確保
    /// 回数（`#[ignore]` の手動計測）だけが見る。`scan_all` の結線の退行も同じである。確保の側で
    /// CI が守るのは、役割の割り当て（`root_roles_over_the_real_shape_leave_the_largest_root_inert`）
    /// までである。
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

/// 走査の専用プールのスレッド数の上限。根拠は `PERFORMANCE.md`「採用: 索引の走査を専用プールで fork-join の並列にする（#1455）」。
const SCAN_THREADS_MAX: usize = 8;

/// 全根を走査し、登録するエントリを返す。
///
/// **`dirs` には `read_dir` に成功したディレクトリの数を足していく**（根も 1 つと数える）。
/// 進捗の単位が登録件数ではなくフォルダ数なのは、登録 0 件で全体を走査する根が実在するから
/// である（`extensions` が空で `include_folders` が偽の根。所要の大半をそこが占めても件数は
/// 動かない）。読み手は走査と並行して別スレッドから読むので、加算は 1 ディレクトリにつき
/// `Relaxed` の 1 回だけにする（単調に増えるだけで、ほかの値との順序を運ばない）。
///
/// **走査は専用のプールで並列に回すが、返す列は 1 スレッドで歩いたときと同じ前順である**
/// （[`scan_directory`]）。検索と kana が使うグローバルなプールは塞がない。
pub fn scan_all(
    scan_paths: &[ScanPath],
    show_hidden_system: bool,
    dirs: &AtomicUsize,
) -> Vec<AppEntry> {
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(SCAN_THREADS_MAX);
    scan_all_with_threads(scan_paths, show_hidden_system, dirs, threads)
}

/// [`scan_all`] の本体。スレッド数を外から決められるのは、テストが 1 本と複数本の結果を比べるためである。
///
/// **重複排除は、根を 1 つ歩き終えてから、その根の列へ順に当てる。** 並列の走査の中では集合に
/// 触れない。根の列は前順なので、根の中でも根をまたいでも、勝つのは 1 スレッドで歩いたときと
/// 同じエントリである（[`Dedup::accept`] は呼ばれた順だけで決まる）。
///
/// プールを建てられなければ、同じ走査をグローバルなプールで回す（結果は同じ）。
fn scan_all_with_threads(
    scan_paths: &[ScanPath],
    show_hidden_system: bool,
    dirs: &AtomicUsize,
    threads: usize,
) -> Vec<AppEntry> {
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
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .thread_name(|i| format!("snotra-scan-{i}"))
        .build()
        .ok();

    for (sp, role) in scan_paths.iter().zip(&roles) {
        dedup.role = *role;
        let ext_set = build_extension_list(&sp.extensions);
        let walk = || {
            scan_directory(
                Path::new(&sp.path),
                &ext_set,
                sp.include_folders,
                show_hidden_system,
                dirs,
            )
        };
        let found = match &pool {
            Some(pool) => pool.install(walk),
            None => walk(),
        };
        entries.extend(found.into_iter().filter(|e| dedup.accept(&e.target_path)));
    }

    entries
}

/// ディレクトリ 1 つの中で、列挙順に見たエントリ。
enum Item {
    /// 登録するファイル。
    Leaf(AppEntry),
    /// 降りる子フォルダ。`folder` はそのフォルダ自身を登録するとき（`include_folders`）だけ持つ。
    Dir {
        folder: Option<AppEntry>,
        path: PathBuf,
    },
}

/// `dir` の下を歩き、登録するエントリを前順（pre-order）で返す: エントリを列挙順に並べ、
/// フォルダの直後にその中身を置く。
///
/// **列挙は 1 スレッドで終え、子フォルダだけを並列に降りる**（fork-join）。子の結果は列挙順に
/// 連結するので、返す列はスレッド数に依らない。重複排除はここでは当てない（[`scan_all_with_threads`]）。
fn scan_directory(
    dir: &Path,
    extensions: &[String],
    include_folders: bool,
    show_hidden_system: bool,
    dirs: &AtomicUsize,
) -> Vec<AppEntry> {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    dirs.fetch_add(1, Ordering::Relaxed);

    let mut items = Vec::new();
    for entry in read_dir.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };

        if !show_hidden_system && is_hidden_or_system(&meta) {
            continue;
        }

        if meta.is_dir() {
            let folder = include_folders
                .then(|| {
                    let name = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();
                    (!name.is_empty()).then(|| AppEntry {
                        name,
                        target_path: path.to_string_lossy().into_owned(),
                        is_folder: true,
                    })
                })
                .flatten();
            items.push(Item::Dir { folder, path });
        } else {
            let ext = path.extension().and_then(|e| e.to_str());
            if ext.is_some_and(|e| matches_extension(extensions, e)) {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                if !name.is_empty() {
                    items.push(Item::Leaf(AppEntry {
                        name,
                        target_path: path.to_string_lossy().into_owned(),
                        is_folder: false,
                    }));
                }
            }
        }
    }

    // `collect` は添字の順を保つ——子の結果は列挙順に並ぶ。
    let children: Vec<Vec<AppEntry>> = items
        .par_iter()
        .map(|item| match item {
            Item::Dir { path, .. } => {
                scan_directory(path, extensions, include_folders, show_hidden_system, dirs)
            }
            Item::Leaf(_) => Vec::new(),
        })
        .collect();

    let mut out = Vec::new();
    for (item, sub) in items.into_iter().zip(children) {
        match item {
            Item::Leaf(entry) => out.push(entry),
            Item::Dir { folder, .. } => {
                out.extend(folder);
                out.extend(sub);
            }
        }
    }
    out
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

/// 正準の並び。**ツリーの形をこの順序が決める**ので、ツリーを建てる者は必ずここを通すこと
/// （写しを書くと、写した側だけが旧い並びでツリーを建てて緑を返す）。
///
/// **`save_cache_sorted(_in)` を呼ぶ側は直前にここを通す契約である。** 親の解決は整列済みを
/// 前提に `target_path` を二分探索するので（[`crate::index_tree`] の `resolve_one`）、崩すと
/// 親が解決されないままツリーが平たくなり、接頭辞共有で削ったフルパスが `table` へ実体で戻る。
/// **症状は「`index.bin` が太る」だけで検索結果は正しいまま**ゆえ、挙動テストでは検出されない。
pub(crate) fn sort_entries_canonical(entries: &mut [AppEntry]) {
    entries.sort_by(|a, b| {
        a.target_path
            .cmp(&b.target_path)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.is_folder.cmp(&b.is_folder))
    });
}
