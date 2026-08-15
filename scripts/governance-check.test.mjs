// governance-check.mjs の検査関数を、フォールトインジェクションフィクスチャ（赤）と正常フィクスチャ（緑）の
// 両方向で検証する。各フィクスチャは「守りたい対象 1 件が入力に現れること」と
// 「判定対象外が入力に混じらないこと」の入力集合検算を兼ねる（.claude/rules/safety-nets.md）。
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, it, expect } from "vitest";
import { snap } from "./governance/test-helpers.mjs";
import { governanceDocs, makeSnapshot, runAll, buildChecks } from "./governance-check.mjs";
// facade を経由しない——per-check 分割（#1093）が確立した「テストは自分の検査モジュールから直接
// import する」形に揃える。facade へ再輸出を残すと `G-module-index.mjs` が静的 import され続け、
// **ファイル消失が manifest 差分ではなく import エラーとして現れる本数が 1 増える**（#1094）。
import { MODULE_INDEX_CRATES } from "./governance/checks/G-module-index.mjs";

// G-module-index/G-references の母集団は手で列挙する定数であり、**crate を足しても何も鳴らない**（沈黙する経路）。
// `snotra-egui-runtime` は「#532 の検証層」として作られたまま両方から漏れ、SU7 で製品の
// 描画層になった後も索引ドリフト・参照切れが検知されない状態が続いていた（#701）。
// 実 `Cargo.toml` を読み、CLAUDE.md を持つ member が両母集団に載っていることを固定する。
describe("G-module-index/G-references 母集団カナリア — #701", () => {
  it("CLAUDE.md を持つ workspace member は MODULE_INDEX_CRATES と governanceDocs の両方に載る", () => {
    const root = fileURLToPath(new URL("..", import.meta.url));
    const src = readFileSync(fileURLToPath(new URL("../Cargo.toml", import.meta.url)), "utf8");

    // 書式が変わったら「読めなかった」と落ちる（fail-closed・post-edit.test.mjs の members カナリアと同型）
    const section = src.match(/\[workspace\]\r?\n([\s\S]*?)(?=\r?\n\[|$)/);
    expect(section, "Cargo.toml の [workspace] セクションを読めなかった").not.toBeNull();
    const m = section[1].match(/^members\s*=\s*\[([^\]]*)\]/m);
    expect(m, "[workspace] の members 行を読めなかった（書式が変わった）").not.toBeNull();
    const members = m[1]
      .split(",")
      .map((s) => s.trim().replace(/^"|"$/g, ""))
      .filter((s) => s.length > 0);

    const docs = governanceDocs(makeSnapshot(root));
    const withClaudeMd = members.filter((c) => existsSync(fileURLToPath(new URL(`../${c}/CLAUDE.md`, import.meta.url))));
    // 母集団が空になる形（members 読み違い・全 crate が CLAUDE.md 無し）を合格に見せない
    expect(withClaudeMd.length, "CLAUDE.md を持つ member が 0 件（母集団の欠落）").toBeGreaterThan(0);

    for (const crate of withClaudeMd) {
      expect(
        Object.keys(MODULE_INDEX_CRATES),
        `${crate} が MODULE_INDEX_CRATES に無い。src/exts を添えて追加すること（索引ドリフトが沈黙で通る）`,
      ).toContain(crate);
      expect(
        docs,
        `${crate}/CLAUDE.md が G-references 母集団に無い。governanceDocs の正規表現を更新すること（参照切れが沈黙で通る）`,
      ).toContain(`${crate}/CLAUDE.md`);
    }
  });
});

describe("runAll（空母集団の明示 fail = 沈黙経路の閉塞）", () => {
  it("対象文書・rules・skills が空なら findings を返す", () => {
    const s = snap({});
    const { findings } = runAll(s);
    expect(findings.length).toBeGreaterThan(0);
  });
  it("計器（G-area-instrument）は検査配列に無い——面積に合否は無い（ADR-retire-area-budget）", () => {
    const ids = buildChecks(snap({}), {}).map((c) => c.id);
    expect(ids).not.toContain("G-area-instrument");
  });
  it("それでも計器の母集団欠落は runAll の findings に残る（検査配列の外でも沈黙しない）", () => {
    const { findings } = runAll(snap({}));
    expect(findings.some((f) => f.message.includes("G-area-instrument 母集団の欠落"))).toBe(true);
  });
});

describe("検査 ID の形（#812 — 序数を引用の語彙から外す）", () => {
  const ids = buildChecks(snap({}), {}).map((c) => c.id);

  it("すべて `G-<kebab>` 形で、数字を含まない", () => {
    for (const id of ids) expect(id, `${id} が G-<name> 形でない`).toMatch(/^G-[a-z][a-z0-9]*(-[a-z0-9]+)*$/);
  });

  it("重複しない（連番なら並行 PR が同じ値を見る形が、名前では起きないことの固定）", () => {
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("サマリの件数は登録表から計算される（範囲の手書きが存在しない）", () => {
    // 「G1..G15 passed」のような範囲表記は、検査を足しても黙って古くなる（#812 実測）
    const { evidence } = runAll(makeSnapshot(fileURLToPath(new URL("..", import.meta.url))));
    expect(evidence).toContain(`検査 ${ids.length} 件`);
  });
});

describe("実リポジトリ スモーク（dogfood）", () => {
  it("現在のリポジトリで全検査が緑", async () => {
    const { makeSnapshot } = await import("./governance-check.mjs");
    const { fileURLToPath } = await import("node:url");
    const s = makeSnapshot(fileURLToPath(new URL("..", import.meta.url)));
    const { findings } = runAll(s);
    expect(findings).toEqual([]);
  });
});

describe("facade の公開面（export { … } の凍結）", () => {
  // export { … } は手書きの一覧であり、書き忘れ・書き足しは npm test にも governance:check にも
  // 現れるとは限らない——テストファイルが直接 import していない名前が消えても、どちらのコマンドも
  // 検知しない。公開面を丸ごと凍結することで、この一覧への変更は気づかず起きることではなく、
  // 意図して行う編集になる。
  //
  // **この凍結が守るものは #1094 で変わった。** かつては「検査を 1 本 checks/ へ移すたびに
  // 書き足す面」を守っていたが、再輸出を実際の消費者まで絞った今、守るのは逆向きである——
  // **`checks/` の名前がここへ戻ってくることを検知する**。戻すとその検査ファイルは facade へ
  // 静的 import され、消失が manifest 差分ではなく import エラーとして現れる側へ帰る
  // （射程の正本は `governance-manifest.test.mjs` のフォールトインジェクション節）。
  it("公開する名前の集合が凍結した一覧と一致する", async () => {
    const mod = await import("./governance-check.mjs");
    expect(Object.keys(mod).sort()).toEqual(["buildChecks", "governanceDocs", "makeSnapshot", "runAll"]);
  });
});
