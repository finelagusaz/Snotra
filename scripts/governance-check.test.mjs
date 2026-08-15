// governance-check.mjs の検査関数を、フォールトインジェクションフィクスチャ（赤）と正常フィクスチャ（緑）の
// 両方向で検証する。各フィクスチャは「守りたい対象 1 件が入力に現れること」と
// 「判定対象外が入力に混じらないこと」の入力集合検算を兼ねる（.claude/rules/safety-nets.md）。
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it, expect } from "vitest";
import { snap } from "./governance/test-helpers.mjs";
import {
  MODULE_INDEX_CRATES,
  governanceDocs,
  makeSnapshot,
  gitIgnoredPaths,
  checkReferences,
  checkNormativeAreaInstrument,
  normativeArea,
  ALWAYS_LOADED_FILES,
  checkHeadingRefs,
  scanHeadingRefs,
  collectAnchors,
  headingRefDocs,
  headingRefSourceDocs,
  checkNearHeadingRefs,
  scanNearHeadingRefs,
  checkCheckSkillEnumeration,
  checkStaleIdentifiers,
  scanStaleIdentifiers,
  staleIdentifierDocs,
  staleIdentifierGuideDocs,
  staleIdentifierTargets,
  currentVocabulary,
  runAll,
  buildChecks,
  checkAdrFileNames,
  adrFiles,
  checkAdrCitations,
  scanAdrCitations,
  adrCitationDocs,
} from "./governance-check.mjs";

// G-module-index/G-references の母集団は手で列挙する定数であり、**crate を足しても何も鳴らない**（沈黙する経路）。
// `snotra-egui-runtime` は「#532 の検証層」として作られたまま両方から漏れ、SU7 で製品の
// 描画層になった後も索引ドリフト・参照切れが検知されない状態が続いていた（#701）。
// 実 `Cargo.toml` を読み、CLAUDE.md を持つ member が両母集団に載っていることを固定する。
describe("G-module-index/G-references 母集団カナリア — #701", () => {
  it("CLAUDE.md を持つ workspace member は MODULE_INDEX_CRATES と governanceDocs の両方に載る", () => {
    const root = fileURLToPath(new URL("..", import.meta.url));
    const src = readFileSync(fileURLToPath(new URL("../Cargo.toml", import.meta.url)), "utf8");

    // 書式が変わったら「読めなかった」と落ちる（fail-closed・post-edit.test.mjs の members カナリアと同型）
    const section = src.match(/\[workspace\]\r?\n([\s\S]*?)(?=\r?\n\[|$)/);
    expect(section, "Cargo.toml の [workspace] セクションを読めなかった").not.toBeNull();
    const m = section[1].match(/^members\s*=\s*\[([^\]]*)\]/m);
    expect(m, "[workspace] の members 行を読めなかった（書式が変わった）").not.toBeNull();
    const members = m[1]
      .split(",")
      .map((s) => s.trim().replace(/^"|"$/g, ""))
      .filter((s) => s.length > 0);

    const docs = governanceDocs(makeSnapshot(root));
    const withClaudeMd = members.filter((c) => existsSync(fileURLToPath(new URL(`../${c}/CLAUDE.md`, import.meta.url))));
    // 母集団が空になる形（members 読み違い・全 crate が CLAUDE.md 無し）を合格に見せない
    expect(withClaudeMd.length, "CLAUDE.md を持つ member が 0 件（母集団の欠落）").toBeGreaterThan(0);

    for (const crate of withClaudeMd) {
      expect(
        Object.keys(MODULE_INDEX_CRATES),
        `${crate} が MODULE_INDEX_CRATES に無い。src/exts を添えて追加すること（索引ドリフトが沈黙で通る）`,
      ).toContain(crate);
      expect(
        docs,
        `${crate}/CLAUDE.md が G-references 母集団に無い。governanceDocs の正規表現を更新すること（参照切れが沈黙で通る）`,
      ).toContain(`${crate}/CLAUDE.md`);
    }
  });
});

