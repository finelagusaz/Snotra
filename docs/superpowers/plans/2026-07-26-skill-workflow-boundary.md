# /start-issue と /implement の責務境界 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `/implement` から計画フェーズを取り除き、計画の所有を `/start-issue` に一本化する。

**Architecture:** `/implement` の Step 1 を「入口判定」に置き換える（`plan.md` の同一性確認 → 3 分岐）。Step 2「計画」を削除して以降を繰り上げ、出力契約から「変更計画」を外す。`/start-issue` は引き渡し契約の明記と参照の正準化のみ。設計の正本は `docs/superpowers/specs/2026-07-26-skill-workflow-boundary-design.md`。

**Tech Stack:** Markdown（`.claude/skills/*/SKILL.md`）・`scripts/governance-check.mjs`（G8/G10/G11）・vitest。

## Global Constraints

- **`main` へ直接コミットしない。** feature ブランチ（`chore/<作業名>`）で作業する。
- **常時ロード面の面積上限は `AREA_BUDGET.alwaysLoaded = 15621` 字、着手時点の実測は 15521 字（余裕 100 字）。** 本計画で触る課税面はルート `CLAUDE.md` と `/implement` の `description` の 2 つだけ。超える場合は `scripts/governance-check.mjs` の `AREA_BUDGET` を**理由コメント付きで**更新する。黙って上げてはならない。
- **他文書の見出しを指すときは正準形 `` `<対象>`「<見出し>」 ``**（対象は `<path>.md` か `/skill-name`）。序数だけの参照を書かない（`.claude/rules/governance-docs.md`）。
- **`*.md` の編集に PostToolUse hook 検査は割り当てられていない。** 沈黙は合格ではない。各タスクで `node scripts/governance-check.mjs` を明示的に実行する。
- **bash の HEREDOC を使わない。** 複数行テキストは一時ファイル（`$env:TEMP` 配下）か PowerShell here-string。パス区切りは `/`。
- 検証コマンドは 2 つだけ: `node scripts/governance-check.mjs` と `npm test`。

---

### Task 0: 検査の走査から `.superpowers/` を除外する（実行中に挿入・#722）

**挿入の経緯**: Task 1 の完了後、実装者レポート（`.superpowers/sdd/**/task-1-report.md`・gitignore 済み）が旧参照 `` `/implement`「5b. …」 `` を地の文で 3 箇所引用していたため、G11 がそれを dangling と判定し `governance:check` が 3 件の赤、`npm test` の dogfood テストが 1 件の赤になった。**Task 2 以降の完了ゲート（`npm test` が通ること）が無関係な理由で満たせない**ため、#722 の修正を本計画へ取り込む。`.superpowers/` は gitignore 済みゆえ CI は緑であり、壊れているのは手元の検査母集団だけである。

**Files:**
- Modify: `scripts/governance-check.mjs`（`WALK_EXCLUDE_PREFIXES` とその直上コメント）
- Test: `scripts/governance-check.test.mjs`（新規 describe を 1 つ）

**Interfaces:**
- Consumes: なし
- Produces: `makeSnapshot` が `.superpowers/` 配下を列挙しなくなる。以降の全タスクはこれを前提に `npm test` の全件 PASS をゲートに使える

- [ ] **Step 1: 失敗するテストを書く**

`scripts/governance-check.test.mjs` の import に fs / os / path を足す（既存の `import { existsSync, readFileSync } from "node:fs";` を置き換える）:

```javascript
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
```

ファイル末尾（`describe("実リポジトリ スモーク（dogfood）"...)` の直前）に次を追加する:

```javascript
describe("makeSnapshot の走査除外（#722）", () => {
  // 守りたい対象 = SDD 作業バッファ。実リポジトリではなく一時ディレクトリの複製に当てる
  // （.claude/rules/safety-nets.md「稼働中のガードを弱めない——複製に変異を当てる」）
  it(".superpowers/ 配下は母集団に入らない（gitignore 済みで CI には存在しない＝手元だけ赤くなる）", () => {
    const root = mkdtempSync(path.join(tmpdir(), "gov-walk-"));
    try {
      mkdirSync(path.join(root, "docs"), { recursive: true });
      mkdirSync(path.join(root, ".superpowers/sdd/p"), { recursive: true });
      writeFileSync(path.join(root, "docs/a.md"), "# a\n");
      writeFileSync(path.join(root, ".superpowers/sdd/p/brief.md"), "# brief\n");
      const s = makeSnapshot(root);
      expect(s.files).toContain("docs/a.md");
      expect(s.files.filter((f) => f.startsWith(".superpowers/"))).toEqual([]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
```

