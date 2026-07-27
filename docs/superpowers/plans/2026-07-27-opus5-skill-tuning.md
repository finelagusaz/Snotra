# Opus 5 プロンプティングガイドに沿ったスキル・エージェント調整 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Opus 5 の成果物肥大・実況過多・自発的委譲を 5 箇所の較正で抑え、引用を持たない汎用自己検証 2 件を削り、実測に支えられた検証（B 列）は一行も触らない。

**Architecture:** 編集対象は Markdown 5 ファイルのみ。コードの変更はなく、自動テストも存在しない。検証は (1) 敵対的読者サブエージェント 2 クラス × 2 巡、(2) `npm run governance:check` の 2 段で行う。設計の SSOT は `docs/superpowers/specs/2026-07-27-opus5-skill-tuning-design.md`。

**Tech Stack:** Markdown / `npm run governance:check`（`scripts/governance-check.mjs`）/ Agent ツール（敵対的読者レビューのみ）

## Global Constraints

- **B 列（issue 番号または「実測」を背負う行）は一行も削らない・弱めない。** 特に `/plan-review` Step 2 の台帳・削除手順、Step 2b の独立再導出、Step 3 の照合、`/health-check` Check 7 は対象外
- **`.claude/skills/**` に PostToolUse 検査は割り当てられていない**——編集後の沈黙は「何も走らなかった」である。Task 5 の `npm run governance:check` を省略しない
- **ブランチは `chore/opus5-skill-tuning`**（作成済み・spec コミット 2 本が乗っている）。`main` へ直接コミットしない
- **`docs/adr/` は作らない。** 本サイクルに却下した代替案の記録は spec が持つ（否定の知識は spec 内に閉じている）
- 編集は **Edit ツールで行い、ファイル全体を Write で置き換えない**（B 列を巻き添えで失う経路を作らない）

---

### Task 1: 長さ較正を 3 スキルへ入れる（変更 1 のうち `/implement` 以外）

**Files:**
- Modify: `.claude/skills/start-issue/SKILL.md`（Step 3 の出力リスト直後・Step 4 のチェックリスト段落末尾）
- Modify: `.claude/skills/plan-review/SKILL.md`（Step 2 の項目 3 末尾）
- Modify: `.claude/skills/retrospective/SKILL.md`（Step 5 の箇条書き）
- Test: 自動テストなし。Task 4 の敵対的読者レビューが検証を担う

**Interfaces:**
- Consumes: なし（先行タスクなし）
- Produces: Task 4 のシナリオ 1（手を抜く読者が較正を口実に根拠・未検証欄を落とす）が読む文言

- [ ] **Step 1: `/start-issue` Step 3 に research.md の較正を足す**

`.claude/skills/start-issue/SKILL.md` の「**未解決の疑問**: 調査で判明しなかった点（あれば）」の行の直後（「**列挙の事実確認**」段落の直前）に、空行を挟んで次の 1 段落を挿入する:

```markdown
**書くのは調査で判明した事実だけである。** 読んだが計画に影響しなかったファイルの要約・前置き・一般論を書かない——`research.md` は次に `plan.md` を書くための入力であって、読んだ量の証明ではない。
```

- [ ] **Step 2: `/start-issue` Step 4 にチェックリスト散文の較正を足す**

同ファイルの「**作業項目はチェックリストで列挙する**」段落の末尾（「……hook が拒否する（`CLAUDE.md`「フック」）。」の直後、同じ段落の続き）に次の一文を追記する:

```markdown
**散文は各チェック項目の判断根拠に限る。**
```

- [ ] **Step 3: `/plan-review` Step 2 項目 3 に較正 + 未検証節の保護を足す**

`.claude/skills/plan-review/SKILL.md` の項目 3 の末尾（「……届いたが空洞なファイルは、実在確認を通り抜ける）」の直後）に追記する。**保護句を同じ文に同居させることが要点**であり、上限だけを分けて書いてはならない:

