import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkNearHeadingRefs, scanNearHeadingRefs } from "./G-near-heading-refs.mjs";

describe("G-near-heading-refs checkNearHeadingRefs（正準形に見えて隣接していない見出し参照・#727）", () => {
  // 守りたい対象 = #725 で実際に書かれた `/start-issue` は「Step 6 — …」の形。
  // 人の目には正準形に見え、G-heading-refs の視界外で、参照先が改番されれば黙って壊れる。
  const TARGET = { "CLAUDE.md": "## Git/GitHub 運用\n\n本文\n" };
  const run = (prose, extra = {}) => checkNearHeadingRefs(snap({ ...TARGET, ...extra, "docs/x.md": prose }), ["docs/x.md"]);

  it("助詞が 1 つ挟まった参照は finding（赤）", () => {
    const f = run("詳細は `CLAUDE.md` の「Git/GitHub 運用」を見よ\n");
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("`CLAUDE.md`「Git/GitHub 運用」と書く");
  });

  it("隣接形は G-heading-refs の担当なので見ない（二重報告しない）", () => {
    expect(run("詳細は `CLAUDE.md`「Git/GitHub 運用」を見よ\n")).toEqual([]);
  });

  it("節番号つきの隣接形も G-heading-refs の担当（#727 実測: 番号を許さないと直せない指摘が残る）", () => {
    const s = { "SPEC.md": "### 見た目の規範\n" };
    expect(checkNearHeadingRefs(snap({ ...s, "docs/x.md": "`SPEC.md` §11「見た目の規範」\n" }), ["docs/x.md"])).toEqual([]);
  });

  it("節番号 + 助詞は finding で、直し方が節番号を落とさない", () => {
    const s = { "SPEC.md": "### 見た目の規範\n" };
    const f = checkNearHeadingRefs(snap({ ...s, "docs/x.md": "`SPEC.md` §11 の「見た目の規範」\n" }), ["docs/x.md"]);
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("`SPEC.md` §11「見た目の規範」と書く");
  });

  it("着地しない引用は見ない（散文と参照を分ける要）", () => {
    expect(run("`CLAUDE.md`（ルート）は「何を実現すべきか」を記す\n")).toEqual([]);
  });

  it("窓幅を超えて離れたものは見ない", () => {
    expect(run("`CLAUDE.md` はとても長い説明を挟んでから「Git/GitHub 運用」\n")).toEqual([]);
  });

  it("判定対象外の不混入: 参照でないバッククォート・コードフェンス内", () => {
    expect(run("`someVar` の「Git/GitHub 運用」\n")).toEqual([]);
    expect(run("```\n`CLAUDE.md` の「Git/GitHub 運用」\n```\n")).toEqual([]);
  });

  it("照合件数を返す（「差分ゼロ」と「照合していない」の区別・#497）", () => {
    const r = scanNearHeadingRefs(snap({ ...TARGET, "docs/x.md": "`CLAUDE.md` の「Git/GitHub 運用」と `CLAUDE.md` の「無い見出し」\n" }), ["docs/x.md"]);
    expect(r.checked).toBe(2);
    expect(r.findings).toHaveLength(1);
  });
});
