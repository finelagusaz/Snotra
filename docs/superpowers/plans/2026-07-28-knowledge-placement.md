# ナレッジ配置基準と規範散文の退避 — 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 器の分類基準（二軸判定＋常時ロード面の不変条件）を `/retrospective` に補完し、その検算として `CLAUDE.md` の squash マージ 4 手順を新スキル `/merge-pr` へ移設する。

**Architecture:** 新文書種は作らない。第一軸（いつ効くか）は `/retrospective`「Step 2」「Step 3」に既存のため、欠けている第二軸（内容種別）と不変条件だけを追記する。手順移設は「スキル新設 → CLAUDE.md 圧縮 → 参照元更新」の 3 点を同一ブランチで行い、governance-check（G-skill-table・G-heading-refs）と独立再導出で裏取りする。

**Tech Stack:** Markdown・`npm run governance:check`・`/norm-review`（フォールトインジェクション）

## Global Constraints

- ブランチ: `chore/knowledge-placement-design`（設計書コミット済み）。main へ直接コミットしない
- 他文書の見出し参照は正準形 `` `<対象>`「<見出し>」 ``（`.claude/rules/governance-docs.md`）
- 各 Task 完了時に `npm run governance:check` を実行してからコミットする（`*.md` は PostToolUse 検査が走らない＝沈黙は「何も走らなかった」）
- `docs/superpowers/plans/` `specs/` 配下の過去文書は履歴であり更新しない
- 教訓本文を 2 箇所に書かない。移設は「移動」であって「複製」ではない

---

### Task 1: `/merge-pr` スキル新設

**Files:**
- Create: `.claude/skills/merge-pr/SKILL.md`

**Interfaces:**
- Produces: スキル名 `merge-pr`（Task 2 のスキル表・CLAUDE.md 参照、Task 3 の ADR 参照が使う）

- [ ] **Step 1: SKILL.md を作成する**

`CLAUDE.md:16-28` の手順本文を移設する。以下の全文で作成する（手順 1〜4 と締めの段落は現行 CLAUDE.md からの逐語移設。導入文のみスキル向けに書き換え）:

````markdown
---
name: merge-pr
description: "PR を squash マージするときに使用。issue auto-close の誤爆を防ぐ、マージ直前の closingIssuesReferences 確認・本文編集・マージ後 3 点検証の手順。"
disable-model-invocation: true
argument-hint: "[PR 番号]"
allowed-tools:
  - Bash(gh *)
  - Read
  - Write
---

PR を squash マージする。マージで閉じる issue を決めるのは PR 本文であり、`gh pr merge` の `--subject` / `--body-file` では抑止できない（#488 実測）。auto-close は本文の**どこにあっても** `close`/`fix`/`resolve` 系 9 形（大文字小文字問わず・表やチェックリスト内も）でマージ時に走り、PR テンプレートが `Closes` を埋めるため**書いた覚えが無くても残る**。hook も見ていない（ルート `CLAUDE.md`「フック」の (A2)）。**だから下の手順が唯一の防御である**。なぜこの機構になるか（2 経路の可視性の非対称・マージ方式では逃げられない・squash 設定と復元レシピ)は `docs/adr/ADR-squash-merge-issue-autoclose.md`。

対象: $ARGUMENTS

手順（squash マージでは常にこの順。`<PR>` は PR 番号、`<issue>` は issue 番号）:

