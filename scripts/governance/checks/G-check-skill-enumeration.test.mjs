import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkCheckSkillEnumeration } from "./G-check-skill-enumeration.mjs";

describe("G-check-skill-enumeration checkCheckSkillEnumeration（4a の列挙 ↔ AGENTS.md 表・#778）", () => {
  // 守りたい対象 = 表へ check スキルを足した人が /implement 4a を直さず、
  // 新しいスキルが報告母集団から沈黙して落ちる形。
  const mk = (tableSkills, step4aSkills, files = []) =>
    snap(
      {
        "AGENTS.md": `## 条件別チェック（トリガー → 参照先）\n\n| t | ${tableSkills.join(" ")} |\n\n## 次節\n`,
        ".claude/skills/implement/SKILL.md": `## Step 4\n\n### 4a. check スキルの実行\n\n変更に応じて ${step4aSkills.join("・")} を実行する。\n\n### 4b. x\n`,
      },
      files,
    );
  const SKILLS = ["/dry-check", "/race-check"].map((s) => `.claude/skills/${s.slice(1)}/SKILL.md`);

  it("集合が一致すれば findings 無し（緑）", () => {
    expect(checkCheckSkillEnumeration(mk(["`/dry-check`", "`/race-check`"], ["`/race-check`", "`/dry-check`"], SKILLS))).toEqual([]);
  });

  it("赤: 表に在って 4a に無い（報告母集団から沈黙して落ちる）", () => {
    const f = checkCheckSkillEnumeration(mk(["`/dry-check`", "`/race-check`"], ["`/dry-check`"], SKILLS));
    expect(f.some((x) => x.message.includes("/race-check") && x.message.includes("4a の列挙に無い"))).toBe(true);
  });

  it("赤: 4a に在って表に無い（起動条件を持たない検査）", () => {
    const f = checkCheckSkillEnumeration(mk(["`/dry-check`"], ["`/dry-check`", "`/race-check`"], SKILLS));
    expect(f.some((x) => x.message.includes("/race-check") && x.message.includes("表に無い"))).toBe(true);
  });

  it("赤: 列挙されたスキルが実在しない（誤記）", () => {
    const f = checkCheckSkillEnumeration(mk(["`/typo-check`"], ["`/typo-check`"], []));
    expect(f.some((x) => x.message.includes("実在しない"))).toBe(true);
  });

  it("判定対象外の不混入: `-check` で終わらないスキルは母集団に入らない", () => {
    // 表にだけ /plan-review が在っても、4a に無いことを咎めない
    const s = mk(["`/dry-check`", "`/plan-review`"], ["`/dry-check`"], SKILLS);
    expect(checkCheckSkillEnumeration(s)).toEqual([]);
  });

  it("赤: 見出しが変わって節を切り出せない（沈黙で通さない）", () => {
    const s = snap({ "AGENTS.md": "## 別の見出し\n", ".claude/skills/implement/SKILL.md": "### 4a. x\n`/dry-check`\n" });
    expect(checkCheckSkillEnumeration(s).some((x) => x.message.includes("見つからない"))).toBe(true);
  });

  it("赤: 空母集団は明示 fail（沈黙経路の閉塞）", () => {
    const f = checkCheckSkillEnumeration(mk([], [], []));
    expect(f.some((x) => x.message.includes("母集団の欠落"))).toBe(true);
  });
});