```markdown
。**各項目は 1〜3 行に収め、根拠は `file:line` か grep 結果そのものを置く。前置き節・要約節を作らない——ただしこの上限は第 4 分類「未検証（理由）」の省略を許さない**（分類 4 は長さの調整対象ではなく、省けば「問題なし」との区別が消える）
```

- [ ] **Step 4: `/retrospective` Step 5 に見出しの較正を足す**

`.claude/skills/retrospective/SKILL.md` の Step 5 の箇条書き（「- 前回の内容は上書きする（追記しない）」で始まる 3 項目）の**先頭**に次の 1 行を挿入する:

```markdown
- **各見出しは 3〜6 行に収める**——`RETROSPECTIVE.md` は次サイクルで上書きされる。分量ではなく「次に効く一点」を書く
```

- [ ] **Step 5: 3 ファイルの編集箇所を読み返し、B 列の行が消えていないことを確認する**

Run: `git diff --stat` および `git diff .claude/skills/`
Expected: 3 ファイル・**追加のみ（削除行ゼロ）**。削除行が 1 行でもあれば B 列を巻き込んでいる疑いがあり、その diff を精査してから進む

- [ ] **Step 6: Commit**

```bash
git add .claude/skills/start-issue/SKILL.md .claude/skills/plan-review/SKILL.md .claude/skills/retrospective/SKILL.md
git commit -m "docs(skills): 成果物の長さ較正を start-issue/plan-review/retrospective へ入れる" -m "Co-authored-by: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: `/implement` の 4 変更（ADR 較正・委譲抑制・ナレーション・diff 削除と序数修正）

**Files:**
- Modify: `.claude/skills/implement/SKILL.md`（Step 4b・Step 5 の ADR 段落・「## 出力」節全体）
- Test: 自動テストなし。Task 4 のシナリオ 2・4 が検証する

**Interfaces:**
- Consumes: なし（Task 1 と独立。同一ファイルを触らない）
- Produces: Task 4 のシナリオ 2（委譲抑制を口実に `/plan-review` の規定 fan-out を落とす）・シナリオ 4（diff 削除を「報告しなくてよい」と読む）が読む文言

- [ ] **Step 1: Step 4b に委譲の上限を足す**

`.claude/skills/implement/SKILL.md` の「### 4b. code-reviewer エージェント」節の本文（「……Critical または High の発見事項は修正してから次に進む。」）の直後に、空行を挟んで次の 1 段落を挿入する:

```markdown
**このスキルが `Agent` を起動してよいのは、この 1 体だけである。** 調査・確認・裏取りは自分の `Grep` / `Read` で完結させる——`allowed-tools` の `Agent` はこの 1 体のために在る。**これは `/implement` の中でのことであって、他スキルが規定する委譲を減らす根拠にはならない**（`/plan-review` Step 2 の並列スカウトと Step 2b の独立導出は、それぞれの実測に支えられた必須手順である）。
```

- [ ] **Step 2: Step 5 の ADR 段落に較正を足す**

同ファイル Step 5 の ADR 段落の末尾（「……無ければ何もしない」の直後）に追記する:

```markdown
。ADR に書くのは**却下した代替案と、却下の理由**である——採用案の再説明を書かない（採用案はコードと `SPEC.md` が持つ）
```

- [ ] **Step 3: 「## 出力」節を差し替える**

同ファイルの「## 出力」節を、次の内容へ**まるごと置き換える**。序数参照の修正（「下の 2〜4」→ 名前による参照）と項目 4 の削除を**同時に**行う——片方だけ適用すると壊れた序数が残る:

```markdown
## 出力

**作業中の実況は、発見があったときと方針を変えたときに限る。**

**Step 1 で止まった場合**（1c-A / 1c-B / 1c-C）は Step 2 以降に到達しないため、**入口判定の結果と、1c が定める渡し方に沿った報告だけ**を行う。最終検証結果とコミットの各項目は、該当経路では生成物が存在しない。

