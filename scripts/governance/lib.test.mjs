import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, it, expect } from "vitest";
import { snap } from "./test-helpers.mjs";
import {
  gitIgnoredPaths,
  makeSnapshot,
  headingRefSourceDocs,
  headingRefCommentDocs,
  headingRefDocs,
  governanceDocs,
  sectionOf,
  linesOutsideFences,
  linesOfComments,
  globToRegex,
  rulePathPatterns,
} from "./lib.mjs";
import { scanHeadingRefs, checkHeadingRefs } from "./checks/G-heading-refs.mjs";
import { checkNearHeadingRefs } from "./checks/G-near-heading-refs.mjs";
import { checkReferences } from "./checks/G-references.mjs";
import { checkAdrCitations, adrCitationDocs } from "./checks/G-adr-citations.mjs";
import { runAll } from "../governance-check.mjs";

// `globToRegex` は `G-rules-globs` と `G-rules-script-coverage` の 2 検査が共有する（#1143 で lib へ移送）。
// **意味論の固定はここ 1 か所で行う**——検査ごとに書き写すと、片方だけを直したときに配送判定が枝分かれする。
describe("globToRegex（rules paths の意味論固定・代表入力）", () => {
  const cases = [
    // [pattern, 一致する例, 一致しない例]
    ["AGENTS.md", "AGENTS.md", "docs/AGENTS.md"], // bare 名はルート直下のみ
    [".claude/hooks/**", ".claude/hooks/a/b.mjs", ".claude/hooksX/a.mjs"],
    ["snotra-core/**/*.rs", "snotra-core/src/lib.rs", "snotra-core/src/lib.ts"],
    ["ui/src/**/*.{ts,tsx}", "ui/src/main.tsx", "ui/main.tsx"],
    ["ui/src/**/*.{ts,tsx}", "ui/src/lib/a.ts", "ui/src/lib/a.rs"],
    ["scripts/governance-check.mjs", "scripts/governance-check.mjs", "scripts/governance-check.test.mjs"],
    // `*` は `/` を跨がない——#1143 の穴そのもの（この 1 行が、部分木が外れる形を固定する）
    ["scripts/*.mjs", "scripts/governance-check.mjs", "scripts/governance/checks/G-module-index.mjs"],
    ["scripts/**", "scripts/governance/checks/G-module-index.mjs", "scriptsX/a.mjs"],
  ];
  for (const [pat, ok, ng] of cases) {
    it(`${pat}: ${ok} に一致し ${ng} に一致しない`, () => {
      const re = globToRegex(pat);
      expect(re.test(ok)).toBe(true);
      expect(re.test(ng)).toBe(false);
    });
  }
  it("未閉ブレースは literal 扱いで停止する（無限ループ回帰・レビュー H2）", () => {
    const re = globToRegex("foo{bar.rs");
    expect(re.test("foo{bar.rs")).toBe(true);
    expect(re.test("foobar.rs")).toBe(false);
  });
});

describe("rulePathPatterns（frontmatter の中だけを見る）", () => {
  it("frontmatter の paths を順に返す", () => {
    expect(rulePathPatterns('---\npaths:\n  - "AGENTS.md"\n  - "scripts/**"\n---\n本文\n')).toEqual(["AGENTS.md", "scripts/**"]);
  });
  it("CRLF checkout でも読める", () => {
    expect(rulePathPatterns('---\r\npaths:\r\n  - "scripts/**"\r\n---\r\n本文\r\n')).toEqual(["scripts/**"]);
  });
  it("frontmatter が無ければ空（本文の箇条書きを拾わない）", () => {
    expect(rulePathPatterns('本文\n  - "scripts/**"\n')).toEqual([]);
  });
  it("本文側の同形の行を拾わない（母集団は frontmatter に閉じる）", () => {
    expect(rulePathPatterns('---\npaths:\n  - "AGENTS.md"\n---\n本文\n  - "scripts/**"\n')).toEqual(["AGENTS.md"]);
  });
});

describe("gitIgnoredPaths（存在に依らずパス名で判定する・#1088）", () => {
  it("ignore 対象は不在でも返り、非 ignore は返らない", () => {
    const got = gitIgnoredPaths([
      "test-results/never-created.json",
      ".claude/settings.local.json",
      "docs/nonexistent-typo.md",
    ]);
    expect(got.has("test-results/never-created.json")).toBe(true);
    expect(got.has(".claude/settings.local.json")).toBe(true);
    expect(got.has("docs/nonexistent-typo.md"), "非 ignore の typo が緑に化けている").toBe(false);
  });
  it("該当なし（git の exit 1）は失敗ではなく空集合", () => {
    expect(gitIgnoredPaths(["docs/nonexistent-typo.md"])).toEqual(new Set());
  });
  it("空入力は空集合を返す", () => {
    expect(gitIgnoredPaths([])).toEqual(new Set());
  });
});

