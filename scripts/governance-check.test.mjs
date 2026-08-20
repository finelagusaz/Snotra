// governance-check.mjs の検査関数を、フォールトインジェクションフィクスチャ（赤）と正常フィクスチャ（緑）の
// 両方向で検証する。各フィクスチャは「守りたい対象 1 件が入力に現れること」と
// 「判定対象外が入力に混じらないこと」の入力集合検算を兼ねる（.claude/rules/safety-nets.md）。
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, it, expect } from "vitest";
import { snap } from "./governance/test-helpers.mjs";
import { governanceDocs, makeSnapshot, runAll, buildChecks } from "./governance-check.mjs";
// facade を経由しない——per-check 分割（#1093）が確立した「テストは自分の検査モジュールから直接
// import する」形に揃える（#1094）。**`G-module-index.mjs` への静的 import が消えるわけではない**
// ——ここへ移っただけで、ペア消失は今も `npm test` の import エラーとして現れる。facade 側から
// 落とす効果は、`governance check` step が `buildChecks` へ到達できるようになることにある。
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
  // 計器の母集団欠落は**格下げ側へ移した**（`ADR-governance-meta-demotion`）。守っている相手が
  // 計器なので、倒れても 21 本の合否は動かない。**沈黙してはいない**——器が変わっただけで、
  // 印字はされ、監査モードでは exit code へ戻る。
  // **両モードとも env を明示的に固定して測る。** 固定しないと、`SNOTRA_GOV_META_AUDIT=1` で
  // 走らせた監査そのものが「格下げされていること」のテストを落とす——検査対象の状態を
  // 検査の実行条件が決めてしまう形である（実測で 1 本落ちた）。
  const withAudit = (v, fn) => {
    const prev = process.env.SNOTRA_GOV_META_AUDIT;
    if (v === undefined) delete process.env.SNOTRA_GOV_META_AUDIT;
    else process.env.SNOTRA_GOV_META_AUDIT = v;
    try {
      return fn();
    } finally {
      if (prev === undefined) delete process.env.SNOTRA_GOV_META_AUDIT;
      else process.env.SNOTRA_GOV_META_AUDIT = prev;
    }
  };
  const hasAreaGap = (fs) => fs.some((f) => f.message.includes("G-area-instrument 母集団の欠落"));

  it("計器の母集団欠落は metaFindings に残る（格下げ後も沈黙しない）", () => {
    const { findings, metaFindings } = withAudit(undefined, () => runAll(snap({})));
    expect(hasAreaGap(metaFindings)).toBe(true);
    expect(hasAreaGap(findings)).toBe(false);
  });
  it("監査モードではメタ層が findings へ合流する（戻す経路が実在する）", () => {
    const { findings } = withAudit("1", () => runAll(snap({})));
    expect(hasAreaGap(findings)).toBe(true);
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

describe("evidence の供給カナリア（#1098）", () => {
  // **名前は「供給」であって「配線」ではない。** ここが見るのは、実リポジトリで evidence の
  // 読みがすべて供給されていること——検査が `ctx.record` を呼ばなくなる／facade の導出が消える、
  // という欠落を捕まえる。
  //
  // **配線（view を通ること）はここでは見えない**——view を外す変異を当てても、供給が揃っていれば
  // 下の 3 条件はすべて満たされたまま緑になる（2026-08-17 実測: `governance:check` exit 0・
  // `npm test` 745 件全緑）。配線は `governance/evidence.mjs` の brand が構造で拒み、
  // その効きは `governance/evidence.test.mjs`「配線:」の 3 件が測る。
  it("実リポジトリの evidence 行は `undefined` も `?` も含まない", () => {
    const { evidence, findings } = runAll(makeSnapshot(fileURLToPath(new URL("..", import.meta.url))));
    expect(evidence).not.toContain("undefined");
    expect(evidence, "未記録の読みが `?` に化けている（供給側が消えた）").not.toContain("?");
    expect(findings.filter((f) => f.message.includes("が未記録である"))).toEqual([]);
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
    expect(Object.keys(mod).sort()).toEqual([
      "buildChecks",
      "governanceDocs",
      "makeSnapshot",
      "metaAuditEnabled",
      "runAll",
    ]);
  });
});
