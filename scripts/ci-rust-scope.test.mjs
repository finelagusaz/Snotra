// scripts/ci-rust-scope.test.mjs
//
// rust-check の後続 step を省くかの判定（#1508）。省くのは「変更が 1 件以上あり、すべて .md」の PR だけで、
// それ以外（push・差分 0 件・差分を取れない・.md 以外を 1 件でも含む）はすべて走らせる側へ倒れることを固定する。

import { afterAll, describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { decide, needsRustCheck, parseNameOnlyZ } from "./ci-rust-scope.mjs";

const SCRIPT = fileURLToPath(new URL("./ci-rust-scope.mjs", import.meta.url));
const T = 20_000; // git を数回起動するため既定 5s では足りない

describe("needsRustCheck — 許可リスト（全部 .md のときだけ省く）", () => {
  it.each([
    [["AGENTS.md", ".claude/skills/x/SKILL.md"]],
    [["src-tauri/CLAUDE.md"]],
    [["docs/日本語.md"]],
  ])("すべて .md なら省く: %j", (paths) => {
    expect(needsRustCheck(paths)).toBe(false);
  });

  it.each([
    [["AGENTS.md", "src-tauri/build.rs"]],
    [["Cargo.toml"]],
    [["rust-toolchain.toml"]],
    [[".github/workflows/ci.yml"]],
    [["scripts/merge-pr.mjs"]],
    [[".gitattributes"]],
    [["README.MD"]], // 大文字の拡張子は省かない（安全側）
    [["docs/x.md/inner.rs"]], // .md を含むが末尾でない
    [["docs/x.md.rs"]],
    [["Makefile"]], // 拡張子なし
  ])(".md 以外を含むなら走らせる: %j", (paths) => {
    expect(needsRustCheck(paths)).toBe(true);
  });

  it("差分 0 件なら走らせる（何も見ていないので省く根拠が無い）", () => {
    expect(needsRustCheck([])).toBe(true);
  });
});

describe("parseNameOnlyZ — `git diff --name-only -z` の出力", () => {
  it("NUL で区切り、末尾の NUL を落とす", () => {
    expect(parseNameOnlyZ("a.md\0b/c.rs\0")).toEqual(["a.md", "b/c.rs"]);
  });

  it("空の出力は 0 件", () => {
    expect(parseNameOnlyZ("")).toEqual([]);
  });

  it("空白・非 ASCII のパスを引用せずそのまま返す（-z は core.quotePath の引用をしない）", () => {
    expect(parseNameOnlyZ("docs/日本語 x.md\0")).toEqual(["docs/日本語 x.md"]);
  });
});

describe("decide — イベントと差分から判定する", () => {
  const ok = (stdout) => () => ({ ok: true, stdout });

  it("pull_request 以外（main への push）は差分を見ずに走らせる", () => {
    let called = false;
    const r = decide({ eventName: "push", diff: () => ((called = true), { ok: true, stdout: "a.md\0" }) });
    expect(r.rust).toBe(true);
    expect(called).toBe(false);
    expect(r.reason).toContain("push");
  });

  it("イベント名が無いときも走らせる", () => {
    expect(decide({ eventName: undefined, diff: ok("a.md\0") }).rust).toBe(true);
  });

  it("pull_request で差分が全部 .md なら省く", () => {
    const r = decide({ eventName: "pull_request", diff: ok("AGENTS.md\0docs/x.md\0") });
    expect(r.rust).toBe(false);
    expect(r.paths).toEqual(["AGENTS.md", "docs/x.md"]);
  });

  it("pull_request で差分を取れなければ走らせ、理由に失敗を書く", () => {
    const r = decide({ eventName: "pull_request", diff: () => ({ ok: false, error: "fatal: bad revision 'HEAD^1'" }) });
    expect(r.rust).toBe(true);
    expect(r.reason).toContain("HEAD^1");
  });

  it("pull_request で差分 0 件なら走らせる", () => {
    const r = decide({ eventName: "pull_request", diff: ok("") });
    expect(r.rust).toBe(true);
    expect(r.reason).toContain("0 件");
  });

  it("改名（--no-renames で旧 .rs の削除と新 .md の追加が出る）なら走らせる", () => {
    expect(decide({ eventName: "pull_request", diff: ok("docs/a.md\0src/a.rs\0") }).rust).toBe(true);
  });
});

// ---- CLI の結合テスト: 一時リポジトリに PR の merge commit を作り、CLI を当てる ----

const scratchDirs = [];
afterAll(() => {
  for (const dir of scratchDirs) rmSync(dir, { recursive: true, force: true });
});

function git(cwd, args) {
  return execFileSync("git", args, { cwd, encoding: "utf8", stdio: "pipe" });
}

function write(dir, rel, body) {
  const file = path.join(dir, rel);
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, body);
}

/** base に src/a.rs と README.md を置き、topic で `change(dir)` を当て、base を 1 コミット進めてから
 *  `merge --no-ff` した repo を返す（actions/checkout が PR で取る refs/pull/N/merge と同じ形: 第 1 親 = base の tip）。 */
