// prose-sweep.mjs の純関数を、壊れたとき「0 件」「掲載済み」と印字する経路ごとに検証する
// （.claude/rules/safety-nets.md「壊れたとき緑のまま推移するか」）。
// fixture の形の出所は #1300 の plan-review で実測した `git grep` の出力である——
// `-I` 無しのバイナリ行、`snotra-core/CLAUDE.md` に埋もれるルート `CLAUDE.md`。
import { describe, it, expect } from "vitest";
import {
  UsageError,
  parseArgs,
  parseGrepZ,
  surfaceOf,
  isListedInPlan,
  runGrep,
  buildReport,
} from "./prose-sweep.mjs";

const hit = (file, line, text) => ({ file, line, text });

describe("parseArgs", () => {
  it("語が無ければ使用法エラー", () => {
    expect(() => parseArgs([])).toThrow(UsageError);
    expect(() => parseArgs(["--plan", "p.md"])).toThrow(UsageError);
  });
  it("空語は弾く（-e \"\" は全行に一致する）", () => {
    expect(() => parseArgs(["foo", ""])).toThrow(UsageError);
  });
  it("未知のフラグはエラー", () => {
    expect(() => parseArgs(["--nope", "foo"])).toThrow(UsageError);
  });
  it("--plan の値が無ければエラー", () => {
    expect(() => parseArgs(["foo", "--plan"])).toThrow(UsageError);
  });
  it("--plan と語が混在してよい", () => {
    expect(parseArgs(["a", "--plan", "p.md", "Step 2 または"])).toEqual({
      plan: "p.md",
      terms: ["a", "Step 2 または"],
    });
  });
  it("-- 以降はフラグに見えても語", () => {
    expect(parseArgs(["--", "--deep", "-x"])).toEqual({ plan: null, terms: ["--deep", "-x"] });
  });
});

describe("parseGrepZ", () => {
  it("本文に : を含む行と日本語パスを壊さない", () => {
    const out = "docs/a.md\u000012\u0000x: y: z\n設計/b.md\u00003\u0000本文\r\n";
    expect(parseGrepZ(out)).toEqual([hit("docs/a.md", 12, "x: y: z"), hit("設計/b.md", 3, "本文")]);
  });
  it("NUL 区切りでない行は黙って捨てずに throw する（-I 欠落のバイナリ行）", () => {
    expect(() => parseGrepZ("Binary file src-tauri/icons/icon.icns matches\n")).toThrow();
  });
  it("行番号の欄が空なら throw する（Number(\"\") は 0）", () => {
    expect(() => parseGrepZ("a.md\u0000\u0000t\n")).toThrow();
  });
});

describe("surfaceOf", () => {
  it.each([
    ["AGENTS.md", "常時ロード規範"],
    ["CLAUDE.md", "常時ロード規範"],
    [".claude/skills/x/SKILL.md", "skills・rules・hooks"],
    [".githooks/pre-commit", "skills・rules・hooks"],
    ["docs/x.md", "生きた文書"],
    ["snotra-core/CLAUDE.md", "生きた文書"],
    ["docs/adr/ADR-x.md", "凍結された歴史"],
    ["docs/superpowers/plans/x.md", "凍結された歴史"],
    ["src-tauri/src/x.rs", "コード"],
  ])("%s → %s", (file, surface) => {
    expect(surfaceOf(file)).toBe(surface);
  });
  it("どの面にも名指されないファイルも出力から消えない", () => {
    for (const f of ["docs/design/x.md", "PERFORMANCE.md", "package.json"]) {
      const r = buildReport({ terms: ["t"], hitsByTerm: [[hit(f, 1, "t")]] });
      expect(r.text).toContain(f);
      expect(r.total).toBe(1);
    }
  });
});

describe("isListedInPlan", () => {
  it("パス境界で照合する — サブディレクトリの CLAUDE.md はルートの掲載にならない", () => {
    const plan = "| `snotra-core/CLAUDE.md` | 変更 |";
    expect(isListedInPlan("CLAUDE.md", plan)).toBe(false);
    expect(isListedInPlan("snotra-core/CLAUDE.md", plan)).toBe(true);
  });
  it("\\ 区切りのサブディレクトリもルートの掲載にならない", () => {
    expect(isListedInPlan("CLAUDE.md", "snotra-core\\CLAUDE.md")).toBe(false);
  });
  it("末尾も境界 — a.mdx は a.md の掲載にならない", () => {
    expect(isListedInPlan("a.md", "`a.mdx`")).toBe(false);
    expect(isListedInPlan("a.md", "`a.md`。")).toBe(true);
    expect(isListedInPlan("a.md", "a.md\\x")).toBe(false);
  });
  it("行頭・バッククォート・空白の後は境界", () => {
    expect(isListedInPlan("AGENTS.md", "AGENTS.md を読む")).toBe(true);
    expect(isListedInPlan("AGENTS.md", "x `AGENTS.md`")).toBe(true);
  });
});

