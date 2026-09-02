# research — issue #761: /plan-review に「文書としての plan.md」レンズを常設するか

調査日: 2026-09-02 ／ ブランチ: `chore/close-761-plan-doc-lens-adr`

## issue の要約

#749（PR #759）の計画は当時の `/plan-review` 標準構成（レイヤー別スカウト 3 + 独立導出 1）を通ったが、ユーザー指示で追加した 4 レンズ（内部矛盾 / 節間の覆い / 責務分割 MECE / 実行可能性）が、標準構成が 1 件も挙げなかった不整合を 15 件超出した。issue はこのレンズを常設するかの**検討**であり、スキル変更はチームの共有物ゆえ合意が要る。考えどころは 4 つ——(1) 常設の費用、(2) 4 レンズ全部が要るか・内部矛盾は機械化できるか、(3) 常設するなら #754 の配送規約に載せる、(4) 常設しないならレンズの定義をどこかへ残す。

**ユーザー裁定（2026-09-02・本セッション）**: 「常設しない・ADR で閉じる」。

## 事実（一次資料つき）

### F1. issue が比較対象にした標準構成はもう存在しない

- `ADR-risk-tiered-plan-review`（#849・2026-07-28 頃）が「すべての issue でレイヤー別スカウトと独立導出を起動する」を却下。現行 `.claude/skills/plan-review/SKILL.md` は通常リスク = 主エージェントの自己照合 7 項目（委譲 0 体）、高リスク = Step 2（計画準拠 1 体）**または** Step 2b（独立導出 1 体）の**どちらか一方**
- 帰結: 4 レンズの常設は「スカウト 4 + レンズ 4 で倍」（issue の見積もり）ではなく、通常リスクで **0 → 4 体**になる。同 ADR の却下案と同じ形

### F2. 4 レンズが見つけた所見の型のうち 2 型は、別の形で既に受け皿を持つ

| issue の表の型 | 現在の受け皿 | 根拠 |
|---|---|---|
| 実行可能性（Phase 単独でビルドが通らない） | `/plan-review` Step 1 項目 6「タスク分割の境界が既存トリガーを跨いでいない（`-D warnings` 下で中間状態が `dead_code` で落ちる）」 | #914（2026-08-04 のトランスクリプト横断分析が根拠。#749 ではなく #755/#801 の手戻りから導入） |
| 内部矛盾（数の腐り） | `AGENTS.md`「検証の作法（全タスク共通）」の「数え上げは偽になる時点が確定している——数ではなく正本を指す」。書く側の規範 | 同節。issue 考えどころ 2 の「機械化」は、計画に数を書かない規範の下では対象を失う |
| 節間の覆い（宣言と手順の分離） | **無し**。Step 1 項目 1 は issue 要件 ↔ 作業項目の対応を見るが、計画内の節どうし（変更ファイル一覧 ↔ Phase 手順）は見ない | `SKILL.md` Step 1 の 7 項目を通読 |
| 責務分割 MECE（設計判断の欠落） | **無し**。ただし発生条件は高リスク判定「複数モジュール間のインターフェースを新設・変更する」に含まれ、Step 2 は「該当するリスク観点だけを渡す」形で観点を固定していない | 同 SKILL.md「リスク判定」「Step 2」 |

### F3. レンズの定義は失われていない

- 4 本の成果物は #759 のマージコミット `5ef346f` の `workspace/plan-review/{consistency-internal,consistency-executability,mece-responsibility,mece-sections}.md` に在る（`git show 5ef346f:workspace/plan-review/<name>.md`）。各ファイル冒頭にレンズの定義（「文書内の記述同士の食い違いのみ」「上から順に実行したとき指示が一意に定まるか」「分割後の責務が相互排他かつ網羅か」「節間の覆い」）が書かれている
- #762（`90f67c2`）で `workspace/` ごと削除された。同コミットは `RETROSPECTIVE.md` に「常設するかは #761 で判断する」と書き、`docs/development-principles.md` へは #749 の別の教訓 2 件（移設で腐る所在の散文・検査の期待値の SPEC 照合）を置いた。**レンズ定義そのものは生きた層のどこにも無い**
- issue の考えどころ 3（Lens D が API エラーで落ちても成果物が完走）は `/start-issue` 3b・`/plan-review` Step 2/2b が「呼び出し側が指定したパスへ書かせる」を既に持つ（ルート `CLAUDE.md`「サブエージェント委譲と worktree」）

### F4. 「文書を成果物として読む枠」は research.md 側には既に在る

- `/start-issue` Step 3b「敵対的調査」（#1068）は `research.md` の全主張を母集団に、壊せた項目と壊せなかった項目を両方宣言させる。対象は `research.md` であって `plan.md` ではない

### F5. 「義務を足す方向」への否定の知識が積み上がっている

- `ADR-retire-norm-review`: 敵対的読者の指摘は対象の質と無関係に増え続け、規範専用の枠を新設すると同じ仕事に 2 つの起動条件ができる
- `ADR-two-class-reader-discrimination` / `ADR-check-skill-skeleton`: 義務を足す方向の塞ぎは忠実な読者の実行可能性を削る（4 回中 4 回）
- ガバナンス実績調査（2026-08-03・メモリ `project_governance_efficacy_audit`）: 効いている核は `/plan-review` Step 2b。4 レンズの捕捉実績は #749 の 1 サイクルのみ

### F6. ADR の運用契約

