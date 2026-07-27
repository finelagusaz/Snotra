# process-gates

## 問題なし
- `gh pr create` の push 済み判定（`hasSafeChain`/`GIT_PUSH`/`readGitState`）は計画の Global Constraint・Task4 Step7 の記述と一致 — 根拠: `.claude/hooks/pre-bash.mjs:104-120,338-346`
- `workspace/plan.md` 未チェック `- [ ]` 拒否（`readPlanState`/`countUnchecked`）は計画の Global Constraint・Task4 Step3 と整合 — 根拠: `.claude/hooks/pre-bash.mjs:369-403`
- bash heredoc 拒否（win32 限定 `usesHeredoc`）は計画が一貫して Write ツール一時ファイル + `-F`/`--body-file` を使う方針と一致 — 根拠: `.claude/hooks/pre-bash.mjs:157-170,288-298`
- 鎖に `cd` を含めると拒否（`CWD_CHANGE`）は Task4 Step7 の注記と一致 — 根拠: `.claude/hooks/pre-bash.mjs:76,324-327`
- Task1 Step1 の ADR 番号確定手順（`ls docs/adr/` → 0011 が空き番号）は実測と一致 — 根拠: `ls docs/adr/` の出力（最終ファイル `0010-implement-step4-report-slot.md`）
- Task2 Step1 の想定基準値 `rules 7956/8056` は現在の実測と完全一致 — 根拠: `npm run governance:check` 出力「rules 7956/8056 字」
- `AREA_BUDGET.rules` の定義位置・値は Task2 の Files 節と一致 — 根拠: `scripts/governance-check.mjs:604`（`{ alwaysLoaded: 13374, rules: 8056 }`）
- Task2 Step4 が引用するコメント末尾文言「上げるときに要るのは我慢ではなく理由であり…」は原文と完全一致 — 根拠: `scripts/governance-check.mjs:601-602`
- Task2 Step2 が引用する safety-nets.md の挿入アンカー文言は原文と完全一致 — 根拠: `.claude/rules/safety-nets.md`「セーフティネットが「規範」…」節
- ADR 本文が参照する見出し（`AGENTS.md`「条件別チェック（トリガー → 参照先）」・`/implement`「4a. check スキルの実行」・`docs/development-principles.md`「構造的設計原則と強制の階梯」）は全て実在し文言も一致 — 根拠: `AGENTS.md:48`, `.claude/skills/implement/SKILL.md:102`, `docs/development-principles.md:66`
- Task3 の停止条件・2 巡手順・件数の数え直し（`grep -cE '^ *[0-9]+\. '`）・降格記録・列挙ではなく原理で塞ぐ方針は `/norm-review` SKILL.md の Step1〜4・出力節と完全に整合 — 根拠: `.claude/skills/norm-review/SKILL.md` 全体
- 作業ブランチと先行コミットの実在（Global Constraint 1）は実測と一致 — 根拠: `git branch --show-current` = `chore/check-skill-skeleton-design`、`git log --oneline -1` 先頭 = `11f533d docs(specs): check 系スキルの共通骨格を…`
- `#781` が現在 `OPEN` であることは Task4 Step9 の期待値・Global Constraint 5 の前提と一致 — 根拠: `gh issue view 781 --json state` → `OPEN`
- `#781` のタイトルが `/` で始まる（`/race-check Step 1 の先行欠陥 20 件…`）ことは Global Constraint 9 の「#781 のタイトルで実測」の裏取りと整合 — 根拠: `gh issue view 781 --json title`
- Task4 Step3・Step4（`plan.md` 全項目 `[x]` 確認後に `workspace/` 削除、否定の知識は Task1 の ADR-0011 で回収済みにつき追加 ADR 不要）は `/implement`「Step 5 — コミット」の削除条件と一致 — 根拠: `.claude/skills/implement/SKILL.md:116-117`
- セルフレビュー節の「4 Task・26 ステップ」の主張は実カウントと一致（Task1:4 + Task2:6 + Task3:7 + Task4:9 = 26） — 根拠: `workspace/plan.md` の `- [ ]` 行数

## 軽微な懸念
- Global Constraint「`.claude/skills/plan-review/SKILL.md` に他セッションの未コミット変更がある」は現時点の `git status`/`git diff HEAD` では確認できない（差分なし）。既に他セッションがコミット済みか、記述が当初から不正確だった可能性がある。計画はこのファイルを一切触らないため実害は無いが、根拠は現時点で裏取りできない — 根拠: `git diff --stat HEAD -- .claude/skills/plan-review/` が空出力、`git status --porcelain -- .claude/skills/` も空出力
- Task4 Step8・9（`closingIssuesReferences` 確認・`#781` OPEN 確認）は PR 作成直後に一度だけ実行される。ルート `CLAUDE.md`「Git/GitHub 運用」手順1は「マージ直前に」の実行を要求するが、本計画は `gh pr merge` を実行せず PR 作成で終わるため、実際のマージ時に手順1を再実行する責務は本計画の外（将来のマージ実行者）に残る。Global Constraint は「手順1〜4」を包括的に参照するが、本計画が満たすのは実質手順1相当のみで、手順3・4（マージ時の closing keyword 回避・マージ後 3 点確認）は計画の範囲外である旨が明示されていない — 根拠: `workspace/plan.md:17,324-332` と ルート `CLAUDE.md`「Git/GitHub 運用」手順1〜4の記述

## 要対処
（無し）

## 未検証（理由）
- 「先頭が `/` の文字列を `gh` の引数へ渡すと Bash（MSYS）がパス変換で壊す」という現象そのものの再現実験 — 理由: `#781` のタイトルが実際に `/` で始まることは確認したが、副作用を避けるため実際に `gh` へ渡して破損を再現する検証は行っていない
- Task3（骨格自体への `/norm-review` 実施結果）の中身の妥当性・ADR-0011 本文の技術的正確性（`/race-check` 61 件・`/symmetric-check` 41 件などの実測値の裏取り） — 理由: 本レイヤー（process-gates）のスコープ外（内容レビューは別スカウトの担当と判断した）
