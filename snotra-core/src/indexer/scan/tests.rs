//! スキャン（根ごとの重複排除の役割・拡張子照合・フォルダ列挙・正準整列）のテスト。

use super::*;
use crate::indexer::test_support::temp_dir;
use std::fs;
use std::sync::atomic::AtomicUsize;

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

/// **境界の 2 つの分岐を 1 本にまとめると、ここが落ちる**（`c:\tools` は `c:\toolsextra` の
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
    let entries = scan_all(&scan, true, &AtomicUsize::new(0));

    assert_eq!(
        entries.len(),
        1,
        "入れ子の根で同じファイルが二度入っている（重複排除が効いていない）"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// **`dirs` は `read_dir` に成功したディレクトリを根も含めて数え、登録件数とは独立である。**
///
/// 2 本目の根は `extensions` が空で `include_folders` が偽——登録は 0 件でも、走査したフォルダは
/// 数える（実運用点の `C:\` の根がこの形で、進捗の単位をフォルダ数にした理由である）。
/// 存在しない根は `read_dir` に失敗するので数えない。期待値は fixture のディレクトリの数え上げ
/// である: 根 `a` とその下の `sub` / `sub\deep`（3）、根 `b` とその下の `c` / `c\d`（3）。
#[test]
fn scan_all_counts_every_directory_it_reads_even_when_it_registers_nothing() {
    let dir = temp_dir("counts_dirs");
    let a = dir.join("a");
    let b = dir.join("b");
    fs::create_dir_all(a.join("sub").join("deep")).expect("create a tree");
    fs::create_dir_all(b.join("c").join("d")).expect("create b tree");
    fs::write(a.join("sub").join("tool.exe"), b"x").expect("write fixture");
    fs::write(b.join("c").join("ignored.exe"), b"x").expect("write fixture");

    let scan = vec![
        ScanPath {
            path: a.to_string_lossy().into_owned(),
            extensions: vec![".exe".to_string()],
            include_folders: false,
        },
        ScanPath {
            path: b.to_string_lossy().into_owned(),
            extensions: vec![],
            include_folders: false,
        },
        ScanPath {
            path: dir.join("missing").to_string_lossy().into_owned(),
            extensions: vec![".exe".to_string()],
            include_folders: true,
        },
    ];
    let dirs = AtomicUsize::new(0);
    let entries = scan_all(&scan, true, &dirs);

    assert_eq!(entries.len(), 1, "登録は根 a の tool.exe の 1 件だけのはず");
    assert_eq!(
        dirs.load(std::sync::atomic::Ordering::Relaxed),
        6,
        "読めたディレクトリの数（根を含む）と一致しない"
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
    let entries = scan_all(&scan, true, &AtomicUsize::new(0));

    assert_eq!(
        entries.len(),
        1,
        "子の根が先に来る順序で同じファイルが二度入っている"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scan_with_extensions_filters_by_ext() {
    let dir = temp_dir("ext_filter");
    fs::write(dir.join("app.exe"), "").unwrap();
    fs::write(dir.join("script.bat"), "").unwrap();
    fs::write(dir.join("readme.txt"), "").unwrap();

    let exts = build_extension_list(&["exe".to_string(), "bat".to_string()]);
    let entries = scan_directory(&dir, &exts, false, true, &AtomicUsize::new(0));

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

    let exts = build_extension_list(&["exe".to_string()]);
    let entries = scan_directory(&dir, &exts, true, true, &AtomicUsize::new(0));

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

    let exts = build_extension_list(&["exe".to_string()]);
    let entries = scan_directory(&dir, &exts, false, true, &AtomicUsize::new(0));

    assert!(entries.iter().all(|e| !e.is_folder));
    assert_eq!(entries.len(), 1);

    let _ = fs::remove_dir_all(&dir);
}

/// **重複排除のキーはパスであって名前ではない。** 先の根が積む側（`record`）に回るよう根を重ね、
/// 同名・別パスの 2 件が両方残ることを `scan_all` の結果で見る。
#[test]
fn scan_keeps_same_name_different_paths() {
    let dir = temp_dir("ext_dedup");
    let sub1 = dir.join("a");
    let sub2 = dir.join("b");
    fs::create_dir_all(&sub1).unwrap();
    fs::create_dir_all(&sub2).unwrap();
    fs::write(sub1.join("tool.exe"), "").unwrap();
    fs::write(sub2.join("tool.exe"), "").unwrap();

    let exe_root = |p: &std::path::Path| ScanPath {
        path: p.to_string_lossy().into_owned(),
        extensions: vec![".exe".to_string()],
        include_folders: false,
    };
    let entries = scan_all(
        &[exe_root(&dir), exe_root(&sub1)],
        true,
        &AtomicUsize::new(0),
    );

    let tools: Vec<&AppEntry> = entries.iter().filter(|e| e.name == "tool").collect();
    assert_eq!(tools.len(), 2);

    let _ = fs::remove_dir_all(&dir);
}

/// テスト側の期待値: `dir` を 1 スレッドで歩き、フォルダと `.exe` のパスを前順（列挙順に並べ、
/// フォルダの直後にその中身を置く）で積む。製品の走査を通さずに組む。
fn preorder_paths(dir: &std::path::Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("read fixture").flatten() {
        let path = entry.path();
        if entry.file_type().expect("file type").is_dir() {
            out.push(path.to_string_lossy().into_owned());
            preorder_paths(&path, out);
        } else if path.extension().is_some_and(|e| e == "exe") {
            out.push(path.to_string_lossy().into_owned());
        }
    }
}

/// **返す列（整列の前）は前順で、スレッド数に依らない。** 重複排除の勝者が列の順で決まる
/// （[`Dedup::accept`]）ことの前提である。子フォルダの数は `SCAN_THREADS_MAX` を跨がせ、
/// 複数のワーカーが実際に分け合う形にする。
#[test]
fn scan_all_returns_the_same_preorder_regardless_of_threads() {
    let dir = temp_dir("preorder");
    for i in 0..SCAN_THREADS_MAX * 4 {
        let child = dir.join(format!("d{i:02}"));
        fs::create_dir_all(child.join("g")).expect("create child");
        fs::write(child.join("a.exe"), b"x").expect("write fixture");
        fs::write(child.join("g").join("b.exe"), b"x").expect("write fixture");
    }
    fs::write(dir.join("z.exe"), b"x").expect("write fixture");

    let mut expected = Vec::new();
    preorder_paths(&dir, &mut expected);
    let scan = [ScanPath {
        path: dir.to_string_lossy().into_owned(),
        extensions: vec![".exe".to_string()],
        include_folders: true,
    }];
    for threads in [1, SCAN_THREADS_MAX] {
        let got: Vec<String> = scan_all_with_threads(&scan, true, &AtomicUsize::new(0), threads)
            .into_iter()
            .map(|e| e.target_path)
            .collect();
        assert_eq!(got, expected, "{threads} スレッドの列が前順と一致しない");
    }

    let _ = fs::remove_dir_all(&dir);
}

/// **同じ根の中の重複では、先に列挙した方が勝つ**（積む側の根のとき）。大文字小文字だけ違う
/// 兄弟は、大文字小文字を区別するディレクトリでしか並ばない。`fsutil` で作れない環境
/// （WSL の機能が無い機械など）ではスキップする。ただし環境変数 `CI` があれば失敗にする。
#[test]
fn scan_all_dedups_in_enumeration_order_within_a_root() {
    let dir = temp_dir("dedup_within_root");
    let cs = dir.join("cs");
    fs::create_dir_all(&cs).expect("create cs dir");
    let made = std::process::Command::new("fsutil")
        .args(["file", "setCaseSensitiveInfo"])
        .arg(&cs)
        .arg("enable")
        .output()
        .is_ok_and(|o| o.status.success());
    fs::write(cs.join("Tool.exe"), b"x").expect("write fixture");
    fs::write(cs.join("tool.exe"), b"x").expect("write fixture");
    let listed: Vec<String> = fs::read_dir(&cs)
        .expect("read cs")
        .flatten()
        .map(|e| e.path().to_string_lossy().into_owned())
        .collect();
    if !made || listed.len() != 2 {
        // この勝者を見る検知器はこの 1 本だけなので、CI では黙ってスキップさせない。
        assert!(
            std::env::var_os("CI").is_none(),
            "CI で大文字小文字を区別するディレクトリを作れなかった（検知器が走っていない）"
        );
        eprintln!("大文字小文字を区別するディレクトリを作れないのでスキップする");
        let _ = fs::remove_dir_all(&dir);
        return;
    }

    let exe_root = |p: &std::path::Path| ScanPath {
        path: p.to_string_lossy().into_owned(),
        extensions: vec![".exe".to_string()],
        include_folders: false,
    };
    // 先の根が積む側、後の根が照合だけの側になる。
    let scan = [exe_root(&dir), exe_root(&cs)];
    for threads in [1, SCAN_THREADS_MAX] {
        let got: Vec<String> = scan_all_with_threads(&scan, true, &AtomicUsize::new(0), threads)
            .into_iter()
            .map(|e| e.target_path)
            .collect();
        assert_eq!(
            got,
            vec![listed[0].clone()],
            "{threads} スレッドで、先に列挙した方が勝っていない"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scan_extensions_case_insensitive() {
    let dir = temp_dir("ext_case");
    fs::write(dir.join("app.EXE"), "").unwrap();

    let exts = build_extension_list(&["exe".to_string()]);
    let entries = scan_directory(&dir, &exts, false, true, &AtomicUsize::new(0));

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "app");

    let _ = fs::remove_dir_all(&dir);
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
fn scan_all_empty_when_no_paths() {
    let entries = scan_all(&[], false, &AtomicUsize::new(0));
    assert!(
        entries.is_empty(),
        "scan_all with no paths should return empty"
    );
}