describe("G-heading-refs / G-near-heading-refs のソースの腕（`.rs`・#925）", () => {
  // 守りたい対象 = `.rs` のコメントに書かれた正準形が、参照先の改題・移動・削除で沈黙すること。
  // 種はすべて合成スナップショットへ蒔く（ライブの検査もリポジトリのファイルも弱めない・
  // `.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」）。
  const TARGET = "## Git/GitHub 運用\n\n本文\n";
  const rs = (src) => snap({ "CLAUDE.md": TARGET, "src/a.rs": src });
  const scanRs = (src) => scanHeadingRefs(rs(src), ["src/a.rs"]);

  it("種 1: `.rs` の正準形が着地しなければ finding（赤）。対照: 着地すれば緑で照合件数が 1 進む", () => {
    const rot = scanRs('/// 詳細は `CLAUDE.md`「Git 運用」を見よ\nfn f() {}\n');
    expect(rot.findings).toHaveLength(1);
    expect(rot.findings[0].message).toContain("見出し参照が着地しない");
    const ok = scanRs('/// 詳細は `CLAUDE.md`「Git/GitHub 運用」を見よ\nfn f() {}\n');
    expect(ok.findings).toEqual([]);
    expect(ok.checked).toBe(1);
  });

  it("種 2: `.rs` の参照対象が解決できなければ finding（パスごと消えた場合）", () => {
    const f = scanRs('/// `docs/gone.md`「節」\nfn f() {}\n').findings;
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("見出し参照の対象が解決できない");
  });

  it("種 3: `#[cfg(test)]` の内側のコメントも見る（テストコードを母集団から外さない）", () => {
    // #925 が実際に見つけた腐り 1 件は `snotra-settings/src/tabs/visual.rs` の `#[cfg(test)]` の
    // 内側にあった。走査元へ `#[cfg(test)]` 以降を落とす変換を入れると、この it が落ちる
    // ——テストコードを母集団から外さないのは意図である
    const src = 'fn f() {}\n\n#[cfg(test)]\nmod tests {\n    // 根拠は `CLAUDE.md`「Git 運用」\n    #[test]\n    fn t() {}\n}\n';
    const f = scanRs(src).findings;
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("見出し参照が着地しない");
  });

  it("種 4: G-near-heading-refs も `.rs` を見る（実リポジトリには生きた事例が無く、ここでしか示せない）", () => {
    const f = checkNearHeadingRefs(rs('/// 詳細は `CLAUDE.md` の「Git/GitHub 運用」を見よ\nfn f() {}\n'), ["src/a.rs"]);
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("`CLAUDE.md`「Git/GitHub 運用」と書く");
  });

  it("種 5: `.rs` の母集団が 0 件なら runAll が明示 fail（md が非空でも鳴る）", () => {
    const { findings } = runAll(snap({ "CLAUDE.md": TARGET }));
    expect(findings.some((f) => f.message.includes("対象ソース（.rs）が 0 件"))).toBe(true);
  });

  it("種 6: md の母集団が 0 件なら runAll が明示 fail（`.rs` が非空でも鳴る）", () => {
    // 種 5 と別の it にする——1 本に束ねると、片方の腕が埋めた長さで他方の消滅が隠れる形を
    // テスト側で再現してしまう（`runAll` のコメントが `staleDocs` / `staleGuides` で名指しした失敗）
    const { findings } = runAll(snap({ "src/a.rs": "fn f() {}\n" }));
    expect(findings.some((f) => f.message.includes("対象 md が 0 件"))).toBe(true);
  });

  it("種 7: 判定対象外の不混入（`.rs` の腕は他の拡張子を拾わない）", () => {
    const s = snap({
      "src/a.rs": "",
      "src/b.ts": "",
      "scripts/c.mjs": "",
      "scripts/d.ps1": "",
      "Cargo.toml": "",
      "CLAUDE.md": "",
    });
    expect(headingRefSourceDocs(s)).toEqual(["src/a.rs"]);
  });

  it("種 8: 配線カナリア（G-heading-refs） — runAll 経由で `.rs` の腐りが findings に出る", () => {
    const { findings } = runAll(rs('/// `CLAUDE.md`「Git 運用」\nfn f() {}\n'));
    expect(findings.some((f) => f.file === "src/a.rs" && f.message.includes("見出し参照が着地しない"))).toBe(true);
  });

  it("種 9: 配線カナリア（G-near-heading-refs） — 近傍形も runAll 経由で `.rs` から出る", () => {
    // **腕ごとに 1 本ずつ要る。** 種 8 だけでは `scanNearHeadingRefs` の引数を md の腕へ戻す変異が
    // 素通りする（種 4 はリテラル母集団で呼ぶので buildChecks を通らず、実リポジトリの `.rs` 近傍参照は
    // 0 件なので dogfood も evidence も動かない）。G-stale-identifiers が配線 describe を 2 本置いたのと同じ形
    const { findings } = runAll(rs('/// 詳細は `CLAUDE.md` の「Git/GitHub 運用」を見よ\nfn f() {}\n'));
    expect(findings.some((f) => f.file === "src/a.rs" && f.message.includes("正準形でない"))).toBe(true);
  });
});