- [ ] **Step 2: テストが落ちることを確認する（Red）**

Run: `npx vitest run scripts/governance-check.test.mjs -t "走査除外"`
Expected: FAIL。`.superpowers/sdd/p/brief.md` が `s.files` に現れるため、`toEqual([])` が落ちる。
**この赤が出ないなら止まって報告する**（除外が既に効いていることになり、この Task の前提が崩れる）。

- [ ] **Step 3: 最小の実装で通す（Green）**

`scripts/governance-check.mjs` の定数を書き換える:

変更前:
```javascript
const WALK_EXCLUDE_PREFIXES = ["workspace", ".claude/worktrees"];
```
変更後:
```javascript
const WALK_EXCLUDE_PREFIXES = ["workspace", ".claude/worktrees", ".superpowers"];
```

直上のコメントの末尾（`workspace/worktrees はルート錨止めにする` の後）に次を足す:
```
 *  `.superpowers/` は SDD（subagent-driven-development）の作業バッファで、gitignore 済みゆえ CI の
 *  チェックアウトには存在しない——走査に含めると同じコマンドが手元と CI で別の母集団を見る（#722）。
```

- [ ] **Step 4: テストが通ることを確認する（Green）**

Run: `npx vitest run scripts/governance-check.test.mjs -t "走査除外"`
Expected: PASS

- [ ] **Step 5: ゲートが回復したことを確認する**

Run: `node scripts/governance-check.mjs && npm test`
Expected: `G1..G11 passed` と全件 PASS（dogfood テストを含む。テスト数は 229 → 230 に増える）

- [ ] **Step 6: コミット**

```bash
git add scripts/governance-check.mjs scripts/governance-check.test.mjs
```
コミットメッセージは一時ファイルへ Write して `git commit -F <tmpfile>`（HEREDOC を使わない）。件名:
```
fix(governance): 走査から .superpowers/ を除外する（#722）
```
本文に含めること: 実際に踏んだ経路（SDD のレポートが旧参照を地の文で引用 → dangling 判定 → dogfood テストまで赤）・gitignore 済みゆえ CI と手元で母集団が食い違うこと・`docs/superpowers/` は別物で既に除外済みであること。

---

### Task 1: `/implement` の入口を作り替える

**Files:**
- Modify: `.claude/skills/implement/SKILL.md`（frontmatter `description` / Step 1・2 / Step 見出しの繰り上げ / 出力）
- Modify: `CLAUDE.md`（スキル表の `/implement` 行・末尾の `code-reviewer` 参照）

**Interfaces:**
- Consumes: なし（このタスクが起点）
- Produces: `/implement` の Step 見出しが `## Step 1 — 入口判定` / `## Step 2 — 実装` / `## Step 3 — 検証（最大5サイクル）` / `## Step 4 — レビュー` / `### 4a. check スキルの実行` / `### 4b. code-reviewer エージェント` / `## Step 5 — コミット` になる。Task 3 の ADR はこの見出し名を参照する。

- [ ] **Step 1: 着手前の面積を測る**

Run:
```bash
node -e "import('./scripts/governance-check.mjs').then(m=>{const s=m.makeSnapshot(process.cwd());console.log(m.normativeArea(s))})"
```
Expected: `{ always: 15521, rules: 8318 }`（着手時点。異なる場合は以降の余裕計算をその値でやり直す）

- [ ] **Step 2: Step 見出しだけ先に繰り上げて「赤」を作る**

`.claude/skills/implement/SKILL.md` の 5 つの見出しを次のとおり書き換える（本文はまだ触らない）:

