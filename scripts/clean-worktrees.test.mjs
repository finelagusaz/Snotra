// scripts/clean-worktrees.test.mjs
//
// clean-worktrees.mjs の実測テスト（issue #517）。
// 使い捨ての git リポジトリに実際の worktree を作って走らせる
// （.githooks/githooks.test.mjs と同じ作法）。
// 守る不変条件: 未コミット変更のある worktree は --force なしでは絶対に消えない。

import { afterAll, describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT = path.join(path.dirname(fileURLToPath(import.meta.url)), "clean-worktrees.mjs");
const T = 20_000;

const scratchDirs = [];
afterAll(() => {
  for (const dir of scratchDirs) rmSync(dir, { recursive: true, force: true });
});

function git(cwd, args) {
  return execFileSync("git", args, { cwd, encoding: "utf8", stdio: "pipe" });
}

function initRepo() {
  const dir = mkdtempSync(path.join(tmpdir(), "snotra-clean-wt-"));
  scratchDirs.push(dir);
  git(dir, ["init", "-b", "main"]);
  git(dir, ["config", "user.email", "test@example.com"]);
  git(dir, ["config", "user.name", "clean-worktrees test"]);
  git(dir, ["config", "commit.gpgsign", "false"]);
  writeFileSync(path.join(dir, "seed.txt"), "seed\n");
  git(dir, ["add", "seed.txt"]);
  git(dir, ["commit", "-m", "seed"]);
  return dir;
}

/** `.claude/worktrees/agent-<id>` に worktree-agent-<id> ブランチの worktree を作る。 */
function addAgentWorktree(dir, id) {
  const wt = path.join(dir, ".claude", "worktrees", `agent-${id}`);
  git(dir, ["worktree", "add", wt, "-b", `worktree-agent-${id}`]);
  return wt;
}

/** スクリプトを実行し { status, stdout+stderr } を返す（非ゼロ exit でも throw しない）。 */
function runScript(cwd, args = []) {
  try {
    const stdout = execFileSync(process.execPath, [SCRIPT, ...args], {
      cwd,
      encoding: "utf8",
      stdio: "pipe",
    });
    return { status: 0, output: stdout };
  } catch (e) {
    return { status: e.status, output: `${e.stdout ?? ""}${e.stderr ?? ""}` };
  }
}

function branches(dir) {
  return git(dir, ["branch", "--format=%(refname:short)"]).trim().split("\n");
}

describe("clean-worktrees", () => {
  it("クリーンな agent worktree は削除され、ブランチも消える", { timeout: T }, () => {
    const repo = initRepo();
    const wt = addAgentWorktree(repo, "clean1");

    const r = runScript(repo);

    expect(r.status).toBe(0);
    expect(existsSync(wt)).toBe(false);
    expect(branches(repo)).not.toContain("worktree-agent-clean1");
  });

  it("未コミット変更のある worktree はスキップされ、非ゼロ exit で報告される", { timeout: T }, () => {
    const repo = initRepo();
    const wt = addAgentWorktree(repo, "dirty1");
    writeFileSync(path.join(wt, "wip.txt"), "uncommitted\n");

    const r = runScript(repo);

    expect(existsSync(path.join(wt, "wip.txt"))).toBe(true);
    expect(branches(repo)).toContain("worktree-agent-dirty1");
    expect(r.status).not.toBe(0);
    expect(r.output).toContain("agent-dirty1");
  });

  it("--force なら dirty worktree も削除される", { timeout: T }, () => {
    const repo = initRepo();
    const wt = addAgentWorktree(repo, "dirty2");
    writeFileSync(path.join(wt, "wip.txt"), "uncommitted\n");

    const r = runScript(repo, ["--force"]);

    expect(r.status).toBe(0);
    expect(existsSync(wt)).toBe(false);
    expect(branches(repo)).not.toContain("worktree-agent-dirty2");
  });

  it(".claude/worktrees/agent-* 以外の worktree には触れない", { timeout: T }, () => {
    const repo = initRepo();
    const other = path.join(repo, "other-worktree");
    git(repo, ["worktree", "add", other, "-b", "feature-x"]);

    const r = runScript(repo);

    expect(r.status).toBe(0);
    expect(existsSync(other)).toBe(true);
    expect(branches(repo)).toContain("feature-x");
  });

  it("未マージコミットのあるブランチは worktree 削除後も残り、--force の再実行で孤児として消える", { timeout: T }, () => {
    const repo = initRepo();
    const wt = addAgentWorktree(repo, "unmerged1");
    writeFileSync(path.join(wt, "work.txt"), "done\n");
    git(wt, ["add", "work.txt"]);
    git(wt, ["commit", "-m", "agent work"]);

    // 1回目: worktree はクリーンなので消えるが、未マージブランチは -d が拒否して残る
    const r1 = runScript(repo);
    expect(existsSync(wt)).toBe(false);
    expect(branches(repo)).toContain("worktree-agent-unmerged1");
    expect(r1.status).not.toBe(0);

    // 2回目 --force: worktree の無い孤児ブランチも掃討される
    const r2 = runScript(repo, ["--force"]);
    expect(r2.status).toBe(0);
    expect(branches(repo)).not.toContain("worktree-agent-unmerged1");
  });

  it("agent worktree が worktree-agent-* 以外のブランチを checkout していたらブランチには触れない", { timeout: T }, () => {
    const repo = initRepo();
    const wt = path.join(repo, ".claude", "worktrees", "agent-oddbranch");
    git(repo, ["worktree", "add", wt, "-b", "keep-me"]);

    const r = runScript(repo);

    expect(r.status).toBe(0);
    expect(existsSync(wt)).toBe(false);
    expect(branches(repo)).toContain("keep-me");
  });

  it("対象ゼロなら exit 0 で「対象なし」を表示する", { timeout: T }, () => {
    const repo = initRepo();

    const r = runScript(repo);

    expect(r.status).toBe(0);
    expect(r.output).toContain("対象なし");
  });
});