describe("G-heading-refs / G-near-heading-refs のスクリプトの腕（コメント行・#1138）", () => {
  // 守りたい対象 = スクリプトのコメントに書かれた正準形が、参照先の改題で沈黙すること。
  // #1137 で `/implement` を改番したとき `.md` の 3 件は名指しされ、`scripts/race-boundaries.mjs`
  // の 1 件だけが沈黙した。種はすべて合成スナップショットへ蒔く。
  const TARGET = "## Git/GitHub 運用\n\n本文\n";
  const src = (p, text) => snap({ "CLAUDE.md": TARGET, [p]: text });
  const scan = (p, text) => scanHeadingRefs(src(p, text), [p]);

  it("種 1: `.mjs` のコメントの正準形が着地しなければ finding（赤）。対照: 着地すれば緑", () => {
    const rot = scan("scripts/a.mjs", '// 詳細は `CLAUDE.md`「Git 運用」\n');
    expect(rot.findings).toHaveLength(1);
    expect(rot.findings[0].message).toContain("見出し参照が着地しない");
    const ok = scan("scripts/a.mjs", '// 詳細は `CLAUDE.md`「Git/GitHub 運用」\n');
    expect(ok.findings).toEqual([]);
    expect(ok.checked).toBe(1);
  });

  it("種 2: 文字列リテラルの中の同じ参照は見ない（負の fixture を偽陽性にしない契約の実体）", () => {
    // **これが「`*.test.mjs` を外す」の代わりに置いた意味の写像である。**拡張子で外すと
    // `*.Tests.ps1` のような別の綴りが素通りし、fixture を持たないテストのコメントまで落ちる。
    // この it が落ちたら、母集団の定義が拡張子の写像へ戻った合図
    const s = scan("scripts/a.test.mjs", 'const doc = "`CLAUDE.md`「Git 運用」";\n');
    expect(s.findings).toEqual([]);
    expect(s.checked).toBe(0);
  });

  it("種 3: ブロックコメント（`/* */`）の中も見る——継続行は行頭に `//` を持たない", () => {
    const f = scan("scripts/a.mjs", '/**\n * 詳細は `CLAUDE.md`「Git 運用」\n */\n').findings;
    expect(f).toHaveLength(1);
    expect(f[0].line).toBe(2);
  });

  it("種 4: PowerShell は `#` と `<# #>` の両方を見る（`.ps1` の実参照は help ブロックにも在る）", () => {
    const hash = scan("scripts/a.ps1", '# 詳細は `CLAUDE.md`「Git 運用」\n').findings;
    expect(hash).toHaveLength(1);
    const block = scan("scripts/a.ps1", '<#\n.SYNOPSIS\n詳細は `CLAUDE.md`「Git 運用」\n#>\n').findings;
    expect(block).toHaveLength(1);
    expect(block[0].line).toBe(3);
  });

  it("種 5: G-near-heading-refs もスクリプトのコメントを見る", () => {
    const f = checkNearHeadingRefs(src("scripts/a.mjs", '// 詳細は `CLAUDE.md` の「Git/GitHub 運用」\n'), ["scripts/a.mjs"]);
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("`CLAUDE.md`「Git/GitHub 運用」と書く");
  });

  it("種 6: スクリプトの母集団が 0 件なら runAll が明示 fail（md と `.rs` が非空でも鳴る）", () => {
    // 腕ごとに 1 本ずつ要る（種 5 / 種 6 が `.rs` と md について置いたのと同型）
    const { findings } = runAll(snap({ "CLAUDE.md": TARGET, "src/a.rs": "fn f() {}\n" }));
    expect(findings.some((f) => f.message.includes("対象スクリプト"))).toBe(true);
  });

  it("種 7: 判定対象外の不混入（母集団はコメント記法を持つ拡張子だけ）", () => {
    const s = snap({
      "scripts/a.mjs": "",
      "scripts/b.ps1": "",
      "scripts/lib/c.psm1": "",
      ".github/workflows/d.yml": "",
      "Cargo.toml": "",
      "src/e.rs": "",
      "CLAUDE.md": "",
      "package-lock.json": "",
      "docs/f.png": "",
    });
    expect(headingRefCommentDocs(s).sort()).toEqual([
      ".github/workflows/d.yml",
      "Cargo.toml",
      "scripts/a.mjs",
      "scripts/b.ps1",
      "scripts/lib/c.psm1",
    ]);
  });

  it("種 8: 配線カナリア — runAll 経由でスクリプトのコメントの腐りが findings に出る", () => {
    const { findings } = runAll(src("scripts/a.mjs", '// `CLAUDE.md`「Git 運用」\n'));
    expect(findings.some((f) => f.file === "scripts/a.mjs" && f.message.includes("見出し参照が着地しない"))).toBe(true);
  });

  it("linesOfComments: コメント記法を持たない対象は契約違反として throw する", () => {
    expect(() => linesOfComments("x\n", "docs/a.md")).toThrow(/コメント記法を持たない/);
  });
});

