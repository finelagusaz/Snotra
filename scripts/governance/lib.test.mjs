import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, it, expect } from "vitest";
import { snap } from "./test-helpers.mjs";
import { gitIgnoredPaths, makeSnapshot, headingRefSourceDocs, headingRefDocs, governanceDocs } from "./lib.mjs";
import { scanHeadingRefs, checkHeadingRefs } from "./checks/G-heading-refs.mjs";
import { checkNearHeadingRefs } from "./checks/G-near-heading-refs.mjs";
import { checkReferences } from "./checks/G-references.mjs";
import { checkAdrCitations, adrCitationDocs } from "./checks/G-adr-citations.mjs";
import { runAll } from "../governance-check.mjs";

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
    // 内側にあった。`productionOnly` 相当を「G-stale-identifiers との対称性の完成」として
    // 入れると、この it が落ちる——非対称は意図である
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
});
