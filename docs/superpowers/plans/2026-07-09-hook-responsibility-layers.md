# hook の責務三層分離 — Phase 1 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** main 保護を Claude Code の hook から git/GitHub の機構へ移し、`block-main-commit` を削除する。

**Architecture:** 三層。Layer 0 = GitHub ruleset（保証・最終防衛線）、Layer 1 = `.githooks/`（ローカルの早期停止・best-effort）、Layer 2 = Claude Code hook（エージェントの意図と認識のみ）。Layer 1 が外れても Layer 0 が push を拒むため、**Layer 1 の不在を検知する仕組みは作らない**。

**Tech Stack:** POSIX sh（git hook）、Node.js + Vitest（hook の実測テスト）、`gh` CLI（ruleset API）、git 2.55。

**設計文書:** `docs/superpowers/specs/2026-07-09-hook-responsibility-layers-design.md`

## Global Constraints

- 作業ブランチは既存の `chore/hook-responsibility-layers`。**main へ直接コミットしない**
- **`git` コマンドを `&&` でチェーンしない。** 本計画の Task 8 完了まで `block-main-commit` が生存しており、payload 全体 grep で誤発火する
- **bash の HEREDOC（`<<EOF`）を使わない。** 複数行テキストは Write ツールで一時ファイルに書き、`git commit -F <tmpfile>` を使う
- 一時ファイルは `$env:TEMP` 配下か scratchpad に置く。`/tmp` は Windows の Bash ツールに存在しない
- 保護対象ブランチ名は `main`（`.githooks/_lib.sh` の `PROTECTED_BRANCH` が唯一の定義）
- リポジトリは `finelagusaz/Snotra`（PUBLIC）。ruleset id は `12941497`、name は `default`
- 検証コマンドの SSOT は `docs/build-commands.md`
- `.claude/settings.json` と `.claude/hooks/**` を編集すると `hook-selftest` が自動発火する。**沈黙は合格**

## 実行の分担（Pre-Flight で合意）

- **worktree 隔離は使わない。** Task 6 はリポジトリ共有の `.git/config`（`core.hooksPath`）を書き、Task 1 は GitHub 側を触る。worktree は config を共有するため隔離が成立しない
- **Task 1・Task 7・Task 8 の測定ステップ（Step 3・4）は main セッションが直接実行する。** サブエージェントのツール呼び出しで PreToolUse hook が発火するかは未実測であり、発火しなければ V4/V5 の対比（Bash では旧 hook が止まる / PowerShell では止まらない）が意味を失う。**発火条件が実測済みの環境で測ることが、この検証の前提**
- サブエージェントが担うのは Task 2・3・4・5・6・9 と、Task 8 の編集（Step 1・2）・セルフテスト（Step 5）・コミット（Step 6）

## Spec からの逸脱（計画時に判明・要承認）

**1. 最重要ルール 2 は「削除」ではなく「narrow」する。**

spec §5 は CLAUDE.md 最重要ルール 2（`git` コマンドを `&&` でチェーンしない）の削除を求めた。しかし調査の結果、**PR 前 push チェック hook（Phase 2 まで生存）が同種の誤爆を起こす**:

```
input=$(cat); if echo "$input" | grep -qE 'gh\s+pr\s+create'; then
  up=$(git rev-parse --abbrev-ref --symbolic-full-name @{u} 2>/dev/null); ...
```

この hook はコマンド実行の**前**に upstream を評価するため、`git push -u origin HEAD && gh pr create` は必ずブロックされる。ゆえに「`gh pr create` を他のコマンドとチェーンしない」という narrow な規則は Phase 2 まで正しい。

副次的な利点として **番号 1〜4 が保たれる**。issue #473 / #475 / #476 / #477 / #479 の本文は「CLAUDE.md 最重要ルール 4」を番号で参照しており、詰めると静かに陳腐化する。

**2. `post-edit.mjs` のコメント 1 行を事実訂正する。**

`.claude/hooks/post-edit.mjs:242-243` のコメントが `block-main-commit` を名指ししている。削除後は虚偽になる。spec §8 の受け入れ条件「`post-edit.mjs` に変更が無い」を「**振る舞いに変更が無い**（コメントの事実訂正のみ許可）」と読み替える。

## File Structure

**Create:**

| ファイル | 責務 |
|---|---|
| `.githooks/_lib.sh` | `PROTECTED_BRANCH` の定義、`die()`、`current_branch()`。各 hook から source される |
| `.githooks/pre-commit` | main 上の commit を拒否 |
| `.githooks/pre-merge-commit` | main 上のマージコミットを拒否（非 FF の `git pull` を含む） |
| `.githooks/pre-rebase` | main を rebase 対象とする操作を拒否 |
| `.githooks/pre-push` | **宛先** `refs/heads/main` への push を拒否 |
| `.githooks/githooks.test.mjs` | 使い捨て git repo を作り、上記 4 hook を実測する |

**Modify:**

| ファイル | 変更 |
|---|---|
| `vitest.config.ts` | `include` に `.githooks/**/*.test.mjs` を追加 |
| `package.json` | `scripts.prepare` を追加（`core.hooksPath` の bootstrap） |
| `.claude/settings.json` | `block-main-commit` hook を削除 |
| `.claude/hooks/post-edit.mjs` | コメントの事実訂正（1 行） |
| `CLAUDE.md` | 最重要ルール 2 を narrow、Git/GitHub 運用の 2 項目、フック表 |
| `docs/build-commands.md` | カテゴリ E（`.githooks/**`）と bootstrap の記述 |

**Config（ファイルなし）:** GitHub ruleset `12941497`

---

## Task 1: Layer 0 — GitHub ruleset を起こす

**Files:**
- Create: `<scratchpad>/ruleset.json`（一時ファイル。リポジトリには入れない）

**Interfaces:**
- Consumes: なし
- Produces: main への直接 push / force-push / 削除が server 側で拒否される状態

**必ず最初に実行すること。** `.githooks/pre-push` を入れる前なので、ローカルの push は素通りし、**server 側の拒否だけを純粋に測れる**。

- [ ] **Step 1: 現状を記録する**

```powershell
gh api repos/finelagusaz/Snotra/rulesets/12941497
```

Expected: `"enforcement": "disabled"`、`rules` は `deletion` と `non_fast_forward` の 2 件。

- [ ] **Step 2: ruleset の body を書く**

Write ツールで scratchpad に `ruleset.json` を作る（HEREDOC 禁止）。

