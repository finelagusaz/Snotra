import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkHookFires } from "./G-hook-fires.mjs";

describe("G-hook-fires checkHookFires（発火一覧 ↔ selectChecks）", () => {
  // フォールトインジェクションは複製へ当てる（.claude/rules/safety-nets.md）——
  // 実物の selectChecks も docs/hooks.md も変異させず、fake の select と snapshot だけを壊す。
  const ROWS = [
    // 補足列にバッククォートを置く（不混入の代表入力: 照合するのは検査 id 列だけである）
    "| `snotra-core/src/lib.rs` | `fmt` `clippy` `core-test` | 修復は `cargo fmt --all` の 1 コマンド |",
    "| `Cargo.toml` | `cargo-check` `hook-selftest` | ルートは両方走る |",
    "| `.githooks/pre-commit` | `githooks-selftest` | |",
    "| `docs/hooks.md` | （なし） | 何も走らない——沈黙は「合格」ではない |",
  ];
  const table = (rows) =>
    ["## 発火一覧", "", "| 編集したファイル（代表パス） | 走る検査 id | 補足 |", "|---|---|---|", ...rows, "", "## 次節"].join("\n");
  const hookSrc = [
    "export function selectChecks(rel) {",
    "  const checks = [];",
    '  if (isRust) checks.push("fmt");',
    '  if (isRust) checks.push("clippy");',
    '  if (isRust && rel.startsWith("snotra-core/")) checks.push("core-test");',
    '  if (CARGO_MANIFEST.test(rel)) checks.push("cargo-check");',
    '  if (CHECK_DEFINITION.has(rel)) checks.push("hook-selftest");',
    '  if (rel.startsWith(".githooks/")) checks.push("githooks-selftest");',
    "  return checks;",
    "}",
  ].join("\n");
  const TRUTH = {
    "snotra-core/src/lib.rs": ["fmt", "clippy", "core-test"],
    "Cargo.toml": ["cargo-check", "hook-selftest"],
    ".githooks/pre-commit": ["githooks-selftest"],
    "docs/hooks.md": [],
  };
  const select = (map) => (rel) => map[rel] ?? [];
  // 代表パスの実在検査があるので、フィクスチャの files にも載せる（read は不要＝存在だけを見る）
  const EXTRA = ["snotra-core/src/lib.rs", "Cargo.toml", ".githooks/pre-commit", ".claude/settings.json"];
  const base = { "docs/hooks.md": table(ROWS), ".claude/hooks/post-edit.mjs": hookSrc };
  const run = (contents, map = TRUTH) => checkHookFires(snap(contents, EXTRA), select(map));

  it("緑: 表の全行が selectChecks と一致し、発行されうる id をすべて覆う", () => {
    expect(run(base)).toEqual([]);
  });
  it("不混入: 補足列のバッククォート（`cargo fmt --all`）は検査 id として読まれない", () => {
    // 上の緑がそれを示す。列を跨いだ抽出をしていれば `cargo fmt --all` が id 扱いで赤になる
    expect(run(base).map((f) => f.message)).toEqual([]);
  });
  it("赤: selectChecks が id を足したのに表が追随していない（#858 で実際に起きた形）", () => {
    const f = run(base, { ...TRUTH, "snotra-core/src/lib.rs": ["fmt", "clippy", "core-test", "doc-test"] });
    expect(f.some((x) => x.message.includes("doc-test") && x.message.includes("一致しない"))).toBe(true);
  });
  it("赤: selectChecks が id を消したのに表に残っている", () => {
    const f = run(base, { ...TRUTH, "snotra-core/src/lib.rs": ["fmt", "clippy"] });
    expect(f.some((x) => x.message.includes("core-test"))).toBe(true);
  });
  it("赤: 表側で id を改名した（行の不一致と母集団の欠落の両方で捕まる）", () => {
    const f = run({ ...base, "docs/hooks.md": table([ROWS[0].replace("`core-test`", "`core-tests`"), ...ROWS.slice(1)]) });
    expect(f.some((x) => x.message.includes("一致しない"))).toBe(true);
    expect(f.some((x) => x.message.includes("どの行にも現れない: core-test"))).toBe(true);
  });
  it("赤: 述語が変わって行の割り当てが偽になった（Cargo.toml が hook-selftest を発火しなくなる）", () => {
    const f = run(base, { ...TRUTH, "Cargo.toml": ["cargo-check"] });
    expect(f.some((x) => x.message.includes("Cargo.toml") && x.message.includes("hook-selftest"))).toBe(true);
  });
  it("赤: 順序だけが違う（集合比較なら緑になってしまう代表入力）", () => {
    const f = run({ ...base, "docs/hooks.md": table([ROWS[0].replace("`fmt` `clippy`", "`clippy` `fmt`"), ...ROWS.slice(1)]) });
    expect(f.some((x) => x.message.includes("一致しない"))).toBe(true);
  });
  it("赤: 空集合の行も比較される（（なし）が「検査しない」に化けない）", () => {
    const f = run(base, { ...TRUTH, "docs/hooks.md": ["fmt"] });
    expect(f.some((x) => x.message.includes("docs/hooks.md") && x.message.includes("fmt"))).toBe(true);
  });
  it("赤: 発行されうる id が表のどの行にも現れない（新カテゴリの追加で行ごと漏れた形）", () => {
    const s = { ...base, ".claude/hooks/post-edit.mjs": hookSrc.replace("  return checks;", '  checks.push("wasm-test");\n  return checks;') };
    const f = run(s);
    expect(f.some((x) => x.message.includes("どの行にも現れない: wasm-test"))).toBe(true);
  });
  it("赤: ヘッダ行が見つからない（母集団欠落の明示的な失敗化）", () => {
    const f = run({ ...base, "docs/hooks.md": "# 表を消した\n" });
    expect(f.some((x) => x.message.includes("母集団の欠落"))).toBe(true);
  });
  it("赤: 代表パス列がバッククォート括りの単一パスでない（glob へ戻す退行）", () => {
    const f = run({ ...base, "docs/hooks.md": table(["| `*.rs` 全般 | `fmt` `clippy` | x |", ...ROWS.slice(1)]) });
    expect(f.some((x) => x.message.includes("単一パスでない"))).toBe(true);
  });
  it("赤: 行に代表パス列と検査 id 列がそろっていない", () => {
    const f = run({ ...base, "docs/hooks.md": table(["| `Cargo.toml` |", ...ROWS.slice(1)]) });
    expect(f.some((x) => x.message.includes("そろっていない"))).toBe(true);
  });
  it("赤: 表の途中に表でない行が紛れる（以降が照合されないまま緑になる経路・code-review H1）", () => {
    // startsWith(\"|\") で走査を打ち切る実装だと、この 1 行で以降 2 行の照合が消える
    const f = run({ ...base, "docs/hooks.md": table([ROWS[0], "注: 以下は補助的な行", ...ROWS.slice(1)]) });
    expect(f.some((x) => x.message.includes("表でない行がある"))).toBe(true);
  });
  it("赤: 空集合の行を消すと「沈黙は合格ではない」の主張が黙って消える（id を持たない行は母集団照合に掛からない）", () => {
    const f = run({ ...base, "docs/hooks.md": table(ROWS.slice(0, 3)) });
    expect(f.some((x) => x.message.includes("検査が 1 つも走らないパスの行が無い"))).toBe(true);
  });
  it("赤: 空集合セルが「（なし）」でない（散文・空バッククォートを空集合と読ませない）", () => {
    const f = run({ ...base, "docs/hooks.md": table([...ROWS.slice(0, 3), "| `docs/hooks.md` | 何も走らない | x |"]) });
    expect(f.some((x) => x.message.includes("「（なし）」と書く"))).toBe(true);
  });
  it("赤: 代表パスが実在しない（改名で死んだ行が接頭辞判定を通り続ける）", () => {
    const f = run(
      { ...base, "docs/hooks.md": table([...ROWS.slice(0, 2), "| `.githooks/gone` | `githooks-selftest` | x |", ROWS[3]]) },
      { ...TRUTH, ".githooks/gone": ["githooks-selftest"] },
    );
    expect(f.some((x) => x.message.includes("代表パスが実在しない"))).toBe(true);
  });
  it("赤: ヘッダ行が 2 本ある（例示の表が本物より先に掴まれる経路）", () => {
    const doubled = `${table(ROWS)}\n\n| 編集したファイル（代表パス） | 走る検査 id | 補足 |\n|---|---|---|\n${ROWS[0]}\n`;
    const f = run({ ...base, "docs/hooks.md": doubled });
    expect(f.some((x) => x.message.includes("ヘッダ行が 2 本ある"))).toBe(true);
  });
  it("赤: docs/hooks.md が読めない", () => {
    const f = run({ ".claude/hooks/post-edit.mjs": hookSrc });
    expect(f.some((x) => x.message.includes("docs/hooks.md が読めない"))).toBe(true);
  });
  it("赤: ヘッダはあるが行が 0 件", () => {
    const f = run({ ...base, "docs/hooks.md": table([]) });
    expect(f.some((x) => x.message.includes("行が 0 件"))).toBe(true);
  });
  it("赤: post-edit.mjs が読めない", () => {
    const f = run({ "docs/hooks.md": table(ROWS) });
    expect(f.some((x) => x.message.includes("post-edit.mjs が読めない"))).toBe(true);
  });
  it("赤: checks.push 抽出 0 件（抽出アンカーの腐敗）", () => {
    const f = run({ ...base, ".claude/hooks/post-edit.mjs": "export function selectChecks() { return []; }" });
    expect(f.some((x) => x.message.includes("1 件も抽出できない"))).toBe(true);
  });
  it("CRLF checkout でも緑（\\r が id の一致を外す回帰・#587/#589）", () => {
    expect(run({ ...base, "docs/hooks.md": table(ROWS).replace(/\n/g, "\r\n") })).toEqual([]);
  });
  it("既定引数は実物の selectChecks である（配線のカナリア・fake と区別できる形）", () => {
    // `.claude/settings.json` は fake の TRUTH に**無い**キーなので、fake なら [] を返して赤になる。
    // 実物だけが ["hook-selftest"] を返す——この非対称が「既定引数が実物か」を識別する
    const contents = {
      "docs/hooks.md": table(["| `.claude/settings.json` | `hook-selftest` | x |", ROWS[3]]),
      ".claude/hooks/post-edit.mjs": hookSrc,
    };
    const mismatch = (f) => f.message.includes("一致しない");
    expect(checkHookFires(snap(contents, EXTRA)).filter(mismatch)).toEqual([]);
    expect(checkHookFires(snap(contents, EXTRA), select(TRUTH)).filter(mismatch).length).toBeGreaterThan(0);
  });
});
