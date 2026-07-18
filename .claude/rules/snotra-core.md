---
paths:
  - "snotra-core/**/*.rs"
---

# snotra-core ルール

詳細は `snotra-core/CLAUDE.md` を参照。

- **SearchEngine フィールド追加時は 5 箇所同時更新**: (1) struct (2) `compute_wave1` / `compute_wave2` (3) `new_with_cached_masks()` v4 パス (4) `EntryView` + `entry_view()` (5) `assemble()` 内の `debug_assert!`。並列 Vec レイアウトの struct 化は禁止（ベンチマーク劣化確認済み）
- **normalize_entry_key を変更したら 3 点確認**: 新規記録（`record_launch` 等）・既存データ移行（`migrate_normalize_keys`）・ルックアップ API の 3 者が揃っているか
- **ビットマスク導出は `query.rs` に一元化済み**: 文字マスクを使う側（`search.rs` / `indexer.rs`）は query.rs のヘルパー経由で得る（`char_bitmask` を直接、または `name_char_mask` 等のラッパ越しに間接的に）。導出ロジックを変える時に触るのは query.rs の1箇所
- **incremental cache 変更時は `/cache-check`** で単調性を検証する
- **`index.bin` を書く新経路は `INDEX_WRITE_LOCK` 経由**: `with_index_write_lock`（権威的書き手・blocking）/ `try_with_index_write_lock`（背景再スキャン・try_lock）。`save_cache_sorted` 自身はロックを取らない（呼び出し側が保持）
- **`icons.bin` に触れない**: アイコンキャッシュは `src-tauri` の資源。背景再スキャンは `RescanOutcome` で結果を伝え、無効化は呼び出し側が行う
- **UI 表示文字列を持たない**: エラーは `is_error: true` フラグで伝え、表示は UI 層の責務
- **設定・コンフィグ変更は後方互換を確認する**: デフォルト値の変更・キー追加・フォーマット変更は、新規インストールだけでなく既存の `config.toml` でも正しく動作するか確認する（互換性の裏取りは `/persistence-check`）
- **比較関数 + データ構造は「先頭要素が最良/最悪どちらか」を一文で明示する**（事前調査。`BinaryHeap`/`Ord` 変更時）
- **挙動を変えない最適化は「意味を変えない不変条件」を箇条書きで定義する**（事前調査）