```json
{
  "name": "default",
  "target": "branch",
  "enforcement": "active",
  "bypass_actors": [],
  "conditions": { "ref_name": { "include": ["~DEFAULT_BRANCH"], "exclude": [] } },
  "rules": [
    { "type": "deletion" },
    { "type": "non_fast_forward" },
    {
      "type": "pull_request",
      "parameters": {
        "required_approving_review_count": 0,
        "dismiss_stale_reviews_on_push": false,
        "require_code_owner_review": false,
        "require_last_push_approval": false,
        "required_review_thread_resolution": false,
        "allowed_merge_methods": ["merge", "squash", "rebase"]
      }
    }
  ]
}
```

- [ ] **Step 3: 適用する**

```powershell
gh api -X PUT repos/finelagusaz/Snotra/rulesets/12941497 --input "<scratchpad>/ruleset.json"
```

Expected: HTTP 200。レスポンスの `enforcement` が `active`。

422 が返る場合は `allowed_merge_methods` を削って再試行し、read-back で既定値を確認する。

- [ ] **Step 4: read-back で確認する**

```powershell
gh api repos/finelagusaz/Snotra/rulesets/12941497 | ConvertFrom-Json | ForEach-Object { $_.enforcement; $_.rules.type }
```

Expected:
```
active
deletion
non_fast_forward
pull_request
```

- [ ] **Step 5: 故障注入 V1 / V3 — main へ push を試みる**

read-back は「設定が入った」ことしか示さない。**実際に破れないことを測る。**

```powershell
git switch -c tmp/ruleset-probe origin/main
```
```powershell
git commit --allow-empty -m "probe: ruleset verification (should never land)"
```
```powershell
git push origin HEAD:main
```

Expected: **拒否**。stderr に `[remote rejected]` と、`protected branch` もしくは `Changes must be made through a pull request` を含む。

続けて force-push も試す（V3）:

```powershell
git push origin HEAD:main --force
```

Expected: 同じく拒否。

> **ブラスト半径**: 万一 ruleset が効いておらず push が成功した場合、main に**空コミットが 1 つ**乗る。コード変更は無いので実害は最小だが、その場合は Step 3 の適用が失敗している。ruleset を修正したうえで、revert PR で main を戻すこと。

- [ ] **Step 6: 後片付け**

```powershell
git switch chore/hook-responsibility-layers
```
```powershell
git branch -D tmp/ruleset-probe
```

- [ ] **Step 7: コミットなし**

このタスクはリポジトリのファイルを変更しない。ruleset の状態は GitHub 側にある。Step 4 の出力を PR 本文に貼ること。

---

## Task 2: `.githooks/_lib.sh` + `pre-commit` + テスト基盤

**Files:**
- Create: `.githooks/_lib.sh`, `.githooks/pre-commit`, `.githooks/githooks.test.mjs`
- Modify: `vitest.config.ts`

**Interfaces:**
- Consumes: なし
- Produces:
  - `_lib.sh` が `PROTECTED_BRANCH`（値 `main`）、`die(message)`（stderr へ `BLOCKED: <message>` を出し exit 1）、`current_branch()`（detached HEAD では空文字列）を提供する
  - `githooks.test.mjs` が `git(cwd, args)`, `commitEmpty(dir, msg)`, `commitCount(dir)`, `initRepo(defaultBranch)`, `initBare()`, `enableHooks(dir)`, `expectBlocked(fn)` を持つ。Task 3・4・5 は**同一ファイルに追記する**ため import は不要。`export` を付けてはならない（使われない export になる）

- [ ] **Step 1: 失敗するテストを書く**

`.githooks/githooks.test.mjs` を新規作成する。

```js
// .githooks/githooks.test.mjs
//
// Layer 1（git ネイティブ hook）の実測テスト。
// 使い捨ての git リポジトリを作り、実際に commit / merge / rebase / push を試みる。
// 「拒否されること」と同じだけ「通ること」（誤爆しないこと）を確かめる。
// 誤爆は CLAUDE.md へ運用ルールとして転移する（#473 の教訓）。

import { describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HOOKS_DIR = path.dirname(fileURLToPath(import.meta.url));
// git config の値に \ を入れない（PowerShell/msys の境界で壊れる）
const HOOKS_DIR_POSIX = HOOKS_DIR.split(path.sep).join("/");
const T = 20_000; // git を数回起動するため既定 5s では足りない

function git(cwd, args) {
  return execFileSync("git", args, { cwd, encoding: "utf8", stdio: "pipe" });
}

function commitEmpty(dir, message) {
  return git(dir, ["commit", "--allow-empty", "-m", message]);
}

/** コミット数。「通る」テストが実際に何かを進めたことを表明するために使う。 */
function commitCount(dir) {
  return git(dir, ["rev-list", "--count", "HEAD"]).trim();
}

/** hook を無効のまま初期コミットまで済ませた repo を作る。 */
function initRepo(defaultBranch = "main") {
  const dir = mkdtempSync(path.join(tmpdir(), "snotra-githooks-"));
  git(dir, ["init", "-b", defaultBranch]);
  git(dir, ["config", "user.email", "test@example.com"]);
  git(dir, ["config", "user.name", "githooks test"]);
  git(dir, ["config", "commit.gpgsign", "false"]);
  writeFileSync(path.join(dir, "seed.txt"), "seed\n");
  git(dir, ["add", "seed.txt"]);
  git(dir, ["commit", "-m", "seed"]);
  return dir;
}

function initBare() {
  const dir = mkdtempSync(path.join(tmpdir(), "snotra-githooks-origin-"));
  git(dir, ["init", "--bare", "-b", "main"]);
  return dir.split(path.sep).join("/");
}

/** リポジトリの .githooks（実体）を hook として有効にする。 */
function enableHooks(dir) {
  git(dir, ["config", "core.hooksPath", HOOKS_DIR_POSIX]);
}

/** 操作が BLOCKED で拒否されたことを確かめる。 */
function expectBlocked(fn) {
  let error;
  try {
    fn();
  } catch (e) {
    error = e;
  }
  expect(error, "操作が拒否されなかった").toBeDefined();
  expect(`${error.stderr ?? ""}${error.stdout ?? ""}`).toContain("BLOCKED");
}

describe("pre-commit", () => {
  it("main 上の commit を拒否する", () => {
    const dir = initRepo();
    enableHooks(dir);
    expectBlocked(() => commitEmpty(dir, "should be blocked"));
  }, T);

  it("feature ブランチの commit は通る（誤爆しない）", () => {
    const dir = initRepo();
    enableHooks(dir);
    git(dir, ["switch", "-c", "feat/x"]);
    commitEmpty(dir, "ok");
    expect(commitCount(dir)).toBe("2");
  }, T);

  it("detached HEAD は判定不能なので通す（それでも main は進まない）", () => {
    const dir = initRepo();
    enableHooks(dir);
    git(dir, ["checkout", "--detach"]);
    commitEmpty(dir, "detached ok");
    expect(commitCount(dir)).toBe("2");
    // 通してよい理由の表明: detached の commit は main を動かさない
    expect(git(dir, ["rev-list", "--count", "main"]).trim()).toBe("1");
  }, T);

  it("別ツリーの cwd から `git -C` で main に commit しても拒否する", () => {
    const target = initRepo();
    enableHooks(target);
    const elsewhere = initRepo(); // cwd 側は無関係な repo（hook 無効）
    expectBlocked(() => git(elsewhere, ["-C", target, "commit", "--allow-empty", "-m", "x"]));
  }, T);
});
```

