# plan.md チェックリスト化と PR 前ゲート 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** スキル起動経路の穴（`disable-model-invocation` による散文指示の弾き）と計画実行漏れ（plan.md 未チェック項目の放置）を、規範改定と PreToolUse hook のゲートで塞ぐ。

**Architecture:** spec は `docs/superpowers/specs/2026-07-27-plan-checklist-gate-design.md`。変更は 3 層——(1) スキル frontmatter のフラグ削除、(2) plan.md チェックリスト規範（起こすのは `/start-issue`・育てるのは `/implement`・削除の前提は全項目 `[x]`）、(3) `pre-bash.mjs` の既存 `gh pr create` 検出点への plan.md 判定追加（fail-closed・注入可能）。

**Tech Stack:** Node.js (mjs) / vitest / Claude Code hooks (PreToolUse)

## Global Constraints

- `main` へ直接コミットしない。作業ブランチは `chore/plan-checklist-gate`（作成済み）
- `pre-bash.mjs` の実装契約（`docs/hooks.md`）を守る: **exit 2 だけがブロック**・既定 `exitCode = 2`・判定は `tool_input.command` のコマンド位置のみ・判定不能は block へ倒す
- `.claude/hooks/**` の編集は PostToolUse が hook-selftest を自動発火する（沈黙 = 合格）。ただし本計画は各ステップで明示的に vitest を実行する
- ガバナンス文書（SKILL.md・CLAUDE.md・docs/hooks.md）の変更後は `npm run governance:check` を実行する
- コミットメッセージ末尾: `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`

---

### Task 1: `disable-model-invocation` の削除（`/implement`・`/start-issue`）

**Files:**
- Modify: `.claude/skills/implement/SKILL.md`（frontmatter 5 行目）
- Modify: `.claude/skills/start-issue/SKILL.md`（frontmatter 4 行目）

**Interfaces:**
- Consumes: なし
- Produces: 両スキルがモデル起動可能になる（Task 2 の規範はこの前提で書く）

- [ ] **Step 1: implement の frontmatter からフラグ行を削除**

`.claude/skills/implement/SKILL.md` の frontmatter から次の 1 行を削除する（Edit で `old_string` に前後行を含めて一意化する）:

```yaml
disable-model-invocation: true
```

- [ ] **Step 2: start-issue の frontmatter からフラグ行を削除**

`.claude/skills/start-issue/SKILL.md` から同じ 1 行を削除する。

- [ ] **Step 3: 削除の確認**

Run: `grep -l disable-model-invocation .claude/skills/implement/SKILL.md .claude/skills/start-issue/SKILL.md`
Expected: 出力 0 件（対象 2 スキルから消滅していること。他スキルに残存があっても spec スコープ外）

- [ ] **Step 4: governance:check**

Run: `npm run governance:check`
Expected: `G1..G11 passed`

- [ ] **Step 5: Commit**

