//! 構築（`assemble` / 各コンストラクタ）の不変条件のテスト。
//!
//! 検索の結果は正しいまま余剰容量だけが常駐する、という失敗は**挙動テストで捕まらない**。
//! ここはその 1 点を守る。

use super::common::{make_entries, real_index_entries};
use crate::indexer::AppEntry;
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
        Some(oversized(owned(&lower))),
        Some(oversized(
            lower.iter().map(|s| Some(s.to_string())).collect(),
        )),
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

/// 旗による読み替えが、潰す前の導出と 1 バイトも違わないことを実インデックスの全件で確かめる。
///
/// **共有は最適化であって意味の変更ではない。** `entry_view` が返す `lower_file_name` は、
/// 索引がその文字列を持っていようといまいと `query::lower_file_name` の導出と一致しなければ
/// ならない——ずれると `has_dot` のクエリで拡張子マッチが静かに変わる（クラッシュせず、
/// 順位だけが動く）。実インデックスが無ければ自動スキップする corpus であり、機構としての
/// 保証は上の合成 fixture のほうが持つ（`real_index_entries` の doc）。
#[test]
fn entry_view_lower_file_name_matches_derivation_over_real_index() {
    let Some(entries) = real_index_entries() else {
        println!("実インデックスが無いためスキップします。");
        return;
    };
    // `new_with_migemo` は entries を消費するので、期待値は先に導出しておく。
    let expected: Vec<Option<String>> = entries
        .iter()
        .map(|e| crate::query::lower_file_name(&e.target_path))
        .collect();
    let engine = SearchEngine::new_with_migemo(entries, false);

    let mut shared = 0usize;
    for (i, want) in expected.iter().enumerate() {
        let view = engine.entry_view(i);
        assert_eq!(
            view.lower_file_name,
            want.as_deref(),
            "index {i} で読み替えが導出とずれている"
        );
        shared += usize::from(view.entry.file_name_is_lower_name);
    }
    println!(
        "{} 件で読み替えが導出と一致しました（共有 {shared} 件・{:.1}%）。",
        expected.len(),
        shared as f64 * 100.0 / expected.len().max(1) as f64,
    );
}