`vitest.config.ts` を変更する。

```ts
import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";

export default defineConfig({
  plugins: [solid({ hot: false })],
  test: {
    include: [
      "ui/src/**/*.test.{ts,tsx}",
      ".claude/hooks/**/*.test.mjs",
      ".githooks/**/*.test.mjs",
    ],
    environment: "node",
  },
});
```

- [ ] **Step 2: テストが落ちることを確認する（Red）**

```powershell
npx vitest run .githooks
```

Expected: 4 件中 2 件 FAIL。`main 上の commit を拒否する` と `git -C ...` が「操作が拒否されなかった」で落ちる（`core.hooksPath` の先に `pre-commit` が無いため commit が成功する）。`feature ブランチ` と `detached HEAD` は PASS。

**この非対称が重要**: 「通る」テストは hook が無くても通る。**hook を入れて初めて全部が緑になる。**

- [ ] **Step 3: `_lib.sh` を書く**

```sh
# .githooks/ の共通部品。各 hook から source される（単体では実行しない）。
#
# 守るもの: main が進むこと。守り方: 操作ごとに git が呼ぶ hook で、
# 「実際に操作されるツリー」のブランチを見て判定する。git は hook を
# working tree のトップを cwd として起動するため、`git -C <other>` でも
# worktree でも、判定対象は常に実際の操作先になる。
#
# この層は best-effort。core.hooksPath はローカル設定なので外れうる。
# 外れたときの最終防衛線は GitHub ruleset（main への直接 push を拒否）。
# ゆえに「この層が生きているか」を検知する仕組みは、意図的に作らない。

PROTECTED_BRANCH=main

# 拒否して終了する。exit 1 が git に操作を中止させる。
die() {
  printf 'BLOCKED: %s\n' "$1" >&2
  printf '  feature ブランチ（feat/ fix/ chore/）を作成してから操作してください。\n' >&2
  printf '  判定: .githooks/（ローカル） / 最終防衛線: GitHub ruleset\n' >&2
  printf '  意図的な操作なら --no-verify で迂回できます（人間専用。エージェントは使用禁止）。\n' >&2
  exit 1
}

# 現在のブランチ名。detached HEAD では空文字列を返す（＝判定不能）。
current_branch() {
  git symbolic-ref --short -q HEAD || true
}
```

- [ ] **Step 4: `pre-commit` を書く**

```sh
#!/bin/sh
# main 上での commit を拒否する。
. "$(dirname "$0")/_lib.sh"

# detached HEAD（rebase 中・bisect 中など）は判定できない。判定できないものは通す。
# その先の push は pre-push と ruleset が捕まえる。
[ "$(current_branch)" = "$PROTECTED_BRANCH" ] && die "main への直接コミットは禁止です。"
exit 0
```

- [ ] **Step 5: 実行ビットを index に記録する**

Windows の working tree には実行ビットが無い。**index に持たせないと Linux の CI で hook が実行されない。**

```powershell
git add .githooks/_lib.sh .githooks/pre-commit .githooks/githooks.test.mjs vitest.config.ts
```
```powershell
git update-index --chmod=+x .githooks/pre-commit
```

`_lib.sh` は source されるだけなので実行ビットは不要。

- [ ] **Step 6: テストが通ることを確認する（Green）**

```powershell
npx vitest run .githooks
```

Expected: 4 passed。

- [ ] **Step 7: コミット**

コミットメッセージは Write ツールで scratchpad に書き、`-F` で渡す（HEREDOC 禁止）。

```powershell
git commit -F "<scratchpad>/msg-task2.txt"
```

メッセージ本文:
```
feat(githooks): main 上の commit を拒否する pre-commit を追加

git ネイティブ hook は working tree のトップを cwd として起動されるため、
git -C でも worktree でも「実際に操作されるツリー」で判定できる。
Claude Code の hook が抱えていた cwd 判定のずれが構造的に起きない。

vitest の include に .githooks を追加し、使い捨て repo で実測する。
「拒否されること」と同じだけ「通ること」（誤爆しないこと）を固定した。

Refs #473
```

---

## Task 3: `pre-merge-commit` と `pre-rebase`

**Files:**
- Create: `.githooks/pre-merge-commit`, `.githooks/pre-rebase`
- Modify: `.githooks/githooks.test.mjs`

**Interfaces:**
- Consumes: `_lib.sh` の `PROTECTED_BRANCH` / `die()` / `current_branch()`、テストの `initRepo` / `enableHooks` / `expectBlocked` / `git` / `commitEmpty`
- Produces: なし（この 2 本を参照するタスクは無い）

- [ ] **Step 1: 失敗するテストを追加する**

`.githooks/githooks.test.mjs` の末尾に追記する。

