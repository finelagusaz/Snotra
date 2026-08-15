import { fileURLToPath } from "node:url";
import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { makeSnapshot, workspaceMembers } from "../lib.mjs";
import { checkModuleLinkage, declaredModuleFiles } from "./G-module-linkage.mjs";

// G-module-linkage が塞ぐのは **G-module-index が塞がない足**である（#1085 で足ごとに実測）。
// 索引にも mod にも書かない形は G-module-index が赤にするので、ここで守るのは
// 「索引には載るが mod 宣言が無い」＝どの検査も緑だった足のほうである。
describe("G-module-linkage checkModuleLinkage", () => {
  const cargo = '[workspace]\nmembers = ["demo"]\nresolver = "2"\n';
  const base = {
    "Cargo.toml": cargo,
    "demo/src/lib.rs": "mod search;\npub mod folder;\n",
    "demo/src/search.rs": "",
    "demo/src/folder.rs": "",
  };
  /** `base` の `lib.rs` へ `body` を足したスナップショット。**既定の構成を書き写さない**——
   *  写すと、`base` を変えたときに片方だけ古い母集団を検査し、しかもテストは通り続ける。 */
  const withLib = (body, extra = {}) => snap({ ...base, "demo/src/lib.rs": base["demo/src/lib.rs"] + body, ...extra });

  it("緑: 全ファイルが crate ルートから mod 宣言で到達できる", () => {
    expect(checkModuleLinkage(snap(base))).toEqual([]);
  });

  it("赤（守りたい足）: 索引には載るが mod 宣言が無い .rs", () => {
    // G-module-index を満たす（＝CLAUDE.md に載っている）状態でも、mod 宣言が無ければ赤になる。
    const s = snap({
      ...base,
      "demo/CLAUDE.md": "## モジュール構成\n- `lib.rs`\n- `search.rs`\n- `folder.rs`\n- `orphan.rs`\n",
      "demo/src/orphan.rs": "pub fn f() {}\n",
    });
    const f = checkModuleLinkage(s);
    expect(f.some((x) => x.file === "demo/src/orphan.rs")).toBe(true);
  });

  it("緑（判定対象外の不混入）: tests/・benches/・examples/・build.rs は mod 宣言を要さない", () => {
    // cargo が target として自動発見するため、宣言が無いまま正当である。
    const s = snap({
      ...base,
      "demo/build.rs": "fn main() {}\n",
      "demo/tests/it.rs": "#[test]\nfn t() {}\n",
      "demo/benches/b.rs": "",
      "demo/examples/e.rs": "",
    });
    expect(checkModuleLinkage(s)).toEqual([]);
  });

  it("緑（誤検出なし）: #[path] は「そのソースファイルが在るディレクトリ」から解決する", () => {
    // 実例: snotra-egui-runtime/src/ime.rs の `#[cfg(windows)] #[path = "windows_ime.rs"] mod platform;`
    // が src/windows_ime.rs を指す（src/ime/windows_ime.rs ではない）。
    const s = withLib("mod ime;\n", {
      "demo/src/ime.rs": '#[cfg(windows)]\n#[path = "windows_ime.rs"]\nmod platform;\n',
      "demo/src/windows_ime.rs": "",
    });
    expect(checkModuleLinkage(s)).toEqual([]);
  });

  it("緑（誤検出なし）: mod.rs 経由のネストを辿る", () => {
    const s = withLib("mod commands;\n", {
      "demo/src/commands/mod.rs": "mod open;\n",
      "demo/src/commands/open.rs": "",
    });
    expect(checkModuleLinkage(s)).toEqual([]);
  });

  // 以下 3 本は 2026-08-14 のレビューが実測で見つけた**沈黙の経路**を塞いだことを固定する。
  // どれも「宣言らしき綴りを拾ってしまい、孤児を到達済みに見せる」形だった。
  it("赤: ブロックコメントの中の mod 宣言を拾わない（孤児が緑にならない）", () => {
    const s = withLib("/*\nmod ghost;\n*/\n", { "demo/src/ghost.rs": "" });
    expect(checkModuleLinkage(s).some((x) => x.file === "demo/src/ghost.rs")).toBe(true);
  });

  it("赤: 複数行文字列リテラルの中の mod 宣言を拾わない", () => {
    const s = withLib('pub const S: &str = "\nmod fake;\n";\n', { "demo/src/fake.rs": "" });
    expect(checkModuleLinkage(s).some((x) => x.file === "demo/src/fake.rs")).toBe(true);
  });

  it("赤: インライン mod の中の mod 宣言を拾わない（同名の兄弟ファイルを緑にしない）", () => {
    // 拾うと基準ディレクトリを外したまま demo/src/inner.rs へ一致し、孤児が到達済みに見えていた。
    const s = withLib("mod outer {\n    mod inner;\n}\n", { "demo/src/inner.rs": "" });
    expect(checkModuleLinkage(s).some((x) => x.file === "demo/src/inner.rs")).toBe(true);
  });

  it("緑（判定対象外の不混入）: src/bin/*.rs は cargo が自動発見するので mod 宣言を要さない", () => {
    const s = snap({ ...base, "demo/src/bin/tool.rs": "fn main() {}\n" });
    expect(checkModuleLinkage(s)).toEqual([]);
  });

  // --- 字句スキャナの両方向（2026-08-14 のレビューが実測した 8 入力＋BOM）---
  // 初版はコメントの潰しと引用符の数え上げが別パスで、一方の数え違いがもう一方の判定を反転させた。
  // **誤検出（赤）と沈黙（緑）の両方が出る**ため、両向きを固定する。
  describe("コードでない部分の判別", () => {
    it("緑: 文字列の中の // をコメントと誤認しない（誤認すると閉じ引用符を食べて以降の宣言が消える）", () => {
      expect(checkModuleLinkage(withLib('pub const U: &str = "http://example.com";\n'))).toEqual([]);
    });
    it("緑: char リテラルの `\"` で位相が反転しない", () => {
      expect(checkModuleLinkage(withLib("pub const Q: char = '\"';\n"))).toEqual([]);
    });
    it("緑: 文字列の中の /* でブロックコメントを開かない", () => {
      expect(checkModuleLinkage(withLib('pub const S: &str = "/*";\n'))).toEqual([]);
    });
    it("緑: 行継続で 2 行にまたがる文字列の中の // を誤認しない", () => {
      expect(checkModuleLinkage(withLib('pub const S: &str = "a\\\n    https://x";\n'))).toEqual([]);
    });
    it("緑: BOM 付きでも 1 行目の宣言を拾う", () => {
      const s = snap({ ...base, "demo/src/lib.rs": `﻿${base["demo/src/lib.rs"]}` });
      expect(checkModuleLinkage(s)).toEqual([]);
    });

    it("赤: raw string の中に奇数個の `\"` があっても孤児を緑にしない", () => {
      const s = withLib('pub const S: &str = r#"x"y\nmod ghost;\n"#;\n', { "demo/src/ghost.rs": "" });
      expect(checkModuleLinkage(s).some((x) => x.file === "demo/src/ghost.rs")).toBe(true);
    });
    it("赤: raw string の中に綴られた #[path] を拾わない", () => {
      const s = withLib('pub const S: &str = r#"\n#[path = "ghost.rs"]\nmod g;\n"#;\n', { "demo/src/ghost.rs": "" });
      expect(checkModuleLinkage(s).some((x) => x.file === "demo/src/ghost.rs")).toBe(true);
    });
    it("赤: 先行する URL 文字列があっても、後続の複数行文字列の中身を拾わない", () => {
      const s = withLib('pub const U: &str = "http://x";\npub const S: &str = "\nmod ghost;\n";\n', { "demo/src/ghost.rs": "" });
      expect(checkModuleLinkage(s).some((x) => x.file === "demo/src/ghost.rs")).toBe(true);
    });
    // 固定長の窓（かつて 24 文字）で raw string の開始を探すと、ハッシュが窓を超えた瞬間に
    // 中身をコードとして読み、そこに綴られた `mod` を拾って孤児を緑にした（レビューが 23 個で閾値を実測）。
    // Rust はハッシュを 255 個まで許すので、**窓の内外をまたぐ個数**で固定する。
    // **中身に `"` を 1 つ挟むのが要点である。** 挟まないと、窓を外した実装でも `"` から始まる
    // 通常の文字列として同じ範囲を潰してしまい、欠陥が再現しない（＝縛らないカナリアになる）。
    // ハッシュ 0 個は `r"…"` が最初の `"` で閉じるためこの形を作れず、母集団から外れる。
    for (const hashes of [1, 22, 23, 40, 255]) {
      it(`赤: ハッシュ ${hashes} 個の raw string の中の mod 宣言を拾わない`, () => {
        const h = "#".repeat(hashes);
        const s = withLib(`pub const S: &str = r${h}"x"y\nmod ghost;\n"${h};\n`, { "demo/src/ghost.rs": "" });
        expect(checkModuleLinkage(s).some((x) => x.file === "demo/src/ghost.rs")).toBe(true);
      });
    }

    it("緑: ハッシュ無しの r\"…\" は最初の `\"` で閉じる（Rust の意味論どおりに扱う）", () => {
      // `r"x"` の後ろは**コード**なので、そこに書かれた宣言は本物として拾われる。
      expect(checkModuleLinkage(withLib('pub const S: &str = r"x";\nmod extra;\n', { "demo/src/extra.rs": "" }))).toEqual([]);
    });

    it("赤: 先行する char リテラルがあっても、後続の複数行文字列の中身を拾わない", () => {
      const s = withLib("pub const Q: char = '\"';\npub const S: &str = \"\nmod ghost;\n\";\n", { "demo/src/ghost.rs": "" });
      expect(checkModuleLinkage(s).some((x) => x.file === "demo/src/ghost.rs")).toBe(true);
    });
  });

  // 実リポジトリで緑（`G-workspace-lints` / `G-clippy-disallowed` が持つのと同型のカナリア）。
  // **ドッグフードとは別の命題である**——あちらは「宣言を拾えるか」、こちらは「いま findings が無いか」。
  // これが無いと、実コードの回帰は `npm test` を素通りして PR CI まで気づけない。
  it("カナリア: 実リポジトリで緑", () => {
    const s = makeSnapshot(fileURLToPath(new URL("../../..", import.meta.url)));
    expect(workspaceMembers(s).error, "母集団が取れない（カナリアの欠落）").toBeNull();
    expect(checkModuleLinkage(s)).toEqual([]);
  });

  // ドッグフード: フィクスチャは自分が想像した形しか守らない。**実ファイルで宣言が拾えること**を測る。
  // 初版はこの形でだけ見つかる欠陥を 3 枚抱えていた（`"http://x"` / `'"'` / `"/*"` を含むファイル）。
  it("実リポジトリの全 .rs へ宣言を注入すると、すべて拾える", () => {
    const root = fileURLToPath(new URL("../../..", import.meta.url));
    const s = makeSnapshot(root);
    const rs = s.files.filter((f) => /^[^/]+\/src\/.*\.rs$/.test(f));
    expect(rs.length, "母集団が空（ドッグフードの欠落）").toBeGreaterThan(50);
    const missed = rs.filter((f) => {
      const injected = `${s.read(f)}\nmod probe_zz;\n`;
      return !declaredModuleFiles(f, injected).some((c) => c.endsWith("/probe_zz.rs"));
    });
    expect(missed, "末尾へ足した mod 宣言を拾えないファイル（字句の判別が壊れている）").toEqual([]);
  });

  it("赤（検査を殺す変異）: ルート Cargo.toml が読めないとき緑を返さない", () => {
    const s = snap({ "demo/src/lib.rs": "", "demo/src/orphan.rs": "" });
    const f = checkModuleLinkage(s);
    expect(f.length).toBeGreaterThan(0);
    expect(f.some((x) => x.message.includes("母集団の欠落"))).toBe(true);
  });

  it("赤（検査を殺す変異）: crate ルートが 1 つも無いとき、母集団の欠落として名指しする", () => {
    // 全ファイルを未到達として列挙すると原因（ルート不在）が伝わらないので 1 件に畳む。
    const s = snap({ "Cargo.toml": cargo, "demo/src/a.rs": "", "demo/src/b.rs": "" });
    const f = checkModuleLinkage(s);
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("crate ルート");
  });

  it("赤（検査を殺す変異）: member の src/ に .rs が 1 件も無いとき緑を返さない", () => {
    const s = snap({ "Cargo.toml": cargo, "demo/README.md": "" });
    const f = checkModuleLinkage(s);
    expect(f.some((x) => x.message.includes("母集団の欠落"))).toBe(true);
  });
});
