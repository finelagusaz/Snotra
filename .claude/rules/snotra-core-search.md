---
paths:
  - "snotra-core/src/search.rs"
---

# search.rs 変更時の追加チェック

- search.rs を変更した場合は `/cache-check` で incremental 単調性を検証してください
- **スコア階層の維持**: ベーススコアは Prefix(10000) > Substring(5000) > kana(4500) > Fuzzy(nucleo) の順序不変条件を持つ。スコア定数やスコア計算を変更するとき、この階層が崩れないか確認する（テスト: `kana_search_direct_match_ranks_above_kana_match`）
- **ビットマスク pre-filter と新マッチパスの相互作用**: Fuzzy モードの `char_masks` / `file_name_char_masks` で候補を刈った後に kana 等の代替マッチパスを試みる（OR 関係）。新しいマッチパスを追加するとき、ビットマスクがその候補を誤って除外しないか確認する（非 ASCII エントリは `u64::MAX` で回避している前提）
- **`new_with_cached_masks` と `new()` の構造的同期**: `new()` にフィールドを追加・変更した場合は `new_with_cached_masks` の両パス（v4 キャッシュヒット / v3 フォールバック）も同時に更新する。`debug_assert!` が両メソッドで同一条件であることを確認する
- **`has_dot` 分岐と incremental ガードの連動**: `has_dot` は file_name スコアリングの有効化と incremental cache の no-dot→dot ガード（`!has_dot || self.prev_query.contains('.')`）の両方に影響する。file_name マッチの条件を変更するときは incremental ガードの前提も見直す
- **`thread_local MATCHER` の利用**: `Matcher::new()` は `alloc_zeroed`（数KB）を伴う。スコアリングで `Matcher` を使う場合は `MATCHER` thread_local 経由で再利用し、`fold` クロージャ内での毎回生成を避ける
