---
paths:
  - "snotra-core/**/*.rs"
---

# snotra-core ルール

詳細は `snotra-core/CLAUDE.md` を参照。

- **SearchEngine フィールド追加時は 5 箇所同時更新**: (1) struct (2) `new()` Wave 1/2 (3) `new_with_cached_masks()` v4+v3 パス (4) `EntryView` + `entry_view()` (5) `debug_assert!`。並列 Vec レイアウトの struct 化は禁止（ベンチマーク劣化確認済み）
- **normalize_entry_key を変更したら 3 点確認**: 新規記録（`record_launch` 等）・既存データ移行（`migrate_normalize_keys`）・ルックアップ API の 3 者が揃っているか
- **ビットマスク二重定義**: `indexer.rs::char_bitmask_for_cache()` と `search.rs::char_bitmask()` は同一ロジック必須。片方だけ変えるとキャッシュヒット/ミスで結果が変わる
- **incremental cache 変更時は `/cache-check`** で単調性を検証する
- **UI 表示文字列を持たない**: エラーは `is_error: true` フラグで伝え、表示は UI 層の責務
