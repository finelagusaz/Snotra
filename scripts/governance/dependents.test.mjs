import { describe, it, expect } from "vitest";
import { snap } from "./test-helpers.mjs";
import { buildDependentIndex, sectionsOf, changedSectionDependents, parseHunks } from "./dependents.mjs";

describe("dependents（節の中身が変わったら依存している参照を報告する・#1140）", () => {
  // 守りたい対象 = 「見出しの名前は同じまま本文が入れ替わり、参照が黙って古びる」ドリフト。
  // G-heading-refs は着地しか見ないのでこの形に鳴らない（#1140）。
  const TARGET = ["# 文書", "", "## Git/GitHub 運用", "本文 A", "", "## 別の節", "本文 B", ""].join("\n");
  const base = {
    "AGENTS.md": TARGET,
    "docs/x.md": "詳細は `AGENTS.md`「Git/GitHub 運用」を見よ\n",
    "scripts/a.mjs": "// 根拠は `AGENTS.md`「Git/GitHub 運用」\n",
  };
  const hunk = (start, len, oldLen) => ({ start, len, oldLen });

  it("節の本文を書き換えると、その節に依存する参照が出る（md とスクリプトの両方から）", () => {
    const s = snap(base);
    const got = changedSectionDependents(s, "AGENTS.md", [hunk(4, 1, 1)]);
    expect(got).toHaveLength(1);
    expect(got[0].refs.sort()).toEqual(["docs/x.md:1", "scripts/a.mjs:1"]);
  });

  it("依存を持たない節を書き換えても出ない（対照）", () => {
    const s = snap(base);
    expect(changedSectionDependents(s, "AGENTS.md", [hunk(7, 1, 1)])).toEqual([]);
  });

  it("純粋な追記（旧側 0 行）では出ない — 既存の参照を偽にしないため", () => {
    // 実測: この除外で発火が 41〜46% から 24〜25% へ半減する（research.md）
    const s = snap(base);
    expect(changedSectionDependents(s, "AGENTS.md", [hunk(4, 1, 0)])).toEqual([]);
  });

  it("参照が見出しの接頭辞を書いていても当たる（G-heading-refs と同じ前方一致）", () => {
    const s = snap({
      "AGENTS.md": "## 条件別チェック（トリガー → 参照先）\n本文\n",
      "docs/x.md": "`AGENTS.md`「条件別チェック」\n",
    });
    const got = changedSectionDependents(s, "AGENTS.md", [hunk(2, 1, 1)]);
    expect(got).toHaveLength(1);
    expect(got[0].refs).toEqual(["docs/x.md:1"]);
  });

  it("太字リード・番号付き項目もアンカーになる（この repo の参照実態）", () => {
    const s = snap({
      "src-tauri/CLAUDE.md": "## 節\n- **イベント駆動 wake の不変条件（#532 SU5）**: 本文\n続き\n",
      "docs/x.md": "`src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」\n",
    });
    const got = changedSectionDependents(s, "src-tauri/CLAUDE.md", [hunk(3, 1, 1)]);
    expect(got).toHaveLength(1);
  });

  it("フェンス内の見出しも節境界になる（受容した残余の固定・着地判定と規則を揃えるため）", () => {
    // collectAnchors はフェンスをマスクしない＝参照はフェンス内の見出しへも着地しうる。
    // 境界だけマスクすると索引と境界が食い違うので、両方マスクしない側へ揃えた（plan.md 未確定 1）。
    // ゆえにテンプレート塊が節を分断し、その直下の散文を触っても報告が出ない（沈黙側の誤り）。
    const s = snap({
      "SKILL.md": ["## 本物の節", "本文", "```markdown", "## テンプレの見出し", "```", "分断された散文", ""].join("\n"),
      "docs/x.md": "`SKILL.md`「本物の節」\n",
    });
    expect(changedSectionDependents(s, "SKILL.md", [hunk(2, 1, 1)])).toHaveLength(1);
    expect(changedSectionDependents(s, "SKILL.md", [hunk(6, 1, 1)])).toEqual([]);
  });

  it("節は入れ子になる — `## 節` は中の箇条書きを含んだまま伸びる（実物で出た欠陥の回帰）", () => {
    // 深さを与えないと「次のアンカーまで」で切れ、`## フック` が最初の太字リード箇条書きの
    // 手前で終わる。実測 2026-08-19: `CLAUDE.md`「フック」を指す参照が在るのに、
    // その節の中の箇条書きを編集しても報告が出なかった
    const s = snap({
      "CLAUDE.md": ["## フック", "前置き", "- **沈黙が合格なのは**: 本文", "- **別の項目**: 本文", "", "## 次の節", "x", ""].join("\n"),
      "docs/x.md": "`CLAUDE.md`「フック」\n",
    });
    // 箇条書きの行（3 行目）を触っても「フック」の依存が出る
    const got = changedSectionDependents(s, "CLAUDE.md", [hunk(3, 1, 1)]);
    expect(got.some((g) => g.labels.includes("フック"))).toBe(true);
    // 次の節を触っても出ない（伸びすぎていないこと）
    expect(changedSectionDependents(s, "CLAUDE.md", [hunk(7, 1, 1)])).toEqual([]);
  });

  it("sectionsOf は 1 行が複数のアンカーを生む場合をまとめて持つ", () => {
    const s = snap({ "a.md": "0. **太字リード** の項目\n本文\n" });
    const secs = sectionsOf(s, "a.md");
    expect(secs).toHaveLength(1);
    expect(secs[0].labels.length).toBeGreaterThan(1);
  });

  it("buildDependentIndex は 3 本の腕すべてから参照を集める", () => {
    const s = snap({
      "AGENTS.md": TARGET,
      "docs/x.md": "`AGENTS.md`「Git/GitHub 運用」\n",
      "src/a.rs": '/// `AGENTS.md`「Git/GitHub 運用」\nfn f() {}\n',
      "scripts/a.ps1": "# `AGENTS.md`「Git/GitHub 運用」\n",
    });
    const index = buildDependentIndex(s);
    const refs = [...index.values()].flat();
    expect(refs.sort()).toEqual(["docs/x.md:1", "scripts/a.ps1:1", "src/a.rs:1"]);
  });

  it("parseHunks は git diff -U0 のヘッダから旧側行数と新側の範囲を取る", () => {
    const diff = ["--- a/AGENTS.md", "+++ b/AGENTS.md", "@@ -4 +4 @@", "-旧", "+新", "@@ -10,0 +11,2 @@", "+足", "+した"].join("\n");
    expect(parseHunks(diff)).toEqual([
      { start: 4, len: 1, oldLen: 1 },
      { start: 11, len: 2, oldLen: 0 },
    ]);
  });
});
