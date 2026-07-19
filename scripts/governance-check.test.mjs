// governance-check.mjs の検査関数を、故障注入フィクスチャ（赤）と正常フィクスチャ（緑）の
// 両方向で検証する。各フィクスチャは「守りたい対象 1 件が入力に現れること」と
// 「判定対象外が入力に混じらないこと」の入力集合検算を兼ねる（.claude/rules/safety-nets.md）。
import { describe, it, expect } from "vitest";
import {
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
  it("ui はテストファイルを逆方向の母集団から除外する（vitest include と一致）", () => {
    const s = snap({
      "ui/CLAUDE.md": "## モジュール構成\n- `main.tsx` — エントリ\n## 次節\n",
      "ui/src/main.tsx": "",
    }, ["ui/src/main.test.tsx", "ui/src/lib/x.test.ts"]);
    expect(checkModuleIndex(s, ["ui"])).toEqual([]);
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

describe("G8 checkSkillTable", () => {
  const claude = (rows) => `# x\n## 利用できるスキル\n\n| スキル | 使うとき |\n|---|---|\n${rows}\n\n## 次節\n`;
  it("緑: 表とディレクトリが双方向で一致", () => {
    const s = snap({ "CLAUDE.md": claude("| `/plan-review` | 計画後 |"), ".claude/skills/plan-review/SKILL.md": "" });
    expect(checkSkillTable(s)).toEqual([]);
  });
  it("赤: 表にあるがディレクトリに無い", () => {
    const s = snap({ "CLAUDE.md": claude("| `/gone-skill` | x |"), ".claude/skills/plan-review/SKILL.md": "" });
    const f = checkSkillTable(s);
    expect(f.some((x) => x.message.includes("gone-skill"))).toBe(true);
  });
  it("赤: ディレクトリにあるが表に無い", () => {
    const s = snap({ "CLAUDE.md": claude("| `/plan-review` | x |"), ".claude/skills/plan-review/SKILL.md": "", ".claude/skills/orphan/SKILL.md": "" });
    const f = checkSkillTable(s);
    expect(f.some((x) => x.message.includes("orphan"))).toBe(true);
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
  it("赤: cargoSpec 抽出 0 件は母集団欠落として fail（抽出アンカー腐敗の loud 化）", () => {
    const s = snap({ ...base, ".claude/hooks/post-edit.mjs": "function buildCommand(){}" });
    const f = checkHookCommands(s);
    expect(f.length).toBeGreaterThan(0);
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

describe("実リポジトリ スモーク（dogfood）", () => {
  it("現在のリポジトリで全検査が緑", async () => {
    const { makeSnapshot } = await import("./governance-check.mjs");
    const { fileURLToPath } = await import("node:url");
    const s = makeSnapshot(fileURLToPath(new URL("..", import.meta.url)));
    const { findings } = runAll(s);
    expect(findings).toEqual([]);
  });
});
