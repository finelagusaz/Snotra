import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkCiTable } from "./G-ci-table.mjs";

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
