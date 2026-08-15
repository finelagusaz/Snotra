import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkSkillTable } from "./G-skill-table.mjs";

describe("G-skill-table checkSkillTable（表の対象は roster に載らない skill だけ）", () => {
  const claude = (rows) => `# x\n## 利用できるスキル\n\n| スキル | 使うとき |\n|---|---|\n${rows}\n\n## 次節\n`;
  /** roster に載る skill（harness が description ごと注入する） */
  const shown = (name) => ({ [`.claude/skills/${name}/SKILL.md`]: `---\nname: ${name}\ndescription: "d"\n---\n本文\n` });
  /** roster に載らない skill（user 起動専用） */
  const hidden = (name) => ({
    [`.claude/skills/${name}/SKILL.md`]: `---\nname: ${name}\ndescription: "d"\ndisable-model-invocation: true\n---\n本文\n`,
  });

  it("緑: 隠しスキルだけが表に載り、roster に載るスキルは載っていない", () => {
    const s = snap({ "CLAUDE.md": claude("| `/health-check` | 定期 |"), ...hidden("health-check"), ...shown("plan-review") });
    expect(checkSkillTable(s)).toEqual([]);
  });
  it("赤: 表にあるがディレクトリに無い", () => {
    const s = snap({ "CLAUDE.md": claude("| `/gone-skill` | x |"), ...hidden("health-check") });
    const f = checkSkillTable(s);
    expect(f.some((x) => x.message.includes("gone-skill") && x.message.includes("SKILL.md が無い"))).toBe(true);
  });
  it("赤: 隠しスキルが表に無い（索引としての意味が消える）", () => {
    const s = snap({ "CLAUDE.md": claude("| `/health-check` | x |"), ...hidden("health-check"), ...hidden("orphan") });
    const f = checkSkillTable(s);
    expect(f.some((x) => x.message.includes("orphan") && x.message.includes("roster に載らないのに"))).toBe(true);
  });
  it("赤: roster に載るスキルが表にある（同じ面での二重課税）", () => {
    const s = snap({ "CLAUDE.md": claude("| `/plan-review` | x |"), ...shown("plan-review"), ...hidden("health-check") });
    const f = checkSkillTable(s);
    expect(f.some((x) => x.message.includes("plan-review") && x.message.includes("roster に載る"))).toBe(true);
  });
  it("緑: frontmatter が壊れた skill は「隠しでない」へ倒れる（表に無くても赤にしない）", () => {
    // 判定不能を「隠し」へ倒すと、書きようのない表の行を要求して赤が意味を失う
    const s = snap({
      "CLAUDE.md": claude("| `/health-check` | x |"),
      ...hidden("health-check"),
      ".claude/skills/broken/SKILL.md": "disable-model-invocation: true\n本文だけで frontmatter の区切りが無い\n",
    });
    expect(checkSkillTable(s)).toEqual([]);
  });
});
