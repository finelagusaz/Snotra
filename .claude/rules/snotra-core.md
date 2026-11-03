---
paths:
  - "snotra-core/**/*.rs"
---

# snotra-core ルール（ルーター）

事実の正本は `snotra-core/CLAUDE.md` とコードである。この rule は「どこを読むか・何を実行するか」だけを示す（要約は置かない）。`search.rs` に固有のことは `snotra-core-search.md` にある。位置はファイル名で決めつけず、見出し名やシンボル名で grep して探す（refactor で移動することがあるため・#588）。

## 読む正本（`snotra-core/CLAUDE.md` の該当節）

- `SearchEngine` に並列 Vec のフィールドを追加する: 「実装前チェック（必須）」
- `normalize_entry_key` を変更する: 「`normalize_entry_key` の冪等性契約」と「history.rs のキー正規化に関するチェックリスト」
- 文字ビットマスクの導出を変更する: 「`char_bitmask` は `query.rs` に一元化済み」
- `index.bin` を書く経路を新しく追加する: 「index.bin 書き込みの排他」
- 索引を更新するきっかけや、形式の昇格に触れる: 「indexer.rs の索引更新の契機」
- `scan_all` の重複排除に触れる: 「`scan_all` の重複排除」
- UI に表示する文字列（`is_error` フラグ）: 「開発ルール」

## トリガー → 検査

- incremental cache・`prev_*`・キャッシュの再利用: 述語が単調かを確かめる（`snotra-core-search.md` の項を見る）
- 設定キー・永続形式・識別子/キー形式の変更: `/persistence-check`（後方互換を確かめる）
- `Ord` / `Reverse` / `BinaryHeap`: 実装の前に「先頭が最良/最悪か」を一文で書き、入力の順序で結果が変わらないことのテストを置く（「実装前チェック」）
- 挙動を変えない最適化: `AGENTS.md`「事前調査」に従い、代表的な入出力をベースラインにして差分を確かめる
