// scripts/verify-tree.test.mjs
//
// verify-tree.mjs の実測テスト（issue #1298）。
// 使い捨ての git リポジトリに実際の worktree を作って走らせる（clean-worktrees.test.mjs と同じ作法）。
// 守る不変条件: 対象が無いまま緑を返さない（コマンド 0 件・未コミット・HEAD の食い違いは exit 2）。

import { afterAll, describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseCategoryCommands } from "./verify-tree.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SCRIPT = path.join(HERE, "verify-tree.mjs");
const T = 60_000;

const scratchDirs = [];
afterAll(() => {
  for (const dir of scratchDirs) rmSync(dir, { recursive: true, force: true });
});

function git(cwd, args) {
  return execFileSync("git", args, { cwd, encoding: "utf8", stdio: "pipe" }).trim();
}

/** 本物と同じ見出しとフェンスの形を持つ検証チェックリスト。各ブロックは無害な node の 1 行 */
function checklist({ e = 'node -e "process.exit(0)"', withCFence = true } = {}) {
  return [
    "## 変更後の検証チェックリスト",
    "",
    "### A. a",
    "",
    "```bash",
    'node -e "process.exit(0)"   # 必須: a',
    "```",
    "",
    "### B. b",
    "",
    "コマンドを持たない。",
    "",
    "### C. c",
    "",
    ...(withCFence ? ["```bash", 'node -e "process.exit(0)"   # 必須: c', "```"] : ["散文だけ。"]),
    "",
    "### D. d",
    "",
    "```powershell",
    "npm run smoke:manual",
    "```",
    "",
    "### E. e",
    "",
    "```bash",
    `${e}   # 必須: e`,
    "```",
    "",
    "### F. f",
    "",
    "```bash",
    'node -e "0"   # 必須: f（E と字面を変える——同じコマンドは 1 回しか回らない）',
    "```",
    "",
    "## その他",
    "",
  ].join("\n");
}

function initRepo(opts = {}) {
  const dir = mkdtempSync(path.join(tmpdir(), "snotra-verify-tree-"));
  scratchDirs.push(dir);
  git(dir, ["init", "-b", "main"]);
  git(dir, ["config", "user.email", "test@example.com"]);
  git(dir, ["config", "user.name", "verify-tree test"]);
  git(dir, ["config", "commit.gpgsign", "false"]);
  mkdirSync(path.join(dir, "docs"));
  writeFileSync(path.join(dir, "docs", "build-commands.md"), checklist(opts));
  writeFileSync(path.join(dir, "package.json"), JSON.stringify({ scripts: { ok: 'node -e "process.exit(0)"' } }));
  writeFileSync(path.join(dir, ".gitignore"), ".claude/worktrees/\nnode_modules/\n");
  git(dir, ["add", "."]);
  git(dir, ["commit", "-m", "seed"]);
  git(dir, ["checkout", "-q", "-b", "feat/x"]);
  return dir;
}

function commitFile(dir, name, body) {
  writeFileSync(path.join(dir, name), body);
  git(dir, ["add", name]);
  git(dir, ["commit", "-m", name]);
  return git(dir, ["rev-parse", "HEAD"]);
}

/** スクリプトを実行し { status, output } を返す（非ゼロ exit でも throw しない） */
function runScript(cwd, args) {
  try {
    const out = execFileSync(process.execPath, [SCRIPT, ...args], { cwd, encoding: "utf8", stdio: "pipe" });
    return { status: 0, output: out };
  } catch (e) {
    return { status: e.status, output: `${e.stdout ?? ""}${e.stderr ?? ""}` };
  }
}

const verifyTree = (repo) => path.join(repo, ".claude", "worktrees", "agent-verify-feat-x");
const summaryOf = (repo) => JSON.parse(readFileSync(`${verifyTree(repo)}.summary.json`, "utf8"));

