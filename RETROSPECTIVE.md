# Retrospective — fire-and-forget Promise の silent catch 解消

## よかったこと

### research → plan → implement の3段階分離が有効だった
先に全体調査で14箇所を分類（要対処/許容/良好）し、計画で「変更しない」箇所と根拠を明示してから実装に入った。不要な変更を避けつつ、副作用リスクの事前検証（`.catch()` が `.then()` や `onCleanup` に干渉しないこと）も計画段階で完了しており、実装時の手戻りはゼロだった。

### レビューエージェントが計画の漏れを補完した
当初7箇所の計画に対し、レビューで同スコープ内の2箇所（`emit("results-render-done")`, `api.notifyResultHovered()`）の漏れを検出。research の網羅性不足をレビューが補う構造が機能した。

---

## 伸びしろ

### research の検索が機能単位に偏り、構文パターンの網羅性が不足した
listen 登録パターンに集中して検索したため、同ファイル内の別用途（emit, API 呼び出し）で同じ `void ... ` + `.catch()` なしのパターンを見落とした。「パターン」を検索する際は、関数名ではなく構文パターン（`void` で始まり `.catch` がない行）で検索すべきだった。

---

## ネクストアクション

- [ ] `fix/silent-catch-cleanup` ブランチをプッシュし PR を作成