```bash
git add .claude/skills/implement/SKILL.md .claude/skills/start-issue/SKILL.md
git commit -m "chore: /implement と /start-issue の disable-model-invocation を外す

散文の依頼（「implement で実装して」）が Skill 起動を拒まれ、SKILL.md 本文を
読まないまま手順を即興再現する経路を #749 セッションで実測した（spec 参照）。
自発起動の抑えは /implement Step 1 の入口判定が既に担っている。

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: plan.md チェックリスト規範（SKILL.md 2 本と spec の追記）

**Files:**
- Modify: `.claude/skills/start-issue/SKILL.md`（Step 4 の計画様式）
- Modify: `.claude/skills/implement/SKILL.md`（Step 2 と Step 5）
- Modify: `docs/superpowers/specs/2026-07-27-plan-checklist-gate-design.md`（削除ゲートの追記）

**Interfaces:**
- Consumes: Task 1（フラグ削除済みの frontmatter）
- Produces: plan.md の様式規則。Task 3 の hook はこの様式（`- [ ]` 行）を判定対象とする

- [ ] **Step 1: start-issue Step 4 へチェックリスト規則を追加**

`.claude/skills/start-issue/SKILL.md` の「計画には以下を含める:」リスト（`- **SPEC.md 更新要否**` の行）の直後に追加する:

```markdown
**作業項目はチェックリストで列挙する**: 実行を要する作業項目は、すべてフェーズ配下の `- [ ]` 行として列挙する。散文は説明であり、**散文にだけ書かれた作業は存在しない扱いになる**。フェーズごとの「完了条件」を別に書かない——チェックボックス列そのものが唯一の作業列挙 = 完了条件である（#749: 本文に書いた作業が完了条件に写らず実行から落ちた）。未チェックの `- [ ]` が残ったままの `gh pr create` は PreToolUse hook が拒否する（`CLAUDE.md`「フック」）。
```

- [ ] **Step 2: implement Step 2 へ消し込み・追記の責務を追加**

`.claude/skills/implement/SKILL.md` Step 2 の箇条書き（`- 計画がある場合は、計画に沿って変更を行う` の行）の直後に追加する:

```markdown
- **plan.md のチェックリストを消し込みながら進める**: 完了した項目はその都度 `- [x]` へ更新し、フェーズのコミットに含める（チェック状態を git に残し、中断・再開時の「どこまでやったか」を接地させる）。実装中に判明した計画外の作業は、その場で `- [ ]` として plan.md へ追記する——途中発見のタスクも PR 前ゲートに乗せる
```

- [ ] **Step 3: implement Step 5 の workspace 削除に前提条件を付ける**

`.claude/skills/implement/SKILL.md` Step 5 の既存行:

```markdown
- `workspace/` ディレクトリが存在する場合、削除してステージに含める（`/start-issue` の引き継ぎバッファは実装完了で役目を終える。git 履歴から復元可能）
```

を次で置き換える:

```markdown
- `workspace/` ディレクトリが存在する場合、**`plan.md` の全項目が `- [x]` であることを確認してから**削除してステージに含める（`/start-issue` の引き継ぎバッファは実装完了で役目を終える。git 履歴から復元可能）。未チェックの項目が残っているなら削除しない——完了させるか、やらないと決めた項目は計画から外して理由を ADR か issue へ記録する。**未チェックを残したままの削除は、PR 前ゲート（PreToolUse hook）の視界から計画を消す**
```

- [ ] **Step 4: spec の「受容する残余」へ削除ゲートの残余を追記**

`docs/superpowers/specs/2026-07-27-plan-checklist-gate-design.md` の「受容する残余」リストに追加する:

```markdown
- `/implement` Step 5 は PR 作成より前に `workspace/` を削除するため、**未チェック項目を残したまま削除すれば hook の視界から計画が消える**。削除の前提条件（全項目 `[x]` の確認）を Step 5 の規範に置いて塞ぐが、規範を破る削除は機構では捕捉できない——虚偽の `[x]` と同じクラスの残余として code-review に委ねる。
```

- [ ] **Step 5: governance:check**

Run: `npm run governance:check`
Expected: `G1..G11 passed`

- [ ] **Step 6: Commit**

```bash
git add .claude/skills docs/superpowers/specs/2026-07-27-plan-checklist-gate-design.md
git commit -m "docs: plan.md の作業項目をチェックリストへ一本化する規範を敷く

起こすのは /start-issue（計画と同時に生まれ /plan-review の検証対象になる）、
育てて消し込むのは /implement（逐次 [x] 化・途中発見の追記・削除は全項目 [x] が前提）。

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: pre-bash.mjs の plan.md ゲート（TDD）

**Files:**
- Modify: `.claude/hooks/pre-bash.mjs`
- Test: `.claude/hooks/pre-bash.test.mjs`

**Interfaces:**
- Consumes: Task 2 の様式（`- [ ]` / `* [ ]` 行が作業項目）
- Produces:
  - `countUnchecked(text: string): number` — 未チェック行数
  - `readPlanState(cwd: string): {ok:true, exists:boolean, unchecked:number} | {ok:false, reason:string}`
  - `decide(payload, readGitState, readPlanState)` — **第 3 引数が必須になる**（既存テストの呼び出しも全件更新する）

- [ ] **Step 1: 失敗するテストを書く**

`.claude/hooks/pre-bash.test.mjs` に追記する。まず import へ `countUnchecked, readPlanState` を足し、`mkdirSync, writeFileSync` を `node:fs` から import する。スタブを既存スタブ（`CLEAN` 等）の直後に置く:

```js
/** plan.md が無い（計画なしタスク・他リポジトリ） */
const NO_PLAN = () => ({ ok: true, exists: false, unchecked: 0 });
/** 全項目チェック済み */
const PLAN_DONE = () => ({ ok: true, exists: true, unchecked: 0 });
/** 未チェック n 件 */
const PLAN_OPEN = (n = 1) => () => ({ ok: true, exists: true, unchecked: n });
/** 存在するのに読めない */
const PLAN_UNREADABLE = () => ({ ok: false, reason: "EACCES" });
```

新しい describe を追加する:

