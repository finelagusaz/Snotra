---
paths:
  - "snotra-core/src/search.rs"
---

# search.rs ルール（ルーター・#588 試行中）

事実の正準はコード側にある。本 rule は「どこを読むか・何を撃つか」だけを示す（要約コピーを置かない）。

## 読む正準

- 責務・スコア階層・全順序の強制: `search.rs` の `//!`・`mod score_tier` の doc（直後の `const _` アサーション含む）・`SearchEngine` の struct doc
- incremental 述語と `prev_*` の規律: `decide_incremental` / `score_one_entry` / `heap_into_results` の doc と `search_with_options` 本体のコメント
- 横断不変条件（並列 Vec レイアウト・ビットマスク一元化・has_path_sep 非互換）: `snotra-core/CLAUDE.md` の search.rs 節・「文字ビットマスク」節・「incremental cache とパスクエリの非互換」節

## 引き金 → 検査

- **incremental・キャッシュ再利用・`prev_*`・`has_dot`/`has_path_sep` に触れたら**: 正準（`decide_incremental` の doc）を読み、`/cache-check` で単調性を検証する
- **新しいマッチ種別を追加するなら**: bitmask pre-filter との整合を `compute_wave2` の doc（false-negative 不変条件）で確認する
- **Wave 1/2・kana 構築に触れたら**: `compute_wave1` / `new_with_cached_masks` の doc（#337）を読む
- **`Ord` / `BinaryHeap` / top-k に触れたら**: `snotra-core/CLAUDE.md` 実装前チェックの規律（先頭が最良/最悪の明記・入力順不変テスト）に従う
