---
paths:
  - "snotra-core/src/search.rs"
---

# search.rs ルール（ルーター・#588 試行中）

事実の正準はコード側にある。本 rule は「どこを読むか・何を撃つか」だけを示す（要約コピーを置かない）。

## 読む正準

- 責務・スコア階層・全順序の強制: `search.rs` の `//!`、`SearchEngine` の struct doc、`mod score_tier` の doc（直後の `const _` 全順序アサーション含む。位置は下の注記参照）
- incremental 述語と `prev_*` の規律: `decide_incremental` / `score_one_entry` / `heap_into_results` の doc と `search_with_options` 本体のコメント

> **位置はファイル名で断定せず、シンボル名で grep して現在地を確認する。** search.rs は責務単位で `search/{build,query_plan,scoring}.rs` へ分割済み（#598〜#600）で、上記シンボルは移動しうる（例: `mod score_tier` / `score_one_entry` / `heap_into_results` は `search/scoring.rs`、`compute_wave1/2` は `search/build.rs`、`prepare_query_plan` は `search/query_plan.rs`）。名前参照は refactor を生き延びるが、`<file> の X` という位置断定は腐る（#600 実測）。
- 横断不変条件（並列 Vec レイアウト・ビットマスク一元化・has_path_sep 非互換）: `snotra-core/CLAUDE.md` の search.rs 節・「文字ビットマスク」節・「incremental cache とパスクエリの非互換」節

## 引き金 → 検査

- **incremental・キャッシュ再利用・`prev_*`・`has_dot`/`has_path_sep` に触れたら**: 正準（`decide_incremental` の doc）を読み、`/cache-check` で単調性を検証する
- **新しいマッチ種別を追加するなら**: bitmask pre-filter との整合を `compute_wave2` の doc（false-negative 不変条件）で確認する
- **Wave 1/2・kana 構築に触れたら**: `compute_wave1` / `new_with_cached_masks` の doc（#337）を読む
- **`Ord` / `BinaryHeap` / top-k に触れたら**: `snotra-core/CLAUDE.md` 実装前チェックの規律（先頭が最良/最悪の明記・入力順不変テスト）に従う
