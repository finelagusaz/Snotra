import { describe, it, expect } from "vitest";
import { makeSnapshot } from "./governance-check.mjs";
import { manifest, diffManifest, undeclared } from "./governance-manifest.mjs";

describe("manifest（構造母集団の集合）", () => {
  it("実リポジトリで 4 列すべてが非空", () => {
    const m = manifest(makeSnapshot(process.cwd()));
    for (const key of ["checks", "docs", "rules", "skills"]) {
      expect(m[key].length, `${key} が空（母集団の欠落）`).toBeGreaterThan(0);
    }
  });
  it("各列は sorted（readdir 順の揺れが差分に化けない）", () => {
    const m = manifest(makeSnapshot(process.cwd()));
    for (const key of ["checks", "docs", "rules", "skills"]) {
      expect(m[key], `${key} が sorted でない`).toEqual([...m[key]].sort());
    }
  });
  it("検査 ID を含む", () => {
    expect(manifest(makeSnapshot(process.cwd())).checks).toContain("G-references");
  });
});

describe("diffManifest（件数ではなく集合を比べる）", () => {
  const base = { checks: ["G-a", "G-b"], docs: [], rules: [], skills: [] };
  it("同一なら空", () => {
    expect(diffManifest(base, base)).toEqual([]);
  });
  it("追加と削除の両方を出す", () => {
    const head = { checks: ["G-a", "G-c"], docs: [], rules: [], skills: [] };
    expect(diffManifest(base, head).sort()).toEqual(["+G-c", "-G-b"]);
  });
  it("1 消して 1 足す入れ替えを沈黙させない（件数では捕まらない形）", () => {
    const head = { checks: ["G-a", "G-z"], docs: [], rules: [], skills: [] };
    expect(diffManifest(base, head).length).toBe(2);
  });
  it("同じ path が docs と rules の両方から消えても 1 件（列をまたいだ重複を畳む）", () => {
    const overlapBase = {
      checks: [],
      docs: [".claude/rules/foo.md"],
      rules: [".claude/rules/foo.md"],
      skills: [],
    };
    const overlapHead = { checks: [], docs: [], rules: [], skills: [] };
    expect(diffManifest(overlapBase, overlapHead)).toEqual(["-.claude/rules/foo.md"]);
  });
});

describe("undeclared（PR 本文に逐語で現れない delta を返す）", () => {
  it("すべて宣言されていれば空", () => {
    const body = "## governance manifest delta\n- checks: +G-c, -G-b\n";
    expect(undeclared(["+G-c", "-G-b"], body)).toEqual([]);
  });
  it("宣言が無ければ全件返る（宣言なし PR で diff が在れば赤）", () => {
    expect(undeclared(["+G-c"], "ふつうの PR 本文")).toEqual(["+G-c"]);
  });
  it("diff が空なら宣言が無くても空（既定の経路を赤にしない）", () => {
    expect(undeclared([], "ふつうの PR 本文")).toEqual([]);
  });
  it("本文が null でも落ちない", () => {
    expect(undeclared(["+G-c"], null)).toEqual(["+G-c"]);
  });
});

describe("フォールトインジェクション — 検査 ID が manifest の集合から消えたときに diffManifest／undeclared が発火するかの実測（#1088）", () => {
  // **この diff は「消失を検知する側」に回っている**（#1094 で facade が検査ごとの静的
  // re-export をやめた）。かつては facade が 19 本すべてを名指し import していたため、ファイルが
  // 物理的に無くなれば `buildChecks`／`manifest()` へ到達する前に `ERR_MODULE_NOT_FOUND` が飛び、
  // この diff は発火の機会を持たなかった。
  //
  // **ただし「検査ファイルが消えれば manifest 差分が捕まえる」と全称では言えない。** 言えるのは
  // 次の下限までである（すべて #1094 で使い捨て worktree に実測）。
  //
  // - **消え方で捕まえる層が違う。** `G-X.mjs` **だけ**が消えて `G-X.test.mjs` が残る形は、隣の
  //   テストが `import { checkX } from "./G-X.mjs"` を持つため `npm test` が落ちる（19/19 のテストが
  //   この形で、`vitest.config.ts` の `include` が `scripts/**/*.test.mjs` を含む）。**この層は
  //   facade と無関係であり、絞る前も後も変わらない。** manifest 差分が唯一の検知器になるのは
  //   `.mjs` と `.test.mjs` が**ペアで**消えたとき——検査を 1 本やめる実際の操作がその形である。
  // - **全 19 本ではない。** facade は evidence の算出のため `G-clippy-disallowed`
  //   （`clippyDisallowedCount`）と `G-adr-file-names`（`adrFiles`）を今も名指し import しており
  //   （`governance-check.mjs` の当該 import のコメントが意図の正本）、`governance/instrument.mjs` は
  //   計器の算出のため `G-skill-table` を import している。この 3 本はペア消失でも `ERR_MODULE_NOT_FOUND`
  //   で落ちる。切り替わったのは残りである。
  // - **検知の性質も変わった。** 旧: `governance check` step の import エラー——push でも
  //   pull_request でも赤く、宣言では回避できない。新: `governance manifest delta` step——
  //   `ci.yml` の `if` により **PR でしか走らず**、差分を PR 本文へ逐語で書けば通る（`undeclared`）。
  //   「不可能にする」から「意図的だと宣言させる」への移行であり、これは #1088 の設計意図そのもの
  //   だが、**守りが一様に増えたわけではない**。
  //
  // このテストが変異させるのは `manifest()` の**返り値の複製**であり、import 経路を経由しない
  // （稼働中の `checks/` へ変異を当てないため・`.claude/rules/safety-nets.md`）。
  it("checks/ から 1 本消えた形は差分として現れる", () => {
    const base = manifest(makeSnapshot(process.cwd()));
    // 稼働中の checks/ は触らない——返り値の複製に変異を当てる
    const mutated = { ...base, checks: base.checks.filter((id) => id !== "G-ci-table") };
    expect(diffManifest(base, mutated)).toEqual(["-G-ci-table"]);
    expect(undeclared(diffManifest(base, mutated), "宣言のない PR 本文")).toEqual(["-G-ci-table"]);
  });
  it("走査の母集団が黙って縮んでも発火する（WALK_EXCLUDE_PATHS へ 1 行足した形）", () => {
    const base = manifest(makeSnapshot(process.cwd()));
    const mutated = { ...base, rules: base.rules.slice(1) };
    expect(diffManifest(base, mutated)).toEqual([`-${base.rules[0]}`]);
  });
  it("skills 列の母集団が黙って縮んでも発火する（SKILL.md が 1 本消えた形）", () => {
    const base = manifest(makeSnapshot(process.cwd()));
    const mutated = { ...base, skills: base.skills.slice(1) };
    expect(diffManifest(base, mutated)).toEqual([`-${base.skills[0]}`]);
  });
  it("変異が無ければ発火しない（常に赤いゲートはゲートが無いのと同じ）", () => {
    const base = manifest(makeSnapshot(process.cwd()));
    expect(diffManifest(base, manifest(makeSnapshot(process.cwd())))).toEqual([]);
  });
});
