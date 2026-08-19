import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkRulesGlobs } from "./G-rules-globs.mjs";

// `globToRegex` の意味論テストは `lib.test.mjs` が持つ（#1143 で移送。2 検査が共有するため）。
describe("G-rules-globs checkRulesGlobs", () => {
  it("緑: 全 glob が 1 件以上にマッチする", () => {
    const s = snap({ ".claude/rules/a.md": '---\npaths:\n  - "AGENTS.md"\n  - "ui/src/**/*.{ts,tsx}"\n---\n本文\n' }, ["AGENTS.md", "ui/src/main.tsx"]);
    expect(checkRulesGlobs(s)).toEqual([]);
  });
  it("赤: マッチ 0 件の glob を検出する", () => {
    const s = snap({ ".claude/rules/a.md": '---\npaths:\n  - "gone/**/*.rs"\n---\n本文\n' }, ["AGENTS.md"]);
    const f = checkRulesGlobs(s);
    expect(f.some((x) => x.message.includes("gone/**/*.rs"))).toBe(true);
  });
});