```js
describe("pre-merge-commit", () => {
  it("main への非 FF マージ（マージコミットが生じる）を拒否する", () => {
    const dir = initRepo();
    git(dir, ["switch", "-c", "feat/x"]);
    commitEmpty(dir, "feat 1");
    git(dir, ["switch", "main"]);
    commitEmpty(dir, "main 1"); // ここで分岐させる（まだ hook 無効）
    enableHooks(dir);
    expectBlocked(() => git(dir, ["merge", "--no-ff", "feat/x", "-m", "merge"]));
  }, T);

  it("main への FF マージは通る（マージコミットが生じないので hook は呼ばれない）", () => {
    const dir = initRepo();
    git(dir, ["switch", "-c", "feat/x"]);
    commitEmpty(dir, "feat 1");
    git(dir, ["switch", "main"]);
    enableHooks(dir);
    git(dir, ["merge", "--ff-only", "feat/x"]);
    expect(commitCount(dir)).toBe("2");
  }, T);
});

describe("pre-rebase", () => {
  it("main そのものを rebase する操作を拒否する", () => {
    const dir = initRepo();
    git(dir, ["switch", "-c", "feat/x"]);
    commitEmpty(dir, "feat 1");
    git(dir, ["switch", "main"]);
    commitEmpty(dir, "main 1");
    enableHooks(dir);
    // main を feat/x の上に載せ替えようとする（$2 省略 → 対象は現在の main）
    expectBlocked(() => git(dir, ["rebase", "feat/x"]));
  }, T);

  it("feature を main の上に rebase するのは通る（誤爆しない）", () => {
    const dir = initRepo();
    git(dir, ["switch", "-c", "feat/x"]);
    commitEmpty(dir, "feat 1");
    git(dir, ["switch", "main"]);
    commitEmpty(dir, "main 1");
    enableHooks(dir);
    git(dir, ["switch", "feat/x"]);
    git(dir, ["rebase", "main"]);
    expect(commitCount(dir)).toBe("3");
  }, T);
});
```

- [ ] **Step 2: テストが落ちることを確認する（Red）**

```powershell
npx vitest run .githooks
```

Expected: 8 件中 2 件 FAIL（`非 FF マージ` と `main そのものを rebase`）。他の 6 件は PASS。

- [ ] **Step 3: `pre-merge-commit` を書く**

```sh
#!/bin/sh
# main 上でマージコミットが作られるのを拒否する（非 FF の `git pull` を含む）。
# FF マージはマージコミットを作らないため、この hook は呼ばれない＝素通りする。
. "$(dirname "$0")/_lib.sh"

[ "$(current_branch)" = "$PROTECTED_BRANCH" ] &&
  die "main へのマージコミットは禁止です（fast-forward なら通ります）。"
exit 0
```

- [ ] **Step 4: `pre-rebase` を書く**

```sh
#!/bin/sh
# main を rebase 対象とする操作を拒否する。
# 引数: $1 = upstream, $2 = rebase される側のブランチ（省略時は現在のブランチ）
. "$(dirname "$0")/_lib.sh"

target="$2"
if [ -z "$target" ]; then
  target="$(current_branch)"
fi

[ "$target" = "$PROTECTED_BRANCH" ] && die "main を rebase 対象にする操作は禁止です。"
exit 0
```

- [ ] **Step 5: 実行ビットを記録する**

```powershell
git add .githooks/pre-merge-commit .githooks/pre-rebase .githooks/githooks.test.mjs
```
```powershell
git update-index --chmod=+x .githooks/pre-merge-commit .githooks/pre-rebase
```

- [ ] **Step 6: テストが通ることを確認する（Green）**

```powershell
npx vitest run .githooks
```

Expected: 8 passed。

- [ ] **Step 7: コミット**

```powershell
git commit -F "<scratchpad>/msg-task3.txt"
```

メッセージ本文:
```
feat(githooks): pre-merge-commit と pre-rebase を追加

非 FF の git pull が main にマージコミットを作る経路を塞ぐ。これにより
CLAUDE.md の「main の同期は git pull --ff-only を使う」という運用ルールが
不要になる（あれは block-main-commit の誤爆を避けるための回避策だった）。

FF マージと feature の rebase が通ることをテストで固定した。

Refs #473
```

---

## Task 4: `pre-push`

**Files:**
- Create: `.githooks/pre-push`
- Modify: `.githooks/githooks.test.mjs`

**Interfaces:**
- Consumes: `_lib.sh`、テストの `initRepo` / `initBare` / `enableHooks` / `expectBlocked` / `git` / `commitEmpty`
- Produces: なし

- [ ] **Step 1: 失敗するテストを追加する**

`.githooks/githooks.test.mjs` の末尾に追記する。

```js
describe("pre-push", () => {
  it("main を宛先とする push を拒否する", () => {
    const origin = initBare();
    const dir = initRepo();
    git(dir, ["remote", "add", "origin", origin]);
    enableHooks(dir);
    expectBlocked(() => git(dir, ["push", "origin", "main"]));
  }, T);

  it("`HEAD:main` の形でも拒否する（source ではなく宛先で判定する）", () => {
    const origin = initBare();
    const dir = initRepo();
    git(dir, ["remote", "add", "origin", origin]);
    git(dir, ["switch", "-c", "feat/x"]);
    commitEmpty(dir, "feat 1");
    enableHooks(dir);
    expectBlocked(() => git(dir, ["push", "origin", "HEAD:main"]));
  }, T);

  it("main の削除 push も拒否する", () => {
    const origin = initBare();
    const dir = initRepo();
    git(dir, ["remote", "add", "origin", origin]);
    git(dir, ["push", "origin", "main"]); // hook 無効のうちに一度送る
    git(dir, ["switch", "-c", "feat/x"]);
    enableHooks(dir);
    expectBlocked(() => git(dir, ["push", "origin", ":main"]));
  }, T);

  it("feature ブランチの push は通る（誤爆しない）", () => {
    const origin = initBare();
    const dir = initRepo();
    git(dir, ["remote", "add", "origin", origin]);
    git(dir, ["switch", "-c", "feat/x"]);
    commitEmpty(dir, "feat 1");
    enableHooks(dir);
    git(dir, ["push", "origin", "feat/x"]);
    expect(git(dir, ["ls-remote", "origin", "refs/heads/feat/x"])).toContain("refs/heads/feat/x");
  }, T);
});
```

- [ ] **Step 2: テストが落ちることを確認する（Red）**

```powershell
npx vitest run .githooks
```

Expected: 12 件中 3 件 FAIL（拒否を期待する 3 件）。`feature ブランチの push は通る` は PASS。

- [ ] **Step 3: `pre-push` を書く**

```sh
#!/bin/sh
# 宛先が refs/heads/main の push を拒否する。
#
# source ではなく destination を見る。ゆえに `git push origin HEAD:main` も
# `git push origin :main`（削除）も捕まえる。これが「リポジトリの状態を守る」語彙。
#
# stdin: <local ref> <local sha> <remote ref> <remote sha>
. "$(dirname "$0")/_lib.sh"

while read -r _local_ref _local_sha remote_ref _remote_sha; do
  [ "$remote_ref" = "refs/heads/$PROTECTED_BRANCH" ] &&
    die "main への直接 push は禁止です（宛先: $remote_ref）。"
done
exit 0
```

- [ ] **Step 4: 実行ビットを記録する**

```powershell
git add .githooks/pre-push .githooks/githooks.test.mjs
```
```powershell
git update-index --chmod=+x .githooks/pre-push
```

