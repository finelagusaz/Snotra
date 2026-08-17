import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkHookCommands } from "./G-hook-commands.mjs";

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
  it("赤: カテゴリ A の終端見出しが失われた（母集団が後続の節へ流れ込む＝この検査では沈黙の向き）", () => {
    // 述語は `docsLines.includes(cmd)` ＝許可集合への所属なので、母集団が広がっても赤くならない。
    // 実測（2026-08-17・実 docs/build-commands.md）: `### B.` を落とすと cargo 行が 8 → 23 行へ広がる。
    // 広がりそのものは沈黙するので、**広がりの原因である「終端の不在」を赤にする**
    const s = snap({ ...base, "docs/build-commands.md": docsA.replace("### B. 次節", "普通の段落") });
    const f = checkHookCommands(s);
    expect(f.some((x) => x.message.includes('ending: "heading"'))).toBe(true);
  });
  it("赤: カテゴリ A のアンカーが 2 本ある（先に現れた方だけが照合され、本物の節が緑のまま素通りする）", () => {
    const s = snap({ ...base, "docs/build-commands.md": `${docsA}\n### A. 二本目\n\n\`\`\`bash\ncargo x\n\`\`\`\n` });
    const f = checkHookCommands(s);
    expect(f.some((x) => x.message.includes("2 本ある"))).toBe(true);
  });
  it("不混入: nodeSpec / vitest 系のコマンドは照合対象にしない", () => {
    // hookSrc の typecheck（nodeSpec）が docs に無くても緑のまま（対象は cargo 系のみ）
    expect(checkHookCommands(snap(base))).toEqual([]);
  });
});