Step 2 以降まで進んだ場合、以下を報告:
1. 入口判定の結果（計画あり／なし。`plan.md` があった場合は同一性とレビュー到達を確認した根拠）
2. 最終検証結果 — check, clippy, test の出力（Step 3）
3. コミットハッシュとメッセージ（Step 5）
```

- [ ] **Step 4: 序数参照が残っていないことを確認する**

Run: `rg -n '下の\s*[0-9２-９]|項目\s*4|2〜4' .claude/skills/implement/SKILL.md`
Expected: **0 件**。1 件でも出れば序数参照が残っており、名前による参照へ直してから進む

- [ ] **Step 5: Commit**

```bash
git add .claude/skills/implement/SKILL.md
git commit -m "docs(skills): implement の委譲上限・実況の型・ADR 較正と、出力の序数参照を直す" -m "Co-authored-by: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: `code-reviewer` Phase 1 の圧縮

**Files:**
- Modify: `.claude/agents/code-reviewer.md:20-32`（「## Phase 1: 実装検証」節）
- Test: 自動テストなし。Task 4 のシナリオ 3 が検証する

**Interfaces:**
- Consumes: なし
- Produces: Task 4 のシナリオ 3（圧縮を「削った観点は見なくてよい」と読む）が読む文言

- [ ] **Step 1: Phase 1 のチェックリストを差し替える**

`.claude/agents/code-reviewer.md` の「## Phase 1: 実装検証」節の本文とチェックリスト 8 行を、次の内容へ置き換える:

```markdown
## Phase 1: 実装検証

コードが意図した変更を正しく実装しているか確認する。

チェックリスト:
- シークレットや API キーが露出していないか
- システム境界（IPC・Win32・ファイル入力）での入力バリデーション
- テストカバレッジ — 実装者側の義務（`AGENTS.md`「開発ワークフロー」の TDD と報告）とは**別の actor として独立に**見る

汎用のコード品質（可読性・命名・エラーハンドリング）はチェックリストに置かない——**見ないという意味ではなく、列挙しなくても行われる層だからである**。DRY は Phase 2c、パフォーマンスは Phase 3 が持つ（同じ検査を 2 度書かない）。
```

- [ ] **Step 2: Phase 2c・Phase 3 が実在し、移譲先として成立していることを確認する**

Run: `rg -n '^### 2c|^## Phase 3' .claude/agents/code-reviewer.md`
Expected: 2 件ヒット（`### 2c. DRY / 関数カバレッジチェック` と `## Phase 3: パフォーマンス検証`）。ヒットしなければ移譲先が存在せず、削除は成立しない

- [ ] **Step 3: Commit**

