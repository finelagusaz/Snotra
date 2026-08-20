import { describe, it, expect } from "vitest";
import { makeSnapshot } from "./governance-check.mjs";
import { manifest, diffManifest, undeclared, KEYS } from "./governance-manifest.mjs";

describe("manifest（構造母集団の集合）", () => {
  // 列の一覧は `KEYS` を読む——ここへ列名を書き写すと、列を足したときにその列だけが
  // 検算から漏れる（新しい列が無検査で入る形。I3 の `domains` 追加で実際に踏みかけた）。
  it("実リポジトリで全列が非空", () => {
    const m = manifest(makeSnapshot(process.cwd()));
    for (const key of KEYS) {
      expect(m[key].length, `${key} が空（母集団の欠落）`).toBeGreaterThan(0);
    }
  });
  it("各列は sorted（readdir 順の揺れが差分に化けない）", () => {
    const m = manifest(makeSnapshot(process.cwd()));
    for (const key of KEYS) {
      expect(m[key], `${key} が sorted でない`).toEqual([...m[key]].sort());
    }
  });
  it("検査 ID を含む", () => {
    expect(manifest(makeSnapshot(process.cwd())).checks).toContain("G-references");
  });
  // `DOMAIN_SPECS` と突き合わせない——manifest がそこから導出している以上、突き合わせは
  // 不動点になり「両辺が同時に減る」当の欠陥を再現する。独立に書いたドメイン名で接地する。
  it("ドメイン名を含む（I3 — ドメインの一覧が manifest の列である）", () => {
    expect(manifest(makeSnapshot(process.cwd())).domains).toContain("governanceDocs");
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
  // I3 の当の欠陥——ドメインが 1 つ丸ごと消える。**消費者を持つドメインは `registry.mjs` が
  // ロード時に throw する**（検査の `domains` 宣言が未知の名前になるため）が、**消費者の無い
  // ドメインはどの層も見なかった**——実測（2026-08-20）: `headingRefDocs` を `DOMAIN_SPECS` から
  // 消すと `governance:check` も `npm test` も緑のままで、この列だけが `-headingRefDocs` を出した。
  // **この列は格下げ中である**（`ADR-governance-meta-demotion`）。錨の層をゲートから外した以上、
  // その帳簿だけをゲートに残すのは筋が通らない——止めたのは「錨が守っている保証」であって、
  // 帳簿はその保証の付属物である。
  //
  // **格下げで実際に見えなくなるものを名指しする**（上のコメントの実測がそのまま残余になる）:
  // 消費者を持たないドメインを `DOMAIN_SPECS` から消しても、既定モードではどの層も鳴らない。
  // 消費者を持つドメインは `registry.mjs` が未知の名前で throw するので、ここは変わらない。
  const withAudit = (v, fn) => {
    const prev = process.env.SNOTRA_GOV_META_AUDIT;
    if (v === undefined) delete process.env.SNOTRA_GOV_META_AUDIT;
    else process.env.SNOTRA_GOV_META_AUDIT = v;
    try {
      return fn();
    } finally {
      if (prev === undefined) delete process.env.SNOTRA_GOV_META_AUDIT;
      else process.env.SNOTRA_GOV_META_AUDIT = prev;
    }
  };
  const b = { checks: [], docs: [], rules: [], skills: [], domains: ["governanceDocs", "adrFiles"] };
  const h = { checks: [], docs: [], rules: [], skills: [], domains: ["governanceDocs"] };

  it("既定ではドメインの消滅は delta に出ない（格下げ中）", () => {
    expect(withAudit(undefined, () => diffManifest(b, h))).toEqual([]);
  });

  it("監査モードではドメインが 1 つ消えると delta に出る（I3・戻す経路が実在する）", () => {
    expect(withAudit("1", () => diffManifest(b, h))).toEqual(["-adrFiles"]);
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
  // re-export をやめた）。かつては facade が `checks/` の全モジュールを名指し import していたため、
  // ファイルが物理的に無くなれば `buildChecks`／`manifest()` へ到達する前に `ERR_MODULE_NOT_FOUND`
  // が飛び、この diff は発火の機会を持たなかった。
  //
  // **ただし「検査ファイルが消えれば manifest 差分が捕まえる」と全称では言えない。** 言えるのは
  // 次の下限までである（下 3 つの層はいずれも #1094 で使い捨て worktree に故障注入して実測した。
  // 最後の「検知の性質」だけは実測ではなく `ci.yml` の読みである）。
  //
  // - **消え方で捕まえる層が違う。** `G-X.mjs` **だけ**が消えて `G-X.test.mjs` が残る形は、隣の
  //   テストが `import { checkX } from "./G-X.mjs"` を持つため `npm test` が落ちる（19/19 のテストが
  //   この形で、`vitest.config.ts` の `include` が `scripts/**/*.test.mjs` を含む）。**この層は
  //   facade と無関係であり、絞る前も後も変わらない。** manifest 差分が唯一の検知器になるのは
  //   `.mjs` と `.test.mjs` が**ペアで**消えたとき——検査を 1 本やめる実際の操作がその形である。
  // - **全 19 本ではない。** `checks/` の**外**から静的 import されている検査は、ペア消失でも
  //   import エラーで落ちる。**数を書かない**——`checks/` を触るたびに腐るので、母集団は次の grep が持つ:
  //     grep -rn 'from ".*checks/G-' --include=*.mjs .
  //   **この母集団には穴がある**——`checks/` の中どうしの import はこの形に当たらない
  //   （`from "./G-X.mjs"` と書かれ、パス断片 `checks/` を含まないため）。隣のテストが持つ
  //   sibling import が落ちるのは意図どおり（上の層の話）だが、**同じ理由で非兄弟の cross-import も
  //   落ちる**。今日は 0 件だが、`checks/` の中で他の検査を import したら、それはこの母集団の外に居る。
  //   少なくとも 3 経路がこれを作る——facade が evidence のため名指しする分（意図の正本は
  //   `governance-check.mjs` の当該 import のコメント）、`instrument.mjs` が計器のため名指しする分、
  //   そして **`checks/` の外に在るテスト**（`governance/lib.test.mjs` / `governance-check.test.mjs`）が
  //   名指しする分である。**3 つ目は見落としやすい**——「テストの import は隣のものだけ」という前提で
  //   走査から除くと母集団から丸ごと落ちる（#1094 の調査が実際にそう誤った）。
  // - **さらに、生きた散文が名指す検査は `governance:check` 自身が拾うことがある。** 検査の識別子や
  //   パスを規範文書が引いていれば、消したときに G-stale-identifiers や G-references が鳴る。
  //   **恒常的な検知器として当てにはできない**（散文を直せば消える）が、「manifest 差分が唯一」を
  //   無条件では言えない理由の 3 つ目である。
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
