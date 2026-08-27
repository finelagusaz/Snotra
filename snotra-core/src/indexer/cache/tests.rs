//! `index.bin` の入出力のテスト——オンディスク形式の凍結、版のフォールバック鎖、旧版の形式昇格、
//! キャッシュと走査の振り分け、書き込みの排他。

use super::super::keys::normalize_entry_key;
use super::*;
use crate::binfmt::{try_deserialize_with_header, try_serialize_with_header};
use crate::indexer::test_support::{INDEX_LOCK_TEST_GUARD, temp_dir};
use crate::query::{file_char_mask, lower_file_name, name_char_mask, to_lower_folded};
use crate::str_arena::LowerFileSlot;
use std::fs;

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
        sorted_by_path: tree_cols.sorted_prefix_len == tree_cols.names.len(),
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
        try_deserialize_with_header(&bytes, INDEX_MAGIC, INDEX_CACHE_VERSION).expect("deserialize");

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
        sorted_by_path: tree_cols.sorted_prefix_len == tree_cols.names.len(),
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
    73, 78, 68, 88, 7, 0, 0, 0, 128, 226, 207, 170, 6, 4, 8, 80, 114, 111, 106, 101, 99, 116, 115,
    3, 97, 112, 112, 7, 70, 105, 114, 101, 102, 111, 120, 4, 100, 111, 99, 115, 4, 1, 0, 0, 1, 4,
    255, 255, 255, 255, 15, 0, 255, 255, 255, 255, 15, 255, 255, 255, 255, 15, 4, 2, 1, 3, 4, 5, 0,
    4, 46, 101, 120, 101, 11, 67, 58, 92, 80, 114, 111, 106, 101, 99, 116, 115, 19, 67, 58, 92, 97,
    112, 112, 115, 92, 102, 105, 114, 101, 102, 111, 120, 46, 108, 110, 107, 7, 67, 58, 92, 100,
    111, 99, 115, 1, 185, 96, 4, 171, 1, 205, 1, 239, 1, 33, 4, 18, 52, 86, 120, 4, 1, 8, 112, 114,
    111, 106, 101, 99, 116, 115, 0, 1, 7, 102, 105, 114, 101, 102, 111, 120, 0, 4, 0, 2, 7, 97,
    112, 112, 46, 101, 120, 101, 2, 11, 102, 105, 114, 101, 102, 111, 120, 46, 108, 110, 107, 1,
];

