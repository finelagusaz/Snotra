---
paths:
  - "snotra-core/**/*.rs"
---

# snotra-core 実装前チェック（必須）

- 共通原則はルート `CLAUDE.md` の「レビュー未然防止の事前調査（必須）」に従う
- `search.rs` で `Ord` / `Reverse` / `BinaryHeap` を扱う変更では、`BinaryHeap` の先頭が最良/最悪のどちらかを実装前に明記する
- `search.rs` の top-k 更新ロジックを変更する場合は、入力順を変えても結果が不変であるテストを追加または更新する
- `SearchEngine` に新しい並列 Vec フィールドを追加するとき: `EntryView` 構造体・`entry_view()` メソッド・`new()` 末尾の `debug_assert!` を同時に更新し、全 Vec 長の同期を保つ（書き込み側 `new()` と読み取り側 `entry_view()` は常にペアで更新する）
- `search.rs` の incremental search キャッシュ（`prev_*` フィールド群）に新しい述語を追加するとき: `use_incremental` の条件式と `prev_*` の更新箇所を同時に変更し、`/cache-check` で単調性を検証する
- `query.rs` の正規化を変更する場合は、タブ・全角スペース・NBSP を `' '` に統一するテストと冪等性テストを追加または更新する