```js
describe("countUnchecked — 未チェック行の数え上げ", () => {
  it("`- [ ]` / `* [ ]`（インデント許容）を数える", () => {
    expect(countUnchecked("- [ ] a\n  - [ ] b\n* [ ] c")).toBe(3);
  });
  it("チェック済み・チェックボックス以外の行は数えない", () => {
    expect(countUnchecked("- [x] done\n- [X] done\n- plain\ntext [ ] inline")).toBe(0);
  });
  it("空文字列は 0", () => {
    expect(countUnchecked("")).toBe(0);
  });
});

describe("decide — plan.md ゲート", () => {
  it("未チェック項目が残っていれば block（push の鎖が安全でも）", () => {
    const cmd = `git push -u origin HEAD && ${ghPrCreate()}`;
    const r = decide(bash(cmd), CLEAN, PLAN_OPEN(3));
    expect(r.action).toBe("block");
    expect(r.reason).toContain("3 件");
  });
  it("全項目チェック済みなら push 判定へ進み allow", () => {
    expect(decide(bash(ghPrCreate()), CLEAN, PLAN_DONE).action).toBe("allow");
  });
  it("plan.md が無ければこの検査は管轄外（push 判定のみで allow）", () => {
    expect(decide(bash(ghPrCreate()), CLEAN, NO_PLAN).action).toBe("allow");
  });
  it("plan.md が存在するのに読めなければ block（fail-closed）", () => {
    expect(decide(bash(ghPrCreate()), CLEAN, PLAN_UNREADABLE).action).toBe("block");
  });
  it("gh pr create を検出しなければ plan.md を読まない", () => {
    const spy = vi.fn(NO_PLAN);
    expect(decide(bash("echo hi"), CLEAN, spy).action).toBe("allow");
    expect(spy).not.toHaveBeenCalled();
  });
  it("PowerShell tool でも未チェック項目で block する", () => {
    expect(decide(pwsh(ghPrCreate()), CLEAN, PLAN_OPEN()).action).toBe("block");
  });
});

describe("readPlanState — 実ファイル読み取り", () => {
  it("workspace/plan.md が無いディレクトリは exists:false", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "pre-bash-plan-"));
    expect(readPlanState(dir)).toEqual({ ok: true, exists: false, unchecked: 0 });
  });
  it("未チェック 2 件・チェック済み 1 件の plan.md を正しく数える", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "pre-bash-plan-"));
    mkdirSync(path.join(dir, "workspace"));
    writeFileSync(path.join(dir, "workspace", "plan.md"), "- [x] a\n- [ ] b\n- [ ] c\n", "utf8");
    expect(readPlanState(dir)).toEqual({ ok: true, exists: true, unchecked: 2 });
  });
});
```

プロセス起動テスト（`プロセス起動 — exit code の契約` describe 内）へ 2 件追加する:

```js
  it("未チェックの plan.md がある repo での gh pr create は exit 2（安全な鎖でも）", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "pre-bash-e2e-plan-"));
    mkdirSync(path.join(dir, "workspace"));
    writeFileSync(path.join(dir, "workspace", "plan.md"), "- [ ] 残タスク\n", "utf8");
    const res = runHook(
      { tool_name: "Bash", tool_input: { command: `git push -u origin HEAD && ${ghPrCreate()}` }, cwd: dir },
      dir,
    );
    expect(res.status).toBe(2);
    expect(res.stderr).toContain("plan.md");
  });

  it("全項目チェック済みの plan.md なら安全な鎖は exit 0", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "pre-bash-e2e-plan-"));
    mkdirSync(path.join(dir, "workspace"));
    writeFileSync(path.join(dir, "workspace", "plan.md"), "- [x] 済\n", "utf8");
    const res = runHook(
      { tool_name: "Bash", tool_input: { command: `git push -u origin HEAD && ${ghPrCreate()}` }, cwd: dir },
      dir,
    );
    expect(res.status).toBe(0);
  });
```

既存の `decide` 呼び出し（分岐表 describe 内の全件）へ第 3 引数 `NO_PLAN` を追加する（例: `decide(bash(ghPrCreate()), NO_UPSTREAM)` → `decide(bash(ghPrCreate()), NO_UPSTREAM, NO_PLAN)`）。

- [ ] **Step 2: テストが落ちることを確認**

Run: `npx vitest run .claude/hooks/pre-bash.test.mjs`
Expected: FAIL（`countUnchecked` / `readPlanState` が export されていない）

- [ ] **Step 3: pre-bash.mjs へ実装**

import へ追加: `existsSync` を `node:fs` から、`path` を `node:path` から。

`readGitState` の直後に追加する:

```js
/** 行頭（インデント許容）の未チェック項目。plan.md のコードブロック内の一致は過剰検出として受容する。 */
export function countUnchecked(text) {
  return (text.match(/^\s*[-*]\s+\[ \]/gm) ?? []).length;
}

/** `<cwd>/workspace/plan.md` の完了状態。無ければ管轄外、存在するのに読めなければ判定不能。 */
export function readPlanState(cwd) {
  const file = path.join(cwd, "workspace", "plan.md");
  if (!existsSync(file)) return { ok: true, exists: false, unchecked: 0 };
  try {
    return { ok: true, exists: true, unchecked: countUnchecked(readFileSync(file, "utf8")) };
  } catch (e) {
    return { ok: false, reason: `workspace/plan.md を読めません: ${e.code ?? e.message}` };
  }
}
```