1. **マージ直前に** `gh pr view <PR> --json closingIssuesReferences` を**必ず**見る。これが GitHub の計算した「いま閉じる issue」である
2. 一覧に閉じたくない issue があれば **PR 本文を編集して手順 1 を実行し直す**（`gh pr edit <PR> --body-file <tmp>`）。**一覧から消えるまで繰り返す。** どの行のどの語が効いたかを推測しない — 認識されるのは `close/closes/closed` `fix/fixes/fixed` `resolve/resolves/resolved` の 9 形で大文字小文字を問わず、表やチェックリストの中の行も、1 行に同居する複数の参照も効く。**編集を終えてよいと決めるのは一覧であって、自分のキーワード走査ではない**。マージ時の `--subject` / `--body-file` では止められない
3. `--subject` / `--body-file` は squash commit のメッセージを整えるためだけに使う。**closing keyword を書いてはならない**（散文の "partially fixes #N" も効く）— 書くと手順 1 の一覧に現れないまま閉じる。省けば squash 本文は **PR 説明文そのもの**になる（表・チェックリスト込みで冗長）
4. マージ後に**必ず**、次の 3 つを確認する。**`closingIssuesReferences` を数えるだけでは足りない** — それは PR 本文からその瞬間に再計算される値であって、閉じた事実そのものではない:
   - 取り直した `gh pr view <PR> --json closingIssuesReferences` の全件が意図どおり閉じたか
   - **残すと決めた issue が今も `OPEN` か**（`gh issue view <issue> --json state`）。正しく動いていればそれらは上の一覧に現れない。**ゆえに一覧を数えるだけでは、守りたい当の issue を一度も見ないことになる**
   - `gh issue list --state closed --search "closed:>=<mergedAt>"`（`mergedAt` は `gh pr view <PR> --json mergedAt`）。どちらの一覧にも属さない「知らないうちに閉じた issue」を拾う、唯一の接地した観測点
   誤って閉じていたら `gh issue reopen <issue>`（close イベントは履歴に残り、close を契機に動く下流は巻き戻らない。**reopen は回復であって、事前確認を省く免罪符ではない**）

**手順 1 の一覧が「閉じる issue のすべて」になるのは、手順 3 を守り、かつ確認からマージまで PR 本文が変わらなかったときだけである。** 本文を凍結する機構は無く、`gh pr merge --auto` は確認とマージを引き離すため**使わない**。
````

- [ ] **Step 2: governance:check を実行する**

Run: `npm run governance:check`
Expected: PASS（この時点でスキル表は未更新だが、G-skill-table は表→スキルの実在方向と disable-model-invocation スキル→表の方向を見る。**赤になった場合は Task 2 のスキル表追加と同一コミットに束ねる**）

- [ ] **Step 3: コミット**（governance:check が Task 2 を要求した場合は Task 2 と同一コミット）

```bash
git add .claude/skills/merge-pr/SKILL.md
git commit -m "feat(skills): squash マージ手順を /merge-pr スキルへ移設"
```

### Task 2: `CLAUDE.md` の圧縮とスキル表更新

**Files:**
- Modify: `CLAUDE.md:16-28`（Git/GitHub 運用の手順本文）、`CLAUDE.md:39`（(A2) の「上の手順 3」）、`CLAUDE.md:68-73`（スキル表）

**Interfaces:**
- Consumes: Task 1 のスキル名 `merge-pr`

- [ ] **Step 1: 手順本文（16〜28 行）を 1 項目へ置き換える**

現行の「マージで閉じる issue を決めるのは…」から「…`gh pr merge --auto` は…使わない。」まで（箇条書き 1 項目＋ネスト手順全体）を削除し、次の 1 項目へ置き換える:

```markdown
- **squash マージは `/merge-pr` の手順で行う** — マージで閉じる issue を決めるのは PR 本文であり、`gh pr merge` の `--subject` / `--body-file` では抑止できない（#488 実測）。手順の全文は `/merge-pr`、機構の理由は `docs/adr/ADR-squash-merge-issue-autoclose.md`。`gh pr merge --auto` は確認とマージを引き離すため**使わない**
```

- [ ] **Step 2: (A2) の内部参照を更新する**

`CLAUDE.md:39` の「残余は上の手順 3 に委ねられる」を「残余は `/merge-pr` 手順 3 に委ねられる」へ変更する。

- [ ] **Step 3: スキル表へ 1 行追加する**

「利用できるスキル」表の `/health-check` 行の上に追加:

```markdown
| `/merge-pr`          | PR の squash マージ実行（closingIssuesReferences 確認 → 本文編集 → マージ後 3 点検証） |
```

- [ ] **Step 4: governance:check 実行 → コミット**

Run: `npm run governance:check`
Expected: PASS

```bash
git add CLAUDE.md
git commit -m "docs(claude-md): squash マージ手順を /merge-pr への参照へ圧縮"
```

### Task 3: 参照元の更新（ADR）

**Files:**
- Modify: `docs/adr/ADR-squash-merge-issue-autoclose.md:27`

- [ ] **Step 1: 「手順の全文は `CLAUDE.md`「Git/GitHub 運用」。」を「手順の全文は `/merge-pr`。」へ変更する**（33 行目の「関連:」の `CLAUDE.md`「Git/GitHub 運用」は見出しが存続するため変更不要だが、`/merge-pr` を関連へ追記する）

