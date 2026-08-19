import { fileURLToPath } from "node:url";
import { describe, it, expect } from "vitest";
import { makeSnapshot } from "./lib.mjs";
import { buildDomains, DOMAIN_SPECS } from "./domains.mjs";

const ROOT = fileURLToPath(new URL("../../", import.meta.url));

describe("buildDomains", () => {
  it("実ツリーで全ドメインのメンバーが非空である", () => {
    const domains = buildDomains(makeSnapshot(ROOT));
    expect(domains.size).toBe(DOMAIN_SPECS.length);
    for (const d of domains.values()) {
      expect(d.members.length, `ドメイン ${d.name} のメンバーが 0 件`).toBeGreaterThan(0);
    }
  });

  it("実ツリーで全ドメインの錨が成立する", () => {
    const snapshot = makeSnapshot(ROOT);
    const domains = buildDomains(snapshot);
    for (const d of domains.values()) {
      for (const a of d.anchors) {
        expect(a.holds(d.members, snapshot), `ドメイン ${d.name} の錨が成立しない: ${a.label}`).toBe(true);
      }
    }
  });

  it("錨はメンバーが縮むと成立しなくなる（錨が空虚でないことの検算）", () => {
    const snapshot = makeSnapshot(ROOT);
    for (const d of buildDomains(snapshot).values()) {
      for (const a of d.anchors) {
        expect(a.holds([], snapshot), `ドメイン ${d.name} の錨 ${a.label} は空の母集団でも成立する＝空虚`).toBe(false);
      }
    }
  });
});
