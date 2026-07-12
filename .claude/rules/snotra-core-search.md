---
paths:
  - "snotra-core/src/search.rs"
---

# search.rs ルール

変更後は `/cache-check` で incremental 単調性を検証する。

- **スコア階層は不変**: Prefix > Substring > Kana > Path > Fuzzy(nucleo)。基準スコアは `mod score_tier`（`PREFIX_BASE`/`SUBSTRING_BASE`/`KANA_BASE`/`PATH_BASE`）に集約済み。基準定数の全順序は直後の `const _` コンパイル時アサーションが全ビルドで強制し、実行時の全順序（位置ペナルティ込み）はテスト `kana_search_direct_match_ranks_above_kana_match` で検証。**`9000`（file_name scoring 短絡閾値）は score_tier ではない**（`PREFIX_BASE` と混同しない）
- **BinaryHeap の Ord は逆順**: `peek()` = worst（スコア最小）。`into_sorted_vec()` は昇順で best が先頭
- **新マッチパス追加時**: ビットマスク pre-filter との OR 関係を確認。非 ASCII は `u64::MAX` で常にパスする前提
- **Wave 1/2 変更 → `compute_wave1` / `compute_wave2` ヘルパーを更新**: `new()` / `new_with_migemo()` / `new_with_cached_masks()` が共有。v4 パスの kana 再計算は `new_with_cached_masks()` 内に残る
- **`kana_lower_names` は migemo 有効時のみ構築（issue #337）**: `compute_wave1(.., migemo_enabled)` と `new_with_cached_masks` の v4/v3 両パスで条件化。空 Vec のとき長さ `{0, entries.len()}`、`assemble` の debug_assert は kana のみ緩和、検索ループは `kana_available` 空ガード必須。migemo は index 構築入力なので、engine の `IndexInputs`（`config_watcher` の kick 判定と `complete_index_drain` の re-diff が共有する単一定義）に含める（#347 Phase 2 で `needs_reindex` / in-flight `needs_rebuild` を統合・削除済み）
- **search_with_options の分割構造（#436）**: 候補準備 `prepare_query_plan`（→ `QueryPlan`）／incremental 可否 `decide_incremental`（`prev_*` は read のみ）／1 エントリ採点 `score_one_entry`（`#[inline]`・bitmask pre-filter を先頭固定）／結果組立 `heap_into_results`（top-k 確定後に K 件だけ所有 clone する。`ScoredEntry<'a>` は SearchEngine から借用）。`prev_*` の write は orchestrator が `heap_into_results` による所有変換の**後**に行う（heap が self を借用しているため）
- **`has_dot` 変更 → incremental ガードも見直す**: `score_one_entry` の file_name スコアリング条件と `decide_incremental` の `!has_dot || prev_query.contains('.')` は連動している
- **`has_path_sep` 変更 → incremental ガードも見直す**: `score_one_entry` のパスマッチ条件と `decide_incremental` の `!has_path_sep` ガードは連動している
