//! `PATH` 走査のテスト——既存エントリの篩い落とし、列挙順、拡張子、ディレクトリ間の重複排除。

use super::*;
use crate::indexer::test_support::temp_dir;
use std::fs;

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
