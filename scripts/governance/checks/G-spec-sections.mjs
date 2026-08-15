//! G-spec-sections — SPEC.md 番号連続性 + SPEC 前置の §N(.x) 参照の実在。
import { finding, linesOutsideFences } from "../lib.mjs";

export const id = "G-spec-sections";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（docs を使う） */
export function run(snapshot, ctx) {
  return checkSpecSections(snapshot, ctx.docs);
}

// ---------------------------------------------------------------------------
// G-spec-sections — SPEC.md 番号連続性 + SPEC 前置の §N(.x) 参照の実在（旧 Check 4 + #587 新規）。
// 裸の `§N` は各文書自身の節参照でありうるため対象外（不混入はテストで固定）。
// ---------------------------------------------------------------------------
export function checkSpecSections(snapshot, docs) {
  const findings = [];
  const spec = snapshot.read("SPEC.md");
  if (spec == null) return [finding("SPEC.md", 1, "SPEC.md が読めない")];
  const sections = new Set();
  let prevTop = null;
  let prevSub = null;
  for (const [lineNo, line] of linesOutsideFences(spec)) {
    const top = line.match(/^## (\d+)\. /);
    if (top) {
      const n = Number(top[1]);
      if (prevTop != null && n !== prevTop + 1) {
        findings.push(finding("SPEC.md", lineNo, `セクション番号が連続しない: ## ${prevTop}. の次が ## ${n}.`));
      }
      prevTop = n;
      prevSub = 0;
      sections.add(`${n}`);
      continue;
    }
    const sub = line.match(/^### (\d+)\.(\d+) /);
    if (sub) {
      const [n, x] = [Number(sub[1]), Number(sub[2])];
      if (n !== prevTop) {
        findings.push(finding("SPEC.md", lineNo, `子セクション ### ${n}.${x} が親 ## ${prevTop}. と不一致`));
      } else if (x !== prevSub + 1) {
        findings.push(finding("SPEC.md", lineNo, `子セクション番号が連続しない: ${n}.${prevSub} の次が ${n}.${x}`));
      }
      prevSub = x;
      sections.add(`${n}.${x}`);
    }
  }
  if (sections.size === 0) findings.push(finding("SPEC.md", 1, "セクション見出し（## N.）が 1 件も無い（G-spec-sections 母集団の欠落）"));
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) continue; // 母集団欠落は G-references が報告する
    for (const [lineNo, line] of linesOutsideFences(text)) {
      for (const m of line.matchAll(/SPEC(?:\.md)?`?(?: の)? ?§(\d+(?:\.\d+)?)/g)) {
        if (!sections.has(m[1])) {
          findings.push(finding(doc, lineNo, `SPEC §${m[1]} が SPEC.md に実在しない`));
        }
      }
    }
  }
  return findings;
}