describe("parseCategoryCommands — 実際の docs/build-commands.md", () => {
  const doc = readFileSync(path.join(HERE, "..", "docs", "build-commands.md"), "utf8");

  it("道具が回すカテゴリ（A・C・E・F）は、どれもコマンドを 1 件以上持つ", () => {
    for (const c of ["A", "C", "E", "F"]) {
      const r = parseCategoryCommands(doc, c);
      expect(r.findings, c).toEqual([]);
      expect(r.commands.length, c).toBeGreaterThan(0);
    }
  });

  it("コメントを落とし、コマンドの本体だけを返す", () => {
    const a = parseCategoryCommands(doc, "A").commands;
    expect(a).toContain("cargo fmt --all -- --check");
    expect(a.every((c) => !c.includes("#"))).toBe(true);
  });

  it("C は実バイナリの起動より前に本体をビルドする（docs/build-commands.md の C 節の前提）", () => {
    const c = parseCategoryCommands(doc, "C").commands;
    expect(c[0]).toBe("cargo build -p snotra");
  });
});

describe("verify-tree run", () => {
  it("要求が無くても E と F を回し、要約が sha と木の HEAD を名乗る", { timeout: T }, () => {
    const repo = initRepo();
    const sha = git(repo, ["rev-parse", "HEAD"]);

    const r = runScript(repo, ["run", "--sha", sha]);

    expect(r.status, r.output).toBe(0);
    const s = summaryOf(repo);
    expect(s.sha).toBe(sha);
    expect(s.categories_run).toEqual(["E", "F"]);
    expect(s.results.map((x) => x.exit)).toEqual([0, 0]);
    expect(git(verifyTree(repo), ["rev-parse", "HEAD"])).toBe(sha);
    expect(s.worktree.includes("\\")).toBe(false);
  });

  it("赤いコマンドがあれば exit 1 にし、残りのコマンドも回す", { timeout: T }, () => {
    const repo = initRepo({ e: 'node -e "process.exit(3)"' });
    const sha = git(repo, ["rev-parse", "HEAD"]);

    const r = runScript(repo, ["run", "--sha", sha]);

    expect(r.status, r.output).toBe(1);
    expect(summaryOf(repo).results.map((x) => [x.category, x.exit])).toEqual([
      ["E", 3],
      ["F", 0],
    ]);
  });

  it("要求したカテゴリのコマンドが 0 件なら exit 2 にし、木を作らない", { timeout: T }, () => {
    const repo = initRepo({ withCFence: false });
    const sha = git(repo, ["rev-parse", "HEAD"]);

    const r = runScript(repo, ["run", "--sha", sha, "--categories", "C"]);

    expect(r.status, r.output).toBe(2);
    expect(existsSync(verifyTree(repo))).toBe(false);
  });

  it("本体に未コミットの変更があれば exit 2（コミットし忘れた変更を緑で返さない）", { timeout: T }, () => {
    const repo = initRepo();
    const sha = git(repo, ["rev-parse", "HEAD"]);
    writeFileSync(path.join(repo, "wip.txt"), "uncommitted\n");

    const r = runScript(repo, ["run", "--sha", sha]);

    expect(r.status, r.output).toBe(2);
    expect(existsSync(verifyTree(repo))).toBe(false);
  });

  it("sha が本体の HEAD でなければ exit 2", { timeout: T }, () => {
    const repo = initRepo();
    const old = git(repo, ["rev-parse", "HEAD"]);
    commitFile(repo, "next.txt", "next\n");

    const r = runScript(repo, ["run", "--sha", old]);

    expect(r.status, r.output).toBe(2);
  });

  it("2 巡目は同じ木を新しい sha へ合わせて再利用する", { timeout: T }, () => {
    const repo = initRepo();
    expect(runScript(repo, ["run", "--sha", git(repo, ["rev-parse", "HEAD"])]).status).toBe(0);
    const next = commitFile(repo, "next.txt", "next\n");

    const r = runScript(repo, ["run", "--sha", next]);

    expect(r.status, r.output).toBe(0);
    expect(git(verifyTree(repo), ["rev-parse", "HEAD"])).toBe(next);
    expect(summaryOf(repo).sha).toBe(next);
  });

  it("再利用する木に追跡ファイルの変更が残っていれば exit 2", { timeout: T }, () => {
    const repo = initRepo();
    expect(runScript(repo, ["run", "--sha", git(repo, ["rev-parse", "HEAD"])]).status).toBe(0);
    writeFileSync(path.join(verifyTree(repo), "package.json"), "{}");
    const next = commitFile(repo, "next.txt", "next\n");

    const r = runScript(repo, ["run", "--sha", next]);

    expect(r.status, r.output).toBe(2);
  });

  it("git の想定外の失敗（再利用する木の checkout が未追跡ファイルと衝突）は、赤（1）ではなく 2", { timeout: T }, () => {
    const repo = initRepo();
    expect(runScript(repo, ["run", "--sha", git(repo, ["rev-parse", "HEAD"])]).status).toBe(0);
    writeFileSync(path.join(verifyTree(repo), "next.txt"), "untracked in the tree\n");
    const next = commitFile(repo, "next.txt", "next\n");

    const r = runScript(repo, ["run", "--sha", next]);

    expect(r.status, r.output).toBe(2);
  });

  it("D は回さず handoff と書く。B は受け付けない", { timeout: T }, () => {
    const repo = initRepo();
    const sha = git(repo, ["rev-parse", "HEAD"]);

    expect(runScript(repo, ["run", "--sha", sha, "--categories", "A,D"]).status).toBe(0);
    const s = summaryOf(repo);
    expect(s.categories_run).toEqual(["A", "E", "F"]);
    expect(s.handoff).toEqual(["D"]);

    expect(runScript(repo, ["run", "--sha", sha, "--categories", "B"]).status).toBe(2);
  });

  it("--also は package.json の npm script だけを受け付ける", { timeout: T }, () => {
    const repo = initRepo();
    const sha = git(repo, ["rev-parse", "HEAD"]);

    expect(runScript(repo, ["run", "--sha", sha, "--also", "missing"]).status).toBe(2);
    const r = runScript(repo, ["run", "--sha", sha, "--also", "ok"]);
    expect(r.status, r.output).toBe(0);
    expect(summaryOf(repo).results.map((x) => x.command)).toContain("npm run ok");
  });

  it("入っている依存が sha の lock と食い違えば警告する", { timeout: T }, () => {
    const repo = initRepo();
    const lock = { lockfileVersion: 3, packages: { "": {}, "node_modules/vitest": { version: "5.0.3" } } };
    const sha = commitFile(repo, "package-lock.json", JSON.stringify(lock));
    mkdirSync(path.join(repo, "node_modules"));
    const installed = { lockfileVersion: 3, packages: { "node_modules/vitest": { version: "4.1.10" } } };
    writeFileSync(path.join(repo, "node_modules", ".package-lock.json"), JSON.stringify(installed));

    const r = runScript(repo, ["run", "--sha", sha]);

    expect(r.status, r.output).toBe(0);
    expect(summaryOf(repo).warnings.join("\n")).toContain("node_modules/vitest");
  });
});

