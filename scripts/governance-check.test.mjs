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
  checkHookCommands,
  checkHookFires,
  gitIgnoredPaths,
  checkReferences,
  checkSpecSections,
  checkBuildCommands,
  checkWorkspaceLints,
  workspaceMembers,
  checkClippyDisallowed,
  clippyDisallowedCount,
  disallowedMethodPaths,
  declaresEguiDependency,
  clippyMethodsDenied,
  REQUIRED_DISALLOWED_METHODS,
  checkCiTable,
  checkRulesGlobs,
  checkSkillTable,
  globToRegex,
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

describe("globToRegex（G-rules-globs の意味論固定・代表入力）", () => {
  const cases = [
    // [pattern, 一致する例, 一致しない例]
    ["AGENTS.md", "AGENTS.md", "docs/AGENTS.md"], // bare 名はルート直下のみ
    [".claude/hooks/**", ".claude/hooks/a/b.mjs", ".claude/hooksX/a.mjs"],
    ["snotra-core/**/*.rs", "snotra-core/src/lib.rs", "snotra-core/src/lib.ts"],
    ["ui/src/**/*.{ts,tsx}", "ui/src/main.tsx", "ui/main.tsx"],
    ["ui/src/**/*.{ts,tsx}", "ui/src/lib/a.ts", "ui/src/lib/a.rs"],
    ["scripts/governance-check.mjs", "scripts/governance-check.mjs", "scripts/governance-check.test.mjs"],
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

describe("G-references checkReferences", () => {
  it("緑: 実在するリンクとパス参照", () => {
    const s = snap({
      "AGENTS.md": "[規約](docs/guide.md) と `docs/guide.md` を参照\n",
      "docs/guide.md": "",
    });
    expect(checkReferences(s, ["AGENTS.md"])).toEqual([]);
  });
  it("赤: 壊れた相対リンクは同 basename の別ファイルがあっても赤（サフィックス解決はバッククォート参照限定・レビュー M1）", () => {
    const s = snap({ "docs/a.md": "[x](guide.md)\n" }, ["ui/src/guide.md"]);
    const f = checkReferences(s, ["docs/a.md"]);
    expect(f.some((x) => x.message.includes("guide.md"))).toBe(true);
  });
  it("赤: 実在しない Markdown リンク先", () => {
    const s = snap({ "AGENTS.md": "[x](docs/gone.md)\n" });
    const f = checkReferences(s, ["AGENTS.md"]);
    expect(f.some((x) => x.message.includes("docs/gone.md"))).toBe(true);
  });
  it("赤: 実在しないバッククォートパス参照", () => {
    const s = snap({ "AGENTS.md": "`docs/gone.md` を見よ\n" });
    const f = checkReferences(s, ["AGENTS.md"]);
    expect(f.some((x) => x.message.includes("docs/gone.md"))).toBe(true);
  });
  it("crate 内相対参照（`lib/x.ts` 等）はサフィックス一致で解決する", () => {
    const s = snap({ "ui/CLAUDE.md": "`lib/types.ts` と `commands/launch.rs` を参照\n" }, ["ui/src/lib/types.ts", "src-tauri/src/commands/launch.rs"]);
    expect(checkReferences(s, ["ui/CLAUDE.md"])).toEqual([]);
  });
  it("赤: サフィックス一致でも解決できない crate 内相対参照", () => {
    const s = snap({ "ui/CLAUDE.md": "`lib/gone.ts` を参照\n" }, ["ui/src/lib/types.ts"]);
    const f = checkReferences(s, ["ui/CLAUDE.md"]);
    expect(f.some((x) => x.message.includes("lib/gone.ts"))).toBe(true);
  });
  it("文書ディレクトリ基準の相対参照も実在と見なす", () => {
    const s = snap({
      "docs/guide.md": "`build-commands.md` は隣… ではなく `docs/build-commands.md`\n",
      "docs/build-commands.md": "",
    });
    expect(checkReferences(s, ["docs/guide.md"])).toEqual([]);
  });
  it("判定対象外の不混入: glob・プレースホルダ・URL・%・ベア名・ランタイム生成物・workspace/ を検査しない", () => {
    const s = snap({
      "AGENTS.md": [
        "`snotra-core/src/*.rs`",
        "`ui/src/**/*.test.{ts,tsx}`",
        "`.claude/worktrees/agent-<id>/`",
        "[外部](https://example.com/x.md)",
        "`%APPDATA%/Snotra/icons.bin`",
        "`config.toml`",
        "`index.bin`",
        "`config.toml.bak`",
        "`workspace/plan.md`",
        "`bincode 3.0.0`",
      ].join("\n"),
    });
    expect(checkReferences(s, ["AGENTS.md"])).toEqual([]);
  });
  it("コードフェンス内の参照は検査しない", () => {
    const s = snap({ "AGENTS.md": "```bash\ncat docs/gone.md\n`docs/gone2.md`\n```\n" });
    expect(checkReferences(s, ["AGENTS.md"])).toEqual([]);
  });
  // --- gitignore の 3 分類（#1088）---
  // 実在する → 緑 / 実在しないが ignore 対象 → 緑 / どちらでもない → 赤
  it("実在しないが ignore 対象なら緑（生成物・ローカル設定を意図して指している）", () => {
    const s = snap({ "AGENTS.md": "実行の記録は `test-results/.last-run.json` に出る\n" });
    const ignored = () => new Set(["test-results/.last-run.json"]);
    expect(checkReferences(s, ["AGENTS.md"], ignored)).toEqual([]);
  });
  it("実在せず ignore 対象でもなければ赤のまま（typo の検出という本来の目的）", () => {
    const s = snap({ "AGENTS.md": "`docs/typo-nonexistent.md` を見よ\n" });
    const f = checkReferences(s, ["AGENTS.md"], () => new Set());
    expect(f.some((x) => x.message.includes("docs/typo-nonexistent.md"))).toBe(true);
  });
  it("既定引数は何も免除しない（注入を忘れた経路は緑でなく赤へ倒れる）", () => {
    const s = snap({ "AGENTS.md": "`test-results/.last-run.json` を見よ\n" });
    expect(checkReferences(s, ["AGENTS.md"])).toHaveLength(1);
  });
  it("文書ディレクトリ基準の候補も判定へ渡る", () => {
    const s = snap({ "docs/a.md": "`gen/out.json` に出る\n" });
    const seen = [];
    const ignored = (paths) => {
      seen.push(...paths);
      return new Set(paths.filter((p) => p === "docs/gen/out.json"));
    };
    expect(checkReferences(s, ["docs/a.md"], ignored)).toEqual([]);
    expect(seen, "ルート基準の候補も渡っていない").toContain("gen/out.json");
  });
  it("Markdown リンクにも同じ 3 分類が当たる", () => {
    const s = snap({ "AGENTS.md": "[記録](test-results/.last-run.json)\n" });
    expect(checkReferences(s, ["AGENTS.md"], () => new Set(["test-results/.last-run.json"]))).toEqual([]);
  });
  it("filterIgnored の呼び出しは 1 回（spawn を束ねる構造の固定）", () => {
    const s = snap({ "AGENTS.md": "`docs/x1.md` と `docs/x2.md` と [y](docs/x3.md)\n" });
    let calls = 0;
    checkReferences(s, ["AGENTS.md"], (paths) => {
      calls += 1;
      return new Set(paths);
    });
    expect(calls).toBe(1);
  });
});

describe("G-spec-sections checkSpecSections", () => {
  const spec = "## 1. 概要\n### 1.1 目的\n### 1.2 範囲\n## 2. 検索\n";
  it("緑: 連続した番号と実在する SPEC § 参照", () => {
    const s = snap({ "SPEC.md": spec, "docs/a.md": "SPEC §1.2 と SPEC.md §2 を参照\n" });
    expect(checkSpecSections(s, ["docs/a.md"])).toEqual([]);
  });
  it("赤: 番号の飛びを検出する", () => {
    const s = snap({ "SPEC.md": "## 1. a\n## 3. b\n" });
    const f = checkSpecSections(s, []);
    expect(f.some((x) => x.message.includes("3"))).toBe(true);
  });
  it("赤: 実在しない SPEC § 参照", () => {
    const s = snap({ "SPEC.md": spec, "docs/a.md": "SPEC §9.9 を参照\n" });
    const f = checkSpecSections(s, ["docs/a.md"]);
    expect(f.some((x) => x.message.includes("9.9"))).toBe(true);
  });
  it("コードフェンス内の # 行（TOML コメント等）を見出しと誤認しない", () => {
    const s = snap({ "SPEC.md": "## 1. a\n```toml\n# 旧形式（廃止）\n## 5. ダミー\n```\n## 2. b\n" });
    expect(checkSpecSections(s, [])).toEqual([]);
  });
  it("バッククォート隣接形（`SPEC.md` §N）も参照として拾う（レビュー L2）", () => {
    const s = snap({ "SPEC.md": spec, "docs/a.md": "`SPEC.md` §9.9 を参照\n" });
    const f = checkSpecSections(s, ["docs/a.md"]);
    expect(f.some((x) => x.message.includes("9.9"))).toBe(true);
  });
  it("SPEC 前置のない裸の §N は検査対象外（不混入検算）", () => {
    const s = snap({ "SPEC.md": spec, "docs/a.md": "設計文書 §99 を参照\n" });
    expect(checkSpecSections(s, ["docs/a.md"])).toEqual([]);
  });
});

describe("G-build-commands checkBuildCommands", () => {
  const pkg = JSON.stringify({ scripts: { test: "vitest run", typecheck: "tsc", "governance:check": "node scripts/governance-check.mjs" } });
  const cargoRoot = '[workspace]\nmembers = ["snotra-core", "src-tauri"]\n';
  const cargoCore = '[package]\nname = "snotra-core"\n';
  const cargoTauri = '[package]\nname = "snotra"\n';
  const base = {
    "package.json": pkg,
    "Cargo.toml": cargoRoot,
    "snotra-core/Cargo.toml": cargoCore,
    "src-tauri/Cargo.toml": cargoTauri,
  };
  it("緑: npm script と crate 名が実在する（-p snotra は src-tauri/ の package name）", () => {
    const s = snap({ ...base, "docs/build-commands.md": "`npm run typecheck` と `npm test`、`cargo test -p snotra` を実行\n" });
    expect(checkBuildCommands(s)).toEqual([]);
  });
  it("赤: 未定義の npm script", () => {
    const s = snap({ ...base, "docs/build-commands.md": "`npm run gone-script` を実行\n" });
    const f = checkBuildCommands(s);
    expect(f.some((x) => x.message.includes("gone-script"))).toBe(true);
  });
  it("赤: 実在しない crate 名", () => {
    const s = snap({ ...base, "docs/build-commands.md": "`cargo test -p snotra-gone`\n" });
    const f = checkBuildCommands(s);
    expect(f.some((x) => x.message.includes("snotra-gone"))).toBe(true);
  });
  it("members に glob 要素が混じると、正当な crate 参照も赤へ倒れる（workspaceMembers 載せ替えで変わる唯一の向き・fail-closed）", () => {
    // #713 の不変条件「G-build-commands の findings は載せ替え前後で変わらない」が
    // 前提条件（glob 要素が無い）つきであることを、機械で留める。
    const s = snap({
      ...base,
      "Cargo.toml": '[workspace]\nmembers = ["crates/*"]\n',
      "docs/build-commands.md": "`cargo test -p snotra`\n",
    });
    expect(checkBuildCommands(s).some((x) => x.message.includes("snotra"))).toBe(true);
  });
});

describe("G-workspace-lints checkWorkspaceLints（rustdoc deny の実効性・#713）", () => {
  // 赤フィクスチャは「cargo が exit 0 で沈黙した入力」そのもの（cargo 1.94.0 で実測）。
  // 述語を弱めた実装はこのどれかで必ず緑へ振れる。
  const ROOT_OK =
    '[workspace]\nmembers = ["a", "b"]\n\n[workspace.lints.rustdoc]\nbroken_intra_doc_links = "deny"\ninvalid_html_tags = "deny"\n';
  const OPT_IN = '[package]\nname = "a"\nversion.workspace = true\n\n[lints]\nworkspace = true\n\n[dependencies]\negui.workspace = true\n';
  const base = { "Cargo.toml": ROOT_OK, "a/Cargo.toml": OPT_IN, "b/Cargo.toml": OPT_IN };
  const member = (contents) => checkWorkspaceLints(snap({ ...base, "a/Cargo.toml": contents }));
  const root = (contents) => checkWorkspaceLints(snap({ ...base, "Cargo.toml": contents }));

  // --- クラス 1（member 側の opt-in）---
  it("緑: 全 member が [lints] workspace = true を持つ", () => {
    expect(checkWorkspaceLints(snap(base))).toEqual([]);
  });
  it("緑: ルート直下の dotted lints.workspace = true（テーブル形と等価・実測 F）", () => {
    expect(member('lints.workspace = true\n\n[package]\nname = "a"\n')).toEqual([]);
  });
  it("緑: 見出し・キーの行末コメントと余分な空白を許す", () => {
    expect(member('[package]\nname = "a"\n\n[lints]   # docgen 検出器の opt-in\nworkspace = true  # 継承\n')).toEqual([]);
  });
  // 赤ケースは件数ではなく **file の並び** で主張する——件数だけだと、クラス 2 が退行して
  // ルート由来の finding が 1 件出た状態でも「1 件」を満たしてしまう
  const memberRed = (contents) => expect(member(contents).map((x) => x.file));
  it("赤: member に [lints] が無い（実測 B・#706 の再現形）", () => {
    memberRed('[package]\nname = "a"\n').toEqual(["a/Cargo.toml"]);
  });
  it("赤: [lints.rustdoc] だけを持つ（workspace テーブルを継承しない・実測 C）", () => {
    memberRed('[package]\nname = "a"\n\n[lints.rustdoc]\ninvalid_html_tags = "deny"\n').toEqual(["a/Cargo.toml"]);
  });
  it("赤: [lints] に workspace = false", () => {
    memberRed('[package]\nname = "a"\n\n[lints]\nworkspace = false\n').toEqual(["a/Cargo.toml"]);
  });
  it("赤: [package] 配下の lints.workspace = true（cargo は警告を出すが exit 0 で通す形・実測 E）", () => {
    memberRed('[package]\nname = "a"\nlints.workspace = true\n').toEqual(["a/Cargo.toml"]);
  });
  it("不混入: version.workspace / <dep>.workspace は opt-in と見なさない（字面一致では常に緑になる）", () => {
    const f = member('[package]\nname = "a"\nversion.workspace = true\n\n[dependencies]\negui.workspace = true\ntauri.workspace = true\n');
    expect(f).toHaveLength(1);
    expect(f[0].file).toBe("a/Cargo.toml");
  });
  it("不混入: ルートの [workspace.lints.rustdoc] は member 側の判定に混入しない（読み取りが <dir>/Cargo.toml に閉じる）", () => {
    const f = member('[package]\nname = "a"\n');
    expect(f.map((x) => x.file)).toEqual(["a/Cargo.toml"]); // ルート由来の finding は出ない
  });

  // --- クラス 2（ルートの rustdoc deny が実効か）---
  it("緑: テーブル形 level = deny と forbid（実測 P/Q）", () => {
    expect(
      root('[workspace]\nmembers = ["a", "b"]\n\n[workspace.lints.rustdoc]\nbroken_intra_doc_links = { level = "deny", priority = 1 }\ninvalid_html_tags = "forbid"\n'),
    ).toEqual([]);
  });
  it("赤: deny → warn へ降格（実測 N）", () => {
    expect(root(ROOT_OK.replace('broken_intra_doc_links = "deny"', 'broken_intra_doc_links = "warn"'))).toHaveLength(1);
  });
  it("赤: [workspace.lints] は在るが rustdoc サブテーブルが無い（実測 R）", () => {
    expect(root('[workspace]\nmembers = ["a", "b"]\n\n[workspace.lints]\n')).toHaveLength(1);
  });
  it("赤: [workspace.lints.rustdoc] が空テーブル", () => {
    expect(root('[workspace]\nmembers = ["a", "b"]\n\n[workspace.lints.rustdoc]\n\n[profile.release]\nlto = true\n')).toHaveLength(1);
  });
  it("赤: rustdoc サブテーブルはあるが broken_intra_doc_links の行だけ消える（実測 R2）", () => {
    // 必須 lint の名指しを落とした述語は**この 1 件だけ**で緑へ振れる（実測で当初仕様が踏んだ欠陥）
    expect(root('[workspace]\nmembers = ["a", "b"]\n\n[workspace.lints.rustdoc]\ninvalid_html_tags = "deny"\n')).toHaveLength(1);
  });
  it("赤: 必須 2 件は deny だが、別の rustdoc lint が warn で足された（意図的な摩擦）", () => {
    expect(root(`${ROOT_OK}private_intra_doc_links = "warn"\n`)).toHaveLength(1);
  });
  it("不混入: [workspace.lints.clippy] が warn でも、rustdoc が deny なら緑（この検査は rustdoc カテゴリだけを見る。clippy 側で見張られているのは disallowed_methods の deny だけ・#950）", () => {
    expect(root(`${ROOT_OK}\n[workspace.lints.clippy]\nall = "warn"\n`)).toEqual([]);
  });

  // --- 母集団の欠落（fail-closed・5 分岐 + member 不読）---
  it("赤: ルート Cargo.toml が読めない → 1 件だけ（同じ欠落を 2 件に増やさない）", () => {
    const f = checkWorkspaceLints(snap({ "a/Cargo.toml": OPT_IN }));
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("母集団の欠落");
  });
  it("赤: [workspace] セクションが無い", () => {
    expect(root('[package]\nname = "x"\n').some((x) => x.message.includes("母集団の欠落"))).toBe(true);
  });
  it("赤: [workspace] は在るが members 行が無い（他の分岐では踏めない経路）", () => {
    expect(root(`[workspace]\nresolver = "2"\n\n${ROOT_OK.split("\n").slice(3).join("\n")}`).some((x) => x.message.includes("母集団の欠落"))).toBe(true);
  });
  it("赤: members が 0 件", () => {
    expect(root(ROOT_OK.replace('members = ["a", "b"]', "members = []")).some((x) => x.message.includes("母集団の欠落"))).toBe(true);
  });
  it("赤: members に glob 要素（展開器を持たないので母集団の欠落として倒す）", () => {
    expect(root(ROOT_OK.replace('"a", "b"', '"crates/*"')).some((x) => x.message.includes("母集団の欠落"))).toBe(true);
  });
  it("default-members が members より前に在っても members を拾う（[workspace] スコープの回帰ガード）", () => {
    // 載せ替え前の全文正規表現は `default-members = [...]` を先に拾って ["a"] を返していた。
    // 現ルートに default-members が無いため、この 1 件だけが素朴な形への差し戻しを検知する。
    const src = '[workspace]\ndefault-members = ["a"]\nmembers = ["a", "b"]\n';
    expect(workspaceMembers(snap({ "Cargo.toml": src })).members).toEqual(["a", "b"]);
  });
  it("赤: member の Cargo.toml が読めない", () => {
    const f = checkWorkspaceLints(snap({ "Cargo.toml": ROOT_OK, "a/Cargo.toml": OPT_IN }));
    expect(f).toHaveLength(1);
    expect(f[0].file).toBe("b/Cargo.toml");
  });

  // --- 回帰・カナリア ---
  it("CRLF でも判定が変わらない（CI の Windows checkout は autocrlf=true・#595 の同型）", () => {
    const crlf = (s) => s.replace(/\n/g, "\r\n");
    expect(checkWorkspaceLints(snap({ "Cargo.toml": crlf(ROOT_OK), "a/Cargo.toml": crlf(OPT_IN), "b/Cargo.toml": crlf(OPT_IN) }))).toEqual([]);
  });
  it("カナリア: 実リポジトリで緑であり、守りたい対象（snotra-egui-runtime）が入力に現れる", () => {
    const s = makeSnapshot(fileURLToPath(new URL("..", import.meta.url)));
    const { members, error } = workspaceMembers(s);
    expect(error, "実 Cargo.toml から members を導出できなかった").toBeNull();
    expect(members, "#706 の当事者が母集団に居ない＝この検査は再発を見ない").toContain("snotra-egui-runtime");
    expect(checkWorkspaceLints(s)).toEqual([]);
  });
});

describe("G-clippy-disallowed checkClippyDisallowed（clippy.toml の空洞化・#950）", () => {
  // 赤フィクスチャは「clippy が exit 0 で沈黙した入力」そのもの（clippy 1.94.0 で実測）。
  // 7 エントリは**リテラルで書く**——カナリアから生成すると緑が恒真になり、名指しの意味が消える。
  // reason に `（#751）` を入れてあるのは意図的で、引用符を見ないコメント除去（tomlLine）が
  // 行を切ることを緑側で押さえる。
  const R = 'reason = "root Ui が pass 冒頭で掴む Arc<Style> に間に合わない（#751）"';
  const CLIPPY_OK = [
    "# 群ごとにコメントで区切った禁止集合（#751 / #900 / #1067）",
    "disallowed-methods = [",
    `    { path = "egui::Context::set_visuals", ${R} },`,
    `    { path = "egui::Context::set_visuals_of", ${R} },`,
    `    { path = "egui::Context::style_mut_of", ${R} },`,
    `    { path = "egui::Context::set_style_of", ${R} },`,
    `    { path = "egui::Context::global_style_mut", ${R} },`,
    `    { path = "egui::Context::set_global_style", ${R} },`,
    `    { path = "egui::Context::all_styles_mut", ${R} },`,
    // 群 2（#1067）。**fixture は群を跨いで持つ**——実ファイルが 1 つの関心ではなくなった以上、
    // 1 群だけの入力で緑にすると「群を足したらカナリアへも足す」運用が検算されない。
    `    { path = "snotra_core::engine::Engine::sorted_by_path", reason = "計測専用の観測口（#1067）" },`,
    "]",
    "",
  ].join("\n");
  const MANIFEST_OK = '[package]\nname = "snotra"\n\n[lints]\nworkspace = true\n\n[dependencies]\nsnotra-egui-runtime = { path = "../snotra-egui-runtime" }\negui.workspace = true\n';
  const ROOT_DENY = '[workspace]\nmembers = ["src-tauri"]\n\n[workspace.lints.clippy]\ndisallowed_methods = "deny"\n';
  const base = { "src-tauri/clippy.toml": CLIPPY_OK, "src-tauri/Cargo.toml": MANIFEST_OK, "Cargo.toml": ROOT_DENY };
  const run = (over) => checkClippyDisallowed(snap({ ...base, ...over }));
  // 赤ケースは件数ではなく **file の並び** で主張する（G-workspace-lints と同じ理由——件数だけだと
  // 別クラスの退行で 1 件出た状態を満たしてしまう）
  const red = (over) => expect(run(over).map((x) => x.file));

  // --- 緑 ---
  it("緑: 実データ相当（複数行のインラインテーブル配列・reason に # を含む）", () => {
    expect(run({})).toEqual([]);
  });
  it("緑: 1 行形の配列（行パースではなく全域 match であること）", () => {
    const oneLine = `disallowed-methods = [${REQUIRED_DISALLOWED_METHODS.map((p) => `{ path = "${p}", ${R} }`).join(", ")}]\n`;
    expect(run({ "src-tauri/clippy.toml": oneLine })).toEqual([]);
  });
  // 引用符を見ない除去（tomlLine）は reason の `（#751）` で行を切る。path が先に在るうちは結果的に
  // 生き残るので、**順序を入れ替えた 1 件**がその不変条件を判別する唯一のフィクスチャである。
  it("緑: reason が path より前に書かれたエントリ（引用符を見ない除去なら赤へ振れる）", () => {
    const reasonFirst = CLIPPY_OK.replace(`{ path = "egui::Context::set_visuals", ${R} }`, `{ ${R}, path = "egui::Context::set_visuals" }`);
    expect(run({ "src-tauri/clippy.toml": reasonFirst })).toEqual([]);
  });
  it("緑: level のテーブル形（= { level = \"deny\", priority = 1 }）", () => {
    expect(run({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = { level = "deny", priority = 1 }\n' })).toEqual([]);
  });
  it("緑: CRLF でも判定が変わらない（CI の Windows checkout は autocrlf=true）", () => {
    const crlf = (s) => s.replace(/\n/g, "\r\n");
    expect(checkClippyDisallowed(snap({ "src-tauri/clippy.toml": crlf(CLIPPY_OK), "src-tauri/Cargo.toml": crlf(MANIFEST_OK), "Cargo.toml": crlf(ROOT_DENY) }))).toEqual([]);
  });

  // --- 赤クラス 1: 内容側（issue #950 が実測した 6 経路 + コメントアウト）---
  it("赤: clippy.toml ごと削除（clippy は沈黙・exit 0）", () => {
    const contents = { ...base };
    delete contents["src-tauri/clippy.toml"];
    expect(checkClippyDisallowed(snap(contents)).map((x) => x.file)).toEqual(["src-tauri/clippy.toml"]);
  });
  it("赤: disallowed-methods の配列ごと消滅（空配列と区別してメッセージを変える）", () => {
    const f = run({ "src-tauri/clippy.toml": "# 説明だけが残った\n" });
    expect(f.map((x) => x.file)).toEqual(["src-tauri/clippy.toml"]);
    expect(f[0].message).toContain("配列が無い");
  });
  it("赤: 空配列化", () => {
    red({ "src-tauri/clippy.toml": "disallowed-methods = []\n" }).toEqual(["src-tauri/clippy.toml"]);
  });
  it("赤: エントリが 1 行だけ消える（∀ 条件だけでは緑を通る形）", () => {
    const f = run({ "src-tauri/clippy.toml": CLIPPY_OK.replace(/^.*all_styles_mut.*$\n/m, "") });
    expect(f.map((x) => x.file)).toEqual(["src-tauri/clippy.toml"]);
    expect(f[0].message, "欠けたパスを名指ししないと直し方が読めない").toContain("egui::Context::all_styles_mut");
  });
  it("赤: メソッド名の書き損じ（clippy は warning だが -D warnings でも exit 0）", () => {
    red({ "src-tauri/clippy.toml": CLIPPY_OK.replace("set_visuals_of", "set_visuals_off") }).toEqual(["src-tauri/clippy.toml"]);
  });
  it("赤: crate 名の書き損じ（clippy は診断そのものを出さない）", () => {
    red({ "src-tauri/clippy.toml": CLIPPY_OK.replaceAll("egui::Context::", "eguii::Context::") }).toEqual(["src-tauri/clippy.toml"]);
  });
  // **配列は開いたままにする**——`= []` を先に置くとコメント除去が無い実装でも先頭の空配列に当たって
  // 赤になり、この不変条件を判別しない。開いたままなら、除去しない実装はコメント内の path を 7 件数えて
  // **緑へ振れる**（実測）。それがこのフィクスチャの守る境界である。
  it("赤: 配列の中でエントリが # でコメントアウトされる（素朴な行パースが緑で通す形・実測で発見）", () => {
    const commented = CLIPPY_OK.split("\n")
      .map((l) => (l.trim().startsWith("{ path") ? `#${l}` : l))
      .join("\n");
    red({ "src-tauri/clippy.toml": commented }).toEqual(["src-tauri/clippy.toml"]);
  });
  it("赤: egui 依存の消滅（禁止パスが解決する前提が消える）", () => {
    red({ "src-tauri/Cargo.toml": '[package]\nname = "snotra"\n\n[dependencies]\nserde = "1"\n' }).toEqual(["src-tauri/Cargo.toml"]);
  });
  it("赤: snotra-egui-runtime だけが在る形（字面一致の述語なら緑になる誤爆検算）", () => {
    red({ "src-tauri/Cargo.toml": '[dependencies]\nsnotra-egui-runtime = { path = "../snotra-egui-runtime" }\n' }).toEqual(["src-tauri/Cargo.toml"]);
  });

  // --- 赤クラス 2: レベル側（沈黙経路 0。**実リポジトリは Phase 1 以降これを満たすので、
  //     clippyMethodsDenied が常に true を返す実装を捕まえるのはこのフィクスチャだけである**）---
  it("赤: ルートから [workspace.lints.clippy] 節ごと消える（2 行削除＝最も起きやすい形）", () => {
    red({ "Cargo.toml": '[workspace]\nmembers = ["src-tauri"]\n\n[workspace.lints.rustdoc]\nbroken_intra_doc_links = "deny"\n' }).toEqual(["Cargo.toml"]);
  });
  it("赤: deny が warn へ降格（禁止が黙って助言になる）", () => {
    red({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = "warn"\n' }).toEqual(["Cargo.toml"]);
  });
  // deny の行を残したまま禁止が完全に消える形。clippy 側は exit 0・診断 0 件になることを実測した
  // （#950。disallowed-methods は clippy::all と clippy::style の両方に属する）
  it("赤: 同じ節の all = \"allow\" が deny を後から打ち消す（deny の行は残っている）", () => {
    red({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = "deny"\nall = "allow"\n' }).toEqual(["Cargo.toml"]);
  });
  it("赤: style = { level = \"allow\", priority = 1 } でも同じく打ち消される", () => {
    red({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = "deny"\nstyle = { level = "allow", priority = 1 }\n' }).toEqual(["Cargo.toml"]);
  });
  it("緑: 群の allow が priority = -1 で先に当たる形は禁止が生き残る（実測どおり緑へ倒す）", () => {
    expect(run({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = "deny"\nall = { level = "allow", priority = -1 }\n' })).toEqual([]);
  });
  it("緑: 打ち消さない lint の allow は誤爆しない（節に allow を書く正当な用途を残す）", () => {
    expect(run({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = "deny"\nneedless_range_loop = "allow"\n' })).toEqual([]);
  });
  // TOML 上等価な 3 綴り。インライン形しか読まない実装は、残り 2 つで **clippy exit 0 なのに緑** を返す（実測）
  it("赤: dotted 形の群 allow（all.level = \"allow\"）", () => {
    red({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = "deny"\nall.level = "allow"\n' }).toEqual(["Cargo.toml"]);
  });
  it("赤: dotted 形で priority を別行に書く形", () => {
    red({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = "deny"\nall.level = "allow"\nall.priority = 0\n' }).toEqual(["Cargo.toml"]);
  });
  it("赤: サブテーブル形（[workspace.lints.clippy.all]）", () => {
    red({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = "deny"\n\n[workspace.lints.clippy.all]\nlevel = "allow"\npriority = 0\n' }).toEqual(["Cargo.toml"]);
  });
  it("緑: disallowed_methods 自身も dotted / サブテーブル形で書ける", () => {
    expect(run({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods.level = "deny"\n' })).toEqual([]);
    expect(run({ "Cargo.toml": "[workspace.lints.clippy.disallowed_methods]\nlevel = \"forbid\"\n" })).toEqual([]);
  });
  it("赤: 数値区切り付きの priority（1_0 は 10 であって 1 ではない）", () => {
    red({
      "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = { level = "deny", priority = 5 }\nall = { level = "allow", priority = 1_0 }\n',
    }).toEqual(["Cargo.toml"]);
  });
  it("赤: 別カテゴリ配下の同じ字面（構文的位置で判定していることの検算）", () => {
    red({ "Cargo.toml": '[workspace.lints.rustdoc]\ndisallowed_methods = "deny"\n' }).toEqual(["Cargo.toml"]);
  });

  // --- 母集団の欠落 ---
  it("赤: 母集団の欠落は 3 入力それぞれで鳴る（沈黙させない側へ倒す）", () => {
    expect(checkClippyDisallowed(snap({})).map((x) => x.file)).toEqual(["src-tauri/clippy.toml", "src-tauri/Cargo.toml", "Cargo.toml"]);
  });

  // --- 述語の単体（境界）---
  it("disallowedMethodPaths は「配列が無い」(null) と「空配列」([]) を区別する", () => {
    expect(disallowedMethodPaths("# 何も無い\n")).toBeNull();
    expect(disallowedMethodPaths("disallowed-methods = []\n")).toEqual([]);
  });
  it("declaresEguiDependency は [package] 配下の同じ字面を数えない", () => {
    expect(declaresEguiDependency("[package]\negui.workspace = true\n")).toBe(false);
    expect(declaresEguiDependency('[dependencies]\negui = "=0.35.0"\n')).toBe(true);
    expect(declaresEguiDependency("[target.'cfg(windows)'.dependencies]\negui.workspace = true\n")).toBe(true);
  });
  // dependencies] で終わる節を広く受けると、どれも実害を持つ 3 つが紛れ込む（対称性検査 2c で発見）
  it("declaresEguiDependency は dev/build/workspace の dependencies を数えない", () => {
    expect(declaresEguiDependency('[dev-dependencies]\negui = "=0.35.0"\n'), "bin/lib ではパスが解決しない").toBe(false);
    expect(declaresEguiDependency('[build-dependencies]\negui = "=0.35.0"\n')).toBe(false);
    expect(declaresEguiDependency('[workspace.dependencies]\negui = "=0.35.0"\n'), "ルート Cargo.toml と取り違えても緑になってしまう").toBe(false);
  });
  it("checkClippyDisallowed の 3 入力は同型ゆえ、取り違えが赤として現れること", () => {
    // ルート Cargo.toml を src-tauri の manifest として渡した形（引数の取り違えの再現）
    const root = '[workspace]\nmembers = ["src-tauri"]\n\n[workspace.dependencies]\negui = "=0.35.0"\n\n[workspace.lints.clippy]\ndisallowed_methods = "deny"\n';
    expect(declaresEguiDependency(root), "ルートを src-tauri と取り違えたら赤でなければならない").toBe(false);
  });
  it("clippyMethodsDenied はハイフン形を非実効と判定する（fail-closed・直し方はソースのコメント）", () => {
    expect(clippyMethodsDenied('[workspace.lints.clippy]\ndisallowed-methods = "deny"\n')).toBe(false);
  });
  it("clippyDisallowedCount は読めない入力で 0 を返す（evidence が undefined にならない）", () => {
    expect(clippyDisallowedCount(snap({}))).toBe(0);
    // **数を書かない。** fixture は `REQUIRED_DISALLOWED_METHODS` を全件持つ形で組んであるので、
    // 禁止を 1 つ足すたびにこの行だけが腐る（#1067 で実際に踏んだ）。測りたいのは
    // 「読めた入力で件数が evidence に出る」ことであって、その時点の件数ではない。
    expect(clippyDisallowedCount(snap(base))).toBe(REQUIRED_DISALLOWED_METHODS.length);
  });

  // --- カナリア ---
  it("カナリア: 実リポジトリで緑であり、守りたい対象が全件入力に現れる", () => {
    const s = makeSnapshot(fileURLToPath(new URL("..", import.meta.url)));
    const real = disallowedMethodPaths(s.read("src-tauri/clippy.toml") ?? "");
    expect(real, "実 clippy.toml から disallowed-methods を導出できなかった").not.toBeNull();
    for (const p of REQUIRED_DISALLOWED_METHODS) expect(real, `${p} が実ファイルの入力に現れない`).toContain(p);
    expect(real, "判定対象外（除外したメソッド）が入力に混じっている").toHaveLength(REQUIRED_DISALLOWED_METHODS.length);
    expect(checkClippyDisallowed(s)).toEqual([]);
  });
});

describe("G-ci-table checkCiTable", () => {
  const base = {
    "package.json": JSON.stringify({ scripts: { test: "vitest run", "smoke:startup": "pwsh -NoProfile -File scripts/smoke-startup.ps1" } }),
    ".github/workflows/ci.yml": "jobs:\n  a:\n    steps:\n      - run: npm test\n",
    ".github/workflows/e2e.yml": "jobs:\n  b:\n    steps:\n      - run: pwsh scripts/smoke-startup.ps1 -Timeout 5\n",
  };
  const table = (rows) => `## CI/CD メモ\n| 検証コマンド | workflow | トリガー |\n|---|---|---|\n${rows}\n`;
  it("緑: 表のコマンドが workflow の run に現れる（wrapper のスクリプトパス出現も可）", () => {
    const s = snap({
      ...base,
      "docs/build-commands.md": table("| `npm test` | `ci.yml`（a） | PR 自動 |\n| `npm run smoke:startup` | `e2e.yml` | paths |"),
    });
    expect(checkCiTable(s)).toEqual([]);
  });
  it("npm ライフサイクル 1 段（prebuild 経由の typecheck）を実行ありと見なす", () => {
    const s = snap({
      "package.json": JSON.stringify({ scripts: { typecheck: "tsc", prebuild: "npm run typecheck", build: "vite build" } }),
      ".github/workflows/ci.yml": "jobs:\n  a:\n    steps:\n      - run: npm run build\n",
      "docs/build-commands.md": table("| `npm run build` / `npm run typecheck` | `ci.yml` | PR |"),
    });
    expect(checkCiTable(s)).toEqual([]);
  });
  it("赤: 崩れた行を黙って飛ばさない（照合されないまま素通りする false green・#863）", () => {
    const s = snap({ ...base, "docs/build-commands.md": table("| `npm test` |\n| `npm test` | `ci.yml` | PR |") });
    const f = checkCiTable(s);
    expect(f.some((x) => x.message.includes("そろっていない"))).toBe(true);
  });
  it("赤: 表の途中に表でない行が紛れる（走査打ち切りで以降が照合されない経路・#863）", () => {
    // 打ち切り実装では 2 行目の `npm run gone` が照合されないまま緑になる
    const s = snap({ ...base, "docs/build-commands.md": table("| `npm test` | `ci.yml` | PR |\n注記の行\n| `npm run gone` | `ci.yml` | PR |") });
    const f = checkCiTable(s);
    expect(f.some((x) => x.message.includes("表でない行がある"))).toBe(true);
    expect(f.some((x) => x.message.includes("npm run gone"))).toBe(true);
  });
  it("赤: 表の workflow ファイルが実在しない", () => {
    const s = snap({ ...base, "docs/build-commands.md": table("| `npm test` | `gone.yml` | PR |") });
    const f = checkCiTable(s);
    expect(f.some((x) => x.message.includes("gone.yml"))).toBe(true);
  });
  it("赤: 表のコマンドが workflow のどの run にも現れない", () => {
    const s = snap({ ...base, "docs/build-commands.md": table("| `npm run gone` | `ci.yml` | PR |") });
    const f = checkCiTable(s);
    expect(f.some((x) => x.message.includes("npm run gone"))).toBe(true);
  });
});

describe("G-rules-globs checkRulesGlobs", () => {
  it("緑: 全 glob が 1 件以上にマッチする", () => {
    const s = snap({ ".claude/rules/a.md": '---\npaths:\n  - "AGENTS.md"\n  - "ui/src/**/*.{ts,tsx}"\n---\n本文\n' }, ["AGENTS.md", "ui/src/main.tsx"]);
    expect(checkRulesGlobs(s)).toEqual([]);
  });
  it("赤: マッチ 0 件の glob を検出する", () => {
    const s = snap({ ".claude/rules/a.md": '---\npaths:\n  - "gone/**/*.rs"\n---\n本文\n' }, ["AGENTS.md"]);
    const f = checkRulesGlobs(s);
    expect(f.some((x) => x.message.includes("gone/**/*.rs"))).toBe(true);
  });
});

describe("G-skill-table checkSkillTable（表の対象は roster に載らない skill だけ）", () => {
  const claude = (rows) => `# x\n## 利用できるスキル\n\n| スキル | 使うとき |\n|---|---|\n${rows}\n\n## 次節\n`;
  /** roster に載る skill（harness が description ごと注入する） */
  const shown = (name) => ({ [`.claude/skills/${name}/SKILL.md`]: `---\nname: ${name}\ndescription: "d"\n---\n本文\n` });
  /** roster に載らない skill（user 起動専用） */
  const hidden = (name) => ({
    [`.claude/skills/${name}/SKILL.md`]: `---\nname: ${name}\ndescription: "d"\ndisable-model-invocation: true\n---\n本文\n`,
  });

  it("緑: 隠しスキルだけが表に載り、roster に載るスキルは載っていない", () => {
    const s = snap({ "CLAUDE.md": claude("| `/health-check` | 定期 |"), ...hidden("health-check"), ...shown("plan-review") });
    expect(checkSkillTable(s)).toEqual([]);
  });
  it("赤: 表にあるがディレクトリに無い", () => {
    const s = snap({ "CLAUDE.md": claude("| `/gone-skill` | x |"), ...hidden("health-check") });
    const f = checkSkillTable(s);
    expect(f.some((x) => x.message.includes("gone-skill") && x.message.includes("SKILL.md が無い"))).toBe(true);
  });
  it("赤: 隠しスキルが表に無い（索引としての意味が消える）", () => {
    const s = snap({ "CLAUDE.md": claude("| `/health-check` | x |"), ...hidden("health-check"), ...hidden("orphan") });
    const f = checkSkillTable(s);
    expect(f.some((x) => x.message.includes("orphan") && x.message.includes("roster に載らないのに"))).toBe(true);
  });
  it("赤: roster に載るスキルが表にある（同じ面での二重課税）", () => {
    const s = snap({ "CLAUDE.md": claude("| `/plan-review` | x |"), ...shown("plan-review"), ...hidden("health-check") });
    const f = checkSkillTable(s);
    expect(f.some((x) => x.message.includes("plan-review") && x.message.includes("roster に載る"))).toBe(true);
  });
  it("緑: frontmatter が壊れた skill は「隠しでない」へ倒れる（表に無くても赤にしない）", () => {
    // 判定不能を「隠し」へ倒すと、書きようのない表の行を要求して赤が意味を失う
    const s = snap({
      "CLAUDE.md": claude("| `/health-check` | x |"),
      ...hidden("health-check"),
      ".claude/skills/broken/SKILL.md": "disable-model-invocation: true\n本文だけで frontmatter の区切りが無い\n",
    });
    expect(checkSkillTable(s)).toEqual([]);
  });
});

describe("G-hook-commands checkHookCommands", () => {
  // clippy は実物どおり複数行折返し + 行末カンマ（単一行前提の抽出を壊す代表入力）
  const hookSrc = [
    "function buildCommand(id, root) {",
    '  switch (id) {',
    '    case "clippy":',
    "      return cargoSpec([",
    '        "clippy", "--workspace",',
    '        "--all-targets", "--message-format", "short", "--", "-D", "warnings",',
    "      ]);",
    '    case "core-test":',
    '      return cargoSpec(["test", "-p", "snotra-core"]);',
    '    case "typecheck":',
    '      return nodeSpec([tsc, "-p", "tsconfig.json"]);',
    "  }",
    "}",
  ].join("\n");
  const docsA = [
    "### A. Rust ファイル（`*.rs`）を変更した場合",
    "",
    "```bash",
    "cargo check --workspace   # 必須",
    "cargo clippy --workspace --all-targets -- -D warnings   # 必須",
    "cargo test -p snotra-core",
    "```",
    "",
    "### B. 次節",
  ].join("\n");
  const base = { ".claude/hooks/post-edit.mjs": hookSrc, "docs/build-commands.md": docsA };
  it("緑: 出力整形フラグ（--message-format short）除去後にカテゴリ A と一致する", () => {
    expect(checkHookCommands(snap(base))).toEqual([]);
  });
  it("赤: hook 側のフラグ乖離（--all-targets 欠落）を検出する", () => {
    const s = snap({ ...base, ".claude/hooks/post-edit.mjs": hookSrc.replace('"--all-targets", ', "") });
    const f = checkHookCommands(s);
    expect(f.some((x) => x.message.includes("clippy"))).toBe(true);
  });
  it("赤: SSOT 側からコマンド行が消えた乖離を検出する", () => {
    const s = snap({ ...base, "docs/build-commands.md": docsA.replace("cargo test -p snotra-core\n", "") });
    const f = checkHookCommands(s);
    expect(f.some((x) => x.message.includes("snotra-core"))).toBe(true);
  });
  it("赤: cargoSpec 抽出 0 件は母集団欠落として fail（抽出アンカー腐敗の明示的な失敗化）", () => {
    const s = snap({ ...base, ".claude/hooks/post-edit.mjs": "function buildCommand(){}" });
    const f = checkHookCommands(s);
    expect(f.length).toBeGreaterThan(0);
  });
  it("CRLF checkout でも一致する（\\r が行末コメント除去を阻む回帰・PR #595）", () => {
    const s = snap({
      ".claude/hooks/post-edit.mjs": hookSrc.replace(/\n/g, "\r\n"),
      "docs/build-commands.md": docsA.replace(/\n/g, "\r\n"),
    });
    expect(checkHookCommands(s)).toEqual([]);
  });
  it("不混入: nodeSpec / vitest 系のコマンドは照合対象にしない", () => {
    // hookSrc の typecheck（nodeSpec）が docs に無くても緑のまま（対象は cargo 系のみ）
    expect(checkHookCommands(snap(base))).toEqual([]);
  });
});

describe("G-hook-fires checkHookFires（発火一覧 ↔ selectChecks）", () => {
  // フォールトインジェクションは複製へ当てる（.claude/rules/safety-nets.md）——
  // 実物の selectChecks も docs/hooks.md も変異させず、fake の select と snapshot だけを壊す。
  const ROWS = [
    // 補足列にバッククォートを置く（不混入の代表入力: 照合するのは検査 id 列だけである）
    "| `snotra-core/src/lib.rs` | `fmt` `clippy` `core-test` | 修復は `cargo fmt --all` の 1 コマンド |",
    "| `Cargo.toml` | `cargo-check` `hook-selftest` | ルートは両方走る |",
    "| `.githooks/pre-commit` | `githooks-selftest` | |",
    "| `docs/hooks.md` | （なし） | 何も走らない——沈黙は「合格」ではない |",
  ];
  const table = (rows) =>
    ["## 発火一覧", "", "| 編集したファイル（代表パス） | 走る検査 id | 補足 |", "|---|---|---|", ...rows, "", "## 次節"].join("\n");
  const hookSrc = [
    "export function selectChecks(rel) {",
    "  const checks = [];",
    '  if (isRust) checks.push("fmt");',
    '  if (isRust) checks.push("clippy");',
    '  if (isRust && rel.startsWith("snotra-core/")) checks.push("core-test");',
    '  if (CARGO_MANIFEST.test(rel)) checks.push("cargo-check");',
    '  if (CHECK_DEFINITION.has(rel)) checks.push("hook-selftest");',
    '  if (rel.startsWith(".githooks/")) checks.push("githooks-selftest");',
    "  return checks;",
    "}",
  ].join("\n");
  const TRUTH = {
    "snotra-core/src/lib.rs": ["fmt", "clippy", "core-test"],
    "Cargo.toml": ["cargo-check", "hook-selftest"],
    ".githooks/pre-commit": ["githooks-selftest"],
    "docs/hooks.md": [],
  };
  const select = (map) => (rel) => map[rel] ?? [];
  // 代表パスの実在検査があるので、フィクスチャの files にも載せる（read は不要＝存在だけを見る）
  const EXTRA = ["snotra-core/src/lib.rs", "Cargo.toml", ".githooks/pre-commit", ".claude/settings.json"];
  const base = { "docs/hooks.md": table(ROWS), ".claude/hooks/post-edit.mjs": hookSrc };
  const run = (contents, map = TRUTH) => checkHookFires(snap(contents, EXTRA), select(map));

  it("緑: 表の全行が selectChecks と一致し、発行されうる id をすべて覆う", () => {
    expect(run(base)).toEqual([]);
  });
  it("不混入: 補足列のバッククォート（`cargo fmt --all`）は検査 id として読まれない", () => {
    // 上の緑がそれを示す。列を跨いだ抽出をしていれば `cargo fmt --all` が id 扱いで赤になる
    expect(run(base).map((f) => f.message)).toEqual([]);
  });
  it("赤: selectChecks が id を足したのに表が追随していない（#858 で実際に起きた形）", () => {
    const f = run(base, { ...TRUTH, "snotra-core/src/lib.rs": ["fmt", "clippy", "core-test", "doc-test"] });
    expect(f.some((x) => x.message.includes("doc-test") && x.message.includes("一致しない"))).toBe(true);
  });
  it("赤: selectChecks が id を消したのに表に残っている", () => {
    const f = run(base, { ...TRUTH, "snotra-core/src/lib.rs": ["fmt", "clippy"] });
    expect(f.some((x) => x.message.includes("core-test"))).toBe(true);
  });
  it("赤: 表側で id を改名した（行の不一致と母集団の欠落の両方で捕まる）", () => {
    const f = run({ ...base, "docs/hooks.md": table([ROWS[0].replace("`core-test`", "`core-tests`"), ...ROWS.slice(1)]) });
    expect(f.some((x) => x.message.includes("一致しない"))).toBe(true);
    expect(f.some((x) => x.message.includes("どの行にも現れない: core-test"))).toBe(true);
  });
  it("赤: 述語が変わって行の割り当てが偽になった（Cargo.toml が hook-selftest を発火しなくなる）", () => {
    const f = run(base, { ...TRUTH, "Cargo.toml": ["cargo-check"] });
    expect(f.some((x) => x.message.includes("Cargo.toml") && x.message.includes("hook-selftest"))).toBe(true);
  });
  it("赤: 順序だけが違う（集合比較なら緑になってしまう代表入力）", () => {
    const f = run({ ...base, "docs/hooks.md": table([ROWS[0].replace("`fmt` `clippy`", "`clippy` `fmt`"), ...ROWS.slice(1)]) });
    expect(f.some((x) => x.message.includes("一致しない"))).toBe(true);
  });
  it("赤: 空集合の行も比較される（（なし）が「検査しない」に化けない）", () => {
    const f = run(base, { ...TRUTH, "docs/hooks.md": ["fmt"] });
    expect(f.some((x) => x.message.includes("docs/hooks.md") && x.message.includes("fmt"))).toBe(true);
  });
  it("赤: 発行されうる id が表のどの行にも現れない（新カテゴリの追加で行ごと漏れた形）", () => {
    const s = { ...base, ".claude/hooks/post-edit.mjs": hookSrc.replace("  return checks;", '  checks.push("wasm-test");\n  return checks;') };
    const f = run(s);
    expect(f.some((x) => x.message.includes("どの行にも現れない: wasm-test"))).toBe(true);
  });
  it("赤: ヘッダ行が見つからない（母集団欠落の明示的な失敗化）", () => {
    const f = run({ ...base, "docs/hooks.md": "# 表を消した\n" });
    expect(f.some((x) => x.message.includes("母集団の欠落"))).toBe(true);
  });
  it("赤: 代表パス列がバッククォート括りの単一パスでない（glob へ戻す退行）", () => {
    const f = run({ ...base, "docs/hooks.md": table(["| `*.rs` 全般 | `fmt` `clippy` | x |", ...ROWS.slice(1)]) });
    expect(f.some((x) => x.message.includes("単一パスでない"))).toBe(true);
  });
  it("赤: 行に代表パス列と検査 id 列がそろっていない", () => {
    const f = run({ ...base, "docs/hooks.md": table(["| `Cargo.toml` |", ...ROWS.slice(1)]) });
    expect(f.some((x) => x.message.includes("そろっていない"))).toBe(true);
  });
  it("赤: 表の途中に表でない行が紛れる（以降が照合されないまま緑になる経路・code-review H1）", () => {
    // startsWith(\"|\") で走査を打ち切る実装だと、この 1 行で以降 2 行の照合が消える
    const f = run({ ...base, "docs/hooks.md": table([ROWS[0], "注: 以下は補助的な行", ...ROWS.slice(1)]) });
    expect(f.some((x) => x.message.includes("表でない行がある"))).toBe(true);
  });
  it("赤: 空集合の行を消すと「沈黙は合格ではない」の主張が黙って消える（id を持たない行は母集団照合に掛からない）", () => {
    const f = run({ ...base, "docs/hooks.md": table(ROWS.slice(0, 3)) });
    expect(f.some((x) => x.message.includes("検査が 1 つも走らないパスの行が無い"))).toBe(true);
  });
  it("赤: 空集合セルが「（なし）」でない（散文・空バッククォートを空集合と読ませない）", () => {
    const f = run({ ...base, "docs/hooks.md": table([...ROWS.slice(0, 3), "| `docs/hooks.md` | 何も走らない | x |"]) });
    expect(f.some((x) => x.message.includes("「（なし）」と書く"))).toBe(true);
  });
  it("赤: 代表パスが実在しない（改名で死んだ行が接頭辞判定を通り続ける）", () => {
    const f = run(
      { ...base, "docs/hooks.md": table([...ROWS.slice(0, 2), "| `.githooks/gone` | `githooks-selftest` | x |", ROWS[3]]) },
      { ...TRUTH, ".githooks/gone": ["githooks-selftest"] },
    );
    expect(f.some((x) => x.message.includes("代表パスが実在しない"))).toBe(true);
  });
  it("赤: ヘッダ行が 2 本ある（例示の表が本物より先に掴まれる経路）", () => {
    const doubled = `${table(ROWS)}\n\n| 編集したファイル（代表パス） | 走る検査 id | 補足 |\n|---|---|---|\n${ROWS[0]}\n`;
    const f = run({ ...base, "docs/hooks.md": doubled });
    expect(f.some((x) => x.message.includes("ヘッダ行が 2 本ある"))).toBe(true);
  });
  it("赤: docs/hooks.md が読めない", () => {
    const f = run({ ".claude/hooks/post-edit.mjs": hookSrc });
    expect(f.some((x) => x.message.includes("docs/hooks.md が読めない"))).toBe(true);
  });
  it("赤: ヘッダはあるが行が 0 件", () => {
    const f = run({ ...base, "docs/hooks.md": table([]) });
    expect(f.some((x) => x.message.includes("行が 0 件"))).toBe(true);
  });
  it("赤: post-edit.mjs が読めない", () => {
    const f = run({ "docs/hooks.md": table(ROWS) });
    expect(f.some((x) => x.message.includes("post-edit.mjs が読めない"))).toBe(true);
  });
  it("赤: checks.push 抽出 0 件（抽出アンカーの腐敗）", () => {
    const f = run({ ...base, ".claude/hooks/post-edit.mjs": "export function selectChecks() { return []; }" });
    expect(f.some((x) => x.message.includes("1 件も抽出できない"))).toBe(true);
  });
  it("CRLF checkout でも緑（\\r が id の一致を外す回帰・#587/#589）", () => {
    expect(run({ ...base, "docs/hooks.md": table(ROWS).replace(/\n/g, "\r\n") })).toEqual([]);
  });
  it("既定引数は実物の selectChecks である（配線のカナリア・fake と区別できる形）", () => {
    // `.claude/settings.json` は fake の TRUTH に**無い**キーなので、fake なら [] を返して赤になる。
    // 実物だけが ["hook-selftest"] を返す——この非対称が「既定引数が実物か」を識別する
    const contents = {
      "docs/hooks.md": table(["| `.claude/settings.json` | `hook-selftest` | x |", ROWS[3]]),
      ".claude/hooks/post-edit.mjs": hookSrc,
    };
    const mismatch = (f) => f.message.includes("一致しない");
    expect(checkHookFires(snap(contents, EXTRA)).filter(mismatch)).toEqual([]);
    expect(checkHookFires(snap(contents, EXTRA), select(TRUTH)).filter(mismatch).length).toBeGreaterThan(0);
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

describe("G-stale-identifiers checkStaleIdentifiers（規範の散文に残る、現行語彙に無い識別子）", () => {
  // 守りたい対象 = #736 の同クラス。UI スタックを入れ替えた後、スキルの散文だけが旧 API 名を
  // 現行 API として指し続ける形。赤フィクスチャは実際に検出された `createObjectURL`（WebView2 期）。
  const DOC = ".claude/skills/x/SKILL.md";
  const base = { "SPEC.md": "# 仕様\n\n本文\n", "src-tauri/src/a.rs": "fn f() { let x = AtomicBool::new(true); }\n" };
  const run = (prose, extra = {}) => checkStaleIdentifiers(snap({ ...base, ...extra, [DOC]: prose }), [DOC]);

  it("現行語彙に無い識別子は finding（赤）", () => {
    const f = run("Blob は `createObjectURL` で作る\n");
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("createObjectURL");
  });

  it("ソースの非コメント本文に在れば finding 無し（緑）", () => {
    expect(run("フラグは `newValue` を見る\n", { "src-tauri/src/a.rs": "let newValue = 1;\n" })).toEqual([]);
  });

  it("ソースのコメントにしか無いものは finding（由来注記を語彙に化けさせない・#736 実測 11 件）", () => {
    const f = run("`resetForShow()` でクリアする\n", { "src-tauri/src/a.rs": "// resetForShow 相当\nlet x = 1;\n" });
    expect(f).toHaveLength(1);
  });

  it("SPEC.md は語彙源ではない（SSOT と写しが同時に鳴る・ADR の却下 4 失効）", () => {
    expect(run("`folderState` を確認する\n", { "SPEC.md": "- `folderState` は直交する\n" })).toHaveLength(1);
  });

  it("テストコードは語彙を寄付しない（検出器自身のフィクスチャが偽陰性を作らない）", () => {
    const f = run("Blob は `createObjectURL` で作る\n", { "scripts/x.test.mjs": "expect(createObjectURL).toBe(1);\n" });
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("createObjectURL");
  });

  it("コマンドは空白を含むので見ない（除外リストを置かない・行粒度のフィルタ無しで成立する）", () => {
    expect(run("母集団は `gh issue list --search x` から取る\n")).toEqual([]);
  });

  it("コマンドと同じ行に単独で書かれた識別子は見る（行粒度フィルタの沈黙経路を閉じた・#993）", () => {
    const f = run("`npm run governance:check` は `createObjectURL` を見る\n");
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("createObjectURL");
  });

  it("判定対象外の不混入: 単語 1 つ・パス・空白入り・コードフェンス内", () => {
    expect(run("`Glob` と `expand` と `docs/a.md` と `git diff main`\n")).toEqual([]);
    expect(run("```\n`createObjectURL`\n```\n")).toEqual([]);
  });

  it("末尾 () の有無で判定が変わらない", () => {
    expect(run("`interpKind()` を見る\n")).toHaveLength(1);
    expect(run("`interpKind` を見る\n")).toHaveLength(1);
  });

  // 赤フィクスチャは実際に検出された `G12_NO_LAUNCHER_READ`（#825 の PR が消した語・SCREAMING_SNAKE
  // 述語が捕まえた実測例）。緑の対 `NO_LAUNCHER_READ` は下の SNAKE_SRC が合成語彙として供給する
  //（実装側の同名定数は #894 で撤去済み——このフィクスチャは実リポジトリに依存しない）。
  const SNAKE_SRC = { "src-tauri/src/a.rs": "const NO_LAUNCHER_READ = 1;\n" };

  it("SCREAMING_SNAKE も見る（赤）", () => {
    const f = run("読まない理由は `G12_NO_LAUNCHER_READ` へ載せる\n", SNAKE_SRC);
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("G12_NO_LAUNCHER_READ");
  });

  it("語彙に在る SCREAMING_SNAKE は鳴らない（緑）", () => {
    expect(run("読まない理由は `NO_LAUNCHER_READ` へ載せる\n", SNAKE_SRC)).toEqual([]);
  });

  it("判定対象外の不混入: `_` を持たない全大文字（こぶ 1 つ以上の要求と同型）", () => {
    expect(run("`CI` と `TODO` と `README`\n")).toEqual([]);
  });

  // 赤フィクスチャは #975 が名指しした**実在の欠陥**——`snotra-core/src/index_tree.rs` の doc が
  // 存在しないテスト名を「実データの全件で固定する」と引いたまま素通りしていた（反復 10）。
  // 緑の対は同じ形で実在する側の名前で、LOWER_SRC が合成語彙として供給する
  //（実リポジトリの語彙に依存させない——#891 が「フィクスチャが偽陰性を作る」で踏んだ経路）。
  const LOWER_SRC = { "snotra-core/src/a.rs": "fn index_tree_file_key_matches_normalize_file_name_key() {}\n" };

  it("lowercase snake_case も見る（赤・#975 の実在の欠陥）", () => {
    const f = run("両腕の一致は `index_tree_file_key_matches_normalize_file_name_key_over_frozen_v6` が固定する\n", LOWER_SRC);
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("over_frozen_v6");
  });

  it("語彙に在る lowercase snake_case は鳴らない（緑）", () => {
    expect(run("両腕の一致は `index_tree_file_key_matches_normalize_file_name_key` が固定する\n", LOWER_SRC)).toEqual([]);
  });

  it("判定対象外の不混入: `_` を持たない小文字 1 語（こぶ 1 つ以上の要求と同型）", () => {
    expect(run("`expand` と `collapse` と `mount`\n")).toEqual([]);
  });

  it("照合件数を返す（「腐りゼロ」と「照合していない」の区別・#497）", () => {
    const r = scanStaleIdentifiers(snap({ ...base, [DOC]: "`someName` と `otherName`\n" }), [DOC]);
    expect(r.checked).toBe(2);
    expect(r.findings).toHaveLength(2);
  });

  it("3 述語は checked を二重計上しない（先頭文字と字種で相互排他ゆえ順に試せる）", () => {
    const r = scanStaleIdentifiers(snap({ ...base, [DOC]: "`someName` と `SOME_NAME` と `some_name`\n" }), [DOC]);
    expect(r.checked).toBe(3);
    expect(r.findings).toHaveLength(3);
  });

  // 赤フィクスチャは #984 の**実在の欠陥**——`docs/comment-guidelines.md` の第一原則が模範例として
  // 指していた関数を同じ PR が削除し、規範の根拠だけが実在しない名前を指す状態になった（#993）。
  // 緑の対は同じ型の現存メンバで、QUALIFIED_SRC が合成語彙として供給する（実リポジトリに依存させない）。
  const QUALIFIED_SRC = { "snotra-core/src/a.rs": "fn from_material() {}\nfn encode_batch_binary() {}\n" };

  it("型で修飾した形は末尾セグメントを見る（赤・#984 の実在の欠陥）", () => {
    const f = run("模範例は `Engine::new_from_cache` である\n", QUALIFIED_SRC);
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("Engine::new_from_cache");
  });

  it("末尾セグメントが語彙に在れば鳴らない（緑）", () => {
    expect(run("模範例は `Engine::from_material` である\n", QUALIFIED_SRC)).toEqual([]);
  });

  it("`.` を含む修飾形も末尾セグメントで判定する（`.` の除外はトークン全体ではなくセグメントへ当てる）", () => {
    expect(run("参照先は `icon.rs::encode_batch_binary` である\n", QUALIFIED_SRC)).toEqual([]);
    expect(run("参照先は `icon.rs::encode_missing_binary` である\n", QUALIFIED_SRC)).toHaveLength(1);
  });

  it("判定対象外の不混入: 型セグメント単語 1 つ・引数つき・パス様の末尾", () => {
    expect(run("`Section::default()` と `snotra-core::Engine`\n", QUALIFIED_SRC)).toEqual([]);
    expect(run("`HistoryStore::load(top_n)` と `Color32::from_rgb(...)`\n", QUALIFIED_SRC)).toEqual([]);
    expect(run("`src-tauri::indexing.rs` を見る\n", QUALIFIED_SRC)).toEqual([]);
  });

  it("修飾形も照合件数を 1 しか進めない（末尾セグメント 1 つだけを見る）", () => {
    const r = scanStaleIdentifiers(snap({ ...base, ...QUALIFIED_SRC, [DOC]: "`Engine::new_from_cache`\n" }), [DOC]);
    expect(r.checked).toBe(1);
  });

  it("読めない文書は母集団の欠落として finding", () => {
    expect(checkStaleIdentifiers(snap(base), ["missing.md"])[0].message).toContain("母集団の欠落");
  });

  it("母集団は skills / rules / agents の md に限る", () => {
    const s = snap({ ".claude/skills/a/SKILL.md": "", ".claude/rules/b.md": "", ".claude/agents/c.md": "", "docs/d.md": "", ".claude/hooks/e.mjs": "" });
    expect(staleIdentifierDocs(s).sort()).toEqual([".claude/agents/c.md", ".claude/rules/b.md", ".claude/skills/a/SKILL.md"]);
  });

  it("語彙は production のソースだけから作られる（SPEC.md も docs もテストも .json も正本ではない）", () => {
    const v = currentVocabulary(
      snap({
        "SPEC.md": "specWord",
        "src-tauri/src/a.rs": "let codeWord = 1;",
        "docs/x.md": "docWord",
        "scripts/a.test.mjs": "testWord",
        "package-lock.json": "lockWord",
      }),
    );
    expect(v).toContain("codeWord");
    expect(v).not.toContain("specWord"); // SPEC.md は語彙源ではなく検査対象である
    expect(v).not.toContain("docWord"); // 一般の docs は語彙の正本ではない
    expect(v).not.toContain("testWord"); // テストコードは「現に動いている実装」ではない
    expect(v).not.toContain("lockWord"); // .json は生成物・依存メタデータを招くので語彙源にしない
  });

  it("`.yml` は語彙を寄付する（CI が実際に実行する実装ゆえ）が、そのコメントは寄付しない", () => {
    const y = { ".github/workflows/ci.yml": "        run: echo x >> $GITHUB_OUTPUT\n" };
    expect(run("`GITHUB_OUTPUT` へ書く\n", y)).toEqual([]);
    expect(run("`GITHUB_OUTPUT` へ書く\n", { ".github/workflows/ci.yml": "# GITHUB_OUTPUT のこと\n" })).toHaveLength(1);
  });

  it("検査対象は規範の散文 + 開発ガイド + 固定パス、母集団の欠落を判ずるのは規範の散文だけ", () => {
    const withProse = snap({ ".claude/rules/b.md": "", "docs/x.md": "", "SPEC.md": "" });
    expect(staleIdentifierTargets(withProse).sort()).toEqual([
      ".claude/rules/b.md",
      "AGENTS.md",
      "CLAUDE.md",
      "SPEC.md",
      "docs/x.md",
      "snotra-settings/SETTINGS-DESIGN.md",
    ]);
    // .claude/** が 1 枚残らず消えても runAll の「対象 md が 0 件」が鳴り続けること。
    // STALE_EXTRA_DOCS や docs/** が長さを埋める側（staleIdentifierDocs）へ混ざるとこの検知は永久に沈黙する
    const noProse = snap({ "SPEC.md": "", "docs/x.md": "" });
    expect(staleIdentifierDocs(noProse)).toEqual([]);
    expect(staleIdentifierGuideDocs(noProse)).toEqual(["docs/x.md"]);
  });

  it("開発ガイドの母集団は docs/** から歴史記録（adr / superpowers）だけを外す", () => {
    const s = snap({
      "docs/architecture.md": "",
      "docs/design/2026-05-31-x.md": "", // 日付を持つが status: Agreed で architecture.md が現在形で指す先
      "docs/adr/ADR-x.md": "", // 却下案＝もう存在しない案
      "docs/superpowers/specs/y.md": "", // #589 で非規範化された当時の設計
      "docs/z.txt": "",
      "PERFORMANCE.md": "",
    });
    expect(staleIdentifierGuideDocs(s).sort()).toEqual(["docs/architecture.md", "docs/design/2026-05-31-x.md"]);
  });
});

// 既存の `runAll（空母集団の明示 fail）` は `snap({})` に対し `findings.length > 0` しか見ないため、
// **どのガードが鳴ったかを区別しない**——`docs/**` 専用の 0 件検知を丸ごと消しても緑で通る（実測）。
// ここは兄弟母集団（`.claude/**` と `STALE_EXTRA_DOCS` の固定パス）を**非空に保ったまま**
// `docs/**` だけを空にして、その 1 件が鳴ることを固定する。
describe("G-stale-identifiers の母集団ごとの 0 件検知（兄弟が非空でも鳴る）", () => {
  const siblings = {
    ".claude/rules/b.md": "",
    "SPEC.md": "",
    "CLAUDE.md": "",
    "AGENTS.md": "",
    "snotra-settings/SETTINGS-DESIGN.md": "",
    "src-tauri/src/a.rs": "let x = 1;\n",
  };
  const guideMisses = (contents) =>
    runAll(snap(contents)).findings.filter((f) => f.message.includes("開発ガイド（docs/**）が 0 件"));

  it("docs/** が 0 件なら鳴る——兄弟母集団が埋まっていても", () => {
    expect(staleIdentifierDocs(snap(siblings))).not.toEqual([]); // 兄弟は非空（この検知の独立性の前提）
    expect(guideMisses(siblings)).toHaveLength(1);
  });

  it("docs/** が在れば鳴らない", () => {
    expect(guideMisses({ ...siblings, "docs/architecture.md": "" })).toHaveLength(0);
  });

  it("docs/adr/ と docs/superpowers/ だけでは埋まらない（歴史記録は母集団ではない）", () => {
    expect(guideMisses({ ...siblings, "docs/adr/ADR-x.md": "", "docs/superpowers/specs/y.md": "" })).toHaveLength(1);
  });
});

// #984 の実在の欠陥を凍結したフィクスチャ。`f1827b0:docs/comment-guidelines.md` の当該行の骨格で、
// **同じ行に `npm` のコマンドと修飾形の名前が同居する**——2 つの穴が同時に効いていた形である（#993）。
// 片足ずつ戻した測定は `ADR-stale-identifier-detector-scope` の #993 の追記節が持つ（テストからは
// 実装を変異させられないため、ここで固定するのは「両足そろえば赤い」ことだけである）。
describe("G-stale-identifiers の凍結フィクスチャ（#984 の実在の欠陥）", () => {
  const DOC = "docs/comment-guidelines.md";
  const SRC = { "snotra-core/src/a.rs": "fn from_material() {}\n" };
  const LINE = (name) => `模範例は \`Engine::${name}\`。\`npm run governance:check\` が見る\n`;

  it("コマンドと同居する行の修飾形が腐っていれば赤", () => {
    const f = checkStaleIdentifiers(snap({ ...SRC, [DOC]: LINE("new_from_cache") }), [DOC]);
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("Engine::new_from_cache");
  });

  it("緑の対: 同じ形で末尾セグメントが語彙に在れば鳴らない", () => {
    expect(checkStaleIdentifiers(snap({ ...SRC, [DOC]: LINE("from_material") }), [DOC])).toEqual([]);
  });
});

// `checkReferences` の既定引数は「何も免除しない」ので、`buildChecks` が実物を渡し忘れると
// gitignore 済みファイルの誤爆が戻る（#1088 で解いた当の欠陥）。この describe だけがそれを縛る。
describe("G-references の配線（buildChecks が gitignore 判定を渡す）", () => {
  const wired = (contents) => buildChecks(snap(contents), {}).find((c) => c.id === "G-references").run();
  it("ignore 対象の不在パスは buildChecks 経由で緑になる", () => {
    // `AGENTS.md` は governanceDocs の母集団に入る（固定パス）
    const f = wired({ "AGENTS.md": "記録は `test-results/.last-run.json` に出る\n" });
    expect(f.filter((x) => x.message.includes("test-results/.last-run.json"))).toEqual([]);
  });
  it("非 ignore の typo は buildChecks 経由でも赤（免除が広がっていない）", () => {
    const f = wired({ "AGENTS.md": "`docs/typo-nonexistent.md` を見よ\n" });
    expect(f.some((x) => x.message.includes("docs/typo-nonexistent.md"))).toBe(true);
  });
});

// 上の describe が固定するのは各関数の戻り値までで、**buildChecks がどちらを検査へ渡すか**は見ていない。
// `staleTargets` を `staleDocs` へ戻しても実リポジトリの finding は 0 / 照合 1 のまま変わらないため、
// dogfood テストも証跡の印字も気づけない——軸 B の主張（SPEC.md が検査対象になった）を守るのは
// この describe だけである（配線を戻すと下の赤フィクスチャが緑に落ちる）。
describe("G-stale-identifiers の配線（buildChecks が SPEC.md を検査対象として渡す）", () => {
  const wired = (contents) => buildChecks(snap(contents), {}).find((c) => c.id === "G-stale-identifiers").run();
  // 母集団の欠落 finding を避けるための最小の母集団（規範の散文 1 本 + 開発ガイド 1 本 +
  // `SPEC.md` 以外の固定パス 3 本。固定パスは実在を問わず検査対象なので、欠けると鳴る）
  const prose = {
    ".claude/rules/b.md": "",
    "docs/architecture.md": "",
    "CLAUDE.md": "",
    "AGENTS.md": "",
    "snotra-settings/SETTINGS-DESIGN.md": "",
  };

  it("SPEC.md の現行語彙に無い識別子は finding（赤）", () => {
    const f = wired({ ...prose, "SPEC.md": "- `deadCamelWord` を使う\n", "src-tauri/src/a.rs": "let x = 1;\n" });
    expect(f).toHaveLength(1);
    expect(f[0].file).toBe("SPEC.md");
    expect(f[0].message).toContain("deadCamelWord");
  });

  it("SPEC.md の識別子がソースの非コメント本文に在れば finding 無し（緑）", () => {
    expect(wired({ ...prose, "SPEC.md": "- `liveCamelWord` を使う\n", "src-tauri/src/a.rs": "let liveCamelWord = 1;\n" })).toEqual([]);
  });

  it("判定対象外の不混入: SPEC.md のフェンス内・コマンド span 内・単語 1 つ", () => {
    const spec = "```\n`fencedCamelWord`\n```\n- `gh pr view --json argCamelWord` を見る\n- `expand` する\n";
    expect(wired({ ...prose, "SPEC.md": spec, "src-tauri/src/a.rs": "let x = 1;\n" })).toEqual([]);
  });
});

// 母集団を広げた側も同じ穴を持つ——配線を戻しても実リポジトリの finding は動かないため、
// dogfood テストも証跡の印字も気づけない。**射程拡大の主張を守るのはこの describe だけである。**
describe("G-stale-identifiers の配線（buildChecks が開発ガイドと固定パス文書を検査対象として渡す）", () => {
  const wired = (contents) => buildChecks(snap(contents), {}).find((c) => c.id === "G-stale-identifiers").run();
  // STALE_EXTRA_DOCS の 4 本と docs/** を最小で埋める（欠けると「母集団の欠落」が混じる）
  const base = {
    ".claude/rules/b.md": "",
    "docs/architecture.md": "",
    "SPEC.md": "",
    "CLAUDE.md": "",
    "AGENTS.md": "",
    "snotra-settings/SETTINGS-DESIGN.md": "",
    "src-tauri/src/a.rs": "let x = 1;\n",
  };
  const rot = "- `deadCamelWord` を使う\n";

  it("開発ガイド（docs/**）の腐りは finding（赤）", () => {
    const f = wired({ ...base, "docs/hooks.md": rot });
    expect(f).toHaveLength(1);
    expect(f[0].file).toBe("docs/hooks.md");
  });

  it("固定パス文書 3 本も検査対象（赤）", () => {
    for (const doc of ["CLAUDE.md", "AGENTS.md", "snotra-settings/SETTINGS-DESIGN.md"]) {
      const f = wired({ ...base, [doc]: rot });
      expect(f, doc).toHaveLength(1);
      expect(f[0].file, doc).toBe(doc);
    }
  });

  it("判定対象外の不混入: 歴史記録（docs/adr/・docs/superpowers/）は検査されない", () => {
    expect(wired({ ...base, "docs/adr/ADR-x.md": rot })).toEqual([]);
    expect(wired({ ...base, "docs/superpowers/specs/y.md": rot })).toEqual([]);
  });

  it("SCREAMING_SNAKE の腐りも配線を通って届く（#825 が消した実在の語）", () => {
    const f = wired({ ...base, "docs/hooks.md": "- `G12_NO_LAUNCHER_READ` へ載せる\n" });
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("G12_NO_LAUNCHER_READ");
  });
});

describe("G-near-heading-refs checkNearHeadingRefs（正準形に見えて隣接していない見出し参照・#727）", () => {
  // 守りたい対象 = #725 で実際に書かれた `/start-issue` は「Step 6 — …」の形。
  // 人の目には正準形に見え、G-heading-refs の視界外で、参照先が改番されれば黙って壊れる。
  const TARGET = { "CLAUDE.md": "## Git/GitHub 運用\n\n本文\n" };
  const run = (prose, extra = {}) => checkNearHeadingRefs(snap({ ...TARGET, ...extra, "docs/x.md": prose }), ["docs/x.md"]);

  it("助詞が 1 つ挟まった参照は finding（赤）", () => {
    const f = run("詳細は `CLAUDE.md` の「Git/GitHub 運用」を見よ\n");
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("`CLAUDE.md`「Git/GitHub 運用」と書く");
  });

  it("隣接形は G-heading-refs の担当なので見ない（二重報告しない）", () => {
    expect(run("詳細は `CLAUDE.md`「Git/GitHub 運用」を見よ\n")).toEqual([]);
  });

  it("節番号つきの隣接形も G-heading-refs の担当（#727 実測: 番号を許さないと直せない指摘が残る）", () => {
    const s = { "SPEC.md": "### 見た目の規範\n" };
    expect(checkNearHeadingRefs(snap({ ...s, "docs/x.md": "`SPEC.md` §11「見た目の規範」\n" }), ["docs/x.md"])).toEqual([]);
  });

  it("節番号 + 助詞は finding で、直し方が節番号を落とさない", () => {
    const s = { "SPEC.md": "### 見た目の規範\n" };
    const f = checkNearHeadingRefs(snap({ ...s, "docs/x.md": "`SPEC.md` §11 の「見た目の規範」\n" }), ["docs/x.md"]);
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("`SPEC.md` §11「見た目の規範」と書く");
  });

  it("着地しない引用は見ない（散文と参照を分ける要）", () => {
    expect(run("`CLAUDE.md`（ルート）は「何を実現すべきか」を記す\n")).toEqual([]);
  });

  it("窓幅を超えて離れたものは見ない", () => {
    expect(run("`CLAUDE.md` はとても長い説明を挟んでから「Git/GitHub 運用」\n")).toEqual([]);
  });

  it("判定対象外の不混入: 参照でないバッククォート・コードフェンス内", () => {
    expect(run("`someVar` の「Git/GitHub 運用」\n")).toEqual([]);
    expect(run("```\n`CLAUDE.md` の「Git/GitHub 運用」\n```\n")).toEqual([]);
  });

  it("照合件数を返す（「差分ゼロ」と「照合していない」の区別・#497）", () => {
    const r = scanNearHeadingRefs(snap({ ...TARGET, "docs/x.md": "`CLAUDE.md` の「Git/GitHub 運用」と `CLAUDE.md` の「無い見出し」\n" }), ["docs/x.md"]);
    expect(r.checked).toBe(2);
    expect(r.findings).toHaveLength(1);
  });
});

describe("G-check-skill-enumeration checkCheckSkillEnumeration（4a の列挙 ↔ AGENTS.md 表・#778）", () => {
  // 守りたい対象 = 表へ check スキルを足した人が /implement 4a を直さず、
  // 新しいスキルが報告母集団から沈黙して落ちる形。
  const mk = (tableSkills, step4aSkills, files = []) =>
    snap(
      {
        "AGENTS.md": `## 条件別チェック（トリガー → 参照先）\n\n| t | ${tableSkills.join(" ")} |\n\n## 次節\n`,
        ".claude/skills/implement/SKILL.md": `## Step 4\n\n### 4a. check スキルの実行\n\n変更に応じて ${step4aSkills.join("・")} を実行する。\n\n### 4b. x\n`,
      },
      files,
    );
  const SKILLS = ["/dry-check", "/race-check"].map((s) => `.claude/skills/${s.slice(1)}/SKILL.md`);

  it("集合が一致すれば findings 無し（緑）", () => {
    expect(checkCheckSkillEnumeration(mk(["`/dry-check`", "`/race-check`"], ["`/race-check`", "`/dry-check`"], SKILLS))).toEqual([]);
  });

  it("赤: 表に在って 4a に無い（報告母集団から沈黙して落ちる）", () => {
    const f = checkCheckSkillEnumeration(mk(["`/dry-check`", "`/race-check`"], ["`/dry-check`"], SKILLS));
    expect(f.some((x) => x.message.includes("/race-check") && x.message.includes("4a の列挙に無い"))).toBe(true);
  });

  it("赤: 4a に在って表に無い（起動条件を持たない検査）", () => {
    const f = checkCheckSkillEnumeration(mk(["`/dry-check`"], ["`/dry-check`", "`/race-check`"], SKILLS));
    expect(f.some((x) => x.message.includes("/race-check") && x.message.includes("表に無い"))).toBe(true);
  });

  it("赤: 列挙されたスキルが実在しない（誤記）", () => {
    const f = checkCheckSkillEnumeration(mk(["`/typo-check`"], ["`/typo-check`"], []));
    expect(f.some((x) => x.message.includes("実在しない"))).toBe(true);
  });

  it("判定対象外の不混入: `-check` で終わらないスキルは母集団に入らない", () => {
    // 表にだけ /plan-review が在っても、4a に無いことを咎めない
    const s = mk(["`/dry-check`", "`/plan-review`"], ["`/dry-check`"], SKILLS);
    expect(checkCheckSkillEnumeration(s)).toEqual([]);
  });

  it("赤: 見出しが変わって節を切り出せない（沈黙で通さない）", () => {
    const s = snap({ "AGENTS.md": "## 別の見出し\n", ".claude/skills/implement/SKILL.md": "### 4a. x\n`/dry-check`\n" });
    expect(checkCheckSkillEnumeration(s).some((x) => x.message.includes("見つからない"))).toBe(true);
  });

  it("赤: 空母集団は明示 fail（沈黙経路の閉塞）", () => {
    const f = checkCheckSkillEnumeration(mk([], [], []));
    expect(f.some((x) => x.message.includes("母集団の欠落"))).toBe(true);
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

describe("G-adr-file-names（ADR のファイル名と見出しの形・#816）", () => {
  // 守りたい対象 = `docs/adr/foo.md` や連番への逆戻り。G-adr-citations は引用側しか見ないので、
  // 誰も引用しなければ逸脱が静かに通る（#789 の見直しで残余として特定）。
  const ok = { "docs/adr/ADR-plan-ownership-boundary.md": "# ADR-plan-ownership-boundary: 計画の所有境界\n" };

  it("形が揃っていれば findings 無し（緑）", () => {
    expect(checkAdrFileNames(snap(ok))).toEqual([]);
  });

  it("赤: 連番へ戻る（#812 が廃した形）", () => {
    const f = checkAdrFileNames(snap({ ...ok, "docs/adr/0019-foo.md": "# ADR-0019: x\n" }));
    expect(f.some((x) => x.message.includes("0019-foo.md"))).toBe(true);
  });

  it("赤: ADR- 前置が無い", () => {
    const f = checkAdrFileNames(snap({ ...ok, "docs/adr/foo.md": "# ADR-foo: x\n" }));
    expect(f.some((x) => x.message.includes("foo.md"))).toBe(true);
  });

  it("赤: 見出しがファイル名と食い違う（stem = 引用文字列の対応が崩れる）", () => {
    const f = checkAdrFileNames(snap({ "docs/adr/ADR-alpha.md": "# ADR-beta: x\n" }));
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("食い違う");
  });

  it("赤: 冒頭が `# ADR-<slug>:` の形でない", () => {
    const f = checkAdrFileNames(snap({ "docs/adr/ADR-alpha.md": "# 計画の所有境界\n" }));
    expect(f[0].message).toContain("形でない");
  });

  it("カナリア: 空母集団は明示 fail（走査が空でも「逸脱なし」に見える沈黙経路を塞ぐ）", () => {
    const f = checkAdrFileNames(snap({ "CLAUDE.md": "" }));
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("母集団の欠落");
  });

  it("判定対象外の不混入: docs/adr/ 直下の md だけを見る", () => {
    const s = snap({ ...ok, "docs/adr/sub/0001-x.md": "", "docs/architecture.md": "", "docs/adr/notes.txt": "" });
    expect(adrFiles(s)).toEqual(["docs/adr/ADR-plan-ownership-boundary.md"]);
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
