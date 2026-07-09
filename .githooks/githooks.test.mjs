// .githooks/githooks.test.mjs
//
// Layer 1（git ネイティブ hook）の実測テスト。
// 使い捨ての git リポジトリを作り、実際に commit / merge / rebase / push を試みる。
// 「拒否されること」と同じだけ「通ること」（誤爆しないこと）を確かめる。
// 誤爆は CLAUDE.md へ運用ルールとして転移する（#473 の教訓）。

import { afterAll, describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HOOKS_DIR = path.dirname(fileURLToPath(import.meta.url));
// git config の値に \ を入れない（PowerShell/msys の境界で壊れる）
const HOOKS_DIR_POSIX = HOOKS_DIR.split(path.sep).join("/");
const T = 20_000; // git を数回起動するため既定 5s では足りない

// 作った使い捨てディレクトリはここに集め、最後にまとめて消す
// （`.claude/hooks/post-edit.test.mjs` と同じ作法）。Task 3・4・5 が
// 足すテストも scratchDir を通す限り自動で片付く。
const scratchDirs = [];
afterAll(() => {
  for (const dir of scratchDirs) rmSync(dir, { recursive: true, force: true });
});

function scratchDir(prefix) {
  const dir = mkdtempSync(path.join(tmpdir(), prefix));
  scratchDirs.push(dir);
  return dir;
}

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
  const dir = scratchDir("snotra-githooks-");
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
  const dir = scratchDir("snotra-githooks-origin-");
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
