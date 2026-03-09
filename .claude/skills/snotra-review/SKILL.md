---
name: snotra-review
description: "Snotra の PR・設計・バグ修正をレビューする。snotra-core、src-tauri、ui、snotra-settings をまたぐ変更、Windows 固有機能、状態遷移、回帰リスクを確認するときに使う。"
---

Snotra の変更を `正しさ -> 回帰 -> 保守性` の順でレビューする。

最初に、どのサブシステムが変わったかを分類する。
- 検索とランキング
- ウィンドウ、ホットキー、トレイのライフサイクル
- フォルダ展開状態
- 起動結果と通知のライフサイクル
- config 反映
- 多言語対応
- 子プロセス管理
- Windows 固有統合

まず次の不変条件を確認する。
- 業務ロジックを `main.rs` に増やさない
- UI 表示文字列を `snotra-core` に持ち込まない
- リスナーとウィンドウ準備後に有効化する
- 子プロセス spawn には exit 時 kill を必ず対にする
- 一時状態のタイマーや購読は単一ライフサイクルで管理する

レビュー前に、一般観点として次を読む。
- `references/review-checklist.md`
- `references/architecture-boundaries.md`
- `references/high-risk-areas.md`

検索変更が含まれる場合は、次も読む。
- `../snotra-search-review/references/search-invariants.md`
- `../snotra-search-review/references/search-state-transitions.md`
- `../snotra-search-review/references/search-test-heuristics.md`

所見を書くときは次を守る。
- バグ、回帰、テスト不足を優先する
- 壊れた不変条件を 1 文で明示する
- ユーザーに見える失敗モードを書く
- それが `正しさ` `ライフサイクル` `責務境界` のどれかを明示する
