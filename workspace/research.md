# research: issue #518 — AGENTS.md「環境制約」節の解体

## issue の要約

「環境制約」節（AGENTS.md:85-91）は性質の異なる5項目が混在し、「Claude Code 固有は CLAUDE.md」という責務宣言と矛盾する。#515 の「引き金の地図」流儀に合わせ、本文を CLAUDE.md へ移し、AGENTS.md には条件別チェック表の引き金1行を残して節を廃止する。

## 関連コード（参照側の数え上げ・真実源 grep 済み）

- **移動元**: AGENTS.md:85-91（節見出し + 5項目。うち worktree cleanup は #517 で1行化済み）
- **節名「環境制約」への参照**: `CLAUDE.md:5` の1箇所のみ（「ワークフロー・事前チェック・環境制約」）→ 文言更新が必要
- **ルーティングの既存参照**: `.claude/skills/retrospective/SKILL.md:65` が「委譲/worktree 運用 → ルート CLAUDE.md」と**既に移動先を指している**（現状は実体が AGENTS.md にあり腐っていた）→ 移動で整合、SKILL.md 側の変更不要
- **内容の重複**: `.claude/skills/health-check/SKILL.md:78,94,100` と `retrospective/SKILL.md:120` に「委譲はコンテキスト非継承・メモリパス明示」の具体運用があるが、これらは Check 7 固有の手順であり一般規則の写しではない（SSOT は移動後の CLAUDE.md 節が一般則、SKILL 側が個別適用）

## 既存パターン

- CLAUDE.md は「太字 = 守る指示、後続 = 理由・過去の事故」形式。移動時にこの形式へ揃える（現状の AGENTS.md 記述はほぼこの形式）
- 条件別チェック表は「引き金（この変更・局面に来たら）| 参照先」の2列

## 技術的制約

- `.claude/rules/governance-docs.md`（自動配送済み）: 節の削除は構造改変 → 参照側を名前・序数の両方で数え上げる（済み: 上記2箇所のみ。「環境制約」は序数参照なし）。PostToolUse hook は AGENTS.md / CLAUDE.md に検査を割り当てない → 完全性は独立再導出で裏取り（/plan-review Step 2b）
- `*.md` 編集は hook 沈黙 =「何も走らなかった」。検証は移動前後の命題対応表 + plan-review

## 未解決の疑問

なし。行き先は issue の分類表 + retrospective SKILL.md の既存ルーティングで一意（CLAUDE.md）。
