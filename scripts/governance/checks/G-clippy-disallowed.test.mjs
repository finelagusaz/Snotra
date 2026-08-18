import { fileURLToPath } from "node:url";
import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { makeSnapshot } from "../lib.mjs";
import {
  checkClippyDisallowed,
  clippyDisallowedCount,
  disallowedMethodPaths,
  declaresEguiDependency,
  clippyMethodsDenied,
  REQUIRED_DISALLOWED_METHODS,
} from "./G-clippy-disallowed.mjs";

describe("G-clippy-disallowed checkClippyDisallowed（clippy.toml の空洞化・#950）", () => {
  // 赤フィクスチャは「clippy が exit 0 で沈黙した入力」そのもの（clippy 1.94.0 で実測）。
  // 各エントリは**リテラルで書く**——カナリアから生成すると緑が恒真になり、名指しの意味が消える。
  // **件数も issue 番号も書かない**——群を足すたびに、そういう行が腐る。
  // reason に `（#751）` を入れてあるのは意図的で、引用符を見ないコメント除去（tomlLine）が
  // 行を切ることを緑側で押さえる。
  const R = 'reason = "root Ui が pass 冒頭で掴む Arc<Style> に間に合わない（#751）"';
  const CLIPPY_OK = [
    "# 群ごとにコメントで区切った禁止集合",
    "disallowed-methods = [",
    `    { path = "egui::Context::set_visuals", ${R} },`,
    `    { path = "egui::Context::set_visuals_of", ${R} },`,
    `    { path = "egui::Context::style_mut_of", ${R} },`,
    `    { path = "egui::Context::set_style_of", ${R} },`,
    `    { path = "egui::Context::global_style_mut", ${R} },`,
    `    { path = "egui::Context::set_global_style", ${R} },`,
    `    { path = "egui::Context::all_styles_mut", ${R} },`,
    // 群 2（#1067）。**fixture は群を跨いで持つ**——実ファイルが 1 つの関心ではなくなった以上、
    // 1 群だけの入力で緑にすると「群を足したらカナリアへも足す」運用が検算されない。
    `    { path = "snotra_core::engine::Engine::sorted_by_path", reason = "計測専用の観測口（#1067）" },`,
    // 群 3（#1122）。上と同じ理由でここにも持つ。
    `    { path = "snotra_core::engine::Engine::config", reason = "engine 錠越しの live-read（#1032）" },`,
    "]",
    "",
  ].join("\n");
  const MANIFEST_OK = '[package]\nname = "snotra"\n\n[lints]\nworkspace = true\n\n[dependencies]\nsnotra-egui-runtime = { path = "../snotra-egui-runtime" }\negui.workspace = true\n';
  const ROOT_DENY = '[workspace]\nmembers = ["src-tauri"]\n\n[workspace.lints.clippy]\ndisallowed_methods = "deny"\n';
  const base = { "src-tauri/clippy.toml": CLIPPY_OK, "src-tauri/Cargo.toml": MANIFEST_OK, "Cargo.toml": ROOT_DENY };
  const run = (over) => checkClippyDisallowed(snap({ ...base, ...over }));
  // 赤ケースは件数ではなく **file の並び** で主張する（G-workspace-lints と同じ理由——件数だけだと
  // 別クラスの退行で 1 件出た状態を満たしてしまう）
  const red = (over) => expect(run(over).map((x) => x.file));

  // --- 緑 ---
  it("緑: 実データ相当（複数行のインラインテーブル配列・reason に # を含む）", () => {
    expect(run({})).toEqual([]);
  });
  it("緑: 1 行形の配列（行パースではなく全域 match であること）", () => {
    const oneLine = `disallowed-methods = [${REQUIRED_DISALLOWED_METHODS.map((p) => `{ path = "${p}", ${R} }`).join(", ")}]\n`;
    expect(run({ "src-tauri/clippy.toml": oneLine })).toEqual([]);
  });
  // 引用符を見ない除去（tomlLine）は reason の `（#751）` で行を切る。path が先に在るうちは結果的に
  // 生き残るので、**順序を入れ替えた 1 件**がその不変条件を判別する唯一のフィクスチャである。
  it("緑: reason が path より前に書かれたエントリ（引用符を見ない除去なら赤へ振れる）", () => {
    const reasonFirst = CLIPPY_OK.replace(`{ path = "egui::Context::set_visuals", ${R} }`, `{ ${R}, path = "egui::Context::set_visuals" }`);
    expect(run({ "src-tauri/clippy.toml": reasonFirst })).toEqual([]);
  });
  it("緑: level のテーブル形（= { level = \"deny\", priority = 1 }）", () => {
    expect(run({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = { level = "deny", priority = 1 }\n' })).toEqual([]);
  });
  it("緑: CRLF でも判定が変わらない（CI の Windows checkout は autocrlf=true）", () => {
    const crlf = (s) => s.replace(/\n/g, "\r\n");
    expect(checkClippyDisallowed(snap({ "src-tauri/clippy.toml": crlf(CLIPPY_OK), "src-tauri/Cargo.toml": crlf(MANIFEST_OK), "Cargo.toml": crlf(ROOT_DENY) }))).toEqual([]);
  });

  // --- 赤クラス 1: 内容側（issue #950 が実測した 6 経路 + コメントアウト）---
  it("赤: clippy.toml ごと削除（clippy は沈黙・exit 0）", () => {
    const contents = { ...base };
    delete contents["src-tauri/clippy.toml"];
    expect(checkClippyDisallowed(snap(contents)).map((x) => x.file)).toEqual(["src-tauri/clippy.toml"]);
  });
  it("赤: disallowed-methods の配列ごと消滅（空配列と区別してメッセージを変える）", () => {
    const f = run({ "src-tauri/clippy.toml": "# 説明だけが残った\n" });
    expect(f.map((x) => x.file)).toEqual(["src-tauri/clippy.toml"]);
    expect(f[0].message).toContain("配列が無い");
  });
  it("赤: 空配列化", () => {
    red({ "src-tauri/clippy.toml": "disallowed-methods = []\n" }).toEqual(["src-tauri/clippy.toml"]);
  });
  it("赤: エントリが 1 行だけ消える（∀ 条件だけでは緑を通る形）", () => {
    const f = run({ "src-tauri/clippy.toml": CLIPPY_OK.replace(/^.*all_styles_mut.*$\n/m, "") });
    expect(f.map((x) => x.file)).toEqual(["src-tauri/clippy.toml"]);
    expect(f[0].message, "欠けたパスを名指ししないと直し方が読めない").toContain("egui::Context::all_styles_mut");
  });
  it("赤: メソッド名の書き損じ（clippy は warning だが -D warnings でも exit 0）", () => {
    red({ "src-tauri/clippy.toml": CLIPPY_OK.replace("set_visuals_of", "set_visuals_off") }).toEqual(["src-tauri/clippy.toml"]);
  });
  it("赤: crate 名の書き損じ（clippy は診断そのものを出さない）", () => {
    red({ "src-tauri/clippy.toml": CLIPPY_OK.replaceAll("egui::Context::", "eguii::Context::") }).toEqual(["src-tauri/clippy.toml"]);
  });
  // **配列は開いたままにする**——`= []` を先に置くとコメント除去が無い実装でも先頭の空配列に当たって
  // 赤になり、この不変条件を判別しない。開いたままなら、除去しない実装はコメント内の path を 7 件数えて
  // **緑へ振れる**（実測）。それがこのフィクスチャの守る境界である。
  it("赤: 配列の中でエントリが # でコメントアウトされる（素朴な行パースが緑で通す形・実測で発見）", () => {
    const commented = CLIPPY_OK.split("\n")
      .map((l) => (l.trim().startsWith("{ path") ? `#${l}` : l))
      .join("\n");
    red({ "src-tauri/clippy.toml": commented }).toEqual(["src-tauri/clippy.toml"]);
  });
  it("赤: egui 依存の消滅（禁止パスが解決する前提が消える）", () => {
    red({ "src-tauri/Cargo.toml": '[package]\nname = "snotra"\n\n[dependencies]\nserde = "1"\n' }).toEqual(["src-tauri/Cargo.toml"]);
  });
  it("赤: snotra-egui-runtime だけが在る形（字面一致の述語なら緑になる誤爆検算）", () => {
    red({ "src-tauri/Cargo.toml": '[dependencies]\nsnotra-egui-runtime = { path = "../snotra-egui-runtime" }\n' }).toEqual(["src-tauri/Cargo.toml"]);
  });

  // --- 赤クラス 2: レベル側（沈黙経路 0。**実リポジトリは Phase 1 以降これを満たすので、
  //     clippyMethodsDenied が常に true を返す実装を捕まえるのはこのフィクスチャだけである**）---
  it("赤: ルートから [workspace.lints.clippy] 節ごと消える（2 行削除＝最も起きやすい形）", () => {
    red({ "Cargo.toml": '[workspace]\nmembers = ["src-tauri"]\n\n[workspace.lints.rustdoc]\nbroken_intra_doc_links = "deny"\n' }).toEqual(["Cargo.toml"]);
  });
  it("赤: deny が warn へ降格（禁止が黙って助言になる）", () => {
    red({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = "warn"\n' }).toEqual(["Cargo.toml"]);
  });
  // deny の行を残したまま禁止が完全に消える形。clippy 側は exit 0・診断 0 件になることを実測した
  // （#950。disallowed-methods は clippy::all と clippy::style の両方に属する）
  it("赤: 同じ節の all = \"allow\" が deny を後から打ち消す（deny の行は残っている）", () => {
    red({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = "deny"\nall = "allow"\n' }).toEqual(["Cargo.toml"]);
  });
  it("赤: style = { level = \"allow\", priority = 1 } でも同じく打ち消される", () => {
    red({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = "deny"\nstyle = { level = "allow", priority = 1 }\n' }).toEqual(["Cargo.toml"]);
  });
  it("緑: 群の allow が priority = -1 で先に当たる形は禁止が生き残る（実測どおり緑へ倒す）", () => {
    expect(run({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = "deny"\nall = { level = "allow", priority = -1 }\n' })).toEqual([]);
  });
  it("緑: 打ち消さない lint の allow は誤爆しない（節に allow を書く正当な用途を残す）", () => {
    expect(run({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = "deny"\nneedless_range_loop = "allow"\n' })).toEqual([]);
  });
  // TOML 上等価な 3 綴り。インライン形しか読まない実装は、残り 2 つで **clippy exit 0 なのに緑** を返す（実測）
  it("赤: dotted 形の群 allow（all.level = \"allow\"）", () => {
    red({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = "deny"\nall.level = "allow"\n' }).toEqual(["Cargo.toml"]);
  });
  it("赤: dotted 形で priority を別行に書く形", () => {
    red({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = "deny"\nall.level = "allow"\nall.priority = 0\n' }).toEqual(["Cargo.toml"]);
  });
  it("赤: サブテーブル形（[workspace.lints.clippy.all]）", () => {
    red({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = "deny"\n\n[workspace.lints.clippy.all]\nlevel = "allow"\npriority = 0\n' }).toEqual(["Cargo.toml"]);
  });
  it("緑: disallowed_methods 自身も dotted / サブテーブル形で書ける", () => {
    expect(run({ "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods.level = "deny"\n' })).toEqual([]);
    expect(run({ "Cargo.toml": "[workspace.lints.clippy.disallowed_methods]\nlevel = \"forbid\"\n" })).toEqual([]);
  });
  it("赤: 数値区切り付きの priority（1_0 は 10 であって 1 ではない）", () => {
    red({
      "Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = { level = "deny", priority = 5 }\nall = { level = "allow", priority = 1_0 }\n',
    }).toEqual(["Cargo.toml"]);
  });
  it("赤: 別カテゴリ配下の同じ字面（構文的位置で判定していることの検算）", () => {
    red({ "Cargo.toml": '[workspace.lints.rustdoc]\ndisallowed_methods = "deny"\n' }).toEqual(["Cargo.toml"]);
  });

  // --- 母集団の欠落 ---
  it("赤: 母集団の欠落は 3 入力それぞれで鳴る（沈黙させない側へ倒す）", () => {
    expect(checkClippyDisallowed(snap({})).map((x) => x.file)).toEqual(["src-tauri/clippy.toml", "src-tauri/Cargo.toml", "Cargo.toml"]);
  });

  // --- 述語の単体（境界）---
  it("disallowedMethodPaths は「配列が無い」(null) と「空配列」([]) を区別する", () => {
    expect(disallowedMethodPaths("# 何も無い\n")).toBeNull();
    expect(disallowedMethodPaths("disallowed-methods = []\n")).toEqual([]);
  });
  it("declaresEguiDependency は [package] 配下の同じ字面を数えない", () => {
    expect(declaresEguiDependency("[package]\negui.workspace = true\n")).toBe(false);
    expect(declaresEguiDependency('[dependencies]\negui = "=0.35.0"\n')).toBe(true);
    expect(declaresEguiDependency("[target.'cfg(windows)'.dependencies]\negui.workspace = true\n")).toBe(true);
  });
  // dependencies] で終わる節を広く受けると、どれも実害を持つ 3 つが紛れ込む（対称性検査 2c で発見）
  it("declaresEguiDependency は dev/build/workspace の dependencies を数えない", () => {
    expect(declaresEguiDependency('[dev-dependencies]\negui = "=0.35.0"\n'), "bin/lib ではパスが解決しない").toBe(false);
    expect(declaresEguiDependency('[build-dependencies]\negui = "=0.35.0"\n')).toBe(false);
    expect(declaresEguiDependency('[workspace.dependencies]\negui = "=0.35.0"\n'), "ルート Cargo.toml と取り違えても緑になってしまう").toBe(false);
  });
  it("checkClippyDisallowed の 3 入力は同型ゆえ、取り違えが赤として現れること", () => {
    // ルート Cargo.toml を src-tauri の manifest として渡した形（引数の取り違えの再現）
    const root = '[workspace]\nmembers = ["src-tauri"]\n\n[workspace.dependencies]\negui = "=0.35.0"\n\n[workspace.lints.clippy]\ndisallowed_methods = "deny"\n';
    expect(declaresEguiDependency(root), "ルートを src-tauri と取り違えたら赤でなければならない").toBe(false);
  });
  it("clippyMethodsDenied はハイフン形を非実効と判定する（fail-closed・直し方はソースのコメント）", () => {
    expect(clippyMethodsDenied('[workspace.lints.clippy]\ndisallowed-methods = "deny"\n')).toBe(false);
  });
  it("clippyDisallowedCount は読めない入力で 0 を返す（evidence が undefined にならない）", () => {
    expect(clippyDisallowedCount(snap({}))).toBe(0);
    // **数を書かない。** fixture は `REQUIRED_DISALLOWED_METHODS` を全件持つ形で組んであるので、
    // 禁止を 1 つ足すたびにこの行だけが腐る（#1067 で実際に踏んだ）。測りたいのは
    // 「読めた入力で件数が evidence に出る」ことであって、その時点の件数ではない。
    expect(clippyDisallowedCount(snap(base))).toBe(REQUIRED_DISALLOWED_METHODS.length);
  });

  // --- カナリア ---
  it("カナリア: 実リポジトリで緑であり、守りたい対象が全件入力に現れる", () => {
    const s = makeSnapshot(fileURLToPath(new URL("../../..", import.meta.url)));
    const real = disallowedMethodPaths(s.read("src-tauri/clippy.toml") ?? "");
    expect(real, "実 clippy.toml から disallowed-methods を導出できなかった").not.toBeNull();
    for (const p of REQUIRED_DISALLOWED_METHODS) expect(real, `${p} が実ファイルの入力に現れない`).toContain(p);
    expect(real, "判定対象外（除外したメソッド）が入力に混じっている").toHaveLength(REQUIRED_DISALLOWED_METHODS.length);
    expect(checkClippyDisallowed(s)).toEqual([]);
  });
});
