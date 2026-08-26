import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkHeadingRefs, scanHeadingRefs } from "./G-heading-refs.mjs";
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

  // 見出し名が鉤括弧を入れ子に含む形（#1188）。旧実装はラベルの文字クラスが `[^「」\n]` で、
  // **一致そのものが生成されなかった**——findings にも checked にも現れないので
  // 「照合していない」と「差分ゼロ」が区別できない。ゆえに緑だけでは接地せず、
  // 赤の fixture が「一致が生成されるようになった」ことを証明する側になる。
  const NESTED = "## 第一原則: コメントは「なぜ」を書く\n\n本文\n";
  const NESTED_REF = { "docs/y.md": "詳細は `docs/c.md`「第一原則: コメントは「なぜ」を書く」を見よ\n" };

  it("見出し名が鉤括弧を入れ子に含んでも全形の参照が照合され着地する（#1188）", () => {
    const s = snap({ ...NESTED_REF, "docs/c.md": NESTED });
    expect(scanHeadingRefs(s, ["docs/y.md"])).toEqual({ findings: [], checked: 1 });
  });

  it("入れ子を含む見出しを改名すると finding（赤・フォールトインジェクション）", () => {
    const s = snap({ ...NESTED_REF, "docs/c.md": NESTED.replace("「なぜ」", "理由") });
    const f = checkHeadingRefs(s, ["docs/y.md"]);
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("見出し参照が着地しない");
  });

  // 宣言する死角の固定（正本は `lib.mjs` の `HEADING_REF` の doc）。
  // **指し先を着地しない見出しにしてある**——一致が生成されれば必ず赤になるので、
  // `checked: 0` が「見なかった」ことを一意に表す（findings 0 件だけでは区別が付かない）。
  it("深さ 2 以上の入れ子は一致を生成しない（宣言する死角）", () => {
    const s = snap({ "docs/y.md": "`docs/c.md`「A「B「C」D」E」\n", "docs/c.md": "## 別の見出し\n" });
    expect(scanHeadingRefs(s, ["docs/y.md"])).toEqual({ findings: [], checked: 0 });
  });

  it("他の参照のラベルの内側に置かれた正準形参照は独立には照合されない（宣言する死角）", () => {
    const s = snap({
      "docs/y.md": "`docs/c.md`「外側 `docs/inner.md`「内側」だ」\n",
      "docs/c.md": "## 外側 `docs/inner.md`「内側」だ\n",
      "docs/inner.md": "## 別の見出し\n",
    });
    // 外側 1 件だけが照合される。内側の参照は着地しないが赤にならない
    // （この行に正準形を書かないこと——テストのコメントも照合母集団である・#1155）
    expect(scanHeadingRefs(s, ["docs/y.md"])).toEqual({ findings: [], checked: 1 });
  });

  // **この変更が作った唯一の観測性の後退**（正本は `lib.mjs` の doc の死角 5）。
  // 外側の対象綴りが無効だと、内側の正当な参照ごと沈黙する——旧実装では内側が独立に照合され、
  // 着地しなければ赤になっていた。**`checked: 0` なので evidence 行からも読めない。**
  it("外側の対象綴りが無効だと内側の正当な参照ごと沈黙する（宣言する死角）", () => {
    const s = snap({
      "docs/y.md": "`HEADING_REF`「外側の語 `docs/inner.md`「消えた見出し」の話」\n",
      "docs/inner.md": "## 別の見出し\n",
    });
    expect(scanHeadingRefs(s, ["docs/y.md"])).toEqual({ findings: [], checked: 0 });
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
