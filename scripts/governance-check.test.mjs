// governance-check.mjs の検査関数を、フォールトインジェクションフィクスチャ（赤）と正常フィクスチャ（緑）の
// 両方向で検証する。各フィクスチャは「守りたい対象 1 件が入力に現れること」と
// 「判定対象外が入力に混じらないこと」の入力集合検算を兼ねる（.claude/rules/safety-nets.md）。
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it, expect } from "vitest";
import { snap } from "./governance/test-helpers.mjs";
import {
  MODULE_INDEX_CRATES,
  governanceDocs,
  makeSnapshot,
  gitIgnoredPaths,
  checkReferences,
  checkNormativeAreaInstrument,
  normativeArea,
  ALWAYS_LOADED_FILES,
  checkHeadingRefs,
  scanHeadingRefs,
  collectAnchors,
  headingRefDocs,
  headingRefSourceDocs,
  checkNearHeadingRefs,
  scanNearHeadingRefs,
  checkCheckSkillEnumeration,
  checkStaleIdentifiers,
  scanStaleIdentifiers,
  staleIdentifierDocs,
  staleIdentifierGuideDocs,
  staleIdentifierTargets,
  currentVocabulary,
  runAll,
  buildChecks,
  checkAdrFileNames,
  adrFiles,
  checkAdrCitations,
  scanAdrCitations,
  adrCitationDocs,
} from "./governance-check.mjs";

