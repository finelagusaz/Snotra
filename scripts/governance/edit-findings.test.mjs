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

  it("判定対象外: `tests/` 配下の `.rs` は索引の照合に掛からない", () => {
    // **このテストが緑である理由は `cfg.src` ではなく帰属フィルタである**（委譲レビューの変異注入が
    // 実測）——`cfg.src` を crate 名の前方一致へ退化させても、`tests/` の rel は逆方向の findings に
    // 現れないので `attributesTo` が 0 件へ落とし、このテストは緑のまま通る。
    // 固定しているのは**挙動**（索引を持たない `tests/` の `.rs` で鳴らないこと）であって、
    // `moduleIndexCrateOf` の判定の形ではない。
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

// ---------------------------------------------------------------------------
// 走査元を絞る検査群（参照の書き方・語彙・ADR の命名）。
//
// **索引側と帰属の作り方が違う。** `G-module-index` の帰属は finding のメッセージへの文字列結合だが、
// ここは**走査元の母集団を `[rel]` へ絞る**ことで帰属が構造的に決まる（参照はその文書に書かれている）。
// 着地先（アンカー・語彙）は snapshot 全体のままなので、判定の強さは CI と同じである。
// ---------------------------------------------------------------------------

describe("scopedFindings — 参照の書き方（heading-refs 3 種）の帰属", () => {
  const refDocs = {
    "AGENTS.md": "# a\n## 節 A\n本文\n",
    "docs/guide.md": "# g\n参照: `AGENTS.md`「節 A」\n",
  };

  it("緑: 正準形が着地していれば findings は無い", () => {
    expect(scopedFindings(snap(refDocs), "docs/guide.md")).toEqual([]);
  });

  it("赤: 着地しない正準形が、編集した文書の分だけ出る", () => {
    const s = snap({ ...refDocs, "docs/guide.md": "# g\n参照: `AGENTS.md`「無い節」\n" });
    const f = scopedFindings(s, "docs/guide.md");
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("見出し参照が着地しない");
  });

  it("赤: 助詞が挟まった近傍形（G-near-heading-refs）", () => {
    const s = snap({ ...refDocs, "docs/guide.md": "# g\n参照: `AGENTS.md` の「節 A」\n" });
    expect(scopedFindings(s, "docs/guide.md")).toHaveLength(1);
  });

  it("赤: 物理改行で折れた形（G-folded-heading-refs）", () => {
    const s = snap({ ...refDocs, "docs/guide.md": "# g\n参照: `AGENTS.md`\n「節 A」\n" });
    expect(scopedFindings(s, "docs/guide.md")).toHaveLength(1);
  });

  it("**帰属**: 他の文書の壊れた参照は混じらない", () => {
    const s = snap({ ...refDocs, "docs/other.md": "# o\n`AGENTS.md`「無い節」\n" });
    expect(scopedFindings(s, "docs/guide.md")).toEqual([]);
  });

  it("判定対象外: `docs/adr/` は走査元の母集団に入らない（凍結された歴史・沈黙する）", () => {
    const s = snap({ ...refDocs, "docs/adr/ADR-x.md": "# ADR-x: 題\n`AGENTS.md`「無い節」\n" });
    expect(scopedFindings(s, "docs/adr/ADR-x.md")).toEqual([]);
  });
});

describe("scopedFindings — 語彙（G-stale-identifiers）の帰属", () => {
  const vocabTree = {
    "snotra-core/src/lib.rs": "pub fn live_helper() {}\n",
    ".claude/rules/sample.md": "---\npaths:\n  - \"x\"\n---\n\n# s\n`live_helper` を呼ぶ\n",
  };

  it("緑: 現行語彙に在る識別子は findings にならない", () => {
    expect(scopedFindings(snap(vocabTree), ".claude/rules/sample.md")).toEqual([]);
  });

  it("赤: 語彙に無い識別子が、編集した文書の分だけ出る", () => {
    const s = snap({
      ...vocabTree,
      ".claude/rules/sample.md": "---\npaths:\n  - \"x\"\n---\n\n# s\n`goneHelper` を呼ぶ\n",
    });
    const f = scopedFindings(s, ".claude/rules/sample.md");
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("現行語彙に無い識別子");
  });

  it("判定対象外: 母集団の外の `.md` は語彙照合に掛からない", () => {
    const s = snap({ ...vocabTree, "notes/scratch.md": "`goneHelper`\n" });
    expect(scopedFindings(s, "notes/scratch.md")).toEqual([]);
  });
});

describe("scopedFindings — ADR の命名（G-adr-file-names）の帰属", () => {
  const adrTree = {
    "docs/adr/ADR-good-slug.md": "# ADR-good-slug: 題\n本文\n",
    "docs/adr/ADR-001-numbered.md": "# ADR-001-numbered: 題\n本文\n",
  };

  it("緑: 形の合った ADR を編集しても findings は無い", () => {
    expect(scopedFindings(snap(adrTree), "docs/adr/ADR-good-slug.md")).toEqual([]);
  });

  it("赤: 連番形のファイル名は、その ADR を編集したときに出る", () => {
    const f = scopedFindings(snap(adrTree), "docs/adr/ADR-001-numbered.md");
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("連番を振らない");
  });

  it("**帰属**: 他の ADR の逸脱は混じらない", () => {
    expect(scopedFindings(snap(adrTree), "docs/adr/ADR-good-slug.md")).toEqual([]);
  });
});

describe("reportFor — hook が読む 1 行", () => {
  it("findings が無ければ空文字（呼び出し側は何も出さない）", () => {
    expect(reportFor(snap(base), "snotra-core/src/lib.rs")).toBe("");
  });

  it("件数・対象・全件を見る再現コマンドを含む", () => {
    // **`extraFiles` ではなく本文を持たせる。** 走査元を絞る検査群（参照の書き方）は `.rs` も読むので、
    // 本文の無い宣言だけのファイルは「母集団が読めない」側の finding を生む——実ツリーでは
    // 編集直後の実在ファイルしか来ないので、その形はここで作らない。
    const s = snap({ ...base, "snotra-core/src/orphan.rs": "" });
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
