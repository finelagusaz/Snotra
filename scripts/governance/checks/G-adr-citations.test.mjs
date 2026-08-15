import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkAdrCitations, scanAdrCitations, adrCitationDocs } from "./G-adr-citations.mjs";

describe("G-adr-citations（ADR の短縮引用が実在するか・#812 の A）", () => {
  // 連番だった頃は書けなかった検査である——`ADR-0007` は引用文字列とファイル名 stem が別物だった。
  // stem = 引用文字列にしたことで初めて機械照合できるようになった。
  const REAL = { "docs/adr/ADR-plan-ownership-boundary.md": "# ADR-plan-ownership-boundary: x\n" };
  const run = (doc, text) => checkAdrCitations(snap({ ...REAL, [doc]: text }), [doc]);

  it("実在する ADR を指す引用は findings 無し（緑）", () => {
    expect(run("CLAUDE.md", "詳細は `ADR-plan-ownership-boundary` を見よ\n")).toEqual([]);
  });

  it("実在しない引用は finding（赤）", () => {
    const f = run("CLAUDE.md", "`ADR-does-not-exist` を見よ\n");
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("ADR-does-not-exist");
  });

  it("製品コードのコメント内の引用も見る（今日まで検出器を持たなかった面）", () => {
    const f = run("src-tauri/src/a.rs", "// 意図的な 2 導出（ADR-gone 却下 1）\n");
    expect(f).toHaveLength(1);
  });

  it("md のコードフェンス内は見ない", () => {
    expect(run("CLAUDE.md", "```\n`ADR-does-not-exist`\n```\n")).toEqual([]);
  });

  it("判定対象外の不混入: 連番形・ADR という語そのもの", () => {
    expect(run("CLAUDE.md", "ADR は否定の知識を記録する。ADR-0007 は旧形式\n")).toEqual([]);
  });

  it("母集団は歴史資料（docs/superpowers/）を含まない", () => {
    const s = snap({ ...REAL, "CLAUDE.md": "", "docs/superpowers/specs/x.md": "", ".claude/skills/a/SKILL.md": "", "src-tauri/src/a.rs": "" });
    const pop = adrCitationDocs(s, ["CLAUDE.md"]);
    expect(pop).not.toContain("docs/superpowers/specs/x.md");
    expect(pop).toContain(".claude/skills/a/SKILL.md");
    expect(pop).toContain("src-tauri/src/a.rs");
  });

  it("テストファイルは母集団外（フィクスチャは赤経路のため意図的に実在しない名前を持つ）", () => {
    const s = snap({ ...REAL, "CLAUDE.md": "", "scripts/x.mjs": "", "scripts/x.test.mjs": "" });
    const pop = adrCitationDocs(s, ["CLAUDE.md"]);
    expect(pop).toContain("scripts/x.mjs");
    expect(pop).not.toContain("scripts/x.test.mjs");
  });

  it("照合件数を返す（「差分ゼロ」と「照合していない」の区別・#497）", () => {
    const r = scanAdrCitations(snap({ ...REAL, "CLAUDE.md": "`ADR-plan-ownership-boundary` と `ADR-gone`\n" }), ["CLAUDE.md"]);
    expect(r.checked).toBe(2);
    expect(r.findings).toHaveLength(1);
  });
});
