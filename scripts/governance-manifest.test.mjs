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
  // このテストは「今日、`checks/` の実ファイルを 1 本消したら何が起きるか」の再現ではない。
  // facade は各検査を `import { checkFoo } from "./governance/checks/G-foo.mjs"` の形で静的に
  // 名指し re-export しているため、ファイルが物理的に無くなれば `buildChecks`／`manifest()` に
  // 到達する前、import 解決の時点で `ERR_MODULE_NOT_FOUND` が飛んで facade ごと落ちる——それが
  // 今日の一次防御線であり、この diff より先に、より大きな音で発火する（実測: 独立コピーで
  // facade を import → 削除前 `imported OK` / 削除後 `ERR_MODULE_NOT_FOUND`）。
  // このテストが変異させるのは `manifest()` の**返り値の複製**であり、import 経路を経由しない。
  // 効いてくるのは facade が検査ごとの静的 re-export をやめた後——そのとき初めて、ファイル消失は
  // import エラーを起こさず manifest の集合からだけ静かに欠けるようになり、この diff が
  // 「消失を検知する側」に回る。今のうちに書くのは、その切り替わりに備えるためである。
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
