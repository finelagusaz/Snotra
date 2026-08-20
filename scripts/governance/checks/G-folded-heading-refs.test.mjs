import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkFoldedHeadingRefs, scanFoldedHeadingRefs } from "./G-folded-heading-refs.mjs";

// 守りたい対象 = #1154 で実測した 33 件。折れた正準形は `HEADING_REF` も `NEAR_REF` も
// **一致を生成しない**ので、`checked` にも `findings` にも現れない——「照合していない」と
// 「差分ゼロ」を分ける証跡すら残らない形である。
// **例示に実在の対象を置かない**——`checks/` はこの検査群の母集団（コメントの腕）だからである。
// プレースホルダは `isRefTargetSpelling` に当たらないので、この検査自身の finding にならない。
describe("G-folded-heading-refs checkFoldedHeadingRefs（正準形の参照が物理改行で折れた形・#1154）", () => {
  const run = (text, file = "docs/x.md") => checkFoldedHeadingRefs(snap({ [file]: text }), [file]);

  describe("形 A — 対象綴りで行が終わり、ラベルが次行から始まる", () => {
    it("`.md` の散文で折れていれば finding", () => {
      const f = run("実測は `PERFORMANCE.md`\n「索引の常駐の内訳」を見よ\n");
      expect(f).toHaveLength(1);
      expect(f[0].line).toBe(1);
      // **形の名指しまで固定する**——2 つの枝は同型（どちらも RegExp を line に当てて finding を 1 件出す）
      // なので、メッセージだけを入れ替えても件数・行番号は変わらない（`/symmetric-check` 2c で検出）。
      expect(f[0].message).toContain("ラベルが次行から始まっている");
    });

    it("`§` + 節番号を挟んで折れていても finding", () => {
      expect(run("`SPEC.md` §4.5\n「最大列挙数」\n")).toHaveLength(1);
    });

    it("`///` / `//!` / `//` の行頭記号を落として次行を見る", () => {
      expect(run("/// 実測は `PERFORMANCE.md`\n/// 「索引の常駐の内訳」\n", "a.rs")).toHaveLength(1);
      expect(run("//! 実測は `PERFORMANCE.md`\n//! 「索引の常駐の内訳」\n", "a.rs")).toHaveLength(1);
      expect(run("// 実測は `PERFORMANCE.md`\n// 「索引の常駐の内訳」\n", "a.rs")).toHaveLength(1);
    });

    it("blockquote `>` を落として次行を見る（33 件のうち 1 件がこの形）", () => {
      expect(run("> 実測は `PERFORMANCE.md`\n> 「索引の常駐の内訳」\n")).toHaveLength(1);
    });

    it("記号のあいだに空白がある 2 記号も落とす（`> - 「` / `/// - 「`）", () => {
      expect(run("> - 実測は `PERFORMANCE.md`\n> - 「索引の常駐の内訳」\n")).toHaveLength(1);
      expect(run("/// - 実測は `PERFORMANCE.md`\n/// - 「索引の常駐の内訳」\n", "a.rs")).toHaveLength(1);
    });

    it("スクリプトのコメント記法（`#` / ` * `）でも見る", () => {
      expect(run("# 実測は `PERFORMANCE.md`\n# 「索引の常駐の内訳」\n", "a.ps1")).toHaveLength(1);
      expect(run("/**\n * 実測は `PERFORMANCE.md`\n * 「索引の常駐の内訳」\n */\n", "a.mjs")).toHaveLength(1);
    });

    it("CRLF でも見る（windows runner は `core.autocrlf=true` でチェックアウトする）", () => {
      expect(run("実測は `PERFORMANCE.md`\r\n「索引の常駐の内訳」\r\n")).toHaveLength(1);
    });

    // **#1155 で意図を反転させた不変条件**——ここは以前「`.mjs` は対象綴りでないので見ない」を
    // 負例として固定していた。`isRefTargetSpelling` が `.mjs` を認めた以上、同じ入力は**赤にする側**である。
    // 反転させた事実を負例から消すだけにすると、`.mjs` の折れを誰も固定しないまま残る（不変条件の孤立）。
    it("`.mjs` を対象にした参照の折れも見る（#1155 で対象綴りに入った）", () => {
      expect(run("// 縛る向きは `governance/evidence.test.mjs`\n// 「配線:」が持つ\n", "a.mjs")).toHaveLength(1);
    });
  });

  describe("形 B — `「` まで同一行に開き、ラベル本文だけが次行へ流れる", () => {
    it("ラベルが次行へ流れていれば finding", () => {
      const f = run("実測は `PERFORMANCE.md`「索引の\n常駐の内訳」を見よ\n");
      expect(f).toHaveLength(1);
      expect(f[0].line).toBe(1);
      expect(f[0].message).toContain("ラベルが次行へ流れている");
    });

    it("ラベルが 3 行以上に割れていても同じ述語で捕まる（次行を見ないため）", () => {
      expect(run("`PERFORMANCE.md`「索引の\n常駐の\n内訳」\n")).toHaveLength(1);
    });

    it("次行が無くても（EOF で閉じない）finding", () => {
      expect(run("`PERFORMANCE.md`「索引の\n")).toHaveLength(1);
    });
  });

  describe("負例 — 折れていないものを赤にしない", () => {
    it("1 物理行に収まった正準形は見ない", () => {
      expect(run("実測は `PERFORMANCE.md`「索引の常駐の内訳」を見よ\n")).toEqual([]);
    });

    it("行末が対象綴りでも、次行が `「` で始まらなければ見ない（箇条書きの並び）", () => {
      expect(run("- 意図（仕様）: `SPEC.md`\n- 実測: `PERFORMANCE.md`\n")).toEqual([]);
    });

    it("行末が対象綴りで次行が空行なら見ない", () => {
      expect(run("詳細は `PERFORMANCE.md`\n\n「索引の常駐の内訳」\n")).toEqual([]);
    });

    it("対象の綴りでないバッククォートは見ない（識別子）", () => {
      expect(run("`someVar`\n「見出し」\n")).toEqual([]);
      expect(run("`someVar`「見出しの\n途中」\n")).toEqual([]);
    });

    // **ドットを持つが受理されない綴りの固定点**（#1155）。`.mjs` を対象綴りへ入れたとき、この役目を
    // 負っていた負例（`domains.test.mjs`）が正例へ移り、残った `someVar` はドットを持たないため
    // **`isRefTargetSpelling` を `includes(".")` へ広げる不注意な変更が全緑で通る**状態になっていた
    // （逆向きの監査が変異注入で実測）。ここで挙げる 2 つは
    // `ADR-canonical-heading-references`「決定」が「入れない」と宣言している死角そのものであり、
    // **死角を埋める日には同じ差分でこの負例も動く**。
    it("ドットを持っても対象綴りでなければ見ない（`.ps1` / `.ts` は宣言された死角）", () => {
      expect(run("詳細は `scripts/manual-smoke.ps1`\n「手順」\n", "a.mjs")).toEqual([]);
      expect(run("詳細は `src/lib/types.ts`\n「型」\n", "a.mjs")).toEqual([]);
    });

    it("コードフェンスの内側は見ない（`.md` の走査はフェンスを落とす）", () => {
      expect(run("```\n`PERFORMANCE.md`\n「索引の常駐の内訳」\n```\n")).toEqual([]);
    });

    it("スクリプトの非コメント行は見ない（文字列リテラルに書かれたデータ）", () => {
      expect(run('const s = "`PERFORMANCE.md`";\nconst t = "「索引の常駐の内訳」";\n', "a.mjs")).toEqual([]);
    });

    it("行頭記号の落としすぎを起こさない（本文が `-` で始まる行は `「` ではない）", () => {
      expect(run("詳細は `PERFORMANCE.md`\n- 「索引の常駐の内訳」ではない項目\n")).toHaveLength(1);
      expect(run("詳細は `PERFORMANCE.md`\n-- 通常の本文\n")).toEqual([]);
    });

    it("`/skill-name` の形も対象綴りとして見る", () => {
      expect(run("手順は `/merge-pr`\n「マージ後の 3 点検証」\n")).toHaveLength(1);
    });
  });

  describe("checked（照合していないと差分ゼロを分ける証跡・#497）", () => {
    it("候補位置を数える（finding が 0 でも非ゼロでありうる）", () => {
      const r = scanFoldedHeadingRefs(snap({ "docs/x.md": "`PERFORMANCE.md`「索引の常駐の内訳」\n詳細は `SPEC.md`\n次の行\n" }), ["docs/x.md"]);
      expect(r.findings).toEqual([]);
      expect(r.checked).toBe(1);
    });

    it("読めない文書は母集団の欠落として finding", () => {
      const f = checkFoldedHeadingRefs(snap({}), ["docs/missing.md"]);
      expect(f).toHaveLength(1);
      expect(f[0].message).toContain("母集団の欠落");
    });
  });
});