- ADR は `docs/adr/ADR-<slug>.md`、否定の知識が生じた決定のみ、連番を振らない（`AGENTS.md`「ドキュメント参照」・`.claude/rules/governance-docs.md`）
- ADR は凍結された歴史（`ADR-adr-frozen-history`）。既存 ADR（`ADR-risk-tiered-plan-review`）へ追記しない
- **被参照ゼロの ADR は削除の対象になった前例がある**（#895・2026-08-03: 全拡張子 grep で自分以外のヒット無しの 6 本を削除）。G-adr-citations は「引用が実在の ADR を指すか」だけを見るので、被引用ゼロ自体は赤にならない
- `ADR-risk-tiered-plan-review` を引くのは superseded 済みの `docs/superpowers/specs/2026-07-28-plan-review-loop-design.md` 冒頭の注記だけ。#895 の grep 基準ではこれで「参照あり」になる

### F7. 計器分割の設計書が #761 を別の意味で引いている

- `docs/superpowers/specs/2026-07-29-plan-review-instrument-split-design.md`（status: Draft・#846）は #761 を「**`plan.md` の機械可読性**」と読み替え、§5 問い #2 と §7 手順 3 を「spike が仮説を支持した場合のみ #761 へ進む」としている。spike（呼び出し元 grep の道具化）は実施されていない（`scripts/` に該当ツール無し・`git log --all --grep=761` は #846 のみ）
- `docs/superpowers/` は歴史資料（#589 で非規範化）で `governance:check` の照合母集団外。同ディレクトリの先例として、loop-design 設計書は冒頭に「Superseded」注記を持つ

## 関連ファイル・モジュール

- `.claude/skills/plan-review/SKILL.md`（Step 1 項目 6・リスク判定・Step 2・Step 2b）——**読むだけ・触らない**
- `docs/adr/ADR-risk-tiered-plan-review.md`・`ADR-retire-norm-review.md`・`ADR-adr-frozen-history.md`——読むだけ
- `docs/adr/ADR-<新 slug>.md`——**新設**
- `docs/superpowers/specs/2026-07-29-plan-review-instrument-split-design.md`——冒頭注記の追加候補（未確定 U2）
- `RETROSPECTIVE.md`——本サイクルでは触らない（`/retrospective` の管轄）

## 再利用できる既存パターン

- ADR の書式: `ADR-retire-norm-review.md`（「廃止の根拠（散在していた実測をここへ集約する）」→「却下 n」→「受容する残余」→「関連」）
- 歴史資料への注記: `2026-07-28-plan-review-loop-design.md` 冒頭の `> **Superseded:** …` 1 行

## 技術的制約

- ADR 本文から生きた層への参照は照合されない（凍結）。生きた層 → ADR の短縮引用 `ADR-<slug>` は G-adr-citations が実在を照合する
- ADR 内で見出しを正準形で書くと G-heading-refs は ADR を走査元から外しているので照合されないが、`.claude/rules/governance-docs.md`「既に消滅した節の名前を正準形で書かない」は当たる。旧構成（スカウト 3 + 独立導出 1）を語るときはバッククォートを外して散文にする
- `*.md` の編集には PostToolUse 検査が走らない。`npm run governance:check`（カテゴリ F）を手で回す
- ADR 名は連番禁止（G-adr-file-names）

## 未解決の疑問

- **U1. ADR を生きた層のどこから引くか。** 引かなければ #895 型の掃除で消えうる（F6）。候補: (a) `/plan-review` SKILL.md リスク判定の末尾へ根拠引用 1 句——スキル変更ゆえ合意が要る、(b) 計器分割設計書の冒頭注記（U2 と同じ 1 行で兼ねる）——歴史資料への注記は先例あり、`docs/superpowers/` は G-adr-citations の母集団外だが #895 の grep 基準では参照として数えられる、(c) 引かない——削除リスクを受容
- **U2. 計器分割設計書の「#761 に依存」をどう扱うか。** #761 が閉じると §7 手順 3 の指し先が「常設しない」の決定になり、機械可読性の問いが宙に浮く。注記 1 行で「#761 は文書レンズの常設を扱い ADR で閉じた。機械可読性の問いは spike が支持した時点で別 issue を起こす」と書くか、触らないか
- **U3. ADR に 4 レンズの定義を写すか、`5ef346f` へのポインタで足りるか。** 写せば ADR 内で自己完結するが 4 段落増える。ポインタなら短いが、読者は `git show` を要する

## 敵対的調査（3b）の所見と採否

`workspace/adversarial-761.txt`（sonnet 1 体）。壊せた 0 / 壊せなかった 7 / ⚠ 3。測定環境（HEAD `35f02ae`・`5ef346f`・`90f67c2` の実在と diff）も一致を確認した。

| ⚠ | 所見 | 採否 | 理由 |
|---|---|---|---|
| 争点 3 | 4 ファイルの定義文言の逐語確認は未実施 | 採らない | 主エージェントが `git show 5ef346f:workspace/plan-review/<name>.md` の冒頭 14 行を 4 本とも読み、定義を確認済み（F3 の根拠はその実読） |
| 争点 7 | issue 本文は `docs/adr/0008-window-coordinator-split-rule.md` という旧番号形式を指し、実ファイルは `ADR-window-coordinator-split-rule.md` のみ | 採る | 新 ADR は現行 slug で引く。issue の腐りは凍結された歴史として触らない |
| 争点 2 | `ADR-window-coordinator-split-rule` に責務分割 MECE レビューの成果（規則 R）が着地している | 採る | 「レンズの定義は生きた層に無い」（F3）は保つが、**レンズの成果は 1 件、生きた層の ADR に残っている**と書き分ける。新 ADR の経緯で引く |
