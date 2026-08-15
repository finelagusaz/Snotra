import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkReferences } from "./G-references.mjs";
import { buildChecks } from "../../governance-check.mjs";

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
