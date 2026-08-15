import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkAdrFileNames, adrFiles } from "./G-adr-file-names.mjs";

describe("G-adr-file-names（ADR のファイル名と見出しの形・#816）", () => {
  // 守りたい対象 = `docs/adr/foo.md` や連番への逆戻り。G-adr-citations は引用側しか見ないので、
  // 誰も引用しなければ逸脱が静かに通る（#789 の見直しで残余として特定）。
  const ok = { "docs/adr/ADR-plan-ownership-boundary.md": "# ADR-plan-ownership-boundary: 計画の所有境界\n" };

  it("形が揃っていれば findings 無し（緑）", () => {
    expect(checkAdrFileNames(snap(ok))).toEqual([]);
  });

  it("赤: 連番へ戻る（#812 が廃した形）", () => {
    const f = checkAdrFileNames(snap({ ...ok, "docs/adr/0019-foo.md": "# ADR-0019: x\n" }));
    expect(f.some((x) => x.message.includes("0019-foo.md"))).toBe(true);
  });

  it("赤: ADR- 前置が無い", () => {
    const f = checkAdrFileNames(snap({ ...ok, "docs/adr/foo.md": "# ADR-foo: x\n" }));
    expect(f.some((x) => x.message.includes("foo.md"))).toBe(true);
  });

  it("赤: 見出しがファイル名と食い違う（stem = 引用文字列の対応が崩れる）", () => {
    const f = checkAdrFileNames(snap({ "docs/adr/ADR-alpha.md": "# ADR-beta: x\n" }));
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("食い違う");
  });

  it("赤: 冒頭が `# ADR-<slug>:` の形でない", () => {
    const f = checkAdrFileNames(snap({ "docs/adr/ADR-alpha.md": "# 計画の所有境界\n" }));
    expect(f[0].message).toContain("形でない");
  });

  it("カナリア: 空母集団は明示 fail（走査が空でも「逸脱なし」に見える沈黙経路を塞ぐ）", () => {
    const f = checkAdrFileNames(snap({ "CLAUDE.md": "" }));
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("母集団の欠落");
  });

  it("判定対象外の不混入: docs/adr/ 直下の md だけを見る", () => {
    const s = snap({ ...ok, "docs/adr/sub/0001-x.md": "", "docs/architecture.md": "", "docs/adr/notes.txt": "" });
    expect(adrFiles(s)).toEqual(["docs/adr/ADR-plan-ownership-boundary.md"]);
  });
});
