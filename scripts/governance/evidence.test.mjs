import { describe, it, expect } from "vitest";
import { assembleEvidence, evidenceView } from "./evidence.mjs";

describe("evidence の読み取りガード（#1098 — undefined を印字して exit 0 になる経路の閉塞）", () => {
  // 守りたい対象 = 検査が `ctx.record` を呼ばなくなったとき。実測（使い捨て worktree・
  // `G-heading-refs.mjs` の `run()` から `ctx.record` の呼び出しだけを外した）で
  // `見出し参照 undefined 件を…照合` と印字しながら exit 0、`npm test` も全緑だった。
  const complete = () => ({
    checkCount: 19,
    docs: ["a.md"],
    rules: 8,
    skills: 12,
    area: { always: 15000, rules: 12000 },
    headingRefs: 219,
    refDocs: ["a.md"],
    refSourceDocs: ["a.rs"],
    workspaceMembers: 4,
    clippyDisallowed: 8,
    stale: 377,
    staleTargets: ["a.md"],
    nearRefs: 14,
    adrFiles: 53,
    adrCitations: 257,
  });

  it("緑: すべて記録済みなら finding は出ず、undefined も `?` も印字しない", () => {
    const findings = [];
    const line = assembleEvidence(evidenceView(complete(), findings));
    expect(findings).toEqual([]);
    expect(line).not.toContain("undefined");
    expect(line).not.toContain("?");
    expect(line).toContain("見出し参照 219 件");
  });

  it("赤: evidence が読むキーが未記録なら finding になり、`undefined` ではなく `?` を印字する", () => {
    const findings = [];
    const src = complete();
    delete src.headingRefs;
    const line = assembleEvidence(evidenceView(src, findings));
    expect(findings).toHaveLength(1);
    expect(findings[0].message).toContain("headingRefs");
    expect(line).not.toContain("undefined");
    expect(line).toContain("見出し参照 ? 件");
  });

  it("赤: 未記録が複数あれば件数ぶん finding が出る（4 検査すべてが record を持つ）", () => {
    const findings = [];
    const src = complete();
    for (const k of ["headingRefs", "nearRefs", "stale", "adrCitations"]) delete src[k];
    assembleEvidence(evidenceView(src, findings));
    expect(findings).toHaveLength(4);
  });

  it("消費点が母集団である: テンプレートへ足したキーは、供給されていなければ自動で赤になる", () => {
    // `REQUIRED_RECORDS` のような一覧を持たない理由がこれである——一覧は腐る写しになるが、
    // 「テンプレートが読むキーの集合」は定義そのものなので腐りようがない
    const findings = [];
    const view = evidenceView(complete(), findings);
    expect(`${view.brandNewKey}`).toBe("?");
    expect(findings).toHaveLength(1);
    expect(findings[0].message).toContain("brandNewKey");
  });

  it("view は読み取り専用（evidence を組む途中で入力を書き換えられない）", () => {
    expect(() => {
      evidenceView(complete(), []).headingRefs = 0;
    }).toThrow(/読み取り専用/);
  });

  it("Symbol の読み取りは finding にしない（テンプレート展開の内部読みで誤爆させない）", () => {
    const findings = [];
    const view = evidenceView(complete(), findings);
    expect(`${view.docs.length}`).toBe("1");
    expect(findings).toEqual([]);
  });
});