`decide` のシグネチャを `decide(payload, readGitState, readPlanState)` に変え、`cwdAt` の block 判定の**直後**（`hasSafeChain` の**前**——安全な鎖でも計画の完了は別問題）に追加する:

```js
  const plan = readPlanState();
  if (!plan.ok) return block(`${plan.reason}。計画の完了状態を判定できません。`);
  if (plan.exists && plan.unchecked > 0) {
    return block(
      `workspace/plan.md に未チェックの作業項目が ${plan.unchecked} 件残っています。` +
        `項目を完了させて [x] にするか、やらないと決めた項目は計画から外して理由を記録してから、PR を作成してください。`,
    );
  }
```

`main()` の呼び出しを更新する:

```js
  const result = decide(payload, () => readGitState(cwd), () => readPlanState(cwd));
```

ファイル冒頭のコメント（1〜2 行目）を更新する:

```js
// PreToolUse (Bash|PowerShell) ガード。`gh pr create` が (1) 空 PR を作るのを防ぎ、
// (2) workspace/plan.md に未チェックの作業項目を残したまま PR になるのを防ぐ（#749）。
```

- [ ] **Step 4: テストが通ることを確認**

Run: `npx vitest run .claude/hooks/pre-bash.test.mjs`
Expected: 全件 PASS

- [ ] **Step 5: post-edit 側の selftest も含めた全 hook テスト**

Run: `npx vitest run .claude/hooks`
Expected: 全件 PASS（post-edit のテストに影響が無いことの確認）

- [ ] **Step 6: Commit**

```bash
git add .claude/hooks/pre-bash.mjs .claude/hooks/pre-bash.test.mjs
git commit -m "feat: gh pr create 前に plan.md の未チェック項目を PreToolUse で捕捉する

発火点は既存のコマンド位置検出のみ。fail-closed（読めない plan.md は block、
無い plan.md は管轄外）。判定は readPlanState 注入で git・fs 無しにテスト可能。

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: 文書同期（CLAUDE.md フック表・docs/hooks.md）

**Files:**
- Modify: `CLAUDE.md`（「フック」節の表）
- Modify: `docs/hooks.md`（PreToolUse の実装契約）

**Interfaces:**
- Consumes: Task 3 の判定仕様（発火条件・fail-closed の倒し方）
- Produces: なし（写しの同期のみ。判定の SSOT は `decide` のまま）

- [ ] **Step 1: CLAUDE.md フック表の行を更新**

「PR 作成前 push チェック（PreToolUse）」行の**発火条件セル**の末尾（`` `&&` で `git push` が先行するなら通る `` の後）に追加する:

```markdown
。また `workspace/plan.md` に未チェックの `- [ ]` が残っているときも拒む（計画の実行漏れ防止・#749。鎖の安全とは独立に判定）
```

同じ行の**正しい対応セル**の末尾に追加する:

```markdown
。未チェック項目は完了させて `[x]` にするか、やらないと決めた項目は計画から外して理由を記録する
```

- [ ] **Step 2: docs/hooks.md の実装契約へ追記**

「PreToolUse（pre-bash.mjs）の実装契約」節の末尾に追加する:

```markdown
- **plan.md ゲート** — `gh pr create` 検出時、`<cwd>/workspace/plan.md` に未チェックの `- [ ]` が残っていれば block する（#749: 計画に書いた作業の実行漏れを PR 前に捕捉する）。判定点は push 検査と同じコマンド位置検出であり、新しい発火点を作らない。fail-closed の倒し方: 存在するのに読めない → block、存在しない → 管轄外（計画なしタスク・他リポジトリを塞がない）。`decide(payload, readGitState, readPlanState)` の注入でファイルシステム無しにテストできる。plan.md のコードブロック内の `- [ ]` への過剰検出は受容する（fail-closed 方向）。
```

- [ ] **Step 3: governance:check**

Run: `npm run governance:check`
Expected: `G1..G11 passed`

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md docs/hooks.md
git commit -m "docs: plan.md ゲートの発火条件と実装契約をフック文書へ同期する

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 完了時の検証（全 Task 後）

- [ ] `npx vitest run .claude/hooks` — 全件 PASS
- [ ] `npm run governance:check` — G1..G11 passed
- [ ] `grep -l disable-model-invocation .claude/skills/implement/SKILL.md .claude/skills/start-issue/SKILL.md` — 0 件（対象 2 スキルから消滅していること。他スキルに残存があっても spec スコープ外）
- [ ] フォールトインジェクションの実測（`.claude/rules/safety-nets.md`）: Task 3 の e2e テスト（未チェック plan.md → exit 2）が「故意に壊して検知される」ことの実測に当たる。ライブの hook は弱めない（temp ディレクトリの複製に対する検証のみ）
