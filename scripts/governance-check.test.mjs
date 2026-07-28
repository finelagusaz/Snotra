// governance-check.mjs の検査関数を、フォールトインジェクションフィクスチャ（赤）と正常フィクスチャ（緑）の
// 両方向で検証する。各フィクスチャは「守りたい対象 1 件が入力に現れること」と
// 「判定対象外が入力に混じらないこと」の入力集合検算を兼ねる（.claude/rules/safety-nets.md）。
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it, expect } from "vitest";
import {
  G1_CRATES,
  governanceDocs,
  makeSnapshot,
  checkHookCommands,
  checkModuleIndex,
  checkArchitectureTable,
  checkReferences,
  checkSpecSections,
  checkBuildCommands,
  checkCiTable,
  checkRulesGlobs,
  checkSkillTable,
  globToRegex,
  checkNormativeAreaBudget,
  normativeArea,
  AREA_BUDGET,
  ALWAYS_LOADED_FILES,
  checkHeadingRefs,
  collectAnchors,
  headingRefDocs,
  checkConfigFieldReachability,
  G12_NO_LAUNCHER_READ,
  checkNearHeadingRefs,
  scanNearHeadingRefs,
  checkCheckSkillEnumeration,
  checkStaleIdentifiers,
  scanStaleIdentifiers,
  staleIdentifierDocs,
  currentVocabulary,
  runAll,
} from "./governance-check.mjs";

/** 最小スナップショット: files はリポジトリ相対（"/" 区切り）、contents は path → 本文 */
function snap(contents, extraFiles = []) {
  const files = [...Object.keys(contents), ...extraFiles];
  return { files, read: (p) => contents[p] ?? null };
}

describe("globToRegex（G7 の意味論固定・代表入力）", () => {
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

// G1/G3 の母集団は手で列挙する定数であり、**crate を足しても何も鳴らない**（沈黙する経路）。
// `snotra-egui-runtime` は「#532 の検証層」として作られたまま両方から漏れ、SU7 で製品の
// 描画層になった後も索引ドリフト・参照切れが検知されない状態が続いていた（#701）。
// 実 `Cargo.toml` を読み、CLAUDE.md を持つ member が両母集団に載っていることを固定する。
describe("G1/G3 母集団カナリア — #701", () => {
  it("CLAUDE.md を持つ workspace member は G1_CRATES と governanceDocs の両方に載る", () => {
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
        Object.keys(G1_CRATES),
        `${crate} が G1_CRATES に無い。src/exts を添えて追加すること（索引ドリフトが沈黙で通る）`,
      ).toContain(crate);
      expect(
        docs,
        `${crate}/CLAUDE.md が G3 母集団に無い。governanceDocs の正規表現を更新すること（参照切れが沈黙で通る）`,
      ).toContain(`${crate}/CLAUDE.md`);
    }
  });
});

describe("G1 checkModuleIndex", () => {
  const base = {
    "snotra-core/CLAUDE.md": "# x\n## モジュール構成\n- `lib.rs` — エントリ\n- `search.rs` — 検索\n\n## 次節\n",
    "snotra-core/src/lib.rs": "",
    "snotra-core/src/search.rs": "",
  };
  it("緑: 索引と実ファイルが双方向で一致する", () => {
    const s = snap(base);
    expect(checkModuleIndex(s, ["snotra-core"])).toEqual([]);
  });
  it("赤（順方向）: 索引に実在しないファイルが載っている", () => {
    const s = snap({ ...base, "snotra-core/CLAUDE.md": base["snotra-core/CLAUDE.md"].replace("`search.rs`", "`gone.rs`") });
    const f = checkModuleIndex(s, ["snotra-core"]);
    expect(f.some((x) => x.message.includes("gone.rs"))).toBe(true);
  });
  it("赤（逆方向）: 実ファイルが索引に載っていない", () => {
    const s = snap(base, ["snotra-core/src/orphan.rs"]);
    const f = checkModuleIndex(s, ["snotra-core"]);
    expect(f.some((x) => x.message.includes("orphan.rs"))).toBe(true);
  });
  it("集約行のベア名列挙（`mod.rs` 等）は basename 照合で誤検出しない", () => {
    const s = snap({
      "src-tauri/CLAUDE.md": "## モジュール構成\n- `commands/`: 分割（`mod.rs` + `search.rs`）\n## 次節\n",
      "src-tauri/src/commands/mod.rs": "",
      "src-tauri/src/commands/search.rs": "",
    });
    expect(checkModuleIndex(s, ["src-tauri"])).toEqual([]);
  });
});

