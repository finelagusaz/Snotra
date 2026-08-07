//! 構築（`assemble` / 各コンストラクタ）の不変条件のテスト。
//!
//! 検索の結果は正しいまま余剰容量だけが常駐する、という失敗は**挙動テストで捕まらない**。
//! ここはその 1 点を守る。

use super::common::make_entries;
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
