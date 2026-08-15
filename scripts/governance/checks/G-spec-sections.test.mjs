import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkSpecSections } from "./G-spec-sections.mjs";

describe("G-spec-sections checkSpecSections", () => {
  const spec = "## 1. 概要\n### 1.1 目的\n### 1.2 範囲\n## 2. 検索\n";
  it("緑: 連続した番号と実在する SPEC § 参照", () => {
    const s = snap({ "SPEC.md": spec, "docs/a.md": "SPEC §1.2 と SPEC.md §2 を参照\n" });
    expect(checkSpecSections(s, ["docs/a.md"])).toEqual([]);
  });
  it("赤: 番号の飛びを検出する", () => {
    const s = snap({ "SPEC.md": "## 1. a\n## 3. b\n" });
    const f = checkSpecSections(s, []);
    expect(f.some((x) => x.message.includes("3"))).toBe(true);
  });
  it("赤: 実在しない SPEC § 参照", () => {
    const s = snap({ "SPEC.md": spec, "docs/a.md": "SPEC §9.9 を参照\n" });
    const f = checkSpecSections(s, ["docs/a.md"]);
    expect(f.some((x) => x.message.includes("9.9"))).toBe(true);
  });
  it("コードフェンス内の # 行（TOML コメント等）を見出しと誤認しない", () => {
    const s = snap({ "SPEC.md": "## 1. a\n```toml\n# 旧形式（廃止）\n## 5. ダミー\n```\n## 2. b\n" });
    expect(checkSpecSections(s, [])).toEqual([]);
  });
  it("バッククォート隣接形（`SPEC.md` §N）も参照として拾う（レビュー L2）", () => {
    const s = snap({ "SPEC.md": spec, "docs/a.md": "`SPEC.md` §9.9 を参照\n" });
    const f = checkSpecSections(s, ["docs/a.md"]);
    expect(f.some((x) => x.message.includes("9.9"))).toBe(true);
  });
  it("SPEC 前置のない裸の §N は検査対象外（不混入検算）", () => {
    const s = snap({ "SPEC.md": spec, "docs/a.md": "設計文書 §99 を参照\n" });
    expect(checkSpecSections(s, ["docs/a.md"])).toEqual([]);
  });
});