// G-module-index/G-references の母集団は手で列挙する定数であり、**crate を足しても何も鳴らない**（沈黙する経路）。
// `snotra-egui-runtime` は「#532 の検証層」として作られたまま両方から漏れ、SU7 で製品の
// 描画層になった後も索引ドリフト・参照切れが検知されない状態が続いていた（#701）。
// 実 `Cargo.toml` を読み、CLAUDE.md を持つ member が両母集団に載っていることを固定する。
describe("G-module-index/G-references 母集団カナリア — #701", () => {
  it("CLAUDE.md を持つ workspace member は MODULE_INDEX_CRATES と governanceDocs の両方に載る", () => {
    const root = fileURLToPath(new URL("..", import.meta.url));
    const src = readFileSync(fileURLToPath(new URL("../Cargo.toml", import.meta.url)), "utf8");

    // 書式が変わったら「読めなかった」と落ちる（fail-closed・post-edit.test.mjs の members カナリアと同型）
    const section = src.match(/\[workspace\]\r?\n([\s\S]*?)(?=\r?\n\[|$)/);
    expect(section, "Cargo.toml の [workspace] セクションを読めなかった").not.toBeNull();
    const m = section[1].match(/^members\s*=\s*\[([^\]]*)\]/m);
    expect(m, "[workspace] の members 行を読めなかった（書式が変わった）").not.toBeNull();
    const members = m[1]
      .split(",")
      .map((s) => s.trim().replace(/^"|"$/g, ""))
      .filter((s) => s.length > 0);

    const docs = governanceDocs(makeSnapshot(root));
    const withClaudeMd = members.filter((c) => existsSync(fileURLToPath(new URL(`../${c}/CLAUDE.md`, import.meta.url))));
    // 母集団が空になる形（members 読み違い・全 crate が CLAUDE.md 無し）を合格に見せない
    expect(withClaudeMd.length, "CLAUDE.md を持つ member が 0 件（母集団の欠落）").toBeGreaterThan(0);

    for (const crate of withClaudeMd) {
      expect(
        Object.keys(MODULE_INDEX_CRATES),
        `${crate} が MODULE_INDEX_CRATES に無い。src/exts を添えて追加すること（索引ドリフトが沈黙で通る）`,
      ).toContain(crate);
      expect(
        docs,
        `${crate}/CLAUDE.md が G-references 母集団に無い。governanceDocs の正規表現を更新すること（参照切れが沈黙で通る）`,
      ).toContain(`${crate}/CLAUDE.md`);
    }
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

describe("runAll（空母集団の明示 fail = 沈黙経路の閉塞）", () => {
  it("対象文書・rules・skills が空なら findings を返す", () => {
    const s = snap({});
    const { findings } = runAll(s);
    expect(findings.length).toBeGreaterThan(0);
  });
  it("計器（G-area-instrument）は検査配列に無い——面積に合否は無い（ADR-retire-area-budget）", () => {
    const ids = buildChecks(snap({}), {}).map((c) => c.id);
    expect(ids).not.toContain("G-area-instrument");
  });
  it("それでも計器の母集団欠落は runAll の findings に残る（検査配列の外でも沈黙しない）", () => {
    const { findings } = runAll(snap({}));
    expect(findings.some((f) => f.message.includes("G-area-instrument 母集団の欠落"))).toBe(true);
  });
});

describe("G-area-instrument checkNormativeAreaInstrument（合否を持たない計器・母集団だけを判定・ADR-retire-area-budget）", () => {
  const x = (n) => "x".repeat(n);
  const rule = (p, n) => ({ [`.claude/rules/${p}`]: x(n) });
  const skill = (name, desc) => ({
    [`.claude/skills/${name}/SKILL.md`]: `---\nname: ${name}\ndescription: "${desc}"\n---\n本文\n`,
  });
  const base = { ...rule("a.md", 1), ...skill("s", "d") };

  it("母集団が揃っていれば findings 無し（緑）", () => {
    const s = snap({ "CLAUDE.md": x(100), "AGENTS.md": x(100), ...base });
    expect(checkNormativeAreaInstrument(s)).toEqual([]);
  });

  // 守りたい対象 = 「面積の大小はもう合否を持たない」こと（ADR-retire-area-budget）。
  // 旧 G-area-budget が赤にした 2 形（常時ロード超過・面替えでの rules 超過）を**そのまま**当て、
  // 緑であることを実測する。上限判定が戻れば（定数を復活させて比較を足せば）この 2 本が落ちる。
  it("常時ロード面がいくら大きくても finding を出さない（旧・火災報知器の廃止）", () => {
    const s = snap({ "CLAUDE.md": x(1_000_000), "AGENTS.md": "", ...base });
    expect(checkNormativeAreaInstrument(s)).toEqual([]);
  });

  it("rules 面がいくら大きくても finding を出さない（面替えにも鳴らない）", () => {
    const s = snap({ "CLAUDE.md": x(10), "AGENTS.md": x(10), ...skill("s", "d"), ...rule("a.md", 1_000_000) });
    expect(checkNormativeAreaInstrument(s)).toEqual([]);
  });

  it("改行を畳んでも面積は改行のぶんしか下がらない（行数指標の誤った勾配を絶つ・ADR-area-metric-characters の核心）", () => {
    const areaOf = (t) => normativeArea(snap({ "CLAUDE.md": t, "AGENTS.md": "", ...base })).always;
    const spread = "あ\n".repeat(100); // 100 行 200 字
    const folded = "あ".repeat(100); // 1 行 100 字（内容は 1 字も減っていない）
    // 行数指標なら 100 → 1 で 99% の「削減」に見えた。文字数指標では改行 100 字ぶんだけ
    expect(areaOf(spread) - areaOf(folded)).toBe(100);
  });

  it("CR は数えない（CRLF checkout で面積が膨らむ沈黙経路の閉塞）", () => {
    const lf = snap({ "CLAUDE.md": "あ\n".repeat(10), "AGENTS.md": "", ...base });
    const crlf = snap({ "CLAUDE.md": "あ\r\n".repeat(10), "AGENTS.md": "", ...base });
    expect(normativeArea(lf).always).toBe(normativeArea(crlf).always);
  });

  it("skill description は常時ロード面に算入される（表→description の面替えを塞ぐ）", () => {
    const withShort = snap({ "CLAUDE.md": "", "AGENTS.md": "", ...rule("a.md", 1), ...skill("s", "d") });
    const withLong = snap({ "CLAUDE.md": "", "AGENTS.md": "", ...rule("a.md", 1), ...skill("s", "d".repeat(50)) });
    expect(normativeArea(withLong).always - normativeArea(withShort).always).toBe(49);
  });

  it("disable-model-invocation の skill の description は算入されない（注入されない字に課税しない）", () => {
    const hiddenSkill = (name, desc) => ({
      [`.claude/skills/${name}/SKILL.md`]: `---\nname: ${name}\ndescription: "${desc}"\ndisable-model-invocation: true\n---\n本文\n`,
    });
    const shortDesc = snap({ "CLAUDE.md": "", "AGENTS.md": "", ...rule("a.md", 1), ...hiddenSkill("h", "d") });
    const longDesc = snap({ "CLAUDE.md": "", "AGENTS.md": "", ...rule("a.md", 1), ...hiddenSkill("h", "d".repeat(50)) });
    expect(normativeArea(longDesc).always).toBe(normativeArea(shortDesc).always);
    // それでも母集団としては数える（skills 0 件の誤検知を出さない）
    expect(checkNormativeAreaInstrument(shortDesc).some((v) => v.file === ".claude/skills")).toBe(false);
  });

  it("description が 1 行スカラーでなければ finding（数えられない沈黙経路の閉塞）", () => {
    const s = snap({
      "CLAUDE.md": "",
      "AGENTS.md": "",
      ...rule("a.md", 1),
      ".claude/skills/s/SKILL.md": "---\nname: s\ndescription: |\n  複数行\n---\n",
    });
    const f = checkNormativeAreaInstrument(s);
    expect(f.some((v) => v.message.includes("1 行スカラーでない"))).toBe(true);
  });

  it("常時ロード文書が読めなければ母集団欠落 finding（沈黙経路の閉塞）", () => {
    const s = snap({ "AGENTS.md": x(1), ...base }); // CLAUDE.md 欠落
    const f = checkNormativeAreaInstrument(s);
    expect(f.some((v) => v.file === "CLAUDE.md" && v.message.includes("母集団の欠落"))).toBe(true);
  });

  it("rules / skills が 0 件なら母集団欠落 finding（グロブ破損の沈黙経路の閉塞）", () => {
    const noRules = snap({ "CLAUDE.md": x(1), "AGENTS.md": x(1), ...skill("s", "d") });
    expect(checkNormativeAreaInstrument(noRules).some((v) => v.file === ".claude/rules")).toBe(true);
    const noSkills = snap({ "CLAUDE.md": x(1), "AGENTS.md": x(1), ...rule("a.md", 1) });
    expect(checkNormativeAreaInstrument(noSkills).some((v) => v.file === ".claude/skills")).toBe(true);
  });

  it("ALWAYS_LOADED_FILES はルート直下の 2 文書", () => {
    expect(ALWAYS_LOADED_FILES).toEqual(["CLAUDE.md", "AGENTS.md"]);
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

describe("G-adr-citations（ADR の短縮引用が実在するか・#812 の A）", () => {
  // 連番だった頃は書けなかった検査である——`ADR-0007` は引用文字列とファイル名 stem が別物だった。
  // stem = 引用文字列にしたことで初めて機械照合できるようになった。
  const REAL = { "docs/adr/ADR-plan-ownership-boundary.md": "# ADR-plan-ownership-boundary: x\n" };
  const run = (doc, text) => checkAdrCitations(snap({ ...REAL, [doc]: text }), [doc]);

  it("実在する ADR を指す引用は findings 無し（緑）", () => {
    expect(run("CLAUDE.md", "詳細は `ADR-plan-ownership-boundary` を見よ\n")).toEqual([]);
  });

  it("実在しない引用は finding（赤）", () => {
    const f = run("CLAUDE.md", "`ADR-does-not-exist` を見よ\n");
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("ADR-does-not-exist");
  });

  it("製品コードのコメント内の引用も見る（今日まで検出器を持たなかった面）", () => {
    const f = run("src-tauri/src/a.rs", "// 意図的な 2 導出（ADR-gone 却下 1）\n");
    expect(f).toHaveLength(1);
  });

  it("md のコードフェンス内は見ない", () => {
    expect(run("CLAUDE.md", "```\n`ADR-does-not-exist`\n```\n")).toEqual([]);
  });

  it("判定対象外の不混入: 連番形・ADR という語そのもの", () => {
    expect(run("CLAUDE.md", "ADR は否定の知識を記録する。ADR-0007 は旧形式\n")).toEqual([]);
  });

  it("母集団は歴史資料（docs/superpowers/）を含まない", () => {
    const s = snap({ ...REAL, "CLAUDE.md": "", "docs/superpowers/specs/x.md": "", ".claude/skills/a/SKILL.md": "", "src-tauri/src/a.rs": "" });
    const pop = adrCitationDocs(s, ["CLAUDE.md"]);
    expect(pop).not.toContain("docs/superpowers/specs/x.md");
    expect(pop).toContain(".claude/skills/a/SKILL.md");
    expect(pop).toContain("src-tauri/src/a.rs");
  });

  it("テストファイルは母集団外（フィクスチャは赤経路のため意図的に実在しない名前を持つ）", () => {
    const s = snap({ ...REAL, "CLAUDE.md": "", "scripts/x.mjs": "", "scripts/x.test.mjs": "" });
    const pop = adrCitationDocs(s, ["CLAUDE.md"]);
    expect(pop).toContain("scripts/x.mjs");
    expect(pop).not.toContain("scripts/x.test.mjs");
  });

  it("照合件数を返す（「差分ゼロ」と「照合していない」の区別・#497）", () => {
    const r = scanAdrCitations(snap({ ...REAL, "CLAUDE.md": "`ADR-plan-ownership-boundary` と `ADR-gone`\n" }), ["CLAUDE.md"]);
    expect(r.checked).toBe(2);
    expect(r.findings).toHaveLength(1);
  });
});

describe("検査 ID の形（#812 — 序数を引用の語彙から外す）", () => {
  const ids = buildChecks(snap({}), {}).map((c) => c.id);

  it("すべて `G-<kebab>` 形で、数字を含まない", () => {
    for (const id of ids) expect(id, `${id} が G-<name> 形でない`).toMatch(/^G-[a-z][a-z0-9]*(-[a-z0-9]+)*$/);
  });

  it("重複しない（連番なら並行 PR が同じ値を見る形が、名前では起きないことの固定）", () => {
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("サマリの件数は登録表から計算される（範囲の手書きが存在しない）", () => {
    // 「G1..G15 passed」のような範囲表記は、検査を足しても黙って古くなる（#812 実測）
    const { evidence } = runAll(makeSnapshot(fileURLToPath(new URL("..", import.meta.url))));
    expect(evidence).toContain(`検査 ${ids.length} 件`);
  });
});

describe("実リポジトリ スモーク（dogfood）", () => {
  it("現在のリポジトリで全検査が緑", async () => {
    const { makeSnapshot } = await import("./governance-check.mjs");
    const { fileURLToPath } = await import("node:url");
    const s = makeSnapshot(fileURLToPath(new URL("..", import.meta.url)));
    const { findings } = runAll(s);
    expect(findings).toEqual([]);
  });
});

describe("facade の公開面（export { … } の凍結）", () => {
  // export { … } は手書きの一覧であり、この per-check 分割で検査を 1 本 checks/ へ移すたびに
  // 書き足す唯一の面である。書き忘れは npm test にも governance:check にも現れるとは限らない——
  // テストファイルが直接 import していない名前が消えても、どちらのコマンドも検知しない。
  // 公開面を丸ごと凍結することで、この一覧への変更は気づかず起きることではなく、
  // 意図して行う編集になる。
  it("公開する名前の集合が凍結した一覧と一致する", async () => {
    const mod = await import("./governance-check.mjs");
    expect(Object.keys(mod).sort()).toEqual([
      "ALWAYS_LOADED_FILES",
      "MODULE_INDEX_CRATES",
      "REQUIRED_DISALLOWED_METHODS",
      "REQUIRED_RUSTDOC_LINTS",
      "STALE_EXTRA_DOCS",
      "adrCitationDocs",
      "adrFiles",
      "buildChecks",
      "checkAdrCitations",
      "checkAdrFileNames",
      "checkArchitectureTable",
      "checkBuildCommands",
      "checkCheckSkillEnumeration",
      "checkCiTable",
      "checkClippyDisallowed",
      "checkHeadingRefs",
      "checkHookCommands",
      "checkHookFires",
      "checkModuleIndex",
      "checkModuleLinkage",
      "checkNearHeadingRefs",
      "checkNormativeAreaInstrument",
      "checkReferences",
      "checkRulesGlobs",
      "checkSkillTable",
      "checkSpecSections",
      "checkStaleIdentifiers",
      "checkWorkspaceLints",
      "clippyDisallowedCount",
      "clippyMethodsDenied",
      "collectAnchors",
      "currentVocabulary",
      "declaredModuleFiles",
      "declaresEguiDependency",
      "disallowedMethodPaths",
      "gitIgnoredPaths",
      "globToRegex",
      "governanceDocs",
      "hasWorkspaceLintsOptIn",
      "headingRefDocs",
      "headingRefSourceDocs",
      "makeSnapshot",
      "modelHiddenSkills",
      "normativeArea",
      "resolveRefTarget",
      "runAll",
      "rustdocLintsAreDenied",
      "scanAdrCitations",
      "scanHeadingRefs",
      "scanNearHeadingRefs",
      "scanStaleIdentifiers",
      "skillDescriptionArea",
      "staleIdentifierDocs",
      "staleIdentifierGuideDocs",
      "staleIdentifierTargets",
      "workspaceMembers",
    ]);
  });
});