| 現在 | 変更後 |
|---|---|
| `## Step 3 — 実装` | `## Step 2 — 実装` |
| `## Step 4 — 検証（最大5サイクル）` | `## Step 3 — 検証（最大5サイクル）` |
| `## Step 5 — レビュー` | `## Step 4 — レビュー` |
| `### 5a. check スキルの実行` | `### 4a. check スキルの実行` |
| `### 5b. code-reviewer エージェント` | `### 4b. code-reviewer エージェント` |
| `## Step 6 — コミット` | `## Step 5 — コミット` |

**この時点では `## Step 2 — 計画`（旧）と `## Step 2 — 実装`（新）が同居する。** Step 5 で旧 Step 1・2 をまとめて消すまでの過渡状態であり、意図したものである。

- [ ] **Step 3: 検査が落ちることを確認する（Red）**

Run: `node scripts/governance-check.mjs`
Expected: FAIL（exit 1）。`CLAUDE.md` を指す次の finding が出る（行番号は編集でずれるので照合しない）:
```
CLAUDE.md:<行>  見出し参照が着地しない: `/implement`「5b. code-reviewer エージェント」（.claude/skills/implement/SKILL.md に該当する見出し・リード文が無い）
```
**この赤が出ないなら止まって原因を調べる。** G11 が参照を見ていない＝この計画の前提（#720 の機構が改番を守る）が成り立っていないことになる。

- [ ] **Step 4: 参照側を直して緑に戻す（Green）**

`CLAUDE.md` の末尾行を書き換える:

変更前:
```
サブエージェント: `code-reviewer`（`.claude/agents/`）— 実装後・コミット前の3フェーズレビュー（実装検証 / 計画判断・SPEC.md 同期 / パフォーマンス）。`/implement`「5b. code-reviewer エージェント」が自動で起動する。
```
変更後:
```
サブエージェント: `code-reviewer`（`.claude/agents/`）— 実装後・コミット前の3フェーズレビュー（実装検証 / 計画判断・SPEC.md 同期 / パフォーマンス）。`/implement`「4b. code-reviewer エージェント」が自動で起動する。
```

Run: `node scripts/governance-check.mjs`
Expected: PASS（`G1..G11 passed`）

- [ ] **Step 5: Step 1 を「入口判定」へ置き換える**

`.claude/skills/implement/SKILL.md` の `## Step 1 — 調査` 節（現行の 5 つの箇条書きすべて）と `## Step 2 — 計画` 節（見出しと本文すべて）を削除し、次の内容に置き換える:

