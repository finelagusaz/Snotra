//! `PATH` 環境変数のディレクトリから実行可能ファイルを拾い、**既存の索引に無いものだけ**を返す。
//!
//! 既存判定は 2 段である——末尾セグメントの篩（`super::keys::normalize_file_name_key_into`）で
//! 候補を絞り、フルパスの正規化キーで確定する。篩は判定ではなく、照合する両辺が同じ手順を通ること
//! だけが「偽陰性を出さない」の根拠である（論証の全文は [`reject_existing`] の doc）。

use std::path::Path;

use crate::index_tree::IndexTree;

use super::AppEntry;
use super::keys::{normalize_entry_key, normalize_entry_key_into, normalize_file_name_key_into};
use super::scan::is_hidden_or_system;

#[cfg(test)]
mod tests;

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
pub(super) fn scan_path_dirs(
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