- [ ] **Step 5: テストが通ることを確認する（Green）**

```powershell
npx vitest run .githooks
```

Expected: 12 passed。

- [ ] **Step 6: コミット**

```powershell
git commit -F "<scratchpad>/msg-task4.txt"
```

メッセージ本文:
```
feat(githooks): 宛先 refs/heads/main への push を拒否する pre-push を追加

CLAUDE.md 最重要ルール 1 は「main へ直接コミット・プッシュしない」だが、
block-main-commit の語彙は commit|merge|rebase のみで push を含まなかった。
ルールの半分に自動ガードが存在していなかった。

source ではなく destination の ref で判定するため、HEAD:main も :main も捕まえる。

Refs #473
```

---

## Task 5: 相対 `core.hooksPath` と worktree の実測（V10）

**Files:**
- Modify: `.githooks/githooks.test.mjs`

**Interfaces:**
- Consumes: 既存のテストヘルパ
- Produces: なし

**このタスクは前提の検証である。** 「相対 `core.hooksPath` は working tree のトップを基準に解決される」は git のドキュメントから読んだ**期待**であって、Windows + worktree での測定結果ではない。ここで転けたら Task 6 の bootstrap を絶対パス方式へ切り替える。

- [ ] **Step 1: 前提を測るテストを追加する**

まず `.githooks/githooks.test.mjs` の冒頭 import を差し替える。このタスクで初めて使う API を、このタスクで足す。

```js
import { chmodSync, cpSync, mkdtempSync, readdirSync, writeFileSync } from "node:fs";
```

そのうえで末尾に追記する。

```js
/** コピーした hook に実行ビットを付ける（Windows は shebang で判定されるので不要）。 */
function makeExecutable(dir) {
  if (process.platform === "win32") return;
  for (const name of readdirSync(dir)) {
    chmodSync(path.join(dir, name), 0o755);
  }
}

describe("相対 core.hooksPath — worktree での解決（V10）", () => {
  it("linked worktree でも .githooks が解決され、main 上の commit が拒否される", () => {
    // 既定ブランチを feat/base にして、main を linked worktree として切り出す
    const dir = initRepo("feat/base");
    cpSync(HOOKS_DIR, path.join(dir, ".githooks"), { recursive: true });
    makeExecutable(path.join(dir, ".githooks"));
    git(dir, ["add", ".githooks"]);
    git(dir, ["commit", "-m", "add hooks"]);

    // ★ 相対パス。main ツリーでも worktree でも同じ設定を共有する
    git(dir, ["config", "core.hooksPath", ".githooks"]);

    // main ツリー側: feat/base なので通る
    commitEmpty(dir, "feat/base ok");

    // worktree 側: main なので拒否される
    const wt = `${dir}-wt`;
    git(dir, ["worktree", "add", "-b", "main", wt]);
    expectBlocked(() => commitEmpty(wt, "worktree main commit"));
  }, T * 2);
});
```

- [ ] **Step 2: 実行して前提を確かめる**

```powershell
npx vitest run .githooks
```

Expected: 13 passed。

**FAIL した場合**: 相対パスが worktree で解決されていない。Task 6 の `prepare` を絶対パス方式へ切り替える:

```json
"prepare": "git config core.hooksPath \"$(git rev-parse --show-toplevel)/.githooks\""
```

この場合、worktree ごとに `npm install` が必要になる旨を `docs/build-commands.md` に明記すること。**測定結果に従い、期待に従わない。**

- [ ] **Step 3: コミット**

```powershell
git add .githooks/githooks.test.mjs
```
```powershell
git commit -F "<scratchpad>/msg-task5.txt"
```

メッセージ本文:
```
test(githooks): 相対 core.hooksPath が worktree で解決されることを実測

git のドキュメントから読んだ期待を、測定に置き換える。#471 の教訓
（安全網が効いていることは故障注入で一度は実測する）に従う。

linked worktree で main を checkout し、そこでの commit が
worktree 内の .githooks/pre-commit に拒否されることを固定した。
```

---

## Task 6: bootstrap（`core.hooksPath`）とドキュメント

**Files:**
- Modify: `package.json`, `docs/build-commands.md`

**Interfaces:**
- Consumes: Task 5 の測定結果（相対パスで良いか）
- Produces: 実リポジトリで `.githooks` が有効になった状態。以降の全 commit が Layer 1 を通る

- [ ] **Step 1: `package.json` に `prepare` を追加する**

`scripts` の先頭（`dev` の前）に置く。npm の `prepare` は `npm install` / `npm ci` の後に自動実行される。

```json
  "scripts": {
    "prepare": "git config core.hooksPath .githooks",
    "dev": "vite",
    "test": "vitest run",
    "smoke:startup": "pwsh -NoProfile -File scripts/smoke-startup.ps1",
    "prepare:sidecar": "pwsh -NoProfile -File scripts/prepare-sidecar.ps1 -Release",
    "e2e:tauri:setup": "cargo install tauri-driver --locked && npm run prepare:sidecar && npx tauri build --no-bundle",
    "e2e:tauri": "playwright test -c playwright.tauri.config.ts",
    "typecheck": "tsc",
    "prebuild": "npm run typecheck",
    "build": "vite build",
    "verify": "cargo check -p snotra-core -p snotra -p snotra-settings && npm run build",
    "preview": "vite preview",
    "tauri": "tauri"
  },
```

- [ ] **Step 2: 実行して有効化する**

```powershell
npm run prepare
```
```powershell
git config --get core.hooksPath
```

Expected: `.githooks`

- [ ] **Step 3: 実リポジトリで誤爆しないことを確かめる（V9 の一部）**

いま作業中の feature ブランチで、通常のコミットが通ることを確かめる。

```powershell
git commit --allow-empty -m "probe: feature ブランチの commit は通る"
```

Expected: 成功。

```powershell
git reset --hard HEAD~1
```

- [ ] **Step 4: `docs/build-commands.md` にカテゴリ E と bootstrap を追記する**

「変更後の検証チェックリスト」のカテゴリ D の直後に追加する。以下は `docs/build-commands.md` へ**そのまま貼る内容**（外側の 4 連バッククォートは囲みであり、貼らない）。

````markdown
### E. git hook（`.githooks/**`）を変更した場合

```bash
npm test    # 必須: 使い捨て repo で hook を実測する（.githooks/githooks.test.mjs）
```