```markdown
## Step 1 — 入口判定

このスキルは**計画を所有しない**。動くのは「レビュー済みの計画がある」か「計画が要らない」かのどちらかのときだけである。

### 1a. workspace/plan.md の同一性確認

`workspace/plan.md` があれば、**それが今から着手するタスクのものか**を確かめる。`/start-issue` は中断（セッション断・放棄）しても `workspace/` を残すため、別タスクの残骸が残っていることがある。

材料（機械的に取れるものを優先する）:

- `git log --oneline main..HEAD -- workspace/` — このブランチ上で `/start-issue` が積んだものか。main から到達できる／別ブランチのものなら他タスクの残骸である
- `plan.md` とそのコミットメッセージ（`chore: workspace 調査・計画 (issue #N)`）の issue 番号が、現在のブランチ名・与えられたタスクと一致するか
- `plan.md` の変更ファイル一覧・目的が、これからやることと噛み合うか
- 未コミットなら git の証跡は無い（`/start-issue` が最後まで走らずに中断した形）。内容の一致だけが根拠になる

**既定は fail-closed である**——確認できなければ、あるものとして扱わない:

| 判定 | 動き |
|---|---|
| 今のタスクのものと確認できた | 1b の「計画あり」へ |
| 別タスクのものと分かった | **消さずに**何の残骸かを報告し、削除か退避かをユーザーに確認してから「計画なし」へ |
| 判断がつかない | **止めて問う** |

倒す向きに非対称があるためである。古い計画を有効と誤れば**違うものを実装し**、検証を全部通ってコミットまで気づけない。有効な計画を古いと誤っても、止まって聞き直すだけで回復できる。

### 1b. 分岐

| 状況 | 動き |
|---|---|
| 同一性を確認した `plan.md` がある | それを実装の指示として読み Step 2 へ。**計画はレビュー済みである**（`/start-issue`「出力」の引き渡し契約）。調査はやり直さない |
| 無く、計画を書かずに直せる | 下の調査を行ってから Step 2 へ |
| 無く、計画が要ると判明した | **止める**（1c） |

計画なしの経路でだけ行う調査:

- `SPEC.md` と関連する `CLAUDE.md` を読み、意図とアーキテクチャを理解する
- `$ARGUMENTS` からエントリポイントと関連モジュールを特定する
- 要求された機能と重複する既存コードを検索する
- 3層モデルの制約（意図は `SPEC.md`、実装はコード）に留意する
- **要求の曖昧さを判定する** — 「何を作るか」がコードとドキュメントから一意に決まらない場合（UI の見た目、対象範囲、既存動作を変えるか追加するか等）、最も影響の大きい 1〜2 点に絞って質問し、回答を得てから進む

**計画が要るサイン**（網羅ではなく徴候である）:

- 複数ファイルにまたがり、変更の順序に依存する
- 新しい状態・プロセス・ウィンドウ・永続形式を導入する
- `SPEC.md` に書かれた挙動を変える
- どう直すかが一つに決まらず、代替案の比較が要る

**サインは実装の途中で立つこともある。** その場合もその時点で止める。

### 1c. 止まったときの引き渡し

**実装を始めない。** 何が計画を要求したかを報告し、**issue 本文の草案を出して起票を打診する**。承認されたら `gh issue create` を実行し、`/start-issue <N>` へ渡す。
```

- [ ] **Step 6: 検査を通す**

Run: `node scripts/governance-check.mjs`
Expected: PASS

- [ ] **Step 7: `description` を実態に合わせる**

`.claude/skills/implement/SKILL.md` の frontmatter:

変更前:
```yaml
description: "コード変更を伴うタスク（機能追加・バグ修正・リファクタリング）の実装時に使用。調査からコミット作成まで自律的に行う。"
```
変更後:
```yaml
description: "コード変更を伴うタスク（機能追加・バグ修正・リファクタリング）の実装時に使用。実装からコミット作成まで自律的に行う（計画が要る変更は /start-issue へ）。"
```

- [ ] **Step 8: 出力契約から「変更計画」を外す**

`.claude/skills/implement/SKILL.md` の `## 出力` 節を次に置き換える:

変更前:
```markdown
以下を報告:
1. 調査結果（Step 1）
2. 変更計画（Step 2）
3. 最終検証結果 — check, clippy, test の出力（Step 4）
4. コミットハッシュとメッセージ（Step 6）
5. 全変更の diff
```
変更後:
```markdown
以下を報告:
1. 入口判定の結果（計画あり／なし。`plan.md` があった場合は同一性を確認した根拠）
2. 最終検証結果 — check, clippy, test の出力（Step 3）
3. コミットハッシュとメッセージ（Step 5）
4. 全変更の diff
```

- [ ] **Step 9: 4a に F5 の 1 行を足す**

`.claude/skills/implement/SKILL.md` の `### 4a. check スキルの実行` 節の末尾（「発見事項があれば修正してから 5b に進む。」を含む段落）を次に置き換える:

変更前:
```
`/symmetric-check` はコードパス変更・バグ修正でほぼ常に該当。発見事項があれば修正してから 5b に進む。
```
変更後:
```
`/symmetric-check` はコードパス変更・バグ修正でほぼ常に該当。発見事項があれば修正してから 4b に進む。**同じ check が `/start-issue` の計画段階で走っていても、ここで実行する**——対象が計画と実装で別だからである（計画に無い変更は実装中に必ず生じる）。
```

- [ ] **Step 10: `CLAUDE.md` のスキル表を実態に合わせる**

`CLAUDE.md` の「利用できるスキル」表の `/implement` 行を書き換える:

変更前:
```
| `/implement`         | コード変更を伴うタスクの実装（調査からコミットまで） |
```
変更後:
```
| `/implement`         | 実装〜コミット（計画が要る変更は `/start-issue` へ） |
```

**この行は常時ロード面である。** 変更前 33 字 → 変更後 36 字（表の整形分を除く実質 +3 字前後）。Step 7 の `description` と合わせて Step 11 で実測する。

- [ ] **Step 11: 面積を測り、上限内か確かめる**

Run:
```bash
node -e "import('./scripts/governance-check.mjs').then(m=>{const s=m.makeSnapshot(process.cwd());console.log(m.normativeArea(s))})"
```
Expected: `always` が 15621 以下。超えていたら `scripts/governance-check.mjs` の `AREA_BUDGET` を**理由コメント付きで**更新し、その旨をコミットメッセージに書く。

- [ ] **Step 12: 全検査を実行する**

Run: `node scripts/governance-check.mjs && npm test`
Expected: `G1..G11 passed` と `Tests 229 passed`（テスト数は着手時点の値。増減があればその理由を説明できること）

- [ ] **Step 13: コミット**

```bash
git add .claude/skills/implement/SKILL.md CLAUDE.md
```
コミットメッセージは一時ファイルへ書いて `git commit -F <tmpfile>`（HEREDOC を使わない）。件名:
```
refactor(implement): 計画フェーズを外し、入口を plan.md の同一性確認にする
```
本文に含めること: F1（到達不能な Step 2 と破れた出力契約）の解消・fail-closed に倒した理由・Step 繰り上げで G11 が `CLAUDE.md` の参照を落としたこと（Red→Green を実測した）・面積の増減。

---

### Task 2: `/start-issue` に引き渡し契約を書き、F2 を直す

**Files:**
- Modify: `.claude/skills/start-issue/SKILL.md`（`### 5b.` 節の 1 文 / `## 出力` の 5 項目め）

**Interfaces:**
- Consumes: Task 1 が確定させた `/implement` の入口（「レビュー済みの計画である」という前提に依存する側）
- Produces: `/start-issue`「出力」に引き渡し契約の記述。Task 1 の Step 5 で書いた `/start-issue`「出力」への参照がこれで着地する

- [ ] **Step 1: F2 を正準形へ直す**

`.claude/skills/start-issue/SKILL.md` の `### 5b. セルフレビュー（plan-review 固有の補完）` 節の冒頭文を書き換える:

変更前:
```
Step 5a の `/plan-review` が **①対称コードパス ②影響範囲の網羅性（呼び出し元 grep）③リソース管理（生成/破棄ペア・`false` に戻さない `AtomicBool`／`unlisten` の無い `listen()`／`kill` の無い子プロセス）④既存パターンとの整合 ⑤YAGNI** を検証済みである（Step 2 の観点＋ Step 2b の独立再導出）。
```
変更後:
```
Step 5a の `/plan-review` が **①対称コードパス ②影響範囲の網羅性（呼び出し元 grep）③リソース管理（生成/破棄ペア・`false` に戻さない `AtomicBool`／`unlisten` の無い `listen()`／`kill` の無い子プロセス）④既存パターンとの整合 ⑤YAGNI** を検証済みである（`/plan-review`「Step 2」の観点＋ `/plan-review`「Step 2b」の独立再導出）。
```

- [ ] **Step 2: 参照が着地することを確認する**

Run: `node scripts/governance-check.mjs`
Expected: PASS。evidence 行の「見出し参照 N 件」が Task 1 完了時より **2 件増える**（`/plan-review`「Step 2」と「Step 2b」が新たに照合対象になる）

- [ ] **Step 3: 出力に引き渡し契約を書く**

`.claude/skills/start-issue/SKILL.md` の `## 出力` 節の 5 項目めを書き換える:

変更前:
```
5. 次のアクション: `/implement` で実装に進めること
```
変更後:
```
5. 次のアクション: `/implement` で実装に進めること

**引き渡し契約**: ここで渡す `workspace/plan.md` は `/plan-review` を通した**レビュー済みの計画**である。`/implement` はこの前提で計画を読み、調査をやり直さない。計画を検証せずに渡してはならない——検証水準が入口によって変わると、`/implement` 側が前提を立てられなくなる。
```

- [ ] **Step 4: 全検査を実行する**

Run: `node scripts/governance-check.mjs && npm test`
Expected: `G1..G11 passed` と `Tests 229 passed`

- [ ] **Step 5: コミット**

```bash
git add .claude/skills/start-issue/SKILL.md
```
件名:
```
docs(start-issue): 引き渡し契約を明記し、Step 参照を正準形へ直す
```
本文に含めること: F2 の中身（自スキルの Step 2 は「main 最新化 & ブランチ作成」なので誤読すること）・引き渡し契約が `/implement` の入口判定の前提であること。

---

### Task 3: ADR-0006 に否定の知識を回収する

**Files:**
- Create: `docs/adr/0006-plan-ownership-boundary.md`

**Interfaces:**
- Consumes: Task 1・Task 2 で確定した見出し名（`/implement`「Step 1 — 入口判定」・`/start-issue`「出力」）
- Produces: なし（記録が終端）

- [ ] **Step 1: ADR を書く**

`docs/adr/0006-plan-ownership-boundary.md` を作成する。形式は `docs/adr/0001-doc-minimization-cap-enforcement.md` に倣い、次を含める:

- **文脈**: 両スキルが「計画」を所有していたため、`plan.md` の判定が 2 箇所に生まれ（F1: 到達不能な分岐と破れた出力契約）、検証水準が入口で変わり（F3）、同じ check が 2 度走る理由を誰も書けなくなっていた（F5）。`/implement` 単独起動は日常的な経路である。
- **決定**: 計画の所有を `/start-issue` に一本化する。`/implement` は「レビュー済みの計画がある」か「計画が要らない」かでのみ動き、計画が要ると判明した時点で止まる。`plan.md` の同一性確認は fail-closed に倒す。
- **検討した代替案と却下理由**（6 件・spec §6 から移す。ADR が「なぜ」の正本になるので spec を参照させず本文を書く）:
  1. `/implement` の中に計画フェーズを昇格させる → 計画フェーズが 2 か所に実装され F1/F5 を再生産する
  2. 計画フェーズを独立スキルへ切り出す → 原因は「概念の所有が曖昧」であって「共通部品が無い」ではないため、部品化は問題の形に対して過剰
  3. `/start-issue` を issue 無しでも回せるようにする → issue 読解・ブランチ作成に条件分岐が増え、F1 と同型の「分岐が 2 か所」を新設する
  4. 単独起動でも常に `/plan-review` を回す → 小さな修正のたびにサブエージェントが走る。境界を定義する方が安い
  5. 規模（触るファイル数など）で自動分岐させる → 閾値の妥当性を誰も検算できない判定が増える
  6. `plan.md` の同一性が不明なとき、あるものとして進む → **本 ADR で最も価値のある否定の知識**。古い計画を有効と誤れば「違うものを実装する」が検証を全部通ってコミットまで気づけず、逆の誤りは止まって聞き直すだけで回復できる。倒す向きの非対称ゆえ fail-closed にする
- **帰結**: `/implement` は 5 段（入口判定・実装・検証・レビュー・コミット）になる。**この設計の中身＝手順の整合性は機構では守れない**（`governance:check` が守るのは参照の着地・面積・スキル表の存在だけ）ため、F1 と同型の欠陥は再発しうる——これは `docs/adr/0004-canonical-heading-references.md` が明記した残余と同じクラスである。未コミットの `plan.md` に対して同一性確認は弱く、判断がつかなければ止まる。

- [ ] **Step 2: 参照が着地することを確認する**

Run: `node scripts/governance-check.mjs`
Expected: PASS。ADR 内に `` `<対象>`「<見出し>」 `` 形で書いた参照がすべて着地する（着地しなければ、その見出しが実在しないので直す）

- [ ] **Step 3: 全検査を実行する**

Run: `node scripts/governance-check.mjs && npm test`
Expected: `G1..G11 passed` と `Tests 229 passed`

- [ ] **Step 4: コミット**

```bash
git add docs/adr/0006-plan-ownership-boundary.md
```
件名:
```
docs(adr): 計画の所有を /start-issue に一本化した判断を記録する
```

---

## 完了条件

- `node scripts/governance-check.mjs` が `G1..G11 passed`
- `npm test` が全件 passed
- 常時ロード面が `AREA_BUDGET.alwaysLoaded` 以下（超えた場合は理由コメント付きで基準を更新済み）
- `/implement` の本文に「計画」を作る指示が 1 つも残っていない（`grep -n "計画" .claude/skills/implement/SKILL.md` の全ヒットが「計画を所有しない」「計画あり／なし」「計画が要る」の文脈であること）
