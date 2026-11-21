// scripts/delete-merged-branch.test.mjs
//
// delete-merged-branch.mjs の実測テスト（issue #1507）。使い捨ての git リポジトリで、判定の理由ごとに 1 ケースを置く。
// 守る不変条件: `origin/main` にも `git branch -d` の基準にも無いコミットを持つブランチは消さない。
// ブランチの始点には sha を渡す。追跡ブランチを名前で渡すと upstream が付き、修正前の `-d` でも通って偽の緑になる。

import { afterAll, describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { deleteMergedBranch, REASON_NOT_IN_ORIGIN, REASON_UNDECIDABLE } from "./delete-merged-branch.mjs";

const T = 20_000;

const scratchDirs = [];
afterAll(() => {
  for (const dir of scratchDirs) rmSync(dir, { recursive: true, force: true });
});

function git(cwd, args) {
  return execFileSync("git", args, { cwd, encoding: "utf8", stdio: "pipe" }).trim();
}

function initRepo() {
  const dir = mkdtempSync(path.join(tmpdir(), "snotra-del-branch-"));
  scratchDirs.push(dir);
  git(dir, ["init", "-b", "main"]);
  git(dir, ["config", "user.email", "test@example.com"]);
  git(dir, ["config", "user.name", "delete-merged-branch test"]);
  git(dir, ["config", "commit.gpgsign", "false"]);
  writeFileSync(path.join(dir, "seed.txt"), "seed\n");
  git(dir, ["add", "seed.txt"]);
  git(dir, ["commit", "-m", "seed"]);
  return dir;
}

/** HEAD（main）の上に、HEAD からは届かないコミットを 1 つ作って sha を返す（HEAD は動かさない） */
function commitOffHead(dir, name) {
  git(dir, ["checkout", "-q", "-b", "tmp-off-head"]);
  writeFileSync(path.join(dir, name), `${name}\n`);
  git(dir, ["add", name]);
  git(dir, ["commit", "-q", "-m", name]);
  const sha = git(dir, ["rev-parse", "HEAD"]);
  git(dir, ["checkout", "-q", "main"]);
  git(dir, ["branch", "-D", "tmp-off-head"]);
  return sha;
}

const exists = (dir, b) => git(dir, ["branch", "--list", b]) !== "";

describe("deleteMergedBranch", () => {
  it("HEAD に merged なら git branch -d で消す", { timeout: T }, () => {
    const repo = initRepo();
    git(repo, ["branch", "b", git(repo, ["rev-parse", "HEAD"])]);

    expect(deleteMergedBranch(repo, "b")).toEqual({ deleted: true, basis: "-d" });
    expect(exists(repo, "b")).toBe(false);
  });

  it("HEAD が古くても、origin/main の祖先なら消す（ローカルの main が古い状態・#1507）", { timeout: T }, () => {
    const repo = initRepo();
    const tip = commitOffHead(repo, "upstream.txt");
    git(repo, ["update-ref", "refs/remotes/origin/main", tip]);
    git(repo, ["branch", "b", tip]);

    expect(deleteMergedBranch(repo, "b")).toEqual({ deleted: true, basis: "origin/main" });
    expect(exists(repo, "b")).toBe(false);
  });

  it("origin/main に無いコミットがあれば消さず、その理由を返す", { timeout: T }, () => {
    const repo = initRepo();
    git(repo, ["update-ref", "refs/remotes/origin/main", commitOffHead(repo, "upstream.txt")]);
    git(repo, ["branch", "b", commitOffHead(repo, "own.txt")]);

    const r = deleteMergedBranch(repo, "b");

    expect(r.deleted).toBe(false);
    expect(r.reason).toBe(REASON_NOT_IN_ORIGIN);
    expect(exists(repo, "b")).toBe(true);
  });

  it("origin/main が無く -d も拒むなら消さず、判定できないと返す（祖先でないとは言わない）", { timeout: T }, () => {
    const repo = initRepo();
    git(repo, ["branch", "b", commitOffHead(repo, "own.txt")]);

    const r = deleteMergedBranch(repo, "b");

    expect(r.deleted).toBe(false);
    expect(r.reason).toBe(REASON_UNDECIDABLE);
    expect(r.detail).not.toBe(""); // -d の stderr（文言は git のロケールで変わるので照合しない）
    expect(exists(repo, "b")).toBe(true);
  });
});
