import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkModuleIndex } from "./G-module-index.mjs";

describe("G-module-index checkModuleIndex", () => {
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
