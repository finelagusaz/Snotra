import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkHeadingRefs } from "./G-heading-refs.mjs";
import { collectAnchors, headingRefDocs } from "../lib.mjs";

describe("G-heading-refs checkHeadingRefs（見出し参照の実在）", () => {
  // 守りたい対象 = 「参照先の見出しが改名・消滅したのに参照側が残る」ドリフト。
  // 同一フィクスチャの複製に変異を当てて赤を実測する（ライブの検査は弱めない）。
  const TARGET = "## Git/GitHub 運用\n\n本文\n";
  const REF = { "docs/x.md": "詳細は `CLAUDE.md`「Git/GitHub 運用」を見よ\n" };

  it("参照先に見出しがあれば findings 無し（緑）", () => {
    const s = snap({ ...REF, "CLAUDE.md": TARGET });
    expect(checkHeadingRefs(s, ["docs/x.md"])).toEqual([]);
  });

  it("見出しを改名すると finding（赤・フォールトインジェクション）", () => {
    const s = snap({ ...REF, "CLAUDE.md": TARGET.replace("Git/GitHub 運用", "Git 運用") });
    const f = checkHeadingRefs(s, ["docs/x.md"]);
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("見出し参照が着地しない");
  });

  it("太字リード・番号付き項目もアンカーになる（この repo の参照実態）", () => {
    const anchors = collectAnchors(
      "## 節\n- **イベント駆動 wake の不変条件（#532 SU5）**: 本文\n0. 「バグ」か「仕様変更」かを判定する\n",
    );
    expect(anchors).toContain("節");
    expect(anchors).toContain("イベント駆動 wake の不変条件（#532 SU5）");
    expect(anchors).toContain("「バグ」か「仕様変更」かを判定する");
  });

  it("後置注記があっても前方一致で着地し、「」の有無は正規化で吸収する", () => {
    const s = snap({
      "docs/x.md":
        "`m/CLAUDE.md`「イベント駆動 wake の不変条件」と `AGENTS.md`「バグか仕様変更かを判定する」\n",
      "m/CLAUDE.md": "- **イベント駆動 wake の不変条件（#532 SU5）**: 本文\n",
      "AGENTS.md": "0. 「バグ」か「仕様変更」かを判定する\n",
    });
    expect(checkHeadingRefs(s, ["docs/x.md"])).toEqual([]);
  });

  it("`/skill-name` は .claude/skills/<name>/SKILL.md へ解決する", () => {
    const s = snap({
      "docs/x.md": "`/plan-review`「Step 2b」\n",
      ".claude/skills/plan-review/SKILL.md": "## Step 2b — 独立導出 + 差分\n",
    });
    expect(checkHeadingRefs(s, ["docs/x.md"])).toEqual([]);
  });

  it("解決できない対象は finding（沈黙経路の閉塞）", () => {
    const s = snap({ "docs/x.md": "`docs/gone.md`「節」\n" });
    const f = checkHeadingRefs(s, ["docs/x.md"]);
    expect(f[0].message).toContain("見出し参照の対象が解決できない");
  });

  it("散文形の参照とコードフェンス内は見ない（受容する偽陰性の固定）", () => {
    const s = snap({
      "docs/x.md": "ルート `CLAUDE.md` のフック節\n```\n`CLAUDE.md`「無い見出し」\n```\n",
      "CLAUDE.md": "## 何か\n",
    });
    expect(checkHeadingRefs(s, ["docs/x.md"])).toEqual([]);
  });

  it("md の腕の母集団は履歴資料・作業バッファ・凍結された歴史（docs/adr/）を除く全 md", () => {
    const s = snap({
      "PERFORMANCE.md": "",
      ".claude/agents/code-reviewer.md": "",
      "docs/superpowers/plans/p.md": "",
      "workspace/plan.md": "",
      "docs/adr/ADR-x.md": "",
      // 判定対象外の不混入（md 側）。`src/main.rs` は #925 以降 `headingRefSourceDocs` が
      // 拾う側へ移ったため、md の腕の負のカナリアは非 md の別拡張子で張り直してある
      "Cargo.toml": "",
      "src/main.rs": "",
    });
    expect(headingRefDocs(s).sort()).toEqual([".claude/agents/code-reviewer.md", "PERFORMANCE.md"]);
  });
});
