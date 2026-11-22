// scripts/verify-tree.test.mjs
//
// verify-tree.mjs の実測テスト（issue #1298）。
// 使い捨ての git リポジトリに実際の worktree を作って走らせる（clean-worktrees.test.mjs と同じ作法）。
// 守る不変条件: 対象が無いまま緑を返さない（コマンド 0 件・未コミット・HEAD の食い違いは exit 2）。

import { afterAll, describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { BUILD_BEFORE, discardBranchArtifacts, mainRoot, parseCategoryCommands, summaryPathFor } from "./verify-tree.mjs";

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
function checklist({ c = 'node -e "process.exit(0)"', e = 'node -e "process.exit(0)"', withCFence = true } = {}) {
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
    ...(withCFence ? ["```bash", `${c}   # 必須: c`, "```"] : ["散文だけ。"]),
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

  it("ツールが回すカテゴリ（A・C・E・F）は、どれもコマンドを 1 件以上持つ", () => {
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

  it("--also で足すビルドは、C のフェンスと同じ字面である（字面が違うと重複を見分けられず 2 回建てる）", () => {
    const c = parseCategoryCommands(doc, "C").commands;
    expect(c.indexOf(BUILD_BEFORE["test:powershell"])).toBeLessThan(c.indexOf("npm run test:powershell"));
    expect(c).toContain(BUILD_BEFORE["test:powershell"]);
  });
});

describe("summaryPathFor — 要約の既定の置き場（hook と共有する導出）", () => {
  it("ブランチ名の / などを - に置き換えた、ツールの worktree の隣のパスを返す", () => {
    const p = summaryPathFor(path.join("C:", "repo"), "fix/pr-gate");
    expect(p).toBe(path.join("C:", "repo", ".claude", "worktrees", "agent-verify-fix-pr-gate.summary.json"));
  });

  it("run が書く要約の置き場と一致する", { timeout: T }, () => {
    const repo = initRepo();
    expect(runScript(repo, ["run", "--sha", git(repo, ["rev-parse", "HEAD"])]).status).toBe(0);
    expect(existsSync(summaryPathFor(repo, "feat/x"))).toBe(true);
  });
});

describe("verify-tree run", () => {
  it("要求が無くても E と F を回し、要約が sha と worktree の HEAD を示す", { timeout: T }, () => {
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

  it("--sha は短い sha も ref も 40 桁へ解決し、HEAD との照合と要約にその値を使う（#1316）", { timeout: T }, () => {
    const repo = initRepo();
    const sha = git(repo, ["rev-parse", "HEAD"]);

    for (const given of [git(repo, ["rev-parse", "--short=9", "HEAD"]), "HEAD"]) {
      const r = runScript(repo, ["run", "--sha", given]);
      expect(r.status, `${given}: ${r.output}`).toBe(0);
      expect(JSON.parse(r.output).sha, given).toBe(sha);
      expect(summaryOf(repo).sha, given).toBe(sha);
    }
  });

  it("標準出力の JSON が、/implement 3c の突き合わせる内訳を要約と同じ値で返す（#1316）", { timeout: T }, () => {
    const repo = initRepo({ e: 'node -e "process.exit(3)"' });
    const lock = { lockfileVersion: 3, packages: { "": {}, "node_modules/vitest": { version: "5.0.3" } } };
    const sha = commitFile(repo, "package-lock.json", JSON.stringify(lock));

    const r = runScript(repo, ["run", "--sha", sha, "--categories", "A"]);

    expect(r.status, r.output).toBe(1);
    const out = JSON.parse(r.output);
    const s = summaryOf(repo);
    expect(out.status).toBe("red");
    expect(out.categories_run).toEqual(["A", "E", "F"]);
    expect(out.categories_run).toEqual(s.categories_run);
    expect(out.results).toEqual(s.results.map(({ category, command, exit }) => ({ category, command, exit })));
    expect(out.results.map((x) => [x.category, x.exit])).toEqual([
      ["A", 0],
      ["E", 3],
      ["F", 0],
    ]);
    expect(out.warnings.length).toBeGreaterThan(0);
    expect(out.warnings).toEqual(s.warnings);
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

  it("要求したカテゴリのコマンドが 0 件なら exit 2 にし、worktree を作らない", { timeout: T }, () => {
    const repo = initRepo({ withCFence: false });
    const sha = git(repo, ["rev-parse", "HEAD"]);

    const r = runScript(repo, ["run", "--sha", sha, "--categories", "C"]);

    expect(r.status, r.output).toBe(2);
    expect(r.output).toContain("カテゴリ C のコマンドが 0 件");
    expect(existsSync(verifyTree(repo))).toBe(false);
  });

  it("作業ツリーに未コミットの変更があれば exit 2（コミットし忘れた変更を緑で返さない）", { timeout: T }, () => {
    const repo = initRepo();
    const sha = git(repo, ["rev-parse", "HEAD"]);
    writeFileSync(path.join(repo, "wip.txt"), "uncommitted\n");

    const r = runScript(repo, ["run", "--sha", sha]);

    expect(r.status, r.output).toBe(2);
    expect(r.output).toContain("に未コミットの変更がある");
    expect(existsSync(verifyTree(repo))).toBe(false);
  });

  it("sha が作業ツリーの HEAD でなければ exit 2", { timeout: T }, () => {
    const repo = initRepo();
    const old = git(repo, ["rev-parse", "HEAD"]);
    commitFile(repo, "next.txt", "next\n");

    const r = runScript(repo, ["run", "--sha", old]);

    expect(r.status, r.output).toBe(2);
    expect(r.output).toContain("の HEAD でない");
  });

  it("--also test:powershell は、計画に本体のビルドが無ければその直前に足す（ツールの worktree は cold か前の sha の本体を持つ）", { timeout: T }, () => {
    const repo = initRepo();
    writeFileSync(path.join(repo, "package.json"), JSON.stringify({ scripts: { "test:powershell": 'node -e "0"' } }));
    git(repo, ["add", "package.json"]);
    git(repo, ["commit", "-m", "pester"]);

    // fixture に Cargo.toml は無いので、ビルドの行は赤になる（cargo が無い環境でも赤）。見るのは順序である
    const r = runScript(repo, ["run", "--sha", git(repo, ["rev-parse", "HEAD"]), "--also", "test:powershell"]);

    expect(r.status, r.output).toBe(1);
    const commands = summaryOf(repo).results.map((x) => x.command);
    expect(commands.slice(-2)).toEqual([BUILD_BEFORE["test:powershell"], "npm run test:powershell"]);
  });

  it("計画にもう本体のビルドがあれば、--also test:powershell の前に重ねて足さない", { timeout: T }, () => {
    const repo = initRepo({ c: BUILD_BEFORE["test:powershell"] });
    writeFileSync(path.join(repo, "package.json"), JSON.stringify({ scripts: { "test:powershell": 'node -e "0"' } }));
    git(repo, ["add", "package.json"]);
    git(repo, ["commit", "-m", "pester"]);

    runScript(repo, ["run", "--sha", git(repo, ["rev-parse", "HEAD"]), "--categories", "C", "--also", "test:powershell"]);

    const commands = summaryOf(repo).results.map((x) => x.command);
    expect(commands.filter((c) => c === BUILD_BEFORE["test:powershell"])).toHaveLength(1);
    expect(commands.indexOf(BUILD_BEFORE["test:powershell"])).toBeLessThan(commands.indexOf("npm run test:powershell"));
  });

  it("2 ラウンド目は同じ worktree を新しい sha へ合わせて再利用する", { timeout: T }, () => {
    const repo = initRepo();
    expect(runScript(repo, ["run", "--sha", git(repo, ["rev-parse", "HEAD"])]).status).toBe(0);
    const next = commitFile(repo, "next.txt", "next\n");

    const r = runScript(repo, ["run", "--sha", next]);

    expect(r.status, r.output).toBe(0);
    expect(git(verifyTree(repo), ["rev-parse", "HEAD"])).toBe(next);
    expect(summaryOf(repo).sha).toBe(next);
  });

  it("再利用する worktree に追跡ファイルの変更が残っていれば exit 2", { timeout: T }, () => {
    const repo = initRepo();
    expect(runScript(repo, ["run", "--sha", git(repo, ["rev-parse", "HEAD"])]).status).toBe(0);
    writeFileSync(path.join(verifyTree(repo), "package.json"), "{}");
    const next = commitFile(repo, "next.txt", "next\n");

    const r = runScript(repo, ["run", "--sha", next]);

    expect(r.status, r.output).toBe(2);
    expect(r.output).toContain("再利用する worktree に追跡ファイルの変更がある");
  });

  it("git の想定外の失敗（再利用する worktree の checkout が未追跡ファイルと衝突）は、赤（1）ではなく 2", { timeout: T }, () => {
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

    const b = runScript(repo, ["run", "--sha", sha, "--categories", "B"]);
    expect(b.status, b.output).toBe(2);
    expect(b.output).toContain("B はコマンドを持たない");
  });

  it("--also は package.json の npm script だけを受け付ける", { timeout: T }, () => {
    const repo = initRepo();
    const sha = git(repo, ["rev-parse", "HEAD"]);

    const missing = runScript(repo, ["run", "--sha", sha, "--also", "missing"]);
    expect(missing.status, missing.output).toBe(2);
    expect(missing.output).toContain("--also の npm script が package.json に無い");
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

  it("入っている依存の記録が壊れていても、回した結果を失わずに警告する", { timeout: T }, () => {
    const repo = initRepo();
    const lock = { lockfileVersion: 3, packages: { "": {}, "node_modules/vitest": { version: "5.0.3" } } };
    const sha = commitFile(repo, "package-lock.json", JSON.stringify(lock));
    mkdirSync(path.join(repo, "node_modules"));
    writeFileSync(path.join(repo, "node_modules", ".package-lock.json"), "{ broken");

    const r = runScript(repo, ["run", "--sha", sha]);

    expect(r.status, r.output).toBe(0);
    const s = summaryOf(repo);
    expect(s.results.length).toBe(2);
    expect(s.warnings.join("\n")).toContain("読めない");
  });

  it("登録だけ残ってディレクトリが消えたツールの worktree を作り直す", { timeout: T }, () => {
    const repo = initRepo();
    const sha = git(repo, ["rev-parse", "HEAD"]);
    expect(runScript(repo, ["run", "--sha", sha]).status).toBe(0);
    rmSync(verifyTree(repo), { recursive: true, force: true });

    const r = runScript(repo, ["run", "--sha", sha]);

    expect(r.status, r.output).toBe(0);
    expect(git(verifyTree(repo), ["rev-parse", "HEAD"])).toBe(sha);
  });
});

describe("linked worktree の呼び出し元（harness の worktree で動く主のセッション・#1436）", () => {
  /**
   * 本体は main に居て、HEAD が呼び出し元と違い、未追跡のファイルで dirty である。呼び出し元は本体の
   * `.claude/worktrees/` の下の linked worktree で feat/x を持つ。照合の相手を本体へ取り違える実装は、
   * どれか（ブランチ・dirty・HEAD）で落ちる
   */
  function sessionRepo() {
    const repo = initRepo();
    git(repo, ["checkout", "-q", "main"]);
    const caller = path.join(repo, ".claude", "worktrees", "session-wt");
    git(repo, ["worktree", "add", "-q", caller, "feat/x"]);
    const sha = commitFile(caller, "feature.txt", "feature\n");
    writeFileSync(path.join(repo, "wip.txt"), "another session's work\n");
    return { repo, caller, sha, mainHead: git(repo, ["rev-parse", "HEAD"]) };
  }
  const mainUntouched = ({ repo, mainHead }) => {
    expect(git(repo, ["branch", "--show-current"])).toBe("main");
    expect(git(repo, ["rev-parse", "HEAD"])).toBe(mainHead);
    expect(existsSync(path.join(repo, "wip.txt"))).toBe(true);
  };

  it("mainRoot は、どの worktree から呼んでも本体のルートを返す", () => {
    const s = sessionRepo();
    expect(mainRoot(s.caller)).toBe(mainRoot(s.repo));
    expect(existsSync(path.join(mainRoot(s.caller), "wip.txt"))).toBe(true);
  });

  it("run は呼び出し元の HEAD を検証し、ツールの worktree と要約を本体の下に置く。本体には触れない", { timeout: T }, () => {
    const s = sessionRepo();

    const r = runScript(s.caller, ["run", "--sha", s.sha]);

    expect(r.status, r.output).toBe(0);
    expect(git(verifyTree(s.repo), ["rev-parse", "HEAD"])).toBe(s.sha);
    expect(summaryOf(s.repo).sha).toBe(s.sha);
    expect(summaryOf(s.repo).branch).toBe("feat/x");
    expect(existsSync(path.join(s.caller, ".claude", "worktrees"))).toBe(false);
    mainUntouched(s);
  });

  it("sha が呼び出し元の HEAD でなければ（本体の HEAD でも）exit 2", { timeout: T }, () => {
    const s = sessionRepo();

    const r = runScript(s.caller, ["run", "--sha", s.mainHead]);

    expect(r.status, r.output).toBe(2);
    expect(r.output).toContain("の HEAD でない");
  });

  it("呼び出し元に未コミットの変更があれば exit 2", { timeout: T }, () => {
    const s = sessionRepo();
    writeFileSync(path.join(s.caller, "forgot.txt"), "uncommitted\n");

    const r = runScript(s.caller, ["run", "--sha", s.sha]);

    expect(r.status, r.output).toBe(2);
    expect(r.output).toContain("forgot.txt");
  });

  it("remove は本体の下のツールの worktree を消し、要約を残す。本体には触れない", { timeout: T }, () => {
    const s = sessionRepo();
    expect(runScript(s.caller, ["run", "--sha", s.sha]).status).toBe(0);

    const r = runScript(s.caller, ["remove"]);

    expect(r.status, r.output).toBe(0);
    expect(existsSync(verifyTree(s.repo))).toBe(false);
    expect(typeof summaryOf(s.repo).removed_at).toBe("string");
    mainUntouched(s);
  });

  it("remove --delegate-tree は、呼び出し元の worktree 自身を消さない", { timeout: T }, () => {
    const s = sessionRepo();

    const r = runScript(s.caller, ["remove", "--delegate-tree", s.caller]);

    expect(r.status, r.output).toBe(2);
    expect(r.output).toContain("委譲先の worktree でない");
    expect(existsSync(s.caller)).toBe(true);
  });
});

describe("verify-tree align", () => {
  function agentTree(repo) {
    const wt = path.join(repo, ".claude", "worktrees", "agent-abc");
    git(repo, ["worktree", "add", "-q", wt, "-b", "worktree-agent-abc"]);
    return wt;
  }

  it("委譲先の worktree を sha へ合わせ、sha と worktree を示す", { timeout: T }, () => {
    const repo = initRepo();
    const wt = agentTree(repo);
    const next = commitFile(repo, "next.txt", "next\n");

    const r = runScript(wt, ["align", "--sha", next]);

    expect(r.status, r.output).toBe(0);
    expect(git(wt, ["rev-parse", "HEAD"])).toBe(next);
    expect(JSON.parse(r.output).sha).toBe(next);
  });

  it("--sha の短い sha を 40 桁へ解決して合わせる（#1316）", { timeout: T }, () => {
    const repo = initRepo();
    const wt = agentTree(repo);
    const next = commitFile(repo, "next.txt", "next\n");

    const r = runScript(wt, ["align", "--sha", next.slice(0, 9)]);

    expect(r.status, r.output).toBe(0);
    expect(JSON.parse(r.output).sha).toBe(next);
    expect(git(wt, ["rev-parse", "HEAD"])).toBe(next);
  });

  it("追跡ファイルの変更（巻き戻されていない変異）が残っていれば exit 2", { timeout: T }, () => {
    const repo = initRepo();
    const wt = agentTree(repo);
    writeFileSync(path.join(wt, "package.json"), "{}");
    const next = commitFile(repo, "next.txt", "next\n");

    const r = runScript(wt, ["align", "--sha", next]);
    expect(r.status, r.output).toBe(2);
    expect(r.output).toContain("変更が残っている");
    expect(git(wt, ["rev-parse", "HEAD"])).not.toBe(next);
  });

  it(".delegate/ の外に未追跡のファイル（変異で足したまま）が残っていれば exit 2", { timeout: T }, () => {
    const repo = initRepo();
    const wt = agentTree(repo);
    writeFileSync(path.join(wt, "leftover.test.mjs"), "// mutant\n");
    const next = commitFile(repo, "next.txt", "next\n");

    const r = runScript(wt, ["align", "--sha", next]);
    expect(r.status, r.output).toBe(2);
    expect(r.output).toContain("leftover.test.mjs");
    expect(git(wt, ["rev-parse", "HEAD"])).not.toBe(next);
  });

  it(".delegate/ に未追跡の報告があっても checkout できる（空白と非 ASCII を含む名前でも）", { timeout: T }, () => {
    const repo = initRepo();
    const wt = agentTree(repo);
    mkdirSync(path.join(wt, ".delegate"));
    writeFileSync(path.join(wt, ".delegate", "round1.txt"), "report\n");
    writeFileSync(path.join(wt, ".delegate", "報告 a.txt"), "report\n");
    const next = commitFile(repo, "next.txt", "next\n");

    expect(runScript(wt, ["align", "--sha", next]).status).toBe(0);
    expect(git(wt, ["rev-parse", "HEAD"])).toBe(next);
  });

  it(".delegate/ が ignore されていれば exit 2（worktree を生かし続ける前提が崩れる）", { timeout: T }, () => {
    const repo = initRepo();
    const wt = agentTree(repo);
    writeFileSync(path.join(repo, ".gitignore"), ".claude/worktrees/\nnode_modules/\n.delegate/\n");
    git(repo, ["add", ".gitignore"]);
    git(repo, ["commit", "-m", "ignore delegate"]);
    const next = git(repo, ["rev-parse", "HEAD"]);

    // 理由まで見る: 直後の fail-closed（check-ignore が判定できない）も同じ exit 2 を出すので、
    // 終了コードだけでは ignore の判定を消す変異が生き残る（#1319 で実測）
    const r = runScript(wt, ["align", "--sha", next]);
    expect(r.status, r.output).toBe(2);
    expect(r.output).toContain(".delegate/ が ignore されている");
  });

  it("本体の作業ツリーでは exit 2（linked worktree でない）", { timeout: T }, () => {
    const repo = initRepo();
    const r = runScript(repo, ["align", "--sha", git(repo, ["rev-parse", "HEAD"])]);
    expect(r.status, r.output).toBe(2);
    expect(r.output).toContain("linked worktree の中で打つこと");
  });
});

describe("discardBranchArtifacts — マージ済みのブランチの worktree と要約を消す（#1359）", () => {
  it("本体が別のブランチへ移った後でも、引数のブランチの worktree・要約・ログを消す", { timeout: T }, () => {
    const repo = initRepo();
    expect(runScript(repo, ["run", "--sha", git(repo, ["rev-parse", "HEAD"])]).status).toBe(0);
    git(repo, ["checkout", "-q", "main"]); // マージの後、本体は feat/x に居ない

    const r = discardBranchArtifacts(repo, "feat/x");

    expect(r.ok, r.notes.join("\n")).toBe(true);
    expect(existsSync(verifyTree(repo))).toBe(false);
    expect(existsSync(`${verifyTree(repo)}.summary.json`)).toBe(false);
    expect(existsSync(`${verifyTree(repo)}.summary.log`)).toBe(false);
    expect(git(repo, ["worktree", "list", "--porcelain"])).not.toContain("agent-verify-feat-x");
    expect(git(repo, ["branch", "--list", "feat/x"])).toContain("feat/x"); // ブランチそのものには触れない
  });

  it("worktree が無く要約だけが残っていても、要約を消して ok", { timeout: T }, () => {
    const repo = initRepo();
    expect(runScript(repo, ["run", "--sha", git(repo, ["rev-parse", "HEAD"])]).status).toBe(0);
    expect(runScript(repo, ["remove"]).status).toBe(0); // /implement の Step 4 の後の形（要約だけが残る）
    git(repo, ["checkout", "-q", "main"]);

    const r = discardBranchArtifacts(repo, "feat/x");

    expect(r.ok, r.notes.join("\n")).toBe(true);
    expect(existsSync(`${verifyTree(repo)}.summary.json`)).toBe(false);
  });

  it("worktree が dirty なら worktree も要約も消さず、理由を返す（投げない）", { timeout: T }, () => {
    const repo = initRepo();
    expect(runScript(repo, ["run", "--sha", git(repo, ["rev-parse", "HEAD"])]).status).toBe(0);
    writeFileSync(path.join(verifyTree(repo), "package.json"), "{}");
    git(repo, ["checkout", "-q", "main"]);

    const r = discardBranchArtifacts(repo, "feat/x");

    expect(r.ok).toBe(false);
    expect(r.notes.join("\n")).toContain("agent-verify-feat-x");
    expect(existsSync(verifyTree(repo))).toBe(true);
    expect(existsSync(`${verifyTree(repo)}.summary.json`)).toBe(true);
  });

  it("worktree も要約も無ければ、何もせず ok", { timeout: T }, () => {
    const repo = initRepo();
    expect(discardBranchArtifacts(repo, "feat/x").ok).toBe(true);
  });
});

describe("verify-tree remove", () => {
  it("ツールの worktree を消す", { timeout: T }, () => {
    const repo = initRepo();
    expect(runScript(repo, ["run", "--sha", git(repo, ["rev-parse", "HEAD"])]).status).toBe(0);

    const r = runScript(repo, ["remove"]);

    expect(r.status, r.output).toBe(0);
    expect(existsSync(verifyTree(repo))).toBe(false);
    expect(existsSync(`${verifyTree(repo)}.summary.log`)).toBe(false);
  });

  it("ツールの worktree を消しても要約は残り、worktree が消えた時刻を持つ（PR 前ゲートが読む・#1299）", { timeout: T }, () => {
    const repo = initRepo();
    const sha = git(repo, ["rev-parse", "HEAD"]);
    expect(runScript(repo, ["run", "--sha", sha]).status).toBe(0);

    expect(runScript(repo, ["remove"]).status).toBe(0);

    expect(existsSync(`${verifyTree(repo)}.summary.json`)).toBe(true);
    const s = summaryOf(repo);
    expect(s.sha).toBe(sha);
    expect(s.status).toBe("green");
    expect(typeof s.removed_at).toBe("string");
  });

  it("ツールの worktree が dirty なら消さずに exit 1", { timeout: T }, () => {
    const repo = initRepo();
    expect(runScript(repo, ["run", "--sha", git(repo, ["rev-parse", "HEAD"])]).status).toBe(0);
    writeFileSync(path.join(verifyTree(repo), "package.json"), "{}");

    expect(runScript(repo, ["remove"]).status).toBe(1);
    expect(existsSync(verifyTree(repo))).toBe(true);
  });

  it("--delegate-tree は .delegate/ だけが残る委譲先の worktree とブランチを消す", { timeout: T }, () => {
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

  it("--delegate-tree は align で detach された worktree でも、harness の命名のブランチを消す", { timeout: T }, () => {
    const repo = initRepo();
    const wt = path.join(repo, ".claude", "worktrees", "agent-abc");
    git(repo, ["worktree", "add", "-q", wt, "-b", "worktree-agent-abc"]);
    const next = commitFile(repo, "next.txt", "next\n");
    expect(runScript(wt, ["align", "--sha", next]).status).toBe(0);
    mkdirSync(path.join(wt, ".delegate"));
    writeFileSync(path.join(wt, ".delegate", "round1.txt"), "report\n");

    const r = runScript(repo, ["remove", "--delegate-tree", wt]);

    expect(r.status, r.output).toBe(0);
    expect(existsSync(wt)).toBe(false);
    expect(git(repo, ["branch", "--list", "worktree-agent-abc"])).toBe("");
  });

  it("--delegate-tree は、git が返す名前と字面の違うパス（junction 経由）でも同じ worktree として照合する", { timeout: T }, () => {
    // GitHub の Windows runner では tmpdir() が 8.3 短縮名（RUNNER~1）を返し、path.resolve では一致しなかった。
    // 短縮名は手元で作れないので、同じ「字面は違うが実体は同じ」を junction で作る
    const repo = initRepo();
    const wt = path.join(repo, ".claude", "worktrees", "agent-abc");
    git(repo, ["worktree", "add", "-q", wt, "-b", "worktree-agent-abc"]);
    mkdirSync(path.join(wt, ".delegate"));
    writeFileSync(path.join(wt, ".delegate", "round1.txt"), "report\n");
    const alias = mkdtempSync(path.join(tmpdir(), "snotra-verify-tree-alias-"));
    scratchDirs.push(alias);
    const link = path.join(alias, "repo");
    symlinkSync(repo, link, "junction");

    const r = runScript(repo, ["remove", "--delegate-tree", path.join(link, ".claude", "worktrees", "agent-abc")]);

    expect(r.status, r.output).toBe(0);
    expect(existsSync(wt)).toBe(false);
  });

  // ロックの理由（harness なら `claude agent agent-<id> (pid N)`）を出す。持ち主の推測は出さない——
  // 委譲先が報告を返したあとも外れない例を実測した（#1533）
  it("--delegate-tree はロックされた worktree を消さず、報告もブランチも残し、ロックの理由を出す", { timeout: T }, () => {
    const repo = initRepo();
    const wt = path.join(repo, ".claude", "worktrees", "agent-abc");
    git(repo, ["worktree", "add", "-q", wt, "-b", "worktree-agent-abc"]);
    git(repo, ["worktree", "lock", "--reason", "claude agent agent-abc (pid 1)", wt]);
    mkdirSync(path.join(wt, ".delegate"));
    writeFileSync(path.join(wt, ".delegate", "round1.txt"), "report\n");

    const r = runScript(repo, ["remove", "--delegate-tree", wt]);
    expect(r.status, r.output).toBe(1);
    expect(r.output).toContain("ロックされている（claude agent agent-abc (pid 1)）——何も消さない");
    expect(r.output).not.toContain("まだ動いている");
    expect(existsSync(path.join(wt, ".delegate", "round1.txt"))).toBe(true);
    expect(git(repo, ["branch", "--list", "worktree-agent-abc"])).toContain("worktree-agent-abc");
    git(repo, ["worktree", "unlock", wt]);
  });

  it("--delegate-tree は理由の無いロックでも消さず、括弧なしで拒む", { timeout: T }, () => {
    const repo = initRepo();
    const wt = path.join(repo, ".claude", "worktrees", "agent-abc");
    git(repo, ["worktree", "add", "-q", wt, "-b", "worktree-agent-abc"]);
    git(repo, ["worktree", "lock", wt]);
    mkdirSync(path.join(wt, ".delegate"));
    writeFileSync(path.join(wt, ".delegate", "round1.txt"), "report\n");

    const r = runScript(repo, ["remove", "--delegate-tree", wt]);
    expect(r.status, r.output).toBe(1);
    expect(r.output).toContain("ロックされている——何も消さない");
    expect(r.output).not.toContain("まだ動いている");
    expect(existsSync(path.join(wt, ".delegate", "round1.txt"))).toBe(true);
    expect(git(repo, ["branch", "--list", "worktree-agent-abc"])).toContain("worktree-agent-abc");
    git(repo, ["worktree", "unlock", wt]);
  });

  it("--delegate-tree は .delegate/ 以外の変更が残る worktree を消さない", { timeout: T }, () => {
    const repo = initRepo();
    const wt = path.join(repo, ".claude", "worktrees", "agent-abc");
    git(repo, ["worktree", "add", "-q", wt, "-b", "worktree-agent-abc"]);
    mkdirSync(path.join(wt, ".delegate"));
    writeFileSync(path.join(wt, ".delegate", "round1.txt"), "report\n");
    writeFileSync(path.join(wt, "package.json"), "{}");

    expect(runScript(repo, ["remove", "--delegate-tree", wt]).status).toBe(1);
    expect(existsSync(path.join(wt, ".delegate", "round1.txt"))).toBe(true);
  });

  // #1507: 本体の HEAD（feat/x）がブランチの先端を含まない状態を作る。始点は sha で渡す
  // （追跡ブランチを名前で渡すと upstream が付き、修正前の `git branch -d` でも通って偽の緑になる）
  /** HEAD からは届かないコミットを 1 つ作って sha を返す（HEAD は動かさない） */
  function commitOffHead(repo, name) {
    const back = git(repo, ["branch", "--show-current"]);
    git(repo, ["checkout", "-q", "-b", "tmp-off-head"]);
    const sha = commitFile(repo, name, `${name}\n`);
    git(repo, ["checkout", "-q", back]);
    git(repo, ["branch", "-D", "tmp-off-head"]);
    return sha;
  }
  function delegateAt(repo, start) {
    const wt = path.join(repo, ".claude", "worktrees", "agent-abc");
    git(repo, ["worktree", "add", "-q", wt, "-b", "worktree-agent-abc", start]);
    mkdirSync(path.join(wt, ".delegate"));
    writeFileSync(path.join(wt, ".delegate", "round1.txt"), "report\n");
    return wt;
  }

  it("--delegate-tree は、本体の HEAD が古くても origin/main の祖先のブランチを消す（#1507）", { timeout: T }, () => {
    const repo = initRepo();
    const tip = commitOffHead(repo, "upstream.txt");
    git(repo, ["update-ref", "refs/remotes/origin/main", tip]);
    const wt = delegateAt(repo, tip);

    const r = runScript(repo, ["remove", "--delegate-tree", wt]);

    expect(r.status, r.output).toBe(0);
    expect(r.output).toContain("origin/main の祖先");
    expect(existsSync(wt)).toBe(false);
    expect(git(repo, ["branch", "--list", "worktree-agent-abc"])).toBe("");
  });

  it("--delegate-tree は、origin/main に無いコミットを持つブランチを残し、理由を出して exit 1（#1507）", { timeout: T }, () => {
    const repo = initRepo();
    git(repo, ["update-ref", "refs/remotes/origin/main", commitOffHead(repo, "upstream.txt")]);
    const wt = delegateAt(repo, commitOffHead(repo, "own.txt"));

    const r = runScript(repo, ["remove", "--delegate-tree", wt]);

    expect(r.status, r.output).toBe(1);
    expect(r.output).toContain("origin/main に無いコミットがある");
    expect(existsSync(wt)).toBe(false);
    expect(git(repo, ["branch", "--list", "worktree-agent-abc"])).not.toBe("");
  });

  it("--delegate-tree は、origin/main の祖先でも git branch -D が失敗したら、ブランチを残して exit 1（2 にしない・#1507）", { timeout: T }, () => {
    const repo = initRepo();
    const tip = commitOffHead(repo, "upstream.txt");
    git(repo, ["update-ref", "refs/remotes/origin/main", tip]);
    const wt = delegateAt(repo, tip);
    writeFileSync(path.join(repo, ".git", "refs", "heads", "worktree-agent-abc.lock"), "");

    const r = runScript(repo, ["remove", "--delegate-tree", wt]);

    expect(r.status, r.output).toBe(1);
    expect(r.output).toContain("git branch -D が失敗した");
    expect(git(repo, ["branch", "--list", "worktree-agent-abc"])).not.toBe("");
  });

  it("--delegate-tree は、origin/main が無く -d も拒むブランチを残し、判定できないと出して exit 1（2 にしない・#1507）", { timeout: T }, () => {
    const repo = initRepo();
    const wt = delegateAt(repo, commitOffHead(repo, "own.txt"));

    const r = runScript(repo, ["remove", "--delegate-tree", wt]);

    expect(r.status, r.output).toBe(1);
    expect(r.output).toContain("origin/main の祖先か判定できない");
    expect(git(repo, ["branch", "--list", "worktree-agent-abc"])).not.toBe("");
  });
});
