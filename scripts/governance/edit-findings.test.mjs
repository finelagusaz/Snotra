import { describe, it, expect } from "vitest";
import { snap } from "./test-helpers.mjs";
import { scopedFindings, reportFor } from "./edit-findings.mjs";

// **`checkModuleIndex` / `checkReferences` はモックしない。** 帰属フィルタは finding のメッセージ書式へ
// 文字列で結合しており（`finding` は `{file, line, message}` で、`file` は**文書側**のパスを指すため
// 編集ファイルとの結合は message にしか無い）、モックするとその書式変更を検知できない。
// 実物を呼ぶことがこの結合の唯一の検知器である。

/** 索引と実ファイルが双方向で一致する最小の木 */
const base = {
  "snotra-core/CLAUDE.md": "# x\n## モジュール構成\n- `lib.rs` — エントリ\n- `search.rs` — 検索\n\n## 次節\n",
  "snotra-core/src/lib.rs": "",
  "snotra-core/src/search.rs": "",
};

describe("scopedFindings — 索引（G-module-index）の帰属", () => {
  it("緑: 索引と実ファイルが一致していれば、その `.rs` に findings は無い", () => {
    expect(scopedFindings(snap(base), "snotra-core/src/lib.rs")).toEqual([]);
  });

  it("赤: 索引に載っていない `.rs` を編集すると、その 1 件だけが出る", () => {
    const s = snap(base, ["snotra-core/src/orphan.rs"]);
    const f = scopedFindings(s, "snotra-core/src/orphan.rs");
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("snotra-core/src/orphan.rs");
  });

  it("**帰属**: 索引漏れが 2 件あっても、編集した当のファイルの分だけが出る", () => {
    // 3b が壊した論点そのもの——絞らないと、債務が残る間その crate への無関係な編集のたびに
    // 同じ findings が全部出て、reminder がゴム印になる
    const s = snap(base, ["snotra-core/src/edited.rs", "snotra-core/src/other.rs"]);
    const f = scopedFindings(s, "snotra-core/src/edited.rs");
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("edited.rs");
    expect(f[0].message).not.toContain("other.rs");
  });

  it("**接頭辞で誤爆しない**: `a.rs` の編集が `a.rs` を接頭辞に持つ別パスの finding を拾わない", () => {
    // `includes` は部分一致なので、境界を測らずに「起こらない」と書かないための固定。
    // `snotra-core/src/a.rs` は `snotra-core/src/a.rs.bak.rs` の一部として現れうる
    const s = snap(base, ["snotra-core/src/a.rs", "snotra-core/src/a.rs.bak.rs"]);
    const f = scopedFindings(s, "snotra-core/src/a.rs");
    expect(f).toHaveLength(1);
    expect(f[0].message).not.toContain("a.rs.bak.rs");
  });

  it("判定対象外: `tests/` 配下の `.rs` は母集団の外（`cfg.src` が `src/` に閉じている）", () => {
    // crate 名で前方一致させると拾ってしまう。実測（直近 20 コミット）で索引を伴わない新規 `.rs` は
    // 2 件ともここに居た——偽の reminder になる形である
    const s = snap(base, ["snotra-core/tests/dir_stat_cost.rs"]);
    expect(scopedFindings(s, "snotra-core/tests/dir_stat_cost.rs")).toEqual([]);
  });

  it("`<crate>/CLAUDE.md` を編集したら索引を全件見る（順方向の typo もその文書に帰属する）", () => {
    const s = snap({ ...base, "snotra-core/CLAUDE.md": base["snotra-core/CLAUDE.md"].replace("`search.rs`", "`gone.rs`") });
    const f = scopedFindings(s, "snotra-core/CLAUDE.md");
    // 順方向（索引に実在しない `gone.rs`）と逆方向（索引から消えた `search.rs`）の両方
    expect(f.some((x) => x.message.includes("gone.rs"))).toBe(true);
    expect(f.some((x) => x.message.includes("search.rs"))).toBe(true);
  });
});