- `.githooks/` は **main 保護のローカル層**。commit / merge / rebase / push の各操作で git が直接呼ぶため、ツール・シェル・worktree・`git -C` のいずれにも依存しない
- **bootstrap**: `npm install` / `npm ci` が `prepare` スクリプトで `git config core.hooksPath .githooks` を実行する。worktree は `.git/config` を共有するため一度で全 worktree に効く
- この層は best-effort。`core.hooksPath` が外れても **GitHub ruleset（`default`）が main への直接 push を拒否する**ため、外れたことを検知する仕組みは意図的に設けていない
````

- [ ] **Step 5: 全テストが通ることを確認する**

```powershell
npm test
```

Expected: `ui/src` と `.claude/hooks` と `.githooks` のテストがすべて PASS。

- [ ] **Step 6: コミット**

```powershell
git add package.json docs/build-commands.md
```
```powershell
git commit -F "<scratchpad>/msg-task6.txt"
```

メッセージ本文:
```
chore(githooks): core.hooksPath の bootstrap と検証カテゴリ E を追加

npm install / npm ci が prepare で core.hooksPath を設定する。worktree は
.git/config を共有するため一度で全 worktree に効く。

この層が外れても GitHub ruleset が push を拒むため、外れたことを検知する
仕組みは意図的に作らない（「安全網の不在を検知する安全網」の無限後退を断つ）。
```

---

## Task 7: 実環境での故障注入（V4 / V5 / V8 / V9）

**Files:** なし（測定のみ）

**Interfaces:**
- Consumes: Task 1〜6 のすべて
- Produces: 測定結果。PR 本文へ貼る

自動テストは使い捨て repo を測る。ここでは**実リポジトリと実ツールで**測る。とくに V5 は「PowerShell tool が `block-main-commit` を素通りする」という実測済みの抜け道が、Layer 1 で塞がったことを示す。

- [ ] **Step 1: V4 — Bash ツールで main 上の commit を試みる**

```powershell
git switch main
```

Bash ツールから:
```bash
git commit --allow-empty -m "V4 probe"
```

Expected: `block-main-commit`（まだ生存）が先に止める。**この時点ではどちらが止めたか区別できない**ため、次の Step で PowerShell を使う。

- [ ] **Step 2: V5 — PowerShell ツールで main 上の commit を試みる**

PowerShell ツールから（`block-main-commit` の `matcher` は `"Bash"` なので発火しない）:

```powershell
git commit --allow-empty -m "V5 probe"
```

Expected: **`BLOCKED: main への直接コミットは禁止です。`**（`.githooks/pre-commit` の出力）

**これが本 Phase の中心的な証拠である。** 前回の測定ではこのコマンドが素通りした。

- [ ] **Step 3: V8 — main を宛先とする push を試みる**

```powershell
git switch chore/hook-responsibility-layers
```
```powershell
git push origin HEAD:main
```

Expected: `BLOCKED: main への直接 push は禁止です（宛先: refs/heads/main）。`（Layer 1 が先に止める。仮に外しても Layer 0 が止める）

- [ ] **Step 4: V9 — 誤爆しないことを確かめる**

`git merge --ff-only origin/main` は、旧 `block-main-commit` が誤爆していた当のコマンドである。**PowerShell ツールから**実行する（Bash ツールだと旧 hook がまだ止めるため、Layer 1 の挙動が測れない）。

```powershell
git switch main
```
```powershell
git pull --ff-only
```
Expected: 成功（FF なのでマージコミットが生じず `pre-merge-commit` は呼ばれない）

```powershell
git merge --ff-only origin/main
```
Expected: 成功（`Already up to date.`）

```powershell
git switch chore/hook-responsibility-layers
```

- [ ] **Step 5: 測定結果を記録する**

Step 1〜4 の出力を scratchpad に保存し、PR 本文へ貼る。**V5 の出力は必ず含める。**

- [ ] **Step 6: コミットなし**

このタスクはファイルを変更しない。

---

## Task 8: `block-main-commit` を削除する

**Files:**
- Modify: `.claude/settings.json`, `.claude/hooks/post-edit.mjs`

**Interfaces:**
- Consumes: Task 7 の測定が全て緑であること（**前提条件。落ちていたら着手しない**）
- Produces: PreToolUse に PR 前 push チェックのみが残った状態

**`.claude/settings.json` の変更は CLAUDE.md 最重要ルール 4（エージェント設定の変更は合意してから）の対象。設計文書の承認をもって合意済みとする。**

- [ ] **Step 1: `.claude/settings.json` から `block-main-commit` を削除する**

`PreToolUse` の `hooks` 配列から最初の要素（`git\s+(commit|merge|rebase)` を grep するもの）だけを取り除く。PR 前 push チェックは**残す**。

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "input=$(cat); if echo \"$input\" | grep -qE 'gh\\s+pr\\s+create'; then up=$(git rev-parse --abbrev-ref --symbolic-full-name @{u} 2>/dev/null); if [ -z \"$up\" ] || [ -n \"$(git log @{u}..HEAD --oneline 2>/dev/null)\" ]; then echo 'BLOCKED: 未 push のコミット、または upstream 未設定です。git push -u origin HEAD してから gh pr create してください（空 PR / Closes 誤 close 防止）。' >&2; exit 2; fi; fi"
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "node \"${CLAUDE_PROJECT_DIR:-.}/.claude/hooks/post-edit.mjs\"",
            "timeout": 900
          }
        ]
      }
    ]
  },
  "enabledPlugins": {
    "rust-analyzer-lsp@claude-plugins-official": true
  }
}
```

この編集は `hook-selftest`（settings.json の JSON 検証 + `vitest run .claude/hooks`）を自動発火する。**沈黙は合格。**

- [ ] **Step 2: `post-edit.mjs` のコメントを事実訂正する**

`.claude/hooks/post-edit.mjs` の `validateSettings` 直前のコメント。**振る舞いは変更しない。**

変更前:
```js
/**
 * `.claude/settings.json` が壊れると、PostToolUse だけでなく block-main-commit を
 * 含む全 hook が停止する。パースは実質 0ms なので、hook 系を編集したら必ず見る。
 */
```

変更後:
```js
/**
 * `.claude/settings.json` が壊れると、PostToolUse だけでなく PR 前 push チェックを
 * 含む全 hook が停止する。パースは実質 0ms なので、hook 系を編集したら必ず見る。
 */