describe("凍結された歴史（ADR-adr-frozen-history）— 精度の辺は畳み、実在の辺は残す", () => {
  // 守りたい契約: ADR 本文**から**外への参照（精度の辺）は照合しない。
  // 生きた層 → ADR と ADR → ADR の短縮引用・生きた層 → ADR 見出し（実在の辺）は照合し続ける。
  // 種はどれも「稼働中のガードを弱めず、合成スナップショットへ蒔く」（.claude/rules/safety-nets.md）。

  it("種 1: ADR 内の腐った正準参照は見ない（緑）。対照: 同じ参照が生きた文書なら赤", () => {
    const rotten = "`docs/gone.md`「消えた節」\n";
    const adr = snap({ "docs/adr/ADR-a.md": `# ADR-a: x\n${rotten}` });
    expect(checkHeadingRefs(adr, headingRefDocs(adr))).toEqual([]);
    const live = snap({ "docs/x.md": rotten });
    expect(checkHeadingRefs(live, headingRefDocs(live))).toHaveLength(1);
  });

  it("種 2: ADR 内の実在しないパス参照は見ない（緑）。対照: 生きた文書なら赤", () => {
    const rotten = "`docs/gone-path.md` を見よ\n";
    const adr = snap({ "docs/adr/ADR-a.md": `# ADR-a: x\n${rotten}` });
    expect(checkReferences(adr, governanceDocs(adr))).toEqual([]);
    const live = snap({ "docs/x.md": rotten });
    expect(checkReferences(live, governanceDocs(live))).toHaveLength(1);
  });

  it("種 4: ADR から消えた ADR への短縮引用は赤のまま（実在の辺は凍結後も守る）", () => {
    const s = snap({ "docs/adr/ADR-a.md": "# ADR-a: x\n`ADR-gone` を却下の根拠とした\n" });
    const f = checkAdrCitations(s, adrCitationDocs(s, governanceDocs(s)));
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("ADR-gone");
  });

  it("種 5: 生きた文書から ADR 見出しへの正準参照は凍結後も照合される（走査元の除外は参照先解決を絞らない）", () => {
    const adr = "# ADR-a: x\n## 却下 6: 別案\n";
    const ok = snap({ "docs/x.md": "`docs/adr/ADR-a.md`「却下 6」\n", "docs/adr/ADR-a.md": adr });
    expect(checkHeadingRefs(ok, headingRefDocs(ok))).toEqual([]);
    const rot = snap({ "docs/x.md": "`docs/adr/ADR-a.md`「消えた見出し」\n", "docs/adr/ADR-a.md": adr });
    expect(checkHeadingRefs(rot, headingRefDocs(rot))).toHaveLength(1);
  });

  it("母集団カナリア: adrCitationDocs は docs/adr/ を明示的に含む（governanceDocs の除外に連動して落ちない）", () => {
    const s = snap({ "docs/adr/ADR-a.md": "# ADR-a: x\n", "CLAUDE.md": "" });
    const docs = governanceDocs(s);
    expect(docs).not.toContain("docs/adr/ADR-a.md");
    expect(adrCitationDocs(s, docs)).toContain("docs/adr/ADR-a.md");
  });
});

