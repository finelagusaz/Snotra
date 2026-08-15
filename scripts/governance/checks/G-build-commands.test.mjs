import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkBuildCommands } from "./G-build-commands.mjs";

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