describe("runGrep", () => {
  const spawnReturning = (r) => () => r;
  it("status 1 は一致なし", () => {
    expect(runGrep("t", { cwd: ".", spawn: spawnReturning({ status: 1, stdout: "", stderr: "" }) })).toEqual([]);
  });
  it("status 0 は解析する", () => {
    const stdout = "a.md\u00001\u0000t\n";
    expect(runGrep("t", { cwd: ".", spawn: spawnReturning({ status: 0, stdout, stderr: "" }) })).toEqual([
      hit("a.md", 1, "t"),
    ]);
  });
  it.each([
    ["status 2", { status: 2, stdout: "", stderr: "fatal" }],
    ["status null（maxBuffer 超過・シグナル）", { status: null, stdout: "", stderr: "" }],
    ["error（git 不在）", { status: null, error: new Error("ENOENT"), stdout: "", stderr: "" }],
  ])("%s は throw し、0 件の報告を作らない", (_, r) => {
    expect(() => runGrep("t", { cwd: ".", spawn: spawnReturning(r) })).toThrow();
  });
  it("母集団の引数: -I・--untracked・-F・workspace 除外", () => {
    let args;
    runGrep("t", {
      cwd: ".",
      spawn: (_cmd, a) => {
        args = a;
        return { status: 1, stdout: "", stderr: "" };
      },
    });
    expect(args).toEqual(expect.arrayContaining(["-I", "--untracked", "-F", "-z", "-e", "t", ":!workspace"]));
  });
});

describe("buildReport", () => {
  it("未掲載ファイルに ★未掲載 が付き件数が合う", () => {
    const r = buildReport({
      terms: ["t"],
      hitsByTerm: [[hit("AGENTS.md", 1, "t"), hit("docs/x.md", 2, "t")]],
      planText: "`docs/x.md` を直す",
    });
    expect(r.unlisted).toBe(1);
    expect(r.text).toMatch(/AGENTS\.md.*★未掲載/);
    expect(r.text).not.toMatch(/docs\/x\.md.*★未掲載/);
  });
  it("plan に snotra-core/CLAUDE.md だけがあるとき、ルート CLAUDE.md に ★未掲載 が付く", () => {
    const r = buildReport({
      terms: ["t"],
      hitsByTerm: [[hit("CLAUDE.md", 1, "t"), hit("snotra-core/CLAUDE.md", 1, "t")]],
      planText: "`snotra-core/CLAUDE.md`",
    });
    expect(r.unlisted).toBe(1);
    expect(r.text).toMatch(/^### CLAUDE\.md.*★未掲載/m);
  });
  it("0 件の語も印字される", () => {
    const r = buildReport({ terms: ["a", "b"], hitsByTerm: [[hit("x.md", 1, "a")], []] });
    expect(r.text).toMatch(/"b": 0/);
  });
  it("コード行だけのヒットも落ちない", () => {
    const r = buildReport({ terms: ["t"], hitsByTerm: [[hit("src/x.rs", 9, "let t = 1;")]] });
    expect(r.text).toContain("let t = 1;");
    expect(r.total).toBe(1);
  });
  it("2 語が同じ行に当たると総件数は 1、語ごとの件数は各 1", () => {
    const r = buildReport({ terms: ["a", "b"], hitsByTerm: [[hit("x.md", 1, "a b")], [hit("x.md", 1, "a b")]] });
    expect(r.total).toBe(1);
    expect(r.text).toMatch(/"a": 1/);
    expect(r.text).toMatch(/"b": 1/);
  });
  it("本文を切り詰めない", () => {
    const long = "あ".repeat(2000);
    const r = buildReport({ terms: ["あ"], hitsByTerm: [[hit("x.md", 1, long)]] });
    expect(r.text).toContain(long);
  });
});
