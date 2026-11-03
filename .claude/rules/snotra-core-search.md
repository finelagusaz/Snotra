---
paths:
  - "snotra-core/src/search.rs"
  - "snotra-core/src/search/**/*.rs"
---

# search.rs ルール（ルーター）

事実の正本はコードにある。この rule は「どこを読むか・何を実行するか」だけを示す（要約のコピーは置かない）。

## 読む正本

- 責務・スコア階層・全順序の強制: `search.rs` の `//!`、`SearchEngine` の struct doc、`mod score_tier` の doc（直後にある `const _` の全順序アサーションを含む。位置は下の注記を参照）
- incremental の述語と、前回の状態の扱い: `IncrementalCache`（`can_reuse` の read と `update` の write を対で持つ・#601）、`score_one_entry` / `heap_into_results` の doc、`search_with_options` 本体のコメント

> 上のシンボルが今ある場所: search.rs は責務ごとに `search/{build,query_plan,scoring}.rs` へ分割してある（#598〜#600）。`mod score_tier` / `score_one_entry` / `heap_into_results` は `search/scoring.rs`、`compute_wave1/2` は `search/build.rs`、`prepare_query_plan` は `search/query_plan.rs` にある（#600 で実測）。位置をファイル名で決めつけないという規律は、`snotra-core.md` に書いてある（search.rs を編集すると同時に配送される）。
- 横断する不変条件（並列 Vec のレイアウト・ビットマスクの一元化・has_path_sep との非互換）: `snotra-core/CLAUDE.md`「モジュール構成」の `search.rs` 行、`snotra-core/CLAUDE.md`「`char_bitmask` は `query.rs` に一元化済み」、`snotra-core/CLAUDE.md`「incremental cache とパスクエリの非互換」

## トリガー → 検査

- incremental・キャッシュの再利用・前回の状態（`IncrementalCache`）・`has_dot`/`has_path_sep` に触れたら、正本（`IncrementalCache::can_reuse` の doc）を読む。再利用の述語ごとに、単調であるか（true から false の向きにしか変わらないか）を確かめる。述語や状態を足すときは、`can_reuse`（read）と `update`（write）を対で変更する（#894 で旧スキルから移した）。
- 新しいマッチ種別を追加するときは、bitmask pre-filter と矛盾しないかを、`compute_wave2` の doc（false-negative の不変条件）で確かめる。
- Wave 1/2 や kana の構築に触れたら、`compute_wave1` / `new_with_cached_masks` の doc（#337）を読む。
- `Ord` / `BinaryHeap` / top-k に触れたら、`snotra-core/CLAUDE.md`「実装前チェック」の規律に従う（先頭が最良か最悪かを明記する。入力の順序で結果が変わらないことをテストする）。
