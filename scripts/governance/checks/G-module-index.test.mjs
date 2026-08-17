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
  it("赤: 「モジュール構成」の終端見出しが失われた（節が EOF まで伸び、他節のバッククォートを索引と誤認する）", () => {
    const s = snap({ ...base, "snotra-core/CLAUDE.md": base["snotra-core/CLAUDE.md"].replace("## 次節", "次節") });
    const f = checkModuleIndex(s, ["snotra-core"]);
    expect(f.some((x) => x.message.includes('ending: "heading"'))).toBe(true);
  });
  it("緑: 本文が空の節は「節が無い」ではない（逆方向の照合が実ファイルを名指しする）", () => {
    // 旧実装の `if (!section)` は空文字列を欠落へ潰し、「節が見つからない」1 件で
    // 逆方向の照合ごと打ち切っていた。空の索引で赤くなるべきは実ファイルの側である
    const s = snap({ ...base, "snotra-core/CLAUDE.md": "# x\n## モジュール構成\n## 次節\n" });
    const f = checkModuleIndex(s, ["snotra-core"]);
    expect(f.every((x) => !x.message.includes("見出しが見つからない"))).toBe(true);
    expect(f.some((x) => x.message.includes("lib.rs"))).toBe(true);
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