describe("scopedFindings — 参照実在（G-references）の帰属", () => {
  const docs = {
    "AGENTS.md": "# a\n本文\n",
    "docs/guide.md": "# g\n参照: `docs/guide.md`\n",
  };

  it("緑: 参照がすべて実在すれば findings は無い", () => {
    expect(scopedFindings(snap(docs), "docs/guide.md")).toEqual([]);
  });

  it("赤: 編集した文書の中の実在しない参照が出る", () => {
    const s = snap({ ...docs, "docs/guide.md": "# g\n参照: `docs/no-such-file.md`\n" });
    const f = scopedFindings(s, "docs/guide.md");
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("docs/no-such-file.md");
  });

  it("**免除が効く**: gitignore 済みのパスへの参照は finding にならない", () => {
    // `filterIgnored` を渡し忘れると偽の赤になる（実測: `node_modules/…` の参照が赤くなった）
    const s = snap({ ...docs, "docs/guide.md": "# g\n参照: `node_modules/vitest/vitest.mjs`\n" });
    const ignored = (paths) => new Set(paths.filter((p) => p.startsWith("node_modules/")));
    expect(scopedFindings(s, "docs/guide.md", ignored)).toEqual([]);
    // 対照: 免除しなければ赤い（免除が「たまたま 0 件」ではないことの接地）
    expect(scopedFindings(s, "docs/guide.md", () => new Set())).toHaveLength(1);
  });

  it("**編集した文書だけを見る**: 他の文書の壊れた参照は混じらない", () => {
    const s = snap({ ...docs, "AGENTS.md": "# a\n参照: `docs/gone.md`\n" });
    expect(scopedFindings(s, "docs/guide.md")).toEqual([]);
  });

  it("判定対象外: `governanceDocs` の外の `.md` は沈黙する（射程の明示）", () => {
    // README.md / PERFORMANCE.md / docs/adr/** 等。**「`.md` を編集すれば参照実在が見える」は偽である**
    const s = snap({ ...docs, "README.md": "# r\n参照: `docs/gone.md`\n" });
    expect(scopedFindings(s, "README.md")).toEqual([]);
  });

  it("`<crate>/CLAUDE.md` は索引と参照実在の**両方**に帰属する", () => {
    const s = snap({ ...base, "snotra-core/CLAUDE.md": `${base["snotra-core/CLAUDE.md"]}\n参照: \`docs/gone.md\`\n` });
    const f = scopedFindings(s, "snotra-core/CLAUDE.md");
    expect(f.some((x) => x.message.includes("docs/gone.md"))).toBe(true);
  });

  it("判定対象外: `.rs` は参照実在を見ない（`governanceDocs` に含まれない）", () => {
    const s = snap({ ...base, "snotra-core/src/lib.rs": "// 参照: `docs/gone.md`\n" });
    expect(scopedFindings(s, "snotra-core/src/lib.rs")).toEqual([]);
  });
});

describe("reportFor — hook が読む 1 行", () => {
  it("findings が無ければ空文字（呼び出し側は何も出さない）", () => {
    expect(reportFor(snap(base), "snotra-core/src/lib.rs")).toBe("");
  });

  it("件数・対象・全件を見る再現コマンドを含む", () => {
    const s = snap(base, ["snotra-core/src/orphan.rs"]);
    const line = reportFor(s, "snotra-core/src/orphan.rs");
    expect(line).toContain("snotra-core/src/orphan.rs");
    expect(line).toContain("1 件");
    expect(line).toContain("npm run governance:check");
  });

  it("上限を超える findings は件数で畳む（全部並べると読まれない）", () => {
    const many = Array.from({ length: 6 }, (_, i) => `snotra-core/src/o${i}.rs`);
    const s = snap({ ...base, "snotra-core/CLAUDE.md": "# x\n## モジュール構成\n\n## 次節\n" }, many);
    const line = reportFor(s, "snotra-core/CLAUDE.md");
    expect(line).toContain("ほか");
  });
});
