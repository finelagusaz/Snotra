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
    areaAlways: 15000,
    areaRules: 12000,
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

  // ここが「配線」の検査である——view を外す変異を落とすのはこの 3 件で、
  // 実リポジトリを走らせるカナリア（`governance-check.test.mjs`）ではない
  // （供給が揃っていればカナリアの 3 条件は view の有無に関わらず満たされる・2026-08-17 実測）
  it("配線: 生の袋を渡すと throw する（view を外す変異が落ちる）", () => {
    expect(() => assembleEvidence(complete())).toThrow(/view でなければならない/);
  });

  it("配線: brand は複製できない（スプレッド・Object.assign で view の見た目だけ作っても通らない）", () => {
    const view = evidenceView(complete(), []);
    expect(() => assembleEvidence({ ...view })).toThrow(/view でなければならない/);
    expect(() => assembleEvidence(Object.assign({}, view))).toThrow(/view でなければならない/);
  });

  it("配線: 引数無し・null でも throw する（`WeakSet.has` は primitive に対して false を返す）", () => {
    expect(() => assembleEvidence()).toThrow(/view でなければならない/);
    expect(() => assembleEvidence(null)).toThrow(/view でなければならない/);
    expect(() => assembleEvidence(42)).toThrow(/view でなければならない/);
  });

  // **判定が「トラップの返り値」だったときに素通りした形。** 未記録のキーへ truthy を返す
  // `get` トラップを持つ Proxy はどれも truthy 判定の brand を満たす——`?? "—"` は欠けた値を
  // ダッシュで印字する自然なリファクタで、`String(t[k])` は文字列化を前へ出しただけである。
  // どちらも供給を 1 つ外した袋に対し **findings 0 件**で行を印字していた（2026-08-17 実測）。
  it("配線: 未記録のキーへ truthy を返す別の Proxy は通らない（値ではなく参照で照合する）", () => {
    const src = complete();
    delete src.areaAlways;
    for (const [name, make] of [
      ["ダッシュで埋める", (t) => new Proxy(t, { get: (o, k) => o[k] ?? "—" })],
      ["文字列化を前へ出す", (t) => new Proxy(t, { get: (o, k) => String(o[k]) })],
      ["本物の view を包み直す", (t) => new Proxy(evidenceView(t, []), { get: (o, k) => o[k] })],
    ]) {
      expect(() => assembleEvidence(make(src)), name).toThrow(/view でなければならない/);
    }
  });

  it("入れ子を持たない: 面積は平坦なキーで読む（2 段目の読みはガードを通らない）", () => {
    const findings = [];
    const src = complete();
    delete src.areaAlways;
    const line = assembleEvidence(evidenceView(src, findings));
    expect(findings).toHaveLength(1);
    expect(findings[0].message).toContain("areaAlways");
    expect(line).toContain("常時ロード ? 字");
    expect(line).not.toContain("undefined");
  });

  it("Symbol の読み取りは finding にしない（テンプレート展開の内部読みで誤爆させない）", () => {
    const findings = [];
    const view = evidenceView(complete(), findings);
    expect(`${view.docs.length}`).toBe("1");
    expect(findings).toEqual([]);
  });
});
