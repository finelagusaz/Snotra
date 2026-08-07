//! 構築（`assemble` / 各コンストラクタ）の不変条件のテスト。
//!
//! 検索の結果は正しいまま余剰容量だけが常駐する、という失敗は**挙動テストで捕まらない**。
//! ここはその 1 点を守る。

use super::common::{make_entries, real_index_entries};
use crate::indexer::{AppEntry, CachedLower, LowerFileName};
use crate::search::*;

/// `len` より大きい容量を持つ Vec を作る（`index.bin` 由来の Vec が持つ余剰の再現）。
/// `with_capacity` 後の `extend` は再確保しないため、容量はそのまま残る。
fn oversized<T>(items: Vec<T>) -> Vec<T> {
    let mut v = Vec::with_capacity(items.len() * 4 + 64);
    v.extend(items);
    v
}

fn owned(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

/// 余剰容量は「検索は正しいが常駐だけが増える」形の劣化ゆえ、挙動テストでは捕まらない。
/// `assemble` の `shrink_to_fit` が消えたらここで落ちる。
///
/// 経路は `new_with_cached_masks` の v4 ヒット枝を選ぶ——`Vec<String>` → `Vec<Box<str>>` の
/// 変換が確保ブロックを再利用して余剰を持ち越すため、実運用で余剰が最も乗る経路である。
#[test]
fn assemble_shrinks_parallel_vecs_to_fit() {
    let names = ["Firefox", "Chrome", "Notepad"];
    let lower = ["firefox", "chrome", "notepad"];
    let entries: Vec<AppEntry> = oversized(make_entries(&names));
    let n = entries.len();
    assert!(
        entries.capacity() > n,
        "fixture の前提が崩れている: 余剰容量のある Vec を渡せていない"
    );

    let engine = SearchEngine::new_with_cached_masks(
        entries,
        oversized(vec![0u64; n]),
        oversized(vec![0u64; n]),
        // **`Raw`（v5/v4 相当）で渡す。** 余剰容量が最も乗るのはこの経路である
        // ——`Vec<String>` → `Vec<Box<str>>` の変換が確保ブロックを再利用して持ち越す。
        Some(CachedLower::Raw {
            lower_names: oversized(owned(&lower)),
            lower_file_names: oversized(lower.iter().map(|s| Some(s.to_string())).collect()),
        }),
        true, // migemo 有効 = kana 系 2 本も構築される
    );

    // `Vec::shrink_to_fit` の契約は「len へできる限り近づける」であり厳密一致ではないが、
    // std の `RawVec::shrink` は要求サイズちょうどで capacity を張り直す。ここが落ちたら
    // `shrink_to_fit` が消えたか、アロケータの契約が変わったかのどちらかである。
    let actual = [
        ("entries", engine.entries.capacity()),
        ("lower_names", engine.lower_names.capacity()),
        ("lower_file_names", engine.lower_file_names.capacity()),
        ("char_masks", engine.char_masks.capacity()),
        (
            "file_name_char_masks",
            engine.file_name_char_masks.capacity(),
        ),
        ("kana_lower_names", engine.kana_lower_names.capacity()),
        ("kana_char_masks", engine.kana_char_masks.capacity()),
    ];
    for (label, capacity) in actual {
        assert_eq!(capacity, n, "{label} に余剰容量が残っている（len = {n}）");
    }
}

/// **`Collapsed`（v6 キャッシュ）経路を通す唯一の検知器。**
///
/// 他の旗の検知器（`shared_file_name_flag_is_measured_not_inferred_from_is_folder` /
/// `shared_lower_name_chain_collapses_both_links`）は**どちらも `Measured` 経路**を通るので、
/// v6 の展開が壊れてもこの 2 本は緑のままである。ここが守るのは
/// **`LowerFileName` の 3 状態が `entry_view` の読み替えへ正しく着地すること**——とくに
/// `Absent` が「file name 成分が無い」のまま残り、`SameAsLowerName` と混ざらないこと。
///
/// **測り直しの有無はここでは測れない**（測り直しても結果は変わらない・`DerivedStrings` の
/// doc）。索引の内部表現（`is_none` と旗）まで見るのは、**検索結果が正しいまま削減だけが
/// 失われる**形の退行を捕まえるためである。
#[test]
fn collapsed_cache_is_not_remeasured_and_absent_file_names_stay_absent() {
    let entries = vec![
        // 1. file name 成分が**無い**（`Absent`）かつ `lower_name` も潰れている。
        //    **素朴な `None == None` 比較で旗が立つのはこのエントリである。**
        AppEntry {
            name: "docs".to_string(),
            target_path: "C:\\docs".to_string(),
            is_folder: true,
        },
        // 2. file name が `lower_name` と同一（`SameAsLowerName`）。旗が立つべき。
        AppEntry {
            name: "same".to_string(),
            target_path: "C:\\real\\same".to_string(),
            is_folder: true,
        },
        // 3. file name が独自の文字列（`Text`）。
        AppEntry {
            name: "firefox".to_string(),
            target_path: "C:\\apps\\firefox.lnk".to_string(),
            is_folder: false,
        },
    ];
    let n = entries.len();

    let engine = SearchEngine::new_with_cached_masks(
        entries,
        vec![0u64; n],
        vec![0u64; n],
        Some(CachedLower::Collapsed {
            // すべて `None` = `name` と同一（3 件とも既に小文字）。
            lower_names: vec![None, None, None],
            lower_file_names: vec![
                LowerFileName::Absent,
                LowerFileName::SameAsLowerName,
                LowerFileName::Text("firefox.lnk".to_string()),
            ],
        }),
        false,
    );

    // 1: **旗が立ってはならない。** ここが `Some("docs")` になる実装が、この PR が
    // 「結果を壊しうる唯一の点」と名指したものである。
    let absent = engine.entry_view(0);
    assert_eq!(absent.lower_name, "docs");
    assert_eq!(
        absent.lower_file_name, None,
        "`Absent` は「`lower_name` と同一」ではない——測り直す実装だとここが Some になる"
    );
    assert!(!absent.entry.file_name_is_lower_name);

    // 2: 旗が立ち、`lower_name` へ解決される。
    let shared = engine.entry_view(1);
    assert_eq!(shared.lower_file_name, Some("same"));
    assert!(shared.entry.file_name_is_lower_name);
    assert!(
        engine.lower_file_names[1].is_none(),
        "共有するエントリの文字列は落ちていなければ削減にならない"
    );

    // 3: 独自の文字列がそのまま残る。
    let text = engine.entry_view(2);
    assert_eq!(text.lower_file_name, Some("firefox.lnk"));
    assert!(!text.entry.file_name_is_lower_name);

    // 記録側の潰し方がそのまま索引の表現になっている（測り直していれば `Some` が復活する）。
    assert!(
        engine.lower_names.iter().all(|n| n.is_none()),
        "`None` = name と同一。展開で実体を作り直してはならない"
    );
}

/// **`lower_file_name` の共有は `is_folder` からの推論ではなく、測った結果である。**
///
/// 実データでは folder の 100%（255,961/255,961）が `lower_file_name == lower_name` になるが、
/// それは indexer が folder の `name` に `file_name()` を使う規則の帰結であって、
/// `SearchEngine::new` が受け取る `AppEntry` の性質ではない。`is_folder` で分岐する実装に
/// 差し替えたら、下の 1 件目が `Some("tail")` ではなく `Some("alias")` を返して落ちる。
///
/// 2 件目は成立する側で、旗が立って `lower_file_names` から文字列が落ちることを見る
/// ——**両方を置く**のは、片方だけだと「常に共有しない」実装も「常に共有する」実装も
/// 通ってしまうためである。
///
/// **migemo の両設定を通す。** kana 系 2 本は `assemble` の**外**で確定してから渡されるので
/// 潰す位置の不変条件は同じはずだが、実運用の config が片方に寄っている以上（計測環境は
/// `migemo_enabled = false`）、通していない側は「壊れても気づかない側」である。
#[test]
fn shared_file_name_flag_is_measured_not_inferred_from_is_folder() {
    let entries = vec![
        // name が末尾成分と一致しない folder。indexer は作らないが API は受け取れる。
        AppEntry {
            name: "alias".to_string(),
            target_path: "C:\\real\\tail".to_string(),
            is_folder: true,
        },
        // 一致する folder（実データの姿）。
        AppEntry {
            name: "same".to_string(),
            target_path: "C:\\real\\same".to_string(),
            is_folder: true,
        },
    ];

    for migemo_enabled in [false, true] {
        let engine = SearchEngine::new_with_migemo(entries.clone(), migemo_enabled);

        let mismatched = engine.entry_view(0);
        assert_eq!(mismatched.lower_name, "alias", "migemo={migemo_enabled}");
        assert_eq!(
            mismatched.lower_file_name,
            Some("tail"),
            "migemo={migemo_enabled}: 末尾成分と name が違う folder で共有してはならない"
        );
        assert!(!mismatched.entry.file_name_is_lower_name);
        assert!(
            engine.lower_file_names[0].is_some(),
            "migemo={migemo_enabled}: 共有しないエントリの文字列を落としてはならない"
        );

        let shared = engine.entry_view(1);
        assert_eq!(
            shared.lower_file_name,
            Some("same"),
            "migemo={migemo_enabled}"
        );
        assert!(shared.entry.file_name_is_lower_name);
        assert!(
            engine.lower_file_names[1].is_none(),
            "migemo={migemo_enabled}: 共有するエントリの `Box<str>` は落ちていなければ削減にならない"
        );
    }
}

/// **共有は鎖になっている**: `lower_file_names[i]` → `lower_names[i]` → `entries[i].name`。
/// 両方の輪が同時に外れるエントリ（既に小文字の folder）で、`entry_view` が鎖を上から解いて
/// 元のバイトを返すことを固定する。
///
/// **鎖の順序を誤ると削減だけが減り、結果は正しいまま**である——`assemble` が先に
/// `lower_names` を潰すと file name 側の比較相手が消えて共有を取りこぼすが、`entry_view` は
/// どちらでも正しい値を返すので**挙動だけを見るテストでは捕まらない**。ゆえに索引が実際に
/// 文字列を落としたか（`is_none`）まで見る。
#[test]
fn shared_lower_name_chain_collapses_both_links() {
    let entries = vec![
        // 既に小文字の folder: `lower_name == name` かつ `lower_file_name == lower_name`。
        AppEntry {
            name: "same".to_string(),
            target_path: "C:\\real\\same".to_string(),
            is_folder: true,
        },
        // 大文字始まりの folder: `lower_name != name` だが `lower_file_name == lower_name`。
        AppEntry {
            name: "Alias".to_string(),
            target_path: "C:\\real\\Alias".to_string(),
            is_folder: true,
        },
    ];

    for migemo_enabled in [false, true] {
        let engine = SearchEngine::new_with_migemo(entries.clone(), migemo_enabled);

        let both = engine.entry_view(0);
        assert_eq!(both.lower_name, "same", "migemo={migemo_enabled}");
        assert_eq!(
            both.lower_file_name,
            Some("same"),
            "migemo={migemo_enabled}"
        );
        assert!(
            engine.lower_names[0].is_none(),
            "migemo={migemo_enabled}: name と同一なら鎖の上段が落ちる"
        );
        assert!(
            engine.lower_file_names[0].is_none(),
            "migemo={migemo_enabled}: 上段が落ちても下段は落ちなければならない（順序の検知器）"
        );

        let folded = engine.entry_view(1);
        assert_eq!(folded.lower_name, "alias", "migemo={migemo_enabled}");
        assert_eq!(
            folded.lower_file_name,
            Some("alias"),
            "migemo={migemo_enabled}"
        );
        assert!(
            engine.lower_names[1].is_some(),
            "migemo={migemo_enabled}: 小文字化で変わる名前は自前の文字列を持つ"
        );
        assert!(
            engine.lower_file_names[1].is_none(),
            "migemo={migemo_enabled}: 上段が残っていても下段は共有できる"
        );
    }
}

/// 共有による読み替えが、潰す前の導出と 1 バイトも違わないことを実インデックスの全件で確かめる。
///
/// **共有は最適化であって意味の変更ではない。** `entry_view` が返す 2 つの文字列は、索引が
/// それを持っていようといまいと導出（`to_lower_folded` / `query::lower_file_name`）と
/// 一致しなければならない——ずれると `lower_name` はタイブレークの並びが、`lower_file_name` は
/// `has_dot` クエリの拡張子マッチが**静かに**変わる（クラッシュせず、順位だけが動く）。
/// 実インデックスが無ければ自動スキップする corpus であり、機構としての保証は上の合成
/// fixture のほうが持つ（`real_index_entries` の doc）。
#[test]
fn entry_view_shared_strings_match_derivation_over_real_index() {
    let Some(entries) = real_index_entries() else {
        println!("実インデックスが無いためスキップします。");
        return;
    };
    // `new_with_migemo` は entries を消費するので、期待値は先に導出しておく。
    let expected: Vec<(String, Option<String>)> = entries
        .iter()
        .map(|e| {
            (
                crate::query::to_lower_folded(&e.name),
                crate::query::lower_file_name(&e.target_path),
            )
        })
        .collect();
    let engine = SearchEngine::new_with_migemo(entries, false);

    let (mut shared_name, mut shared_file_name) = (0usize, 0usize);
    for (i, (want_name, want_file_name)) in expected.iter().enumerate() {
        let view = engine.entry_view(i);
        assert_eq!(
            view.lower_name, want_name,
            "index {i} で lower_name の読み替えが導出とずれている"
        );
        assert_eq!(
            view.lower_file_name,
            want_file_name.as_deref(),
            "index {i} で lower_file_name の読み替えが導出とずれている"
        );
        shared_name += usize::from(engine.lower_names[i].is_none());
        shared_file_name += usize::from(view.entry.file_name_is_lower_name);
    }
    let pct = |k: usize| k as f64 * 100.0 / expected.len().max(1) as f64;
    println!(
        "{} 件で読み替えが導出と一致しました（lower_name の共有 {shared_name} 件・{:.1}% / \
         lower_file_name の共有 {shared_file_name} 件・{:.1}%）。",
        expected.len(),
        pct(shared_name),
        pct(shared_file_name),
    );
}