/// **v7 化の前に実際に書かれていた v6 バイト列**（`target_path` を実体で全件持つ形式）。
/// `config_hash` は 12345、entries は Firefox / Projects / docs の 3 件。
///
/// 末尾 3 バイト `0, 1` の前後は `LowerFileName` のタグである（割り当ては `GOLDEN_V7` の
/// doc が正本。v6 と v7 で同じであり、変えれば既存の `index.bin` を無言で誤読する）。
const GOLDEN_V6: &[u8] = &[
    73, 78, 68, 88, 6, 0, 0, 0, 128, 226, 207, 170, 6, 3, 7, 70, 105, 114, 101, 102, 111, 120, 19,
    67, 58, 92, 97, 112, 112, 115, 92, 102, 105, 114, 101, 102, 111, 120, 46, 108, 110, 107, 0, 8,
    80, 114, 111, 106, 101, 99, 116, 115, 11, 67, 58, 92, 80, 114, 111, 106, 101, 99, 116, 115, 1,
    4, 100, 111, 99, 115, 7, 67, 58, 92, 100, 111, 99, 115, 1, 185, 96, 3, 171, 1, 205, 1, 239, 1,
    3, 18, 52, 86, 3, 1, 7, 102, 105, 114, 101, 102, 111, 120, 1, 8, 112, 114, 111, 106, 101, 99,
    116, 115, 0, 3, 2, 11, 102, 105, 114, 101, 102, 111, 120, 46, 108, 110, 107, 0, 1,
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
    73, 78, 68, 88, 5, 0, 0, 0, 128, 226, 207, 170, 6, 2, 7, 70, 105, 114, 101, 102, 111, 120, 19,
    67, 58, 92, 97, 112, 112, 115, 92, 102, 105, 114, 101, 102, 111, 120, 46, 108, 110, 107, 0, 8,
    80, 114, 111, 106, 101, 99, 116, 115, 11, 67, 58, 92, 80, 114, 111, 106, 101, 99, 116, 115, 1,
    185, 96, 2, 171, 1, 205, 1, 2, 18, 52, 2, 7, 102, 105, 114, 101, 102, 111, 120, 8, 112, 114,
    111, 106, 101, 99, 116, 115, 2, 1, 11, 102, 105, 114, 101, 102, 111, 120, 46, 108, 110, 107, 0,
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
    73, 78, 68, 88, 4, 0, 0, 0, 128, 226, 207, 170, 6, 2, 7, 70, 105, 114, 101, 102, 111, 120, 19,
    67, 58, 92, 97, 112, 112, 115, 92, 102, 105, 114, 101, 102, 111, 120, 46, 108, 110, 107, 0, 8,
    80, 114, 111, 106, 101, 99, 116, 115, 11, 67, 58, 92, 80, 114, 111, 106, 101, 99, 116, 115, 1,
    185, 96, 2, 171, 1, 205, 1, 2, 18, 52, 2, 7, 102, 105, 114, 101, 102, 111, 120, 8, 112, 114,
    111, 106, 101, 99, 116, 115, 2, 1, 11, 102, 105, 114, 101, 102, 111, 120, 46, 108, 110, 107, 0,
    2, 19, 99, 58, 92, 97, 112, 112, 115, 92, 102, 105, 114, 101, 102, 111, 120, 46, 108, 110, 107,
    11, 99, 58, 92, 112, 114, 111, 106, 101, 99, 116, 115,
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

    let (_, returned) = save_cache_sorted_in(&dir, entries.clone(), config_hash, BuiltAt::Scanned);

    let result =
        load_cache_in(&dir, config_hash, LegacyUpgrade::Skip).expect("load cache written to dir");
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
        second.stats.scan,
        Duration::ZERO,
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
    let written =
        try_deserialize_with_header::<IndexCache<'static>>(&raw, INDEX_MAGIC, INDEX_CACHE_VERSION)
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

/// **旧版昇格の save 時間は `LoadCacheResult::upgrade_save` として見える化されている。**
///
/// 昇格 save（`upgrade_legacy_cache_in` → `save_cache_sorted_in`。旧版起動 1 回だけ発生する
/// `derive_columns` の再導出 + postcard シリアライズ + 数百 ms 級の書き込み）は、呼び出し元の
/// `cache_load` 計測区間の内側で起きる。運ばずに `None` を返すと、実際に save が起きた
/// 起動で「保存していない」という**偽の測定値**を `LoadOrScanStats::cache_save` が報告する
/// ことになる（`LoadOrScanStats` の doc「`cache_load` と `total` の間に処理を足すときは
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
fn load_cache_reports_upgrade_save_only_when_it_upgrades_a_legacy_format() {
    // `Write` は `INDEX_WRITE_LOCK` を取る（上のテストと同じ理由）。
    let _guard = INDEX_LOCK_TEST_GUARD
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // 旧版（v4）: 昇格が走るので save 時間が乗る。
    let legacy_dir = temp_dir("upgrade_save_legacy");
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
    // **速さではなく通ったかを見る**（`Some(Duration::ZERO)` も合格。理由はこのテストの doc）。
    assert!(
        legacy_result.upgrade_save.is_some(),
        "旧版を Write で読んだら昇格 save の枝を通ること（`None` は\
         `upgrade_legacy_cache_in` のクロージャを一度も通っていないことを意味する）"
    );

    // 現行版（v7）: 昇格しないので save 時間は乗らない。
    let current_dir = temp_dir("upgrade_save_current");
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
        sorted_by_path: derived_cols.sorted_prefix_len == derived_cols.names.len(),
        config_hash,
        char_masks: Cow::Borrowed(&derived.char_masks),
        file_name_char_masks: Cow::Borrowed(&derived.file_name_char_masks),
        lower_names: Cow::Borrowed(&derived.lower_names),
        lower_file_names: Cow::Borrowed(&derived.lower_file_names),
    };
    let bytes =
        try_serialize_with_header(INDEX_MAGIC, INDEX_CACHE_VERSION, &cache).expect("serialize");
    assert!(cache_bin_file_in(&current_dir).save_bytes(&bytes), "save");

    let current_result =
        load_cache_in(&current_dir, config_hash, LegacyUpgrade::Write).expect("v7 が読めること");
    assert_eq!(
        current_result.upgrade_save, None,
        "現行版は昇格しないので枝を通らないこと（`Some` は速さに関わらず\
         `upgrade_legacy_cache_in` を通ったことを意味する）"
    );

    let _ = fs::remove_dir_all(&legacy_dir);
    let _ = fs::remove_dir_all(&current_dir);
}

