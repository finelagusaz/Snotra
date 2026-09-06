---
paths:
  - "snotra-core/src/search.rs"
  - "snotra-core/src/search/**/*.rs"
---

# search.rs ルール（ルーター・#588 試行中）

事実の正本はコード側にある。本 rule は「どこを読むか・何を実行するか」だけを示す（要約コピーを置かない）。

## 読む正本

- 責務・スコア階層・全順序の強制: `search.rs` の `//!`、`SearchEngine` の struct doc、`mod score_tier` の doc（直後の `const _` 全順序アサーション含む。位置は下の注記参照）
- incremental 述語と前回状態の規律: `IncrementalCache`（`can_reuse` の read / `update` の write を対に持つ・#601）・`score_one_entry` / `heap_into_results` の doc と `search_with_options` 本体のコメント

> **上記シンボルの現在地**（位置をファイル名で断定しない規律そのものは、search.rs の編集で同時に配送される `snotra-core.md` が持つ）: search.rs は責務単位で `search/{build,query_plan,scoring}.rs` へ分割済み（#598〜#600）——`mod score_tier` / `score_one_entry` / `heap_into_results` は `search/scoring.rs`、`compute_wave1/2` は `search/build.rs`、`prepare_query_plan` は `search/query_plan.rs`（#600 実測）。
- 横断不変条件（並列 Vec レイアウト・ビットマスク一元化・has_path_sep 非互換）: `snotra-core/CLAUDE.md`「モジュール構成」の `search.rs` 行・`snotra-core/CLAUDE.md`「`char_bitmask` は `query.rs` に一元化済み」・`snotra-core/CLAUDE.md`「incremental cache とパスクエリの非互換」

## トリガー → 検査

- **incremental・キャッシュ再利用・前回状態（`IncrementalCache`）・`has_dot`/`has_path_sep` に触れたら**: 正本（`IncrementalCache::can_reuse` の doc）を読み、再利用述語ごとに**単調性**（true → false の向きにしか転ばないか）を確認する。述語や状態を足すときは `can_reuse`（read）と `update`（write）を対で変更する（#894 で旧スキルから吸収）
- **新しいマッチ種別を追加するなら**: bitmask pre-filter との整合を `compute_wave2` の doc（false-negative 不変条件）で確認する
- **Wave 1/2・kana 構築に触れたら**: `compute_wave1` / `new_with_cached_masks` の doc（#337）を読む
- **`Ord` / `BinaryHeap` / top-k に触れたら**: `snotra-core/CLAUDE.md`「実装前チェック」の規律（先頭が最良/最悪の明記・入力順不変テスト）に従う