describe("verify-tree align", () => {
  function agentTree(repo) {
    const wt = path.join(repo, ".claude", "worktrees", "agent-abc");
    git(repo, ["worktree", "add", "-q", wt, "-b", "worktree-agent-abc"]);
    return wt;
  }

  it("委譲先の木を sha へ合わせ、sha と木を名乗る", { timeout: T }, () => {
    const repo = initRepo();
    const wt = agentTree(repo);
    const next = commitFile(repo, "next.txt", "next\n");

    const r = runScript(wt, ["align", "--sha", next]);

    expect(r.status, r.output).toBe(0);
    expect(git(wt, ["rev-parse", "HEAD"])).toBe(next);
    expect(JSON.parse(r.output).sha).toBe(next);
  });

  it("追跡ファイルの変更（巻き戻されていない変異）が残っていれば exit 2", { timeout: T }, () => {
    const repo = initRepo();
    const wt = agentTree(repo);
    writeFileSync(path.join(wt, "package.json"), "{}");
    const next = commitFile(repo, "next.txt", "next\n");

    expect(runScript(wt, ["align", "--sha", next]).status).toBe(2);
    expect(git(wt, ["rev-parse", "HEAD"])).not.toBe(next);
  });

  it(".delegate/ に未追跡の報告があっても checkout できる", { timeout: T }, () => {
    const repo = initRepo();
    const wt = agentTree(repo);
    mkdirSync(path.join(wt, ".delegate"));
    writeFileSync(path.join(wt, ".delegate", "round1.txt"), "report\n");
    const next = commitFile(repo, "next.txt", "next\n");

    expect(runScript(wt, ["align", "--sha", next]).status).toBe(0);
    expect(git(wt, ["rev-parse", "HEAD"])).toBe(next);
  });

  it(".delegate/ が ignore されていれば exit 2（木を生かし続ける前提が崩れる）", { timeout: T }, () => {
    const repo = initRepo();
    const wt = agentTree(repo);
    writeFileSync(path.join(repo, ".gitignore"), ".claude/worktrees/\nnode_modules/\n.delegate/\n");
    git(repo, ["add", ".gitignore"]);
    git(repo, ["commit", "-m", "ignore delegate"]);
    const next = git(repo, ["rev-parse", "HEAD"]);

    expect(runScript(wt, ["align", "--sha", next]).status).toBe(2);
  });

  it("本体の作業ツリーでは exit 2（linked worktree でない）", { timeout: T }, () => {
    const repo = initRepo();
    expect(runScript(repo, ["align", "--sha", git(repo, ["rev-parse", "HEAD"])]).status).toBe(2);
  });
});

