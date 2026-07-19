---
paths:
  - "snotra-core/**/*.rs"
---

# snotra-core ルール（ルーター）

事実の正本は `snotra-core/CLAUDE.md` とコード。本 rule は「どこを読むか・何を実行するか」だけを示す（要約を置かない）。`search.rs` 固有は `snotra-core-search.md`。位置はファイル名で断定せず**見出し名・シンボル名で grep** して辿る（refactor で移動しうる・#588）。

## 読む正本（`snotra-core/CLAUDE.md` の該当節）

- `SearchEngine` に並列 Vec フィールドを追加: 「実装前チェック（必須）」
- `normalize_entry_key` を変更: 「`normalize_entry_key` の冪等性契約」+「history.rs のキー正規化に関するチェックリスト」
- 文字ビットマスクの導出を変更: 「`char_bitmask` は `query.rs` に一元化済み」
- `index.bin` を書く新経路を追加: 「index.bin 書き込みの排他」
- 背景再スキャン・`icons.bin` に触れる: 「indexer.rs の背景再スキャン」
- UI 表示文字列（`is_error` フラグ）: 「開発ルール」

## 引き金 → 検査

- incremental cache・`prev_*`・キャッシュ再利用: `/cache-check`（単調性）
- 設定キー・永続形式・識別子/キー形式の変更: `/persistence-check`（後方互換）
- `Ord` / `Reverse` / `BinaryHeap`: 「先頭が最良/最悪か」を実装前に一文で明示し入力順不変テストを置く（「実装前チェック」）
- 挙動を変えない最適化: `AGENTS.md`「事前調査」で代表入出力をベースライン差分検証
