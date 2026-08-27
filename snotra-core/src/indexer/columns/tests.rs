//! 派生データのテスト——追記（`extend_cached_masks`）が潰し方とマスクを保つこと。

use super::*;

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
