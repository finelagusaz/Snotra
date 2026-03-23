---
paths:
  - "snotra-core/src/search.rs"
---

# search.rs ルール

変更後は `/cache-check` で incremental 単調性を検証する。

- **スコア階層は不変**: Prefix(10000) > Substring(5000) > Kana(4500) > Path(3000) > Fuzzy(nucleo)。テスト `kana_search_direct_match_ranks_above_kana_match` で検証
- **BinaryHeap の Ord は逆順**: `peek()` = worst（スコア最小）。`into_sorted_vec()` は昇順で best が先頭
- **新マッチパス追加時**: ビットマスク pre-filter との OR 関係を確認。非 ASCII は `u64::MAX` で常にパスする前提
- **`new()` 変更 → `new_with_cached_masks()` も同時更新**: v4 パス（Wave 1 スキップ）と v3 フォールバック（Wave 1 再計算）の両方
- **`has_dot` 変更 → incremental ガードも見直す**: file_name スコアリング条件と `!has_dot || self.prev_query.contains('.')` は連動している
- **`has_path_sep` 変更 → incremental ガードも見直す**: パスマッチ条件と `(!has_path_sep || self.prev_query.contains('\\') || self.prev_query.contains('/'))` は連動している
