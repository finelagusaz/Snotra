import { fileURLToPath } from "node:url";
import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { makeSnapshot } from "../lib.mjs";
import { checkRulesScriptCoverage, COVERAGE } from "./G-rules-script-coverage.mjs";

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

  // 母集団の下界のうち、**走査（`makeSnapshot`）側**を縛る。上の fixture テストはこちらに反応できない
  // ——`makeSnapshot` を呼ばないためである。実測（2026-08-19）: `WALK_EXCLUDE_PATHS` へ `scripts/governance` を
  // 足すと #1093 の再発形の検知が 106 件 → 0 件へ落ち、`governance:check` も manifest delta も沈黙した。
  // `scripts/lib/` は `.psm1` / `.ps1` しか持たないので、この 1 本が `SCRIPT_EXT` の狭窄にも同時に反応する。
  //
  // **縛るのは `scripts/` 部分木だけである**（`.claude/hooks/` と `.githooks/` は意図的に見ない）。
  // かつて入れていたが、実測で (1) どちらの狭窄の検知にも寄与せず、(2) `.githooks/` は母集団へ
  // `githooks.test.mjs` 1 件しか出さないため、**そのテストを移すだけでこの検査が赤くなり**、
  // しかもメッセージが `WALK_EXCLUDE_PATHS` を指して原因から目を逸らさせた。検知器は必要な分だけ縛る。
  it("canary: 実ツリーの母集団が走査側で縮んでいない（WALK_EXCLUDE_PATHS の狭窄を捕まえる）", () => {
    const files = makeSnapshot(fileURLToPath(new URL("../../../", import.meta.url))).files;
    const [safetyNets, governanceDocs] = COVERAGE;
    const pop = files.filter(safetyNets.inPopulation);
    // **前方一致ではなく「そのディレクトリ直下」で見る。** 前方一致だと、`scripts/governance/` 直下の 13 件
    // （`registry.mjs` を含む——#1143 の発端そのもの）が走査から消えても、配下の `checks/` が同じ接頭辞に
    // 当たるので沈黙する（実測: その層を落として exit 0 / 9 passed）。
    const dirOf = (f) => f.slice(0, f.lastIndexOf("/"));
    for (const dir of ["scripts/governance/checks", "scripts/governance", "scripts/lib"]) {
      expect(
        pop.some((f) => dirOf(f) === dir),
        `${dir}/ 直下が母集団から消えている`,
      ).toBe(true);
    }
    expect(
      files.filter(governanceDocs.inPopulation).some((f) => dirOf(f) === "scripts/governance/checks"),
      "governance-docs 側の母集団から scripts/governance/checks/ 直下が消えている",
    ).toBe(true);
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