```

- [ ] **Step 3: 削除後も守られていることを再測する**

PowerShell ツールから:
```powershell
git switch main
```
```powershell
git commit --allow-empty -m "V4 after removal"
```
Expected: `BLOCKED: main への直接コミットは禁止です。`（`.githooks` が止めている）

```powershell
git switch chore/hook-responsibility-layers
```

- [ ] **Step 4: 誤爆が消えたことを測る**

**Bash ツールから**（旧 hook なら誤爆したコマンド）:
```bash
git switch main
```
```bash
git merge --ff-only origin/main
```
Expected: `Already up to date.`（旧 `block-main-commit` なら `BLOCKED` だった）

```bash
git switch chore/hook-responsibility-layers
```

- [ ] **Step 5: フックのセルフテストを走らせる**

```powershell
npx vitest run .claude/hooks
```

Expected: すべて PASS。

- [ ] **Step 6: コミット**

```powershell
git add .claude/settings.json .claude/hooks/post-edit.mjs
```
```powershell
git commit -F "<scratchpad>/msg-task8.txt"
```

メッセージ本文:
```
refactor(hooks): block-main-commit を削除し、main 保護を git 機構へ移す

Layer 0（GitHub ruleset）と Layer 1（.githooks/）が実測で立ったため、
Claude Code の hook から (A1) リポジトリ状態の保証を降ろす。

削除する理由は「不要になった」ではなく「そもそも守れていなかった」:
- PowerShell tool は matcher "Bash" に一致せず素通りする
- git -C / git -c / --no-pager / push / pull はいずれも語彙外
- payload 全体 grep のため description に該当語があるだけで誤爆する

PR 前 push チェック（外部 API への不可逆呼び出し）は hook にしか見えない
領域なので残す。その構造化は Phase 2。

Refs #471, #473
```

---

## Task 9: CLAUDE.md を整理する

**Files:**
- Modify: `CLAUDE.md`

**Interfaces:**
- Consumes: Task 8（hook が削除済みであること）
- Produces: なし

- [ ] **Step 1: 最重要ルール 2 を narrow する（`CLAUDE.md:16`）**

番号 1〜4 は保つ（issue 本文が番号で参照しているため）。

変更前:
```markdown
1. **`main` へ直接コミット・プッシュしない** — 必ず feature ブランチ（`feat/<機能名>` / `fix/<バグ名>` / `chore/<作業名>`）を作成してからコミットする
2. **`git` コマンドを `&&` でチェーンしない** — 1操作 = 1呼び出し（→「Git/GitHub 運用」）
```

変更後:
```markdown
1. **`main` へ直接コミット・プッシュしない** — 必ず feature ブランチ（`feat/<機能名>` / `fix/<バグ名>` / `chore/<作業名>`）を作成してからコミットする。強制するのは `.githooks/`（ローカル）と GitHub ruleset（最終防衛線）であり、Claude Code の hook ではない
2. **`gh pr create` を他のコマンドとチェーンしない** — PR 前 push チェック hook はコマンド実行の**前**に upstream を評価するため、`git push -u origin HEAD && gh pr create` は必ずブロックされる（→「Git/GitHub 運用」）
```

- [ ] **Step 2: Git/GitHub 運用の 2 項目を置き換える（`CLAUDE.md:37-38`）**

変更前:
```markdown
- **`git` コマンドをチェーンしない** — `checkout` と `rebase`、`add` と `commit` のように影響範囲の異なる操作はそれぞれ独立した呼び出しに分ける。`git checkout <branch> && git rebase main` のような連鎖は `block-main-commit` フックを誤発火させた実績がある
- **main の fast-forward 同期は `git pull --ff-only` を使う** — `git merge --ff-only origin/main` はコミットを作らない FF でも `block-main-commit` フックに弾かれる（コマンド文字列一致で判定するため）
```

変更後:
```markdown
- **main 保護は `.githooks/` と GitHub ruleset が担う** — `.githooks/{pre-commit,pre-merge-commit,pre-rebase,pre-push}` が commit / merge / rebase / push を、GitHub ruleset `default` が origin 側の直接 push・force-push・削除を拒否する。git が hook を「操作されるツリーのトップ」を cwd として呼ぶため、PowerShell でも `git -C` でも worktree でも subworktree でも判定は正しい。bootstrap は `npm install`（`prepare` が `core.hooksPath` を設定する）
- **`--no-verify` は人間専用** — `.githooks/` を迂回する。Claude は使用してはならない。ローカルの hook を外しても GitHub ruleset が push を拒むため、迂回しても main には届かない
- **`gh pr create` を他のコマンドとチェーンしない** — PR 前 push チェック hook は `tool_input` 全体を grep したうえで、コマンド実行の**前**に `@{u}` を評価する。`git push -u origin HEAD && gh pr create` は upstream 未設定と判定されて必ずブロックされる（この誤爆の根治は Phase 2）
```

- [ ] **Step 3: フック表から `block-main-commit` の行を削除する（`CLAUDE.md:49`）**

削除する行:
```markdown
| `block-main-commit`（PreToolUse） | main ブランチ上の `git commit` / `merge` / `rebase` | feature ブランチを作成してから操作する |
```

そのうえで、表の直前の説明文（`CLAUDE.md:45`）の末尾に一文を足す。

変更前:
```markdown
エージェントの操作には以下のフックが介入する。PreToolUse の発火条件は `.claude/settings.json` を、PostToolUse の発火条件と検査対応表は **`.claude/hooks/post-edit.mjs` の `selectChecks`** を SSOT とする。
```

変更後:
```markdown
エージェントの操作には以下のフックが介入する。PreToolUse の発火条件は `.claude/settings.json` を、PostToolUse の発火条件と検査対応表は **`.claude/hooks/post-edit.mjs` の `selectChecks`** を SSOT とする。**main 保護はここに無い** — リポジトリの状態は hook の視界の外にあるため、`.githooks/` と GitHub ruleset が担う（→「Git/GitHub 運用」）。
```

- [ ] **Step 4: 変更後の CLAUDE.md を通読して整合を確認する**

- `CLAUDE.md:76` の「（→「最重要ルール」1）」は 1 番のまま有効
- 「シェル環境」表の HEREDOC 行は最重要ルール 3 と対応しており、番号は変わらない
- `.claude/skills/start-issue/SKILL.md:45` の `git pull --ff-only` は**そのまま**。main の同期コマンドとして正しく、回避策ではなくなっただけ

- [ ] **Step 5: コミット**

```powershell
git add CLAUDE.md
```
```powershell
git commit -F "<scratchpad>/msg-task9.txt"
```

メッセージ本文:
```
docs(claude-md): main 保護の担い手を .githooks + ruleset に書き換える