/// **`LoadOrScanStats::cache_save` レベルで固定する。** 上のテストは
/// `LoadCacheResult::upgrade_save`（`load_cache_in` の返り値）までしか見ておらず、
/// それを呼び出し元の `cache_save` へ運ぶ配線（`load_or_scan_with_stats_in` の
/// cache-hit 枝、`cache_save: result.upgrade_save`）自体は固定していない
/// ——**最終レビュー Important 1 の実際の欠陥はこの配線が `cache_save: 0` を
/// 焼き込んでいたことであり**、`upgrade_save` 単体の検知器はこの配線を落とす
/// 退行（`result.upgrade_save` を使わず `0` を書く）では落ちない。
#[test]
fn load_or_scan_with_stats_reports_upgrade_save_in_cache_save() {
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

    // 旧版（v4）: cache-hit しつつ昇格が走るので cache_save が非 ZERO になること。
    //
    // **ここを `LoadCacheResult` 側と同じ variant 判定へ替えることはできない**（#1054 /
    // #1063 で替えたのは向こうだけである）——`LoadOrScanStats::cache_save` は
    // 外向き計器で覗く variant を持たず、しかもこの assert が「配線が `result.upgrade_save`
    // を捨てて `ZERO` を焼き込む」退行を捕まえる唯一の検知器である。時間を見るのをやめると
    // 検知器が 1 つ減る。
    //
    // **20,000 件という規模は、もはやこの検知器の必要条件ではない**（#1178）。かつては
    // 判定が `as_millis()` の整数値だったため、昇格 save が 1 ms を切ると「配線は生きて
    // いるのに 0」で落ちた——1 件の治具では 8 回中 3 回落ちており、**時計を跨がせるのは
    // 閾値ではなく仕事量である**というのが規模の根拠だった。`Duration` は ns 分解能なので
    // その量子化は消えている。**規模を据え置いたのは、縮める判断が #1178 の範囲外だから
    // であって、跨がせる必要が残っているからではない。**
    let legacy_dir = temp_dir("stats_upgrade_save_legacy");
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
    // `cache_save > 0` にはなりうるが、それは cache-miss 枝の独立フェーズとしての
    // save であって、昇格 save の配線を固定したことにはならない——この assert が
    // 検知器の前提を保証する。
    assert!(
        result.stats.cache_hit,
        "config_hash を揃えたので cache-hit になること（miss だとこの検知器は無意味になる）"
    );
    assert!(
        result.stats.cache_save > Duration::ZERO,
        "cache-hit 枝で旧版昇格が走ったら LoadOrScanStats::cache_save が非 ZERO になること\
         （load_or_scan_with_stats_in の配線 result.upgrade_save を落とす退行の検知器）"
    );

    // 現行版（v7）: cache-hit だが昇格しないので cache_save は 0 のまま。
    let current_dir = temp_dir("stats_upgrade_save_current");
    let derived = derive_columns(entries);
    let derived_cols = derived.tree.columns();
    let cache = IndexCache {
        built_at: 1_700_000_000,
        names: Cow::Borrowed(derived_cols.names),
        is_folder: Cow::Borrowed(derived_cols.is_folder),
        parent: Cow::Borrowed(derived_cols.parent),
        aux: Cow::Borrowed(derived_cols.aux),
        table: Cow::Borrowed(derived_cols.table),
        sorted_by_path: derived_cols.sorted_prefix_len == derived_cols.names.len(),
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
        result.stats.cache_save,
        Duration::ZERO,
        "現行版は昇格しないので cache_save は ZERO のままであること"
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
        sorted_by_path: derived_cols.sorted_prefix_len == derived_cols.names.len(),
        config_hash,
        char_masks: Cow::Borrowed(&derived.char_masks),
        file_name_char_masks: Cow::Borrowed(&derived.file_name_char_masks),
        lower_names: Cow::Borrowed(&derived.lower_names),
        lower_file_names: Cow::Borrowed(&derived.lower_file_names),
    };
    let bytes =
        try_serialize_with_header(INDEX_MAGIC, INDEX_CACHE_VERSION, &cache).expect("serialize");
    assert!(cache_bin_file_in(&dir).save_bytes(&bytes), "save");

    let result = load_cache_in(&dir, config_hash, LegacyUpgrade::Write).expect("v7 が読めること");
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
    let v2_result = load_cache_in(&dir, config_hash, LegacyUpgrade::Skip).expect("v2 が読めること");
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