```bash
git add .claude/agents/code-reviewer.md
git commit -m "docs(agents): code-reviewer Phase 1 から重複・自明な 5 項目を外す" -m "Co-authored-by: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: 敵対的読者レビュー（2 クラス × 上限 2 巡）

**Files:**
- Read only: `.claude/skills/{start-issue,plan-review,retrospective,implement}/SKILL.md`・`.claude/agents/code-reviewer.md`
- Create（サブエージェントが書く）: scratchpad 配下の 2 ファイル（下記の絶対パス）
- Modify: 発見された抜け道に応じて上記スキルファイル、および `docs/superpowers/specs/2026-07-27-opus5-skill-tuning-design.md`「受容する残余」

**Interfaces:**
- Consumes: Task 1〜3 の編集済みファイル（**3 タスク全部の完了後に開始する**——途中で走らせると、変更した対象を読ませることになる。`CLAUDE.md`「委譲した検査が対象を読む時刻は制御できない＝検査対象を変更しながら検査を走らせない」）
- Produces: spec の「受容する残余」へ追記される、2 巡後に残った抜け道の一覧

- [ ] **Step 1: 出力ディレクトリを作る**

Run（Bash ツール）:
```bash
rm -rf "$SCRATCH/adv" && mkdir -p "$SCRATCH" && mkdir "$SCRATCH/adv"
```
`$SCRATCH` は `C:/Users/Eoh/AppData/Local/Temp/claude/C--workspace-Snotra/a0db25cb-6b4c-4393-b704-8d5e29c1b7ad/scratchpad`。
Expected: 葉の `mkdir` は `-p` を付けない——前回分の削除が silent fail していれば、ここで exit 1 して止まる

- [ ] **Step 2: 2 体を同時に起動する（1 メッセージ・2 tool use）**

型は `general-purpose`（`Write` を持つ型でなければ成果物を書けない）。`model` は指定しない（判断の質が要る席）。`name` は渡さない。**両体のプロンプトに次を明示する**——委譲はコンテキストを継承しないため、書かなければ届かない:

- 読む対象ファイルの**絶対パス 5 本**
- 出力先の**絶対パス**（体ごとに 1 本・下記）
- 「書いてよいのは割り当てられた 1 ファイルだけである」
- 「`git diff main...HEAD` で今回の変更差分を見てよいが、`docs/superpowers/specs/` は読まない」（設計の意図を先に読ませると、抜け道でなく設計の再確認になる）

**体 A（手を抜く読者）** — 出力先 `<SCRATCH>/adv/lazy.md`:

> あなたは、これらのスキル文書に従うことになっているが、**できるだけ手数を減らしたい**読者である。文言だけを根拠に、「規則には違反していないが、検査の実質を省ける」読み方を探せ。特に次の 2 つが可能かを判定せよ: (1) 新しく入った「各項目 1〜3 行」「前置き節を作らない」といった長さの上限を口実に、根拠（`file:line`）や第 4 分類「未検証（理由）」を落とせるか。(2) `/implement` の「`Agent` を起動してよいのは 1 体だけ」を口実に、`/plan-review` Step 2 の並列スカウトや Step 2b の独立導出を省けるか。可能なら**その読み方の逐語引用と、省いた結果どう見えるか**を書け。不可能なら何がそれを塞いでいるかを引用せよ。

**体 B（規則を全部守る読者）** — 出力先 `<SCRATCH>/adv/strict.md`:

> あなたは、これらのスキル文書を**忠実に、全部守ろうとする**読者である。文言に従った結果として**誤った行動へ導かれる**箇所を探せ。特に次の 2 つを判定せよ: (1) `code-reviewer` Phase 1 からチェックリスト項目が減ったことを、「削られた観点（可読性・命名・エラーハンドリング・DRY・パフォーマンス）はレビューで見なくてよい」と読めるか。(2) `/implement`「出力」から「全変更の diff」が消えたことを、「変更内容を報告しなくてよい」と読めるか。加えて、**序数・項目数・「N 項目」といった数え上げの記述が、実際の項目数と食い違っている箇所**を全ファイルで探して列挙せよ。可能なら逐語引用を、塞がれているならその文言を引用せよ。

- [ ] **Step 3: 2 ファイルの実在と中身を確認する**

Run: `ls -la "$SCRATCH/adv/"` および両ファイルの `Read`
Expected: `lazy.md` と `strict.md` が実在し、**空・スタブでない**こと。不着なら**同じ指示で 1 度だけ再起動する**（2 度目は行わない）。それでも不着なら、そのクラスは**独立レビュー不成立**として spec の「受容する残余」へ記録する——「抜け道なし」と読み替えない

- [ ] **Step 4: 成立した抜け道を塞ぐ（1 巡目）**

各指摘について、**逐語引用が実際にその文言で成立するかを自分で開いて確認する**（サブエージェントの実測を一次証拠にしない）。成立したものだけを塞ぐ。塞ぎ方は「文言の追加」であり、B 列の削除・弱体化を伴ってはならない。

- [ ] **Step 5: 2 巡目を回す**

Step 1〜4 を**もう一度**実行する（`rm -rf` から。前回の成果物を残すと、落ちた体の前回分を今回分と読む経路が開く）。プロンプトには 1 巡目で塞いだ箇所を伝えず、同じ 2 プロンプトを使う——伝えると、塞いだ場所の周辺だけを見に行く。

- [ ] **Step 6: 2 巡後に残った抜け道を spec へ記録する**

`docs/superpowers/specs/2026-07-27-opus5-skill-tuning-design.md` の「## 受容する残余」へ、2 巡目で成立したまま残った抜け道を箇条書きで追記する（無ければ「2 巡とも新規の成立なし」と書く）。**上限 2 巡は設計で先に決めた停止条件であり、残りを受容することは想定内である**（`.claude/rules/safety-nets.md`「停止条件を先に決める」）。

- [ ] **Step 7: Commit**

```bash
git add .claude/ docs/superpowers/specs/2026-07-27-opus5-skill-tuning-design.md
git commit -m "docs: 敵対的読者レビュー 2 巡の結果を反映し、残余を設計へ記録" -m "Co-authored-by: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: ガバナンス検査と引き渡し