function mergedRepo(change) {
  const dir = mkdtempSync(path.join(tmpdir(), "snotra-ci-rust-scope-"));
  scratchDirs.push(dir);
  git(dir, ["init", "-b", "main"]);
  git(dir, ["config", "user.email", "test@example.com"]);
  git(dir, ["config", "user.name", "ci-rust-scope test"]);
  git(dir, ["config", "commit.gpgsign", "false"]);
  git(dir, ["config", "core.quotePath", "true"]); // runner の既定。-z を外す変異をここで捕まえる
  write(dir, "src/a.rs", "fn a() {}\n");
  write(dir, "README.md", "# r\n");
  git(dir, ["add", "-A"]);
  git(dir, ["commit", "-m", "base"]);
  git(dir, ["switch", "-c", "topic"]);
  change(dir);
  git(dir, ["add", "-A"]);
  git(dir, ["commit", "-m", "topic"]);
  git(dir, ["switch", "main"]);
  // base が先へ進んでも、PR の差分に混ざらないこと。src/ の外へ置く——src/ の唯一のファイルを改名する
  // テストで、git の directory rename 検出が src/b.rs を巻き込み、merge が止まる
  write(dir, "lib/b.rs", "fn b() {}\n");
  git(dir, ["add", "-A"]);
  git(dir, ["commit", "-m", "base moves"]);
  git(dir, ["merge", "--no-ff", "-m", "merge", "topic"]);
  return dir;
}

function runCli(cwd, eventName) {
  const out = path.join(mkdtempSync(path.join(tmpdir(), "snotra-ci-rust-scope-out-")), "output");
  scratchDirs.push(path.dirname(out));
  writeFileSync(out, "");
  const stdout = execFileSync(process.execPath, [SCRIPT], {
    cwd,
    encoding: "utf8",
    stdio: "pipe",
    env: { ...process.env, GITHUB_EVENT_NAME: eventName, GITHUB_OUTPUT: out },
  });
  return { output: readFileSync(out, "utf8"), stdout };
}

describe("CLI — 一時リポジトリの merge commit に当てる", () => {
  it("PR の変更が .md だけなら rust=false（base 側で進んだ .rs は数えない）", { timeout: T }, () => {
    const dir = mergedRepo((d) => write(d, "docs/x.md", "x\n"));
    expect(runCli(dir, "pull_request").output).toBe("rust=false\n");
  });

  it("PR が .rs を変えたら rust=true", { timeout: T }, () => {
    const dir = mergedRepo((d) => write(d, "src/a.rs", "fn a() { 1; }\n"));
    expect(runCli(dir, "pull_request").output).toBe("rust=true\n");
  });

  it(".rs を .md へ改名したら rust=true（改名元を母集団に入れる）", { timeout: T }, () => {
    const dir = mergedRepo((d) => git(d, ["mv", "src/a.rs", "a.md"]));
    expect(runCli(dir, "pull_request").output).toBe("rust=true\n");
  });

  it("非 ASCII 名の .md だけなら rust=false（core.quotePath=true でも引用されない）", { timeout: T }, () => {
    const dir = mergedRepo((d) => write(d, "docs/日本語.md", "x\n"));
    expect(runCli(dir, "pull_request").output).toBe("rust=false\n");
  });

  it("push では、変更が .md だけでも rust=true", { timeout: T }, () => {
    const dir = mergedRepo((d) => write(d, "docs/x.md", "x\n"));
    expect(runCli(dir, "push").output).toBe("rust=true\n");
  });

  it("HEAD に第 1 親が無い（差分を取れない）なら rust=true", { timeout: T }, () => {
    const dir = mkdtempSync(path.join(tmpdir(), "snotra-ci-rust-scope-single-"));
    scratchDirs.push(dir);
    git(dir, ["init", "-b", "main"]);
    git(dir, ["config", "user.email", "test@example.com"]);
    git(dir, ["config", "user.name", "ci-rust-scope test"]);
    git(dir, ["config", "commit.gpgsign", "false"]);
    write(dir, "README.md", "# r\n");
    git(dir, ["add", "-A"]);
    git(dir, ["commit", "-m", "only"]);
    const r = runCli(dir, "pull_request");
    expect(r.output).toBe("rust=true\n");
    expect(r.stdout).toContain("差分を取れない");
  });

  it("HEAD が merge commit でない（PR の head を直接 checkout した形）なら、変更が .md だけでも rust=true", { timeout: T }, () => {
    const dir = mergedRepo((d) => write(d, "docs/x.md", "x\n"));
    git(dir, ["switch", "topic"]); // HEAD^1..HEAD は topic の最後のコミットだけ（.md）になる
    const r = runCli(dir, "pull_request");
    expect(r.output).toBe("rust=true\n");
    expect(r.stdout).toContain("merge commit でない");
  });

  it("GITHUB_OUTPUT が無ければ非ゼロで終わる（黙って省かない）", { timeout: T }, () => {
    // git を読む前に止まるので、repo は要らない
    const dir = mkdtempSync(path.join(tmpdir(), "snotra-ci-rust-scope-noout-"));
    scratchDirs.push(dir);
    const env = { ...process.env, GITHUB_EVENT_NAME: "pull_request" };
    delete env.GITHUB_OUTPUT;
    let error;
    try {
      execFileSync(process.execPath, [SCRIPT], { cwd: dir, encoding: "utf8", stdio: "pipe", env });
    } catch (e) {
      error = e;
    }
    expect(error, "非ゼロで終わらなかった").toBeDefined();
    expect(error.status).toBe(2);
    expect(String(error.stderr)).toContain("GITHUB_OUTPUT");
  });
});