削除した運用ルール:
- 「git コマンドを && でチェーンしない」の一般形（block-main-commit の
  誤爆回避策だった。git ネイティブ hook は実行時に実ツリーで判定するため
  チェーンしても誤爆しない）
- 「main の fast-forward 同期は git pull --ff-only を使う」（同上。
  非 FF の pull は pre-merge-commit が本物の判定で止める）

残した narrow な規則:
- 「gh pr create を他のコマンドとチェーンしない」— PR 前 push チェック hook
  はコマンド実行前に upstream を評価するため、この誤爆は Phase 2 まで実在する

コードのバグが文書ルールへ転移していた。バグを直したので文書が減る。
```

---

## Task 10: PR と issue の処遇

**Files:** なし（GitHub 側）

**Interfaces:**
- Consumes: Task 1〜9
- Produces: レビュー可能な PR

- [ ] **Step 1: 全検証を走らせる**

```powershell
npm test
```
Expected: 全 PASS（`ui/src` + `.claude/hooks` + `.githooks`）

```powershell
npm run build
```
Expected: 成功

Rust には触れていないためカテゴリ A は不要（`docs/build-commands.md` の該当カテゴリは B・E）。

- [ ] **Step 2: push する**

```powershell
git push -u origin HEAD
```

Expected: 成功（宛先は `refs/heads/chore/hook-responsibility-layers` なので `pre-push` は通す）

- [ ] **Step 3: PR 本文を書く**

Write ツールで scratchpad に `pr-body.md` を作る。以下を必ず含める。

- 設計文書へのリンク（`docs/superpowers/specs/2026-07-09-hook-responsibility-layers-design.md`）
- **Task 1 Step 4 の read-back 出力**（ruleset が active であること）
- **Task 1 Step 5 の V1/V3 出力**（main への push が server で拒否されたこと）
- **Task 7 Step 2 の V5 出力**（PowerShell ツールからの commit が `.githooks` に拒否されたこと）
- **Task 8 Step 4 の出力**（`git merge --ff-only origin/main` が Bash ツールから通ること＝誤爆の消滅）
- Spec からの逸脱 2 点（最重要ルール 2 の narrow、`post-edit.mjs` のコメント訂正）
- PR ライフサイクル内のチェックリスト:
  - [ ] CI グリーン確認
  - [ ] `gh pr merge --squash` が動作すること（＝V2 の実測。マージ時に確認）

`Closes` は書かない。#473 は Phase 2 用に書き換えて残す。

- [ ] **Step 4: PR を作る**

**他のコマンドとチェーンしない**（Global Constraints）。

```powershell
gh pr create --title "refactor(hooks): main 保護を git/GitHub の機構へ移し block-main-commit を削除する (#473)" --body-file "<scratchpad>/pr-body.md"
```

- [ ] **Step 5: #473 を Phase 2 用に書き換える**

`gh issue edit 473` で本文を差し替える。残すのは PR 前 push チェック側のみ:

- `tool_input` 全体を grep している（`description` に `gh pr create` と書くだけで発火する）
- `matcher` が `"Bash"` のみで、**PowerShell tool から `gh pr create` を叩くと素通りする**
- 期待する解: `pre-bash.mjs` を作り、`tool_input.command` だけを見て、`matcher` を `Bash|PowerShell` に広げ、判定不能なら fail-closed に倒す
- タイトルも `fix(hooks): PR 前 push チェックの判定情報と検査対象がずれている（PowerShell 素通り・payload 全体 grep）` へ変更

`block-main-commit` に関する記述（§1 前半・§2）は削除する。**本 PR で hook ごと消えたため。**

- [ ] **Step 6: 新規 issue を起票する**

タイトル: `fix(hooks): PreToolUse の matcher "Bash" は PowerShell tool に一致しない`

本文に含めること:
- 実測: PowerShell ツールから `git commit` を含むコマンドが `block-main-commit` を素通りした
- この環境の primary shell は PowerShell である
- 残る PR 前 push チェックも同じ穴を持つ
- Phase 2（`pre-bash.mjs`）で `matcher: "Bash|PowerShell"` として解消する
- `Refs #473`

- [ ] **Step 7: マージは手動**

`gh pr merge --squash` で main へ入れる。**これ自体が V2 の実測**（`pull_request` 規則の下で squash マージが動くこと）。

`--subject` に `(#PR)` を付け、本文で `Closes` を制御する（CLAUDE.md「Git/GitHub 運用」）。#473 は **open のまま残す**（Phase 2 で使う）。

---

## Self-Review

**Spec 網羅性:**

| spec の節 | 対応タスク |
|---|---|
| §3 Layer 0（ruleset） | Task 1 |
| §4 Layer 1（`.githooks/` 4 本 + `_lib.sh`） | Task 2, 3, 4 |
| §4 bootstrap | Task 6 |
| §4 エスケープハッチ | Task 9 Step 2（`--no-verify` を CLAUDE.md に明記） |
| §5 `block-main-commit` 削除 | Task 8 |
| §5 CLAUDE.md から消える 2 ルール | Task 9（**1 つは narrow に変更 — 逸脱として明記**） |
| §6 V1 / V3 | Task 1 Step 5 |
| §6 V2 | Task 10 Step 7（マージ時に実測） |
| §6 V4 / V5 / V8 / V9 | Task 7 |
| §6 V6 | Task 2 Step 1（`git -C` テスト） |
| §6 V7 | Task 3 Step 1（非 FF マージテスト） |
| §6 V10 | Task 5 |
| §7 実施順序 | Task 番号がそのまま順序 |
| §9 issue の処遇 | Task 10 Step 5, 6 |

**逸脱 2 件は「Spec からの逸脱」節に明記済み。**

**型・名前の一貫性:** sh 側は `PROTECTED_BRANCH` / `die` / `current_branch`（Task 2 の `_lib.sh` で定義、Task 3・4 が source）。js 側は `git` / `commitEmpty` / `commitCount` / `initRepo` / `initBare` / `enableHooks` / `expectBlocked` / `HOOKS_DIR` / `HOOKS_DIR_POSIX` / `T` が Task 2 で定義され、Task 3・4・5 で同名のまま使われる（同一ファイルへの追記なので import は不要、`export` も付けない）。`makeExecutable` のみ Task 5 で定義される。

**テスト衛生:** 「通る」ことを確かめるテストはすべて `expect` を持つ。`detached HEAD` のテストは `commitCount` に加えて `main` のコミット数が増えないことを表明する（通してよい理由そのものの表明）。

**プレースホルダ:** `<scratchpad>` は実行時のセッション scratchpad ディレクトリを指す。それ以外に TBD / TODO は無い。