**Files:**
- Read only: 全変更ファイル
- Test: `npm run governance:check`

**Interfaces:**
- Consumes: Task 1〜4 の全コミット
- Produces: PR 作成の可否判断

- [ ] **Step 1: governance:check を実行する**

Run: `npm run governance:check`
Expected: G1..G11 passed（exit 0）。**`.claude/skills/**` の編集に PostToolUse 検査は無く、ここまで一度も自動検査が走っていない**——沈黙は合格を意味しない

- [ ] **Step 2: 赤なら直す**

`governance:check` が拾うのはスキル定義の整合（G8）・参照の実在（G3）・rules の glob（G7）等である。赤が出たら全件を直し、Step 1 から再実行する。

- [ ] **Step 3: 変更全体を通しで読む**

Run: `git diff main...HEAD -- .claude/`
Expected: 削除行が現れるのは `code-reviewer.md` の Phase 1 チェックリストと `/implement`「出力」節のみ。**それ以外のファイルに削除行があれば B 列を巻き込んでいる**——差分を精査して復元する

- [ ] **Step 4: push して PR を作るかをユーザーに確認する**

**この改修は `.claude/` 配下＝エージェント設定であり、`CLAUDE.md`「最重要ルール」2 の合意対象である。** 設計は合意済みだが、PR 作成の可否は別に確認する。作る場合:

```bash
git push -u origin HEAD && gh pr create --title "docs: Opus 5 ガイドに沿ってスキル・エージェントを較正する" --body-file <本文ファイル>
```

PR 本文には **closing keyword を書かない**（対応する issue が存在しないため。`CLAUDE.md`「Git/GitHub 運用」）。

---

## Self-Review

**1. Spec coverage:**

| spec の節 | 実装するタスク |
|---|---|
| 変更 1（較正 5 箇所） | Task 1（4 箇所）+ Task 2 Step 2（ADR） |
| 変更 2（委譲抑制） | Task 2 Step 1 |
| 変更 3（ナレーション） | Task 2 Step 3 |
| 変更 4（Phase 1 圧縮） | Task 3 |
| 変更 5（diff 削除 + 序数修正） | Task 2 Step 3・Step 4 |
| 敵対的読者レビュー（2 クラス・2 巡・停止条件） | Task 4 |
| 検証（`governance:check`） | Task 5 |
| effort の後続実測 | **意図的に対象外**（spec の「受容する残余」に記載済み） |

**2. Placeholder scan:** 全ステップに実文言または実コマンドを記載済み。「適切に」「必要に応じて」の類は無い。

**3. 一貫性:** Task 1 と Task 2 は別ファイルを触るため並列実行可能だが、**Task 4 は Task 1〜3 の完了後にのみ開始できる**（検査対象を変更しながら検査を走らせない）。Task 3 の移譲先（Phase 2c・Phase 3）は Step 2 で実在を確認する。
