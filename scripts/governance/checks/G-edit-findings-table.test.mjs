import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkEditFindingsTable, scanEditFindingsTable, tableJudgments } from "./G-edit-findings-table.mjs";

// 守りたい対象 = `docs/hooks.md`「検査ではない reminder（発火一覧に現れない）」の表と、
// それを生む判定の食い違い。**この形は一度腐っている**——`selectChecks` の写しである
// 発火一覧が同じ型で腐り、その手当てが `G-hook-fires` である（#863）。#992 で reminder を
// 1 本足したとき、表と実装の対応を**誰も検算していない**ことが分かったので同じ形を置く。
//
// **表そのものを fixture に書く。** 実ツリーの表を読む形にすると、テストが実データの
// 変化で落ちるようになり、判定の正しさと現在のツリーの状態が分離できない。
const HEADER = "| 発火条件 | 出るもの | 判定 |\n|---|---|---|\n";
const HEADING_LINE = "## 検査ではない reminder（発火一覧に現れない）";
const doc = (rows) => `${HEADING_LINE}\n\n前置き\n\n${HEADER}${rows}\n\n後書き\n`;
const at = (text) => snap({ "docs/hooks.md": text });

/** 実装が持つ判定名の全体（この順序に意味は無い——照合は集合で行う） */
const ALL = [
  "checkModuleIndex",
  "checkReferences",
  "checkHeadingRefs",
  "checkNearHeadingRefs",
  "checkFoldedHeadingRefs",
  "checkFoldedCodeSpans",
  "checkStaleIdentifiers",
  "checkAdrFileNames",
  "reportFor",
];
const rowsFor = (names) => names.map((n) => `| 何かを編集した | 何か | \`${n}\` |`).join("\n");

describe("G-edit-findings-table（reminder の表 ↔ 判定の照合・#992）", () => {
  describe("緑", () => {
    it("表の判定名の集合が実装と一致していれば findings は無い", () => {
      expect(checkEditFindingsTable(at(doc(rowsFor(ALL))))).toEqual([]);
    });

    it("同じ判定が複数行に現れてよい（`checkModuleIndex` は 2 行が実際にそう）", () => {
      expect(checkEditFindingsTable(at(doc(rowsFor([...ALL, "checkModuleIndex"]))))).toEqual([]);
    });

    it("判定列に注釈が付いていても名前を取る（`reportFor`（`dependents.mjs`） の形）", () => {
      const rows = rowsFor(ALL.slice(0, -1)) + "\n| `.md` を編集した | 何か | `reportFor`（`dependents.mjs`） |";
      expect(checkEditFindingsTable(at(doc(rows)))).toEqual([]);
    });

    it("**注釈が判定名より前でも緑**（順序に依存しない）", () => {
      // 先頭 1 span だけを見ていた頃は、これだけで 2 件の偽陽性になった（2026-08-22 実測）
      const rows = rowsFor(ALL.slice(0, -1)) + "\n| `.md` を編集した | 何か | `dependents.mjs`（`reportFor`） |";
      expect(checkEditFindingsTable(at(doc(rows)))).toEqual([]);
    });
  });

  describe("赤: 表だけが動いた", () => {
    it("実装に無い判定名が表にあれば finding", () => {
      const f = checkEditFindingsTable(at(doc(rowsFor([...ALL, "checkGhost"]))));
      expect(f).toHaveLength(1);
      expect(f[0].message).toContain("checkGhost");
      expect(f[0].message).toContain("実装に無い");
    });

    it("実装に在る判定が表から落ちていれば finding（**行の削除がこれで捕まる**）", () => {
      const f = checkEditFindingsTable(at(doc(rowsFor(ALL.filter((n) => n !== "checkFoldedCodeSpans")))));
      expect(f).toHaveLength(1);
      expect(f[0].message).toContain("checkFoldedCodeSpans");
      expect(f[0].message).toContain("表に無い");
    });
  });

  describe("赤: 母集団そのものが壊れた", () => {
    it("節が無ければ finding（見出しの改題で照合が空になる経路）", () => {
      const f = checkEditFindingsTable(at("# x\n本文だけ\n"));
      expect(f).toHaveLength(1);
      expect(f[0].message).toContain("母集団の欠落");
    });

    it("ヘッダが 2 本あれば finding（どれが本物か決まらない）", () => {
      const text = doc(rowsFor(ALL)).replace("後書き", `後書き\n\n${HEADER}${rowsFor(ALL)}`);
      const f = checkEditFindingsTable(at(text));
      expect(f).toHaveLength(1);
      expect(f[0].message).toContain("母集団の曖昧化");
    });

    it("**見出しより前の同形の表を本物と取り違えない**（本物が消えたら母集団の欠落）", () => {
      // 文書全体からヘッダを探していた頃は、decoy を唯一の候補として採り、
      // 本物の消滅を報告しないまま 8 件の別種の偽陽性へすり替わった（2026-08-22 実測）
      const text = `# 別の節\n\n${HEADER}| a | b | \`checkModuleIndex\` |\n\n${HEADING_LINE}\n\n表が消えた\n`;
      const f = checkEditFindingsTable(at(text));
      expect(f).toHaveLength(1);
      expect(f[0].message).toContain("母集団の欠落");
    });

    it("行が 0 件なら finding", () => {
      const f = checkEditFindingsTable(at(doc("")));
      expect(f.some((x) => x.message.includes("母集団の欠落"))).toBe(true);
    });

    it("判定列にバッククォートが無い行は finding（散文へ崩れると照合が静かに緩む）", () => {
      const rows = rowsFor(ALL) + "\n| 何かを編集した | 何か | 同上 |";
      const f = checkEditFindingsTable(at(doc(rows)));
      expect(f).toHaveLength(1);
      expect(f[0].message).toContain("判定列");
    });

    it("`docs/hooks.md` が読めなければ finding", () => {
      const f = checkEditFindingsTable(snap({}));
      expect(f).toHaveLength(1);
      expect(f[0].message).toContain("母集団の欠落");
    });
  });

  describe("証跡", () => {
    it("checked は照合した行数を返す（「差分ゼロ」と「照合していない」を分ける）", () => {
      const r = scanEditFindingsTable(at(doc(rowsFor(ALL))));
      expect(r.checked).toBe(ALL.length);
      expect(r.findings).toEqual([]);
    });

    it("実装側の判定名は SCAN_SCOPED を読んで導く（一覧を写さない）", () => {
      // **写しを持たないことの固定である。** `tableJudgments` が返す集合は `SCAN_SCOPED` の
      // `check.name` から導かれるので、配列へ 1 本足せばこの集合も自動で増える
      expect(tableJudgments()).toEqual(new Set(ALL));
    });
  });
});
