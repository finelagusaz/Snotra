import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkFoldedCodeSpans, scanFoldedCodeSpans } from "./G-folded-code-spans.mjs";

// 守りたい対象 = #992 で実測した 6 箇所。**害は `grep` にしか出ない**——折れたスパンは正しい
// CommonMark であり、rustdoc は soft line break を跨いで正しく描画する。ゆえに整形器も lint も
// 見ない（固定版 1.98 で実測: rustfmt の `wrap_comments` は stable で無視され、clippy は
// doc lint を全部有効にしても 0 件）。
//
// **例示に実在の折れを置かない**——`checks/` はこの検査自身の走査母集団である。
// fixture は文字列リテラルなので `linesOfComments` が落とすが、`.md` の散文へ書くなら
// コードフェンスの内側でなければならない。
describe("G-folded-code-spans checkFoldedCodeSpans（コードスパンが物理改行を跨いだ形・#992）", () => {
  const run = (text, file = "docs/x.md") => checkFoldedCodeSpans(snap({ [file]: text }), [file]);

  describe("赤: スパンが行末を越える", () => {
    it("`.md` の散文で跨いでいれば finding", () => {
      const f = run("旧実装は判定と採用が `if seen.insert(key)\n{ push }` の同一式にあった。\n");
      expect(f).toHaveLength(1);
      expect(f[0].line).toBe(1);
      expect(f[0].message).toContain("コードスパンが物理改行を跨いでいる");
    });

    it("`///` / `//!` / `//` のコメント行でも見る", () => {
      expect(run("/// 実測: `RawInput.system_theme =\n/// Some(Light)` を 1 フレーム流す。\n", "a.rs")).toHaveLength(1);
      expect(run("//! 実測: `RawInput.system_theme =\n//! Some(Light)` を流す。\n", "a.rs")).toHaveLength(1);
      expect(run("// 実測: `RawInput.system_theme =\n// Some(Light)` を流す。\n", "a.rs")).toHaveLength(1);
    });

    it("スクリプトのコメント記法（`#` / ` * `）でも見る", () => {
      expect(run("# `-ErrorAction\n# SilentlyContinue` は空を返す。\n", "a.ps1")).toHaveLength(1);
      expect(run("/**\n * `gh pr view <PR> --json\n * closingIssuesReferences` は空白を含む。\n */\n", "a.mjs")).toHaveLength(1);
    });

    it("3 行以上に割れても finding は 1 件で、開始行を指す", () => {
      const f = run("説明は `let a =\nb +\nc;` である。\n");
      expect(f).toHaveLength(1);
      expect(f[0].line).toBe(1);
    });

    it("閉じないまま文書が終わっても finding", () => {
      expect(run("説明は `let a =\n")).toHaveLength(1);
    });

    it("CRLF でも見る（windows runner は `core.autocrlf=true` でチェックアウトする）", () => {
      expect(run("旧実装は `if seen.insert(key)\r\n{ push }` にあった。\r\n")).toHaveLength(1);
    });
  });

  describe("緑: 判定対象外が混ざらない", () => {
    it("同一行で閉じるスパンは finding にならない", () => {
      expect(run("`SearchEngine::new` と `to_kana` を全件へ当てる。\n")).toEqual([]);
    });

    it("コードフェンスの記号（3 連）で誤検知しない", () => {
      expect(run("/// ```text\n/// 図\n/// ```\n", "a.rs")).toEqual([]);
    });

    it("`` ` `` のエスケープ形で誤検知しない", () => {
      expect(run("この 3 条件が `` ` `` と `~` が互いを閉じない理由である\n")).toEqual([]);
      expect(run("かつてここは `` /^\\s*```/ `` に当たる行を数えていた\n")).toEqual([]);
    });

    it("`.rs` のコード行は走査しない（char リテラルのバッククォートを拾わない）", () => {
      // 行頭 `*` のデリファレンス代入文を「コメント」と数える述語では、この母集団が汚れる（実測 59 行）
      const src = "fn f(s: &str) {\n    let start = s.find('`')? + 1;\n    *self.v.write().unwrap() = 1;\n}\n";
      expect(run(src, "a.rs")).toEqual([]);
    });

    it("段落（コメント行の連なり）が途切れたら累積を捨てる", () => {
      // 1 行目で開いたスパンは 1 件。空行を挟んだ 4 行目は「内側」を引き継がないので、
      // 5 行目のスパンが独立して閉じていると読める
      const f = run("// `a =\n// b`\n\n// `c` と `d`\n", "a.rs");
      expect(f).toHaveLength(1);
      expect(f[0].line).toBe(1);
    });

    it("バッククォートを持たない行は finding にならない", () => {
      expect(run("折返しそのものは禁じない。素の散文は自由に折ってよい。\n")).toEqual([]);
    });
  });

  describe("母集団の健全性", () => {
    it("読めない文書は finding にする（母集団の欠落）", () => {
      const f = checkFoldedCodeSpans(snap({}), ["docs/missing.md"]);
      expect(f).toHaveLength(1);
      expect(f[0].message).toContain("母集団の欠落");
    });

    it("checked はスパンの開始点を数える（「差分ゼロ」と「照合していない」を分ける）", () => {
      const r = scanFoldedCodeSpans(snap({ "docs/x.md": "`a` と `b` と `c`\n" }), ["docs/x.md"]);
      expect(r.checked).toBe(3);
      expect(r.findings).toEqual([]);
    });

    it("母集団が空でも throw しない（0 件検知は runAll が持つ）", () => {
      expect(scanFoldedCodeSpans(snap({}), [])).toEqual({ findings: [], checked: 0 });
    });
  });
});
