import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, it, expect } from "vitest";
import { CHECK_MODULES, checkModulesFrom } from "./registry.mjs";

describe("registry（checks/ の走査から導出する・#1088）", () => {
  it("検査モジュールが 1 本以上あり、すべて id と run を持つ", () => {
    expect(CHECK_MODULES.length).toBeGreaterThan(0);
    for (const m of CHECK_MODULES) {
      expect(typeof m.id, `${m.id} の id が string でない`).toBe("string");
      expect(typeof m.run, `${m.id} の run が function でない`).toBe("function");
    }
  });
  it("id は昇順（readdir 順の揺れが出力順に出ない）", () => {
    const ids = CHECK_MODULES.map((m) => m.id);
    expect(ids).toEqual([...ids].sort());
  });
  it("id が重複しない", () => {
    const ids = CHECK_MODULES.map((m) => m.id);
    expect(new Set(ids).size, `重複: ${ids.join(",")}`).toBe(ids.length);
  });
});

describe("registry の形の検証（複製に変異を当てる）", () => {
  const withDir = async (files, fn) => {
    const dir = mkdtempSync(path.join(tmpdir(), "gov-registry-"));
    try {
      for (const [name, body] of Object.entries(files)) writeFileSync(path.join(dir, name), body);
      return await fn(dir);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  };
  it("id を持たないモジュールがあれば throw する（沈黙して落とさない）", async () => {
    await withDir({ "G-a.mjs": 'export const id = "G-a";\nexport function run() { return []; }\n', "G-bad.mjs": "export function run() { return []; }\n" }, async (dir) => {
      await expect(checkModulesFrom(dir)).rejects.toThrow(/G-bad\.mjs/);
    });
  });
  it("run を持たないモジュールがあれば throw する", async () => {
    await withDir({ "G-bad.mjs": 'export const id = "G-bad";\n' }, async (dir) => {
      await expect(checkModulesFrom(dir)).rejects.toThrow(/G-bad\.mjs/);
    });
  });
  it("ファイル名の stem と id が食い違えば throw する（一覧が ID の一覧であることの担保）", async () => {
    await withDir({ "G-a.mjs": 'export const id = "G-different";\nexport function run() { return []; }\n' }, async (dir) => {
      await expect(checkModulesFrom(dir)).rejects.toThrow(/G-a\.mjs/);
    });
  });
  it("`.test.mjs` は検査として読まない", async () => {
    await withDir({ "G-a.mjs": 'export const id = "G-a";\nexport function run() { return []; }\n', "G-a.test.mjs": "export const nothing = 1;\n" }, async (dir) => {
      expect((await checkModulesFrom(dir)).map((m) => m.id)).toEqual(["G-a"]);
    });
  });
});
