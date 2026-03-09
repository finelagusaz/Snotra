---
name: snotra-search-review
description: "Snotra の検索変更をレビューする。incremental search、migemo/kana、slash command、instant command、ランキング、query parsing、入力状態遷移が関係するときに使う。"
---

検索変更は文字列マッチではなく状態遷移としてレビューする。

最初にこの不変条件を確認する。
- 増分検索を使ってよいのは、今回の一致条件が前回条件の部分集合であるときだけ

次のような「候補集合を広げうる変化」があるかを見る。
- 閾値跨ぎで migemo が無効から有効になる
- 前回は `kana_query` がなく、今回初めて生成される
- 通常検索、slash command、instant command の間でモードが切り替わる
- 設定変更で active predicate が変わる

候補集合が広がりうるなら、コード側で安全性を証明しない限り前回候補の再利用を認めない。

レビュー時は次を読む。
- `references/search-invariants.md`
- `references/search-state-transitions.md`
- `references/search-test-heuristics.md`

所見を書くときは次を含める。
- 壊れた不変条件を 1 文で書く
- 具体的な入力遷移を 1 つ出す
- なぜ前回候補が不十分か、または stale かを書く
- 想定される false negative / false positive を書く
