import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkRulesScriptCoverage } from "./G-rules-script-coverage.mjs";

/** 実物と同じ 2 本の rule を持つ fixture を組む。`paths` の中身だけを差し替える。 */
const rules = (safetyNetsPaths, governanceDocsPaths) => ({
  ".claude/rules/safety-nets.md": `---\npaths:\n${safetyNetsPaths.map((p) => `  - "${p}"`).join("\n")}\n---\n本文\n`,
  ".claude/rules/governance-docs.md": `---\npaths:\n${governanceDocsPaths.map((p) => `  - "${p}"`).join("\n")}\n---\n本文\n`,
});

describe("G-rules-script-coverage checkRulesScriptCoverage", () => {
  // #1093 の実形の変異——判定がディレクトリごと `scripts/governance/` へ移り、`scripts/*.mjs`（直下のみ）の
  // 外へ出た状態。`G-rules-globs` はこの状態でも緑である（facade がマッチするのでマッチ 0 件にならない）。
  it("赤: paths が scripts/ 直下しか覆わないとき、部分木のファイルを名指す（#1093 の再発形）", () => {
    const s = snap(rules(["scripts/*.mjs"], ["scripts/*.mjs"]), ["scripts/governance-check.mjs", "scripts/governance/checks/G-x.mjs"]);
    const f = checkRulesScriptCoverage(s);
    expect(f.some((x) => x.file === ".claude/rules/safety-nets.md" && x.message.includes("scripts/governance/checks/G-x.mjs"))).toBe(true);
    expect(f.some((x) => x.file === ".claude/rules/governance-docs.md" && x.message.includes("scripts/governance/checks/G-x.mjs"))).toBe(true);
    // 覆われている facade は名指されない（誤検出しない向きの検算）
    expect(f.some((x) => x.message.includes("scripts/governance-check.mjs"))).toBe(false);
  });

  it("緑: paths が scripts/** を持てば部分木ごと覆う", () => {
    const s = snap(rules(["scripts/**", ".claude/hooks/**", ".githooks/**"], ["scripts/**"]), [
      "scripts/governance/checks/G-x.mjs",
      "scripts/lib/SnotraSmoke.psm1",
      "scripts/smoke-egui.ps1",
      ".claude/hooks/post-edit.mjs",
      ".githooks/githooks.test.mjs",
    ]);
    expect(checkRulesScriptCoverage(s)).toEqual([]);
  });

  it("safety-nets.md の母集団は scripts/ の外へも及ぶ（.claude/hooks/ の .mjs）", () => {
    const s = snap(rules(["scripts/**"], ["scripts/**"]), ["scripts/governance/checks/G-x.mjs", ".claude/hooks/post-edit.mjs"]);
    const f = checkRulesScriptCoverage(s);
    expect(f.some((x) => x.file === ".claude/rules/safety-nets.md" && x.message.includes(".claude/hooks/post-edit.mjs"))).toBe(true);
    // governance-docs.md の母集団は scripts/ 部分木に限る——あちらの射程外を要求しない
    expect(f.some((x) => x.file === ".claude/rules/governance-docs.md")).toBe(false);
  });

  // 母集団の**上界**——対象外の拡張子を巻き込まない（`.sh` は死角として宣言済み）。
  it("判定の対象は .mjs / .ps1 / .psm1 に限る（.sh は死角として宣言済み）", () => {
    const s = snap(rules(["scripts/*.mjs"], ["scripts/*.mjs"]), ["scripts/run-codex.sh", "scripts/x.mjs"]);
    expect(checkRulesScriptCoverage(s)).toEqual([]);
  });

  // 母集団の**下界**——上界だけを縛ると `SCRIPT_EXT` を `.mjs` へ狭める変異が全層で沈黙する
  // （実測: 実ツリーは全件被覆ゆえ live も narrowed も 0 件、テストも `governance:check` も緑のまま）。
  // **本 issue が直した欠陥と同じ形が、この検査自身の中に生まれる**ので、下界を名指しで固定する。
  it("母集団の下界: .ps1 / .psm1 も対象である（SCRIPT_EXT を狭める変異を捕まえる）", () => {
    const ps = ["scripts/lib/SnotraSmoke.psm1", "scripts/smoke-egui.ps1"];
    const s = snap(rules(["scripts/*.mjs"], ["scripts/*.mjs"]), ps);
    const f = checkRulesScriptCoverage(s);
    for (const rel of ps) {
      expect(f.some((x) => x.file === ".claude/rules/safety-nets.md" && x.message.includes(rel)), `${rel} が母集団から落ちている`).toBe(true);
      expect(f.some((x) => x.file === ".claude/rules/governance-docs.md" && x.message.includes(rel)), `${rel} が母集団から落ちている`).toBe(true);
    }
  });

  // --- 下界の canary（被覆形の述語は母集団が縮む側で沈黙する） -----------------
  it("canary: rule が snapshot に無いと finding を出す（母集団ごと消えても緑にしない）", () => {
    const s = snap({}, ["scripts/governance/checks/G-x.mjs"]);
    const f = checkRulesScriptCoverage(s);
    expect(f.filter((x) => x.message.includes("rule が snapshot に無い"))).toHaveLength(2);
  });

  it("canary: paths が 0 件なら finding 1 件で打ち切る（全件未被覆で溢れさせない）", () => {
    const s = snap({ ".claude/rules/safety-nets.md": "本文だけ\n", ".claude/rules/governance-docs.md": "本文だけ\n" }, [
      "scripts/a.mjs",
      "scripts/b.mjs",
      "scripts/c.mjs",
    ]);
    const f = checkRulesScriptCoverage(s);
    expect(f).toHaveLength(2);
    expect(f.every((x) => x.message.includes("paths パターンが 1 件も無い"))).toBe(true);
  });

  it("canary: 母集団が 0 件なら finding を出す（走査の欠落を緑にしない）", () => {
    const s = snap(rules(["scripts/**"], ["scripts/**"]), ["AGENTS.md"]);
    const f = checkRulesScriptCoverage(s);
    expect(f.filter((x) => x.message.includes("母集団が 0 件"))).toHaveLength(2);
  });
});
