import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkFullwidthDocLinkBrackets, scanFullwidthDocLinkBrackets } from "./G-fullwidth-doc-link-bracket.mjs";

// 守りたい対象 = #1172 で実測した形。production の `///` に `[`X`］`（開き半角・閉じ全角）が 3 本
// あった状態で `cargo doc`（`broken_intra_doc_links = deny`）・clippy・test・`governance:check`・
// PostToolUse がすべて緑だった。rustdoc はリンクと認識せずリテラル出力し、**リンクですらないものには
// 何も言わない**。見つけたのは `/code-review` の 2 巡目である。
//
// fixture は文字列リテラルなので、この検査の母集団（`.rs` の doc 行）には入らない。
describe("G-fullwidth-doc-link-bracket checkFullwidthDocLinkBrackets（intra-doc link の角括弧の半角/全角混在・#1172）", () => {
  const run = (text, file = "a.rs") => checkFullwidthDocLinkBrackets(snap({ [file]: text }), [file]);

  describe("赤: 混在した対", () => {
    it("issue の実例——1 行に 2 つ同居する `[`X`］` は 2 件", () => {
      const f = run("/// 消費者は [`read_bar_anchor`］・[`derive_bar_rect_phys`］ が同じ合成を通る\n");
      expect(f).toHaveLength(2);
      expect(f[0].line).toBe(1);
      expect(f[0].message).toContain("半角と全角で混在");
      expect(f[0].message).toContain("read_bar_anchor");
    });

    it("逆向き `［`X`]` も赤", () => {
      expect(run("/// ［`FrameGeom`] を見る\n")).toHaveLength(1);
    });

    it("バッククォート無しの素のリンク形 `[Type::method］` も赤", () => {
      expect(run("/// see [Type::method］ for details\n")).toHaveLength(1);
    });

    it("`//!`（module doc）でも見る", () => {
      expect(run("//! 正本は [`layout::logical_to_phys`］ である\n")).toHaveLength(1);
    });

    it("インデントされた `///`（impl ブロック内の doc）でも見る", () => {
      // `linesOfComments` は trim 前の raw を返す。呼び出し側が trimStart しないと
      // 実ツリーの大半を占めるこの形を全滅で見逃す（敵対的調査 2026-09-02 が壊した点）
      const src = "impl X {\n    /// 合成は [`FrameGeom::bar_height_phys`］ が持つ\n    fn f() {}\n}\n";
      const f = run(src);
      expect(f).toHaveLength(1);
      expect(f[0].line).toBe(2);
    });
  });

  describe("緑: 判定対象外が混ざらない", () => {
    it("正しい `[`X`]` は finding にならない", () => {
      expect(run("/// 合成は [`FrameGeom::bar_height_phys`] が持ち、[`read_bar_anchor`] が通る\n")).toEqual([]);
    });

    it("全角同士 `［…］` はリテラルとして正しいので赤にしない", () => {
      expect(run("/// ［注意］この形はリンクではない\n")).toEqual([]);
    });

    it("バッククォートで包んだ非リンク角括弧 `` `[今すぐ更新]` `` は見ない", () => {
      expect(run("/// `[今すぐ更新]` を描く\n")).toEqual([]);
    });

    it("`//` インラインコメントは母集団外（表で × の面）", () => {
      expect(run("// 消費者は [`read_bar_anchor`］ を通る\n")).toEqual([]);
    });

    it("コード行（文字列リテラル）は見ない", () => {
      expect(run('fn f() { let s = "[`x`］"; }\n')).toEqual([]);
    });

    it("`.rs` 以外のファイルは母集団外", () => {
      expect(run("/// see [`X`］\n", "a.mjs")).toEqual([]);
      expect(run("see [`X`］\n", "docs/x.md")).toEqual([]);
    });
  });

  describe("証跡", () => {
    it("`checked` は角括弧の対の総数（正しい対も混在も数える）", () => {
      const r = scanFullwidthDocLinkBrackets(snap({ "a.rs": "/// [`A`] と [`B`］ と ［C］\n" }), ["a.rs"]);
      expect(r.checked).toBe(3);
      expect(r.findings).toHaveLength(1);
    });

    it("読めない文書は「母集団の欠落」として finding になる", () => {
      const r = scanFullwidthDocLinkBrackets(snap({}), ["missing.rs"]);
      expect(r.findings).toHaveLength(1);
      expect(r.findings[0].message).toContain("母集団の欠落");
    });
  });
});
