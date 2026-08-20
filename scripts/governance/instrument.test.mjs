import { describe, it, expect } from "vitest";
import { snap } from "./test-helpers.mjs";
import { checkNormativeAreaInstrument, normativeArea, ALWAYS_LOADED_FILES } from "./instrument.mjs";

describe("G-area-instrument checkNormativeAreaInstrument（合否を持たない計器・母集団だけを判定・ADR-retire-area-budget）", () => {
  const x = (n) => "x".repeat(n);
  const rule = (p, n) => ({ [`.claude/rules/${p}`]: x(n) });
  const skill = (name, desc) => ({
    [`.claude/skills/${name}/SKILL.md`]: `---\nname: ${name}\ndescription: "${desc}"\n---\n本文\n`,
  });
  const base = { ...rule("a.md", 1), ...skill("s", "d") };

  it("母集団が揃っていれば findings 無し（緑）", () => {
    const s = snap({ "CLAUDE.md": x(100), "AGENTS.md": x(100), ...base });
    expect(checkNormativeAreaInstrument(s)).toEqual([]);
  });

  // 守りたい対象 = 「面積の大小はもう合否を持たない」こと（ADR-retire-area-budget）。
  // 旧 G-area-budget が赤にした 2 形（常時ロード超過・面替えでの rules 超過）を**そのまま**当て、
  // 緑であることを実測する。上限判定が戻れば（定数を復活させて比較を足せば）この 2 本が落ちる。
  it("常時ロード面がいくら大きくても finding を出さない（旧・火災報知器の廃止）", () => {
    const s = snap({ "CLAUDE.md": x(1_000_000), "AGENTS.md": "", ...base });
    expect(checkNormativeAreaInstrument(s)).toEqual([]);
  });

  it("rules 面がいくら大きくても finding を出さない（面替えにも鳴らない）", () => {
    const s = snap({ "CLAUDE.md": x(10), "AGENTS.md": x(10), ...skill("s", "d"), ...rule("a.md", 1_000_000) });
    expect(checkNormativeAreaInstrument(s)).toEqual([]);
  });

  it("改行を畳んでも面積は改行のぶんしか下がらない（行数指標の誤った勾配を絶つ・ADR-area-metric-characters の核心）", () => {
    const areaOf = (t) => normativeArea(snap({ "CLAUDE.md": t, "AGENTS.md": "", ...base })).always;
    const spread = "あ\n".repeat(100); // 100 行 200 字
    const folded = "あ".repeat(100); // 1 行 100 字（内容は 1 字も減っていない）
    // 行数指標なら 100 → 1 で 99% の「削減」に見えた。文字数指標では改行 100 字ぶんだけ
    expect(areaOf(spread) - areaOf(folded)).toBe(100);
  });

  it("CR は数えない（CRLF checkout で面積が膨らむ沈黙経路の閉塞）", () => {
    const lf = snap({ "CLAUDE.md": "あ\n".repeat(10), "AGENTS.md": "", ...base });
    const crlf = snap({ "CLAUDE.md": "あ\r\n".repeat(10), "AGENTS.md": "", ...base });
    expect(normativeArea(lf).always).toBe(normativeArea(crlf).always);
  });

  it("skill description は常時ロード面に算入される（表→description の面替えを塞ぐ）", () => {
    const withShort = snap({ "CLAUDE.md": "", "AGENTS.md": "", ...rule("a.md", 1), ...skill("s", "d") });
    const withLong = snap({ "CLAUDE.md": "", "AGENTS.md": "", ...rule("a.md", 1), ...skill("s", "d".repeat(50)) });
    expect(normativeArea(withLong).always - normativeArea(withShort).always).toBe(49);
  });

  it("disable-model-invocation の skill の description は算入されない（注入されない字に課税しない）", () => {
    const hiddenSkill = (name, desc) => ({
      [`.claude/skills/${name}/SKILL.md`]: `---\nname: ${name}\ndescription: "${desc}"\ndisable-model-invocation: true\n---\n本文\n`,
    });
    const shortDesc = snap({ "CLAUDE.md": "", "AGENTS.md": "", ...rule("a.md", 1), ...hiddenSkill("h", "d") });
    const longDesc = snap({ "CLAUDE.md": "", "AGENTS.md": "", ...rule("a.md", 1), ...hiddenSkill("h", "d".repeat(50)) });
    expect(normativeArea(longDesc).always).toBe(normativeArea(shortDesc).always);
    // それでも母集団としては数える（skills 0 件の誤検知を出さない）
    expect(checkNormativeAreaInstrument(shortDesc).some((v) => v.file === ".claude/skills")).toBe(false);
  });

  it("description が 1 行スカラーでなければ finding（数えられない沈黙経路の閉塞）", () => {
    const s = snap({
      "CLAUDE.md": "",
      "AGENTS.md": "",
      ...rule("a.md", 1),
      ".claude/skills/s/SKILL.md": "---\nname: s\ndescription: |\n  複数行\n---\n",
    });
    const f = checkNormativeAreaInstrument(s);
    expect(f.some((v) => v.message.includes("1 行スカラーでない"))).toBe(true);
  });

  it("常時ロード文書が読めなければ母集団欠落 finding（沈黙経路の閉塞）", () => {
    const s = snap({ "AGENTS.md": x(1), ...base }); // CLAUDE.md 欠落
    const f = checkNormativeAreaInstrument(s);
    expect(f.some((v) => v.file === "CLAUDE.md" && v.message.includes("母集団の欠落"))).toBe(true);
  });

  it("rules / skills が 0 件なら母集団欠落 finding（グロブ破損の沈黙経路の閉塞）", () => {
    const noRules = snap({ "CLAUDE.md": x(1), "AGENTS.md": x(1), ...skill("s", "d") });
    expect(checkNormativeAreaInstrument(noRules).some((v) => v.file === ".claude/rules")).toBe(true);
    const noSkills = snap({ "CLAUDE.md": x(1), "AGENTS.md": x(1), ...rule("a.md", 1) });
    expect(checkNormativeAreaInstrument(noSkills).some((v) => v.file === ".claude/skills")).toBe(true);
  });

  it("ALWAYS_LOADED_FILES はルート直下の 2 文書", () => {
    expect(ALWAYS_LOADED_FILES).toEqual(["CLAUDE.md", "AGENTS.md"]);
  });
});
