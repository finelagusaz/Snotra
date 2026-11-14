---
paths:
  - "snotra-egui-runtime/**/*.rs"
---

# snotra-egui-runtime ルール（ルーター）

事実の正本は `snotra-egui-runtime/CLAUDE.md`「不変条件」とコードである。この rule は「どこを読むか」だけを示す（要約は置かない）。

## トリガー → 検査

- 並行境界（repaint の配送・`WindowWaker`・イベントループの外からの呼び出し・共有状態）を追加/変更したら: `/race-check`（述語の全文の SSOT は、常時ロードされる `AGENTS.md`「条件別チェック」表である。ここに写しは置かない）