describe("makeSnapshot の走査除外（#722）", () => {
  // 守りたい対象 = SDD 作業バッファ。実リポジトリではなく一時ディレクトリの複製に当てる
  // （.claude/rules/safety-nets.md「稼働中のガードを弱めない——複製に変異を当てる」）
  it(".superpowers/ 配下は母集団に入らない（gitignore 済みで CI には存在しない＝手元だけ赤くなる）", () => {
    const root = mkdtempSync(path.join(tmpdir(), "gov-walk-"));
    try {
      mkdirSync(path.join(root, "docs"), { recursive: true });
      mkdirSync(path.join(root, ".superpowers/sdd/p"), { recursive: true });
      writeFileSync(path.join(root, "docs/a.md"), "# a\n");
      writeFileSync(path.join(root, ".superpowers/sdd/p/brief.md"), "# brief\n");
      const s = makeSnapshot(root);
      expect(s.files).toContain("docs/a.md");
      expect(s.files.filter((f) => f.startsWith(".superpowers/"))).toEqual([]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("生成物の除外はルート錨止めである（任意の深さの同名ディレクトリを落とさない・#1089）", () => {
    // 守りたい対象 = ネストしたソースディレクトリ。名前一致・全階層で落とすと、
    // `demo/src/target/orphan.rs` のような `.rs` が**どの深さでも**母集団から消え、
    // `mod` 宣言を持たない孤児でも findings 0（緑）になる——向きは沈黙である。
    // 今日のリポジトリにネストした target/dist/node_modules は 0 件（2026-08-17 実測）ゆえ
    // 露出は 0 で、塞いだのは将来この形が現れたときの沈黙である。
    const root = mkdtempSync(path.join(tmpdir(), "gov-walk-nested-"));
    try {
      for (const d of ["demo/src/target", "target", "node_modules", "dist", "ui/node_modules"]) {
        mkdirSync(path.join(root, d), { recursive: true });
      }
      writeFileSync(path.join(root, "demo/src/target/orphan.rs"), "fn f() {}\n");
      writeFileSync(path.join(root, "target/build.rs"), "");
      writeFileSync(path.join(root, "node_modules/pkg.js"), "");
      writeFileSync(path.join(root, "dist/bundle.js"), "");
      writeFileSync(path.join(root, "ui/node_modules/dep.js"), "");
      const files = makeSnapshot(root).files;
      expect(files, "ネストした target/ 配下が沈黙で母集団から消えている").toContain("demo/src/target/orphan.rs");
      // ルート直下の生成物は従来どおり落ちる
      expect(files.filter((f) => f.startsWith("target/"))).toEqual([]);
      expect(files.filter((f) => f.startsWith("node_modules/"))).toEqual([]);
      expect(files.filter((f) => f.startsWith("dist/"))).toEqual([]);
      // 失うもの: 2 つ目の npm パッケージを置くと走査に入る。向きはノイズ（安全側）
      expect(files).toContain("ui/node_modules/dep.js");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});

describe("linesOutsideFences — マスクと、釣り合わないフェンスの検知", () => {
  // 守りたい対象 = この関数を通す検査の母集団。閉じないフェンスは以降の全行を「内側」にするので、
  // 検知を**この関数の中**へ置く（外の検査にすると、その文書一覧が消費側の写しになって腐る）
  const F = "doc.md";

  it("緑: 釣り合っていればフェンスの内側だけを落とし、finding は出ない", () => {
    const findings = [];
    const doc = ["外1", "```bash", "内1", "```", "外2", ""].join("\n");
    const lines = linesOutsideFences(doc, F, findings);
    expect(findings).toEqual([]);
    expect(lines.map(([, l]) => l)).toEqual(["外1", "外2", ""]);
    expect(lines.map(([n]) => n), "行番号は元の文書の 1 起点").toEqual([1, 5, 6]);
  });

  it("緑: フェンスが 1 つも無ければ全行が返る", () => {
    const findings = [];
    expect(linesOutsideFences("a\nb\n", F, findings)).toHaveLength(3);
    expect(findings).toEqual([]);
  });

  it("赤: 開いたまま閉じないフェンスは finding になり、開いた行を名指しする", () => {
    const findings = [];
    const doc = ["外1", "```bash", "内1", "内2", ""].join("\n");
    linesOutsideFences(doc, F, findings);
    expect(findings).toHaveLength(1);
    expect(findings[0].file).toBe(F);
    expect(findings[0].line, "開いた行（2 行目）を名指しする").toBe(2);
    expect(findings[0].message).toContain("開いたまま閉じていない");
  });

  it("赤: 2 つ目のフェンスが閉じないときは、その開いた行を名指しする（1 つ目ではない）", () => {
    const findings = [];
    const doc = ["```", "a", "```", "b", "```", "c", ""].join("\n");
    linesOutsideFences(doc, F, findings);
    expect(findings).toHaveLength(1);
    expect(findings[0].line).toBe(5);
  });

  it("finding を積むだけで、返す行は変えない（マスクの意味論を静かに変えない）", () => {
    const doc = ["外1", "```bash", "内1", ""].join("\n");
    expect(linesOutsideFences(doc, F, []).map(([, l]) => l)).toEqual(["外1"]);
  });

  // ここから下は「パリティを数える形」が壊れていた地点である。パリティは「対になる行の文字も
  // 長さも同じ」という前提の上でしか成り立たず、4 連バッククォート（3 連を含む例を書くための記法・
  // このリポジトリで既に使われている）と `~~~` でその前提が崩れる。**修正前はいずれも
  // findings 0 件のままマスクが壊れていた**（下の it ごとに旧挙動を注記する）
  const B3 = "```";
  const B4 = "````";
  const T3 = "~~~";

  it("4 連バッククォートの中の 3 連は閉じない（閉じフェンスは開いた長さ以上でなければならない）", () => {
    // 旧: 3 連 2 本がパリティを反転させ、findings 0 件のまま「外2」が内側に落ちていた
    const findings = [];
    const doc = ["外1", B4, B3, "内", B3, B4, "外2", ""].join("\n");
    const lines = linesOutsideFences(doc, F, findings);
    expect(findings).toEqual([]);
    expect(lines.map(([, l]) => l)).toEqual(["外1", "外2", ""]);
  });

  it("`~~~` もフェンスとして扱う（旧: `~~~` は 1 行もフェンスに数えられなかった）", () => {
    const findings = [];
    const doc = ["外1", T3, "内", T3, "外2", ""].join("\n");
    expect(linesOutsideFences(doc, F, findings).map(([, l]) => l)).toEqual(["外1", "外2", ""]);
    expect(findings).toEqual([]);
  });

  it("開き文字の種類が違えば閉じない（`` ` `` と `~` は互いを閉じない）", () => {
    // 旧: `~~~` の中の 3 連がフェンスを 1 つ開いたことになり、**釣り合いの finding まで偽で出ていた**
    // （閉じないフェンスとして赤くなる形。今日の露出は 0 なので誰も踏んでいない）
    const findings = [];
    const doc = ["外1", T3, B3, "内", T3, "外2", ""].join("\n");
    expect(linesOutsideFences(doc, F, findings).map(([, l]) => l)).toEqual(["外1", "外2", ""]);
    expect(findings, "旧実装はここで偽の「閉じていない」finding を 1 件出していた").toEqual([]);
  });

  it("閉じフェンスは開いた長さ以上ならよい（4 連を 5 連で閉じられる）", () => {
    const findings = [];
    const doc = ["外1", B4, "内", B4 + "`", "外2", ""].join("\n");
    expect(linesOutsideFences(doc, F, findings).map(([, l]) => l)).toEqual(["外1", "外2", ""]);
    expect(findings).toEqual([]);
  });

  it("情報文字列を持つ行は閉じない（この修正が作った残余——赤側へ落ちる）", () => {
    // パリティの下では ```bash で開いて ```bash で閉じる書き方が通っていた。今は開いたままになり
    // 釣り合わない finding で赤くなる。走査中の 201 文書ではこの書き方は 1 件も無い（実測）
    const findings = [];
    const doc = ["外1", B3 + "bash", "内", B3 + "bash", "外2", ""].join("\n");
    linesOutsideFences(doc, F, findings);
    expect(findings).toHaveLength(1);
    expect(findings[0].message).toContain("開いたまま閉じていない");
  });

  it("file / findings は必須（呼び出し側の契約違反は throw・文書の欠陥とは別扱い）", () => {
    expect(() => linesOutsideFences("a", undefined, [])).toThrow(/file/);
    expect(() => linesOutsideFences("a", "", [])).toThrow(/file/);
    expect(() => linesOutsideFences("a", F)).toThrow(/findings/);
    expect(() => linesOutsideFences("a", F, null)).toThrow(/findings/);
  });
});

describe("sectionOf — 節の切り出しの契約（母集団の 4 つの壊れ方を赤にする）", () => {
  // 守りたい対象 = 節を食う検査の母集団。切り出し器は狭すぎ／広すぎの 2 方向に壊れ、
  // 向きを決めるのは切り出し器ではなく**切り出した結果を食う述語**である
  // （許可集合への所属なら広がりは沈黙、集合の一致なら誤報）。ゆえにここでは
  // 「宣言と実際の文書構造の食い違い」だけを機構で縛る。
  const opts = (ending) => ({ file: "doc.md", ending, by: "G-fixture" });
  const DOC = ["# t", "## A", "本文1", "### A1", "本文2", "## B", "本文3", ""].join("\n");

  it("緑: ending:\"heading\" は同レベル以上の見出しで終端する（下位見出しは終端しない）", () => {
    const r = sectionOf(DOC, /^## A$/, opts("heading"));
    expect(r.findings).toEqual([]);
    expect(r.body).toBe("本文1\n### A1\n本文2");
  });

  it("緑: ending:\"eof\" は終端が無いときだけ通り、body は EOF まで伸びる", () => {
    const r = sectionOf(DOC, /^## B$/, opts("eof"));
    expect(r.findings).toEqual([]);
    expect(r.body).toBe("本文3\n");
  });

  it("赤①: アンカーが 0 件（見出しの改題・消滅で母集団が空になる）", () => {
    const r = sectionOf(DOC, /^## Z$/, opts("heading"));
    expect(r.body).toBeNull();
    expect(r.findings).toHaveLength(1);
    expect(r.findings[0].message).toContain("見出しが見つからない");
  });

  it("赤②: アンカーが 2 件以上（どれが本物か決まらない・母集団の曖昧化）", () => {
    // findIndex 系の実装は先に現れた方を掴み、本物の節が照合されないまま緑になる
    // （G-hook-fires が表のヘッダ多重度に対して置いた検知と同型）
    const dup = ["# t", "## A", "x", "## B", "y", "## A", "z", "## C", ""].join("\n");
    const r = sectionOf(dup, /^## A$/, opts("heading"));
    expect(r.body).toBeNull();
    expect(r.findings).toHaveLength(1);
    expect(r.findings[0].message).toContain("2 本ある");
    expect(r.findings[0].line, "2 本目を名指しする（1 本目は正しいかもしれない）").toBe(6);
  });

  it("赤③: ending:\"heading\" なのに終端が無い（節が EOF まで伸びる＝母集団が広がる）", () => {
    const r = sectionOf(DOC, /^## B$/, opts("heading"));
    expect(r.body).toBeNull();
    expect(r.findings[0].message).toContain('ending: "heading"');
  });

  it("赤④: ending:\"eof\" なのに終端が在る（宣言が腐った——片側だけ検算すると宣言が写しとして腐る）", () => {
    // ④ が無いと `ending` の宣言そのものが「誰も検算しない散文」になり、次に読む人はそれを信じる
    const r = sectionOf(DOC, /^## A$/, opts("eof"));
    expect(r.body).toBeNull();
    expect(r.findings[0].message).toContain('ending: "eof"');
    expect(r.findings[0].line, "終端が現れた行を名指しする").toBe(6);
  });

  it("緑: 本文が空の節（アンカーの直後が終端）は有効——空文字列を「節が無い」と読まない", () => {
    const r = sectionOf(["## A", "## B", "x", ""].join("\n"), /^## A$/, opts("heading"));
    expect(r.findings).toEqual([]);
    expect(r.body).toBe("");
  });

  it("CRLF チェックアウトでも LF と同じ body を返す（`$` は `\\r` の手前に当たらない）", () => {
    expect(sectionOf(DOC.replace(/\n/g, "\r\n"), /^## A$/, opts("heading")).body).toBe("本文1\n### A1\n本文2");
  });

  it("g / y フラグ付きの正規表現は throw（lastIndex の持ち越しで行ごとの判定がずれる）", () => {
    expect(() => sectionOf(DOC, /^## A$/g, opts("heading"))).toThrow(/g \/ y/);
    expect(() => sectionOf(DOC, /^## A$/y, opts("heading"))).toThrow(/g \/ y/);
  });

  it("アンカーが ATX 見出しでない行に当たったら赤（レベルを推測しない）", () => {
    const r = sectionOf(["# t", "本文 ## A", ""].join("\n"), /^本文/, opts("heading"));
    expect(r.body).toBeNull();
    expect(r.findings[0].message).toContain("見出しでない");
  });

  it("コードフェンス内の列 0 の `#` 行は終端にならない（body がフェンスの途中で切れない）", () => {
    // 字面だけを見る実装は `# 整形` を終端に取り、body を 0 行へ縮めたまま findings 0 件で返す
    const doc = ["# t", "## A", "```bash", "# 整形", "cargo fmt", "```", "本文", "## B", "x", ""].join("\n");
    const r = sectionOf(doc, /^## A$/, opts("heading"));
    expect(r.findings).toEqual([]);
    expect(r.body).toBe("```bash\n# 整形\ncargo fmt\n```\n本文");
  });

  it("コードフェンス内の見出し様の行はアンカーに数えない（②の誤爆と、偽のアンカーの掴み違いを防ぐ）", () => {
    const doc = ["# t", "```", "## A", "```", "## A", "本文", "## B", "x", ""].join("\n");
    const r = sectionOf(doc, /^## A$/, opts("heading"));
    expect(r.findings, "フェンス内の 1 本を数えると②（2 本ある）で誤爆する").toEqual([]);
    expect(r.body).toBe("本文");
  });

  it("`ending: \"eof\"` の④はフェンス内の見出し様の行では発火しない", () => {
    const doc = ["# t", "## A", "本文", "```", "## B", "```", ""].join("\n");
    const r = sectionOf(doc, /^## A$/, opts("eof"));
    expect(r.findings).toEqual([]);
    expect(r.body).toBe("本文\n```\n## B\n```\n");
  });

  it("閉じていないフェンスは ending の両側で赤くなる（`eof` 側の沈黙を塞いだ地点）", () => {
    // **この検査は一度、逆の主張を固定していた。** マスクを入れた直後は `ending: "eof"` が
    // findings 0 件・body が EOF まで伸びる形で沈黙し（マスク以前は④で赤かった）、その姿を
    // 残余として固定していた。`linesOutsideFences` が釣り合いを検算するようになって塞がっている。
    const doc = ["# t", "## A", "本文", "```bash", "## X", "後続本文", ""].join("\n");
    for (const ending of ["heading", "eof"]) {
      const r = sectionOf(doc, /^## A$/, opts(ending));
      expect(r.body, `${ending}: マスクが信用できないので body を返さない`).toBeNull();
      expect(r.findings).toHaveLength(1);
      expect(r.findings[0].message).toContain("開いたまま閉じていない");
      expect(r.findings[0].line, "フェンスを開いた行を名指しする").toBe(4);
    }
  });

  it("4 連バッククォート・`~~~` の中の見出しも終端にならずアンカーにもならない", () => {
    // **パリティを数えていた頃、この 2 記法では 43e0c216 が塞いだ症状がそのまま再現していた**
    // ——`## にせ終端` が終端に採られて body が黙って縮み（4 連版は "本文1\n````\n```" だった）、
    // かつ `## にせ終端` 自身がアンカーとして採用された（2026-08-17 実測）
    const F3 = "```";
    for (const [name, fence] of [["4 連バッククォート", "````"], ["~~~", "~~~"]]) {
      const inner = fence === "````" ? [F3, "## にせ終端", F3] : ["## にせ終端"];
      const doc = ["# t", "## A", "本文1", fence, ...inner, fence, "本文2", "## B", "x", ""].join("\n");
      const r = sectionOf(doc, /^## A$/, opts("heading"));
      expect(r.findings, name).toEqual([]);
      expect(r.body, `${name}: 節がフェンスの途中で縮まない`).toBe([("本文1"), fence, ...inner, fence, "本文2"].join("\n"));
      const r2 = sectionOf(doc, /^## にせ終端$/, opts("heading"));
      expect(r2.body, `${name}: フェンス内の見出しはアンカーに採らない`).toBeNull();
      expect(r2.findings[0].message).toContain("見出しが見つからない");
    }
  });

  it("by は必須（finding が対象文書しか名指さないと、直す先の `ending` へ辿り着けない）", () => {
    expect(() => sectionOf(DOC, /^## A$/, { file: "doc.md", ending: "heading" })).toThrow(/by/);
    expect(() => sectionOf(DOC, /^## A$/, { file: "doc.md", ending: "heading", by: "" })).toThrow(/by/);
  });

  it("finding は宣言元の検査 id を名乗る（①〜④のすべて）", () => {
    const dup = ["# t", "## A", "x", "## A", "y", "## C", ""].join("\n");
    const msgs = [
      sectionOf(DOC, /^## Z$/, opts("heading")), // ①
      sectionOf(dup, /^## A$/, opts("heading")), // ②
      sectionOf(DOC, /^## B$/, opts("heading")), // ③
      sectionOf(DOC, /^## A$/, opts("eof")), // ④
    ];
    for (const r of msgs) {
      expect(r.body).toBeNull();
      expect(r.findings[0].message).toContain("宣言元: G-fixture");
    }
  });
});
