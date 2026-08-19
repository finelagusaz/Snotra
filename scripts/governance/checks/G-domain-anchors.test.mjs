import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkDomainAnchors } from "./G-domain-anchors.mjs";

const dom = (name, members, anchors) => new Map([[name, { name, members, anchors }]]);
const always = { label: "常に成立", holds: () => true };

describe("G-domain-anchors checkDomainAnchors", () => {
  it("緑: 錨が成立し、メンバーが非空", () => {
    expect(checkDomainAnchors(dom("d", ["a.md"], [always]), snap({}))).toEqual([]);
  });

  it("赤: 錨が成立しないとき、ドメイン名と錨の label を名指す", () => {
    const f = checkDomainAnchors(dom("d", ["a.md"], [{ label: "b.md が居る", holds: (m) => m.includes("b.md") }]), snap({}));
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("d");
    expect(f[0].message).toContain("b.md が居る");
  });

  it("赤: メンバーが 0 件（走査の欠落）", () => {
    const f = checkDomainAnchors(dom("d", [], [always]), snap({}));
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("メンバーが 0 件");
  });

  it("赤: 錨が 0 件（メンバーは非空——保護の欠落が無言で緑にならない）", () => {
    const f = checkDomainAnchors(dom("d", ["a.md"], []), snap({}));
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("d");
    expect(f[0].message).toContain("錨が 0 件");
  });

  it("赤: ドメインが 1 つも無い（メタ検査自身の母集団の欠落）", () => {
    const f = checkDomainAnchors(new Map(), snap({}));
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("ドメインが 0 件");
  });
});