describe("runAll（空母集団の明示 fail = 沈黙経路の閉塞）", () => {
  it("対象文書・rules・skills が空なら findings を返す", () => {
    const s = snap({});
    const { findings } = runAll(s);
    expect(findings.length).toBeGreaterThan(0);
  });
  it("計器（G-area-instrument）は検査配列に無い——面積に合否は無い（ADR-retire-area-budget）", () => {
    const ids = buildChecks(snap({}), {}).map((c) => c.id);
    expect(ids).not.toContain("G-area-instrument");
  });
  it("それでも計器の母集団欠落は runAll の findings に残る（検査配列の外でも沈黙しない）", () => {
    const { findings } = runAll(snap({}));
    expect(findings.some((f) => f.message.includes("G-area-instrument 母集団の欠落"))).toBe(true);
  });
});

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

describe("検査 ID の形（#812 — 序数を引用の語彙から外す）", () => {
  const ids = buildChecks(snap({}), {}).map((c) => c.id);

  it("すべて `G-<kebab>` 形で、数字を含まない", () => {
    for (const id of ids) expect(id, `${id} が G-<name> 形でない`).toMatch(/^G-[a-z][a-z0-9]*(-[a-z0-9]+)*$/);
  });

  it("重複しない（連番なら並行 PR が同じ値を見る形が、名前では起きないことの固定）", () => {
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("サマリの件数は登録表から計算される（範囲の手書きが存在しない）", () => {
    // 「G1..G15 passed」のような範囲表記は、検査を足しても黙って古くなる（#812 実測）
    const { evidence } = runAll(makeSnapshot(fileURLToPath(new URL("..", import.meta.url))));
    expect(evidence).toContain(`検査 ${ids.length} 件`);
  });
});

describe("実リポジトリ スモーク（dogfood）", () => {
  it("現在のリポジトリで全検査が緑", async () => {
    const { makeSnapshot } = await import("./governance-check.mjs");
    const { fileURLToPath } = await import("node:url");
    const s = makeSnapshot(fileURLToPath(new URL("..", import.meta.url)));
    const { findings } = runAll(s);
    expect(findings).toEqual([]);
  });
});

describe("facade の公開面（export { … } の凍結）", () => {
  // export { … } は手書きの一覧であり、この per-check 分割で検査を 1 本 checks/ へ移すたびに
  // 書き足す唯一の面である。書き忘れは npm test にも governance:check にも現れるとは限らない——
  // テストファイルが直接 import していない名前が消えても、どちらのコマンドも検知しない。
  // 公開面を丸ごと凍結することで、この一覧への変更は気づかず起きることではなく、
  // 意図して行う編集になる。
  it("公開する名前の集合が凍結した一覧と一致する", async () => {
    const mod = await import("./governance-check.mjs");
    expect(Object.keys(mod).sort()).toEqual([
      "ALWAYS_LOADED_FILES",
      "MODULE_INDEX_CRATES",
      "REQUIRED_DISALLOWED_METHODS",
      "REQUIRED_RUSTDOC_LINTS",
      "STALE_EXTRA_DOCS",
      "adrCitationDocs",
      "adrFiles",
      "buildChecks",
      "checkAdrCitations",
      "checkAdrFileNames",
      "checkArchitectureTable",
      "checkBuildCommands",
      "checkCheckSkillEnumeration",
      "checkCiTable",
      "checkClippyDisallowed",
      "checkHeadingRefs",
      "checkHookCommands",
      "checkHookFires",
      "checkModuleIndex",
      "checkModuleLinkage",
      "checkNearHeadingRefs",
      "checkNormativeAreaInstrument",
      "checkReferences",
      "checkRulesGlobs",
      "checkSkillTable",
      "checkSpecSections",
      "checkStaleIdentifiers",
      "checkWorkspaceLints",
      "clippyDisallowedCount",
      "clippyMethodsDenied",
      "collectAnchors",
      "currentVocabulary",
      "declaredModuleFiles",
      "declaresEguiDependency",
      "disallowedMethodPaths",
      "gitIgnoredPaths",
      "globToRegex",
      "governanceDocs",
      "hasWorkspaceLintsOptIn",
      "headingRefDocs",
      "headingRefSourceDocs",
      "makeSnapshot",
      "modelHiddenSkills",
      "normativeArea",
      "resolveRefTarget",
      "runAll",
      "rustdocLintsAreDenied",
      "scanAdrCitations",
      "scanHeadingRefs",
      "scanNearHeadingRefs",
      "scanStaleIdentifiers",
      "skillDescriptionArea",
      "staleIdentifierDocs",
      "staleIdentifierGuideDocs",
      "staleIdentifierTargets",
      "workspaceMembers",
    ]);
  });
});
