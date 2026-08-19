//! G-architecture-table — `docs/architecture.md` の対応表 ↔ 実ファイル。
import { finding, linesOutsideFences } from "../lib.mjs";

export const id = "G-architecture-table";
export const domains = "unmigrated";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return checkArchitectureTable(snapshot);
}

// ---------------------------------------------------------------------------
// G-architecture-table — docs/architecture.md にファイル単位モジュール表が再導入されていないか（旧 Check 2）
// ---------------------------------------------------------------------------
export function checkArchitectureTable(snapshot) {
  const findings = [];
  const p = "docs/architecture.md";
  const text = snapshot.read(p);
  if (text == null) return [finding(p, 1, "docs/architecture.md が読めない")];
  for (const [lineNo, line] of linesOutsideFences(text, p, findings)) {
    if (/^\|\s*`[^`]+\.(rs|ts|tsx|mts|mjs)`\s*\|/.test(line)) {
      findings.push(finding(p, lineNo, `ファイル単位のモジュール表行が再導入されている: ${line.trim().slice(0, 60)}（責務の正本は //! / TSDoc・#562）`));
    }
  }
  return findings;
}
