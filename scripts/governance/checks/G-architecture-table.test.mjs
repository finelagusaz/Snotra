import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkArchitectureTable } from "./G-architecture-table.mjs";

describe("G-architecture-table checkArchitectureTable", () => {
  it("緑: ファイル単位のモジュール表が無い", () => {
    const s = snap({ "docs/architecture.md": "# a\n| 型 | 役割 |\n|---|---|\n| `Engine` | 入口 |\n" });
    expect(checkArchitectureTable(s)).toEqual([]);
  });
  it("赤: 先頭セルがバッククォート付きファイル名の表行を検出する", () => {
    const s = snap({ "docs/architecture.md": "| `engine.rs` | 検索エンジン |\n" });
    const f = checkArchitectureTable(s);
    expect(f.some((x) => x.message.includes("engine.rs"))).toBe(true);
  });
  it("コードフェンス内の表行は無視する", () => {
    const s = snap({ "docs/architecture.md": "```\n| `engine.rs` | x |\n```\n" });
    expect(checkArchitectureTable(s)).toEqual([]);
  });
});