describe("G2 checkArchitectureTable", () => {
  it("緑: ファイル単位のモジュール表が無い", () => {
    const s = snap({ "docs/architecture.md": "# a\n| 型 | 役割 |\n|---|---|\n| `Engine` | 入口 |\n" });
    expect(checkArchitectureTable(s)).toEqual([]);
  });
  it("赤: 先頭セルがバッククォート付きファイル名の表行を検出する", () => {
    const s = snap({ "docs/architecture.md": "| `engine.rs` | 検索エンジン |\n" });
    const f = checkArchitectureTable(s);
    expect(f.some((x) => x.message.includes("engine.rs"))).toBe(true);
  });
  it("コードフェンス内の表行は無視する", () => {
    const s = snap({ "docs/architecture.md": "```\n| `engine.rs` | x |\n```\n" });
    expect(checkArchitectureTable(s)).toEqual([]);
  });
});

describe("G3 checkReferences", () => {
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
});

describe("G4 checkSpecSections", () => {
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

describe("G5 checkBuildCommands", () => {
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
});

describe("G6 checkCiTable", () => {
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

describe("G7 checkRulesGlobs", () => {
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

describe("G8 checkSkillTable（表の対象は roster に載らない skill だけ）", () => {
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

describe("G9 checkHookCommands", () => {
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

describe("runAll（空母集団の明示 fail = 沈黙経路の閉塞）", () => {
  it("対象文書・rules・skills が空なら findings を返す", () => {
    const s = snap({});
    const { findings } = runAll(s);
    expect(findings.length).toBeGreaterThan(0);
  });
});

describe("G10 checkNormativeAreaBudget（二面独立 ratchet・文字数指標・#593 / ADR-0005）", () => {
  const x = (n) => "x".repeat(n);
  const rule = (p, n) => ({ [`.claude/rules/${p}`]: x(n) });
  const skill = (name, desc) => ({
    [`.claude/skills/${name}/SKILL.md`]: `---\nname: ${name}\ndescription: "${desc}"\n---\n本文\n`,
  });
  const base = { ...rule("a.md", 1), ...skill("s", "d") };

  it("両面とも基準以内なら findings 無し（緑）", () => {
    const s = snap({ "CLAUDE.md": x(100), "AGENTS.md": x(100), ...base });
    expect(checkNormativeAreaBudget(s)).toEqual([]);
  });

  it("常時ロードが基準超過なら finding（赤）", () => {
    const s = snap({ "CLAUDE.md": x(AREA_BUDGET.alwaysLoaded + 1), "AGENTS.md": "", ...base });
    const f = checkNormativeAreaBudget(s);
    expect(f.some((v) => v.message.includes("常時ロード規範") && v.message.includes("> 基準"))).toBe(true);
  });

  it("面替えでは下がらない（rules へ移せば rules 側が超過する）", () => {
    const s = snap({ "CLAUDE.md": x(10), "AGENTS.md": x(10), ...skill("s", "d"), ...rule("a.md", AREA_BUDGET.rules + 1) });
    const f = checkNormativeAreaBudget(s);
    expect(f.some((v) => v.message.includes("rules 合計") && v.message.includes("> 基準"))).toBe(true);
  });

  it("改行を畳んでも面積は改行のぶんしか下がらない（行数指標の誤った勾配を絶つ・ADR-0005 の核心）", () => {
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
    expect(checkNormativeAreaBudget(shortDesc).some((v) => v.file === ".claude/skills")).toBe(false);
  });

  it("description が 1 行スカラーでなければ finding（数えられない沈黙経路の閉塞）", () => {
    const s = snap({
      "CLAUDE.md": "",
      "AGENTS.md": "",
      ...rule("a.md", 1),
      ".claude/skills/s/SKILL.md": "---\nname: s\ndescription: |\n  複数行\n---\n",
    });
    const f = checkNormativeAreaBudget(s);
    expect(f.some((v) => v.message.includes("1 行スカラーでない"))).toBe(true);
  });

  it("常時ロード文書が読めなければ母集団欠落 finding（沈黙経路の閉塞）", () => {
    const s = snap({ "AGENTS.md": x(1), ...base }); // CLAUDE.md 欠落
    const f = checkNormativeAreaBudget(s);
    expect(f.some((v) => v.file === "CLAUDE.md" && v.message.includes("母集団の欠落"))).toBe(true);
  });

  it("rules / skills が 0 件なら母集団欠落 finding（グロブ破損の沈黙経路の閉塞）", () => {
    const noRules = snap({ "CLAUDE.md": x(1), "AGENTS.md": x(1), ...skill("s", "d") });
    expect(checkNormativeAreaBudget(noRules).some((v) => v.file === ".claude/rules")).toBe(true);
    const noSkills = snap({ "CLAUDE.md": x(1), "AGENTS.md": x(1), ...rule("a.md", 1) });
    expect(checkNormativeAreaBudget(noSkills).some((v) => v.file === ".claude/skills")).toBe(true);
  });

  it("ALWAYS_LOADED_FILES はルート直下の 2 文書", () => {
    expect(ALWAYS_LOADED_FILES).toEqual(["CLAUDE.md", "AGENTS.md"]);
  });
});

describe("G11 checkHeadingRefs（見出し参照の実在）", () => {
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

  it("母集団は履歴資料と作業バッファを除く全 md", () => {
    const s = snap({
      "PERFORMANCE.md": "",
      ".claude/agents/code-reviewer.md": "",
      "docs/superpowers/plans/p.md": "",
      "workspace/plan.md": "",
      "src/main.rs": "",
    });
    expect(headingRefDocs(s).sort()).toEqual([".claude/agents/code-reviewer.md", "PERFORMANCE.md"]);
  });
});

describe("G13 checkStaleIdentifiers（規範の散文に残る、現行語彙に無い識別子）", () => {
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

  it("SPEC.md の語彙は腐りではない（写しを SSOT より先に直させない・#735）", () => {
    expect(run("`folderState` を確認する\n", { "SPEC.md": "- `folderState` は直交する\n" })).toEqual([]);
  });

  it("外部ツールのコマンドが同じ行に在れば、その行は見ない（除外リストを置かない）", () => {
    expect(run("母集団は `closingIssuesReferences` ではなく `gh issue list --search x` から取る\n")).toEqual([]);
  });

  it("判定対象外の不混入: 単語 1 つ・パス・空白入り・コードフェンス内", () => {
    expect(run("`Glob` と `expand` と `docs/a.md` と `git diff main`\n")).toEqual([]);
    expect(run("```\n`createObjectURL`\n```\n")).toEqual([]);
  });

  it("末尾 () の有無で判定が変わらない", () => {
    expect(run("`interpKind()` を見る\n")).toHaveLength(1);
    expect(run("`interpKind` を見る\n")).toHaveLength(1);
  });

  it("照合件数を返す（「腐りゼロ」と「照合していない」の区別・#497）", () => {
    const r = scanStaleIdentifiers(snap({ ...base, [DOC]: "`someName` と `otherName`\n" }), [DOC]);
    expect(r.checked).toBe(2);
    expect(r.findings).toHaveLength(2);
  });

  it("読めない文書は母集団の欠落として finding", () => {
    expect(checkStaleIdentifiers(snap(base), ["missing.md"])[0].message).toContain("母集団の欠落");
  });

  it("母集団は skills / rules / agents の md に限る", () => {
    const s = snap({ ".claude/skills/a/SKILL.md": "", ".claude/rules/b.md": "", ".claude/agents/c.md": "", "docs/d.md": "", ".claude/hooks/e.mjs": "" });
    expect(staleIdentifierDocs(s).sort()).toEqual([".claude/agents/c.md", ".claude/rules/b.md", ".claude/skills/a/SKILL.md"]);
  });

  it("語彙は SPEC.md とソースの両方から作られる", () => {
    const v = currentVocabulary(snap({ "SPEC.md": "specWord", "src-tauri/src/a.rs": "let codeWord = 1;", "docs/x.md": "docWord" }));
    expect(v).toContain("specWord");
    expect(v).toContain("codeWord");
    expect(v).not.toContain("docWord"); // 一般の docs は語彙の正本ではない
  });
});

describe("G14 checkNearHeadingRefs（正準形に見えて隣接していない見出し参照・#727）", () => {
  // 守りたい対象 = #725 で実際に書かれた `/start-issue` は「Step 6 — …」の形。
  // 人の目には正準形に見え、G11 の視界外で、参照先が改番されれば黙って壊れる。
  const TARGET = { "CLAUDE.md": "## Git/GitHub 運用\n\n本文\n" };
  const run = (prose, extra = {}) => checkNearHeadingRefs(snap({ ...TARGET, ...extra, "docs/x.md": prose }), ["docs/x.md"]);

  it("助詞が 1 つ挟まった参照は finding（赤）", () => {
    const f = run("詳細は `CLAUDE.md` の「Git/GitHub 運用」を見よ\n");
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("`CLAUDE.md`「Git/GitHub 運用」と書く");
  });

  it("隣接形は G11 の担当なので見ない（二重報告しない）", () => {
    expect(run("詳細は `CLAUDE.md`「Git/GitHub 運用」を見よ\n")).toEqual([]);
  });

  it("節番号つきの隣接形も G11 の担当（#727 実測: 番号を許さないと直せない指摘が残る）", () => {
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

describe("G15 checkCheckSkillEnumeration（4a の列挙 ↔ AGENTS.md 表・#778）", () => {
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

describe("G12 checkConfigFieldReachability", () => {
  const config = [
    "#[derive(Deserialize)]",
    "pub struct VisualConfig {",
    "    pub background_color: String,",
    "    pub preset: ThemePreset,",
    "}",
  ].join("\n");
  const base = {
    "snotra-core/src/config.rs": config,
    "snotra-core/src/opener.rs": "",
    "src-tauri/src/view.rs": "let c = &v.background_color;",
  };
  const table = { "VisualConfig.preset": "設定 UI のみが読む（フィクスチャ）" };
  const structs = ["VisualConfig"];
  const check = (contents, t = table, extra = []) => checkConfigFieldReachability(snap(contents, extra), t, structs);

  it("緑: ランチャが読まないフィールドの集合と表が一致する", () => {
    expect(check(base)).toEqual([]);
  });
  it("赤（逆方向）: 読まれないフィールドが表に無い", () => {
    expect(check(base, {}).some((x) => x.message.includes("VisualConfig.preset"))).toBe(true);
  });
  it("赤（順方向）: 表にあるが実際は読まれている", () => {
    const f = check({ ...base, "src-tauri/src/view.rs": "let c = &v.background_color; let p = &v.preset;" });
    expect(f.some((x) => x.message.includes("表の記載が古い"))).toBe(true);
  });
  it("赤（順方向）: 表のキーが config.rs に実在しない", () => {
    const f = check(base, { ...table, "VisualConfig.gone": "x" });
    expect(f.some((x) => x.message.includes("VisualConfig.gone"))).toBe(true);
  });
  it("判定対象外の不混入: コメント内の読みを数えない（`preset` が doc コメントへ埋もれる実測・opener.rs）", () => {
    expect(check({ ...base, "src-tauri/src/other.rs": "/// see v.preset for details\n// let p = &v.preset;\n" })).toEqual([]);
  });
  it("判定対象外の不混入: `#[cfg(test)]` 内の読みを数えない（`visible_rows` が engine.rs のテスト 4 件で落ちた実測）", () => {
    expect(check({ ...base, "src-tauri/src/other.rs": "#[cfg(test)]\nmod tests {\n let p = &v.preset;\n}\n" })).toEqual([]);
  });
  it("判定対象外の不混入: `#[cfg(test)]` 以降の struct は母集団に入らない", () => {
    const withTests = `${config}\n#[cfg(test)]\nmod tests {\n#[derive(Deserialize)]\npub struct Fixture {\n    pub never_read: u8,\n}\n}\n`;
    expect(check({ ...base, "snotra-core/src/config.rs": withTests })).toEqual([]);
  });
  it("判定対象外の不混入: `Deserialize` を derive しない struct は母集団に入らない（`OpenerPreset` 等）", () => {
    const withPlain = `${config}\n\n/// 検出結果であって config のキーではない\npub struct OpenerPreset {\n    pub never_read: u8,\n}\n`;
    expect(check({ ...base, "snotra-core/src/config.rs": withPlain })).toEqual([]);
  });
  it("CRLF でも derive 判定のブロック切り出しが壊れない（CI の Windows checkout・autocrlf=true で実測）", () => {
    const crlf = (s) => s.replaceAll("\n", "\r\n");
    // Deserialize を持たない struct が後ろに続く形。空行分割が壊れると母集団へ混じって赤になる
    const withPlain = crlf(`${config}\n\n/// config のキーではない\npub struct OpenerPreset {\n    pub never_read: u8,\n}\n`);
    expect(check({ ...base, "snotra-core/src/config.rs": withPlain })).toEqual([]);
  });
  it("赤: 期待する struct が抽出できない（抽出アンカーの部分腐敗）", () => {
    const f = checkConfigFieldReachability(snap(base), table, ["VisualConfig", "GoneConfig"]);
    expect(f.some((x) => x.message.includes("部分腐敗"))).toBe(true);
  });
  it("母集団の欠落: config.rs が読めない", () => {
    expect(check({ "src-tauri/src/view.rs": "" }).some((x) => x.message.includes("母集団の欠落"))).toBe(true);
  });
  it("母集団の欠落: フィールドが 1 件も抽出できない", () => {
    const f = check({ "snotra-core/src/config.rs": "// no struct\n", "snotra-core/src/opener.rs": "", "src-tauri/src/view.rs": "" });
    expect(f.some((x) => x.message.includes("母集団の欠落"))).toBe(true);
  });
  it("母集団の欠落: ランチャ側ソースが 0 件", () => {
    const f = check({ "snotra-core/src/config.rs": config, "snotra-core/src/opener.rs": "" });
    expect(f.some((x) => x.message.includes("母集団の欠落"))).toBe(true);
  });
});

// safety-nets.md「検査の入力集合を、具体対象で検算する」— 守りたい対象 1 件が実リポジトリの
// 入力に現れることを固定する。フィクスチャだけでは「実リポジトリでは何も見ていない検査」が緑で通る。
describe("G12 カナリア — 守りたい対象が実リポジトリの入力に現れる", () => {
  it("`VisualConfig.preset` を表から外すと実リポジトリで赤になる", () => {
    const s = makeSnapshot(fileURLToPath(new URL("..", import.meta.url)));
    const without = { ...G12_NO_LAUNCHER_READ };
    delete without["VisualConfig.preset"];
    const f = checkConfigFieldReachability(s, without);
    expect(f.some((x) => x.message.includes("VisualConfig.preset"))).toBe(true);
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
