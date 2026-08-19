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

  it("adrFiles ドメインは docs/adr/ 直下の md である", () => {
    const m = buildDomains(makeSnapshot(ROOT)).get("adrFiles").members;
    expect(m.length).toBeGreaterThan(0);
    expect(m.every((f) => /^docs\/adr\/[^/]+\.md$/.test(f))).toBe(true);
  });

  // I2 / ⚠️3 の修正検算——`holds([], snapshot)` の合成 [] では、腕を丸ごと足し忘れても
  // 「非空虚」テストは黙って通る（triage #2 が指摘した死角）。ここでは実ツリーの members から
  // 当該腕だけを引いた集合を渡し、対応する錨が実際に false（発火）へ倒れることを検算する。
  it("governanceDocs は docs/ / .claude/rules/ / .claude/skills/ の各腕が消えると対応する錨が倒れる", () => {
    const snapshot = makeSnapshot(ROOT);
    const spec = DOMAIN_SPECS.find((s) => s.name === "governanceDocs");
    const full = spec.members(snapshot);
    const arms = [
      ["docs/ の腕", (f) => f.startsWith("docs/")],
      [".claude/rules/ の腕", (f) => f.startsWith(".claude/rules/")],
      [".claude/skills/ の腕", (f) => f.startsWith(".claude/skills/")],
    ];
    for (const [label, pred] of arms) {
      const narrowed = full.filter((f) => !pred(f));
      const anchor = spec.anchors.find((a) => a.label === label);
      expect(anchor, `錨 ${label} が見つからない`).toBeDefined();
      expect(anchor.holds(narrowed, snapshot), `腕 ${label} を除いても錨が成立している＝沈黙する`).toBe(false);
    }
  });

  // #1143 の当の母集団。腕ごとに「その腕だけを引いた集合」で錨が倒れることを実ツリーで測る
  // ——`holds([], snapshot)` の合成 [] は、腕を足し忘れても黙って通る。
  it("judgingScripts は腕ごとの絞り込みで対応する錨が倒れる", () => {
    const snapshot = makeSnapshot(ROOT);
    const spec = DOMAIN_SPECS.find((s) => s.name === "judgingScripts");
    const full = spec.members(snapshot);
    const dirOf = (f) => f.slice(0, f.lastIndexOf("/"));
    const arms = [
      ["scripts/ 直下", (f) => dirOf(f) === "scripts"],
      ["scripts/governance/ 直下", (f) => dirOf(f) === "scripts/governance"],
      ["scripts/governance/checks/ 直下", (f) => dirOf(f) === "scripts/governance/checks"],
      ["scripts/lib/ 直下", (f) => dirOf(f) === "scripts/lib"],
      [".claude/hooks/ 直下", (f) => dirOf(f) === ".claude/hooks"],
      ["ps 族（.ps1/.psm1）の腕", (f) => /\.(ps1|psm1)$/i.test(f)],
    ];
    for (const [label, pred] of arms) {
      const narrowed = full.filter((f) => !pred(f));
      expect(narrowed.length, `腕 ${label} が実ツリーで空——絞り込みが何も引いていない`).toBeLessThan(full.length);
      const anchor = spec.anchors.find((a) => a.label === label);
      expect(anchor, `錨 ${label} が見つからない`).toBeDefined();
      expect(anchor.holds(narrowed, snapshot), `腕 ${label} を除いても錨が成立している＝沈黙する`).toBe(false);
    }
  });

  it("headingRefCommentDocs は ps 族（.ps1/.psm1/.psd1）の腕が消えると対応する錨が倒れる", () => {
    const snapshot = makeSnapshot(ROOT);
    const spec = DOMAIN_SPECS.find((s) => s.name === "headingRefCommentDocs");
    const full = spec.members(snapshot);
    const narrowed = full.filter((f) => !/\.(ps1|psm1|psd1)$/i.test(f));
    const anchor = spec.anchors.find((a) => a.label.includes("ps 族"));
    expect(anchor, "ps 族の錨が見つからない").toBeDefined();
    expect(anchor.holds(narrowed, snapshot), "ps 族の腕を除いても錨が成立している＝沈黙する").toBe(false);
  });
});