describe("verify-tree remove", () => {
  it("道具の木を消す", { timeout: T }, () => {
    const repo = initRepo();
    expect(runScript(repo, ["run", "--sha", git(repo, ["rev-parse", "HEAD"])]).status).toBe(0);

    const r = runScript(repo, ["remove"]);

    expect(r.status, r.output).toBe(0);
    expect(existsSync(verifyTree(repo))).toBe(false);
    expect(existsSync(`${verifyTree(repo)}.summary.json`)).toBe(false);
    expect(existsSync(`${verifyTree(repo)}.summary.log`)).toBe(false);
  });

  it("道具の木が dirty なら消さずに exit 1", { timeout: T }, () => {
    const repo = initRepo();
    expect(runScript(repo, ["run", "--sha", git(repo, ["rev-parse", "HEAD"])]).status).toBe(0);
    writeFileSync(path.join(verifyTree(repo), "package.json"), "{}");

    expect(runScript(repo, ["remove"]).status).toBe(1);
    expect(existsSync(verifyTree(repo))).toBe(true);
  });

  it("--delegate-tree は .delegate/ だけが残る委譲先の木とブランチを消す", { timeout: T }, () => {
    const repo = initRepo();
    const wt = path.join(repo, ".claude", "worktrees", "agent-abc");
    git(repo, ["worktree", "add", "-q", wt, "-b", "worktree-agent-abc"]);
    mkdirSync(path.join(wt, ".delegate"));
    writeFileSync(path.join(wt, ".delegate", "round1.txt"), "report\n");

    const r = runScript(repo, ["remove", "--delegate-tree", wt]);

    expect(r.status, r.output).toBe(0);
    expect(existsSync(wt)).toBe(false);
    expect(git(repo, ["branch", "--list", "worktree-agent-abc"])).toBe("");
  });

  it("--delegate-tree は .delegate/ 以外の変更が残る木を消さない", { timeout: T }, () => {
    const repo = initRepo();
    const wt = path.join(repo, ".claude", "worktrees", "agent-abc");
    git(repo, ["worktree", "add", "-q", wt, "-b", "worktree-agent-abc"]);
    mkdirSync(path.join(wt, ".delegate"));
    writeFileSync(path.join(wt, ".delegate", "round1.txt"), "report\n");
    writeFileSync(path.join(wt, "package.json"), "{}");

    expect(runScript(repo, ["remove", "--delegate-tree", wt]).status).toBe(1);
    expect(existsSync(path.join(wt, ".delegate", "round1.txt"))).toBe(true);
  });
});