- [ ] **Step 2: 残存参照の検算**

Run: `grep -rn "手順 1" --include="*.md" CLAUDE.md AGENTS.md docs/adr .claude/rules .claude/skills .claude/agents CONTRIBUTING.md`
Expected: 移設済み手順への参照が `/merge-pr` 経由以外に残らない（`docs/superpowers/` の履歴文書は対象外）

- [ ] **Step 3: governance:check 実行 → コミット**

```bash
git add docs/adr/ADR-squash-merge-issue-autoclose.md
git commit -m "docs(adr): squash マージ手順の所在を /merge-pr へ更新"
```

### Task 4: `/retrospective` へ第二軸と不変条件を補完、`/norm-review` から参照

**Files:**
- Modify: `.claude/skills/retrospective/SKILL.md`（Step 3 の階梯）
- Modify: `.claude/skills/norm-review/SKILL.md`（導入部）

- [ ] **Step 1: retrospective Step 3 の階梯 1 の直後（項 1 と項 2 の間）へ第二軸の項を挿入する**

```markdown
1.5. **内容の種別で器が決まるものを先に振り分ける（第二軸）**: **手順**（ステップ列として実行できる）→ スキル（規範文書に手順本文を書かない）。**否定の知識**（なぜ B を却下したか）→ `docs/adr/ADR-<slug>.md`。**失敗の一次証跡**（何が起きたか）→ GitHub issue（`RETROSPECTIVE.md` は揮発でよい）。**意図（仕様）** → `SPEC.md`
```

番号体裁は編集時に既存リストへ合わせる（`1.5.` が Markdown で崩れるなら項 2 として挿入し以降を繰り下げる。**以降の項番号を繰り下げた場合、項番号を引く他文書が無いか grep する**——`Step 3` への参照は見出し単位なので影響しない）。

- [ ] **Step 2: retrospective Step 3 の「検収条件」段落の先頭へ不変条件を追記する**

```markdown
**不変条件**: 常時ロード面（ルート `CLAUDE.md` / `AGENTS.md`）に置いてよいのは「トリガー＋参照＋根拠 issue 番号」まで。手順の本文を置いてはならない（手順はスキルへ・#488 の 4 手順を `/merge-pr` へ移設した検算が先例）。
```

- [ ] **Step 3: norm-review 導入部の「対象: $ARGUMENTS」の直後へ 1 行追記する**

```markdown
新設する条項の**置き場**が適切かは `/retrospective`「Step 3 — 教訓の配置（トリガーに括り付ける）」の判定フローで先に確かめる（本スキルは置かれた条項が効くかだけを測る）。
```

- [ ] **Step 4: governance:check 実行 → コミット**

```bash
git add .claude/skills/retrospective/SKILL.md .claude/skills/norm-review/SKILL.md
git commit -m "feat(skills): 教訓配置の第二軸と常時ロード面の不変条件を /retrospective へ補完"
```

### Task 5: 裏取り（独立再導出＋norm-review）

**Files:**
- 検証のみ（修正が出た場合は該当ファイルへ）

- [ ] **Step 1: 移設の独立再導出**

`git show main:CLAUDE.md` の 16〜28 行を SSOT とし、そこに含まれる全命題（手順 1〜4 の各文・締めの段落・9 形の列挙・3 点検証）が `.claude/skills/merge-pr/SKILL.md` か既存 ADR のどちらかに着地していることを、計画作者と別枠組み（命題を列挙 → 着地先を 1 対 1 で照合）で確認する。サブエージェントへ委譲する場合は成果物を `workspace/rederive-merge-pr.md` へ書かせる。着地しない命題が見つかったら `/merge-pr` へ追記してから再照合する。

- [ ] **Step 2: norm-review の実施**

判定を足した変更は Task 4 の 2 条項（第二軸・不変条件）。`/norm-review` を起動し、種は本サイクルの実事例から取る（例: 「#488 の 4 手順のような手順散文を教訓として得た状況で、CLAUDE.md へ直接書く欠陥が紛れ込む／条項が効けばスキルへ振り向けられる」）。素通りが出たら修正 1 文を該当条項へ入れ、同じ種で再走する。

- [ ] **Step 3: 最終確認 → push**

Run: `npm run governance:check`
Expected: PASS

```bash
git push -u origin HEAD
```

PR 作成はユーザー確認後（PR 本文に closing keyword を書かない。対応 issue は無い）。
