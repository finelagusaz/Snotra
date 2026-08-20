import { fileURLToPath } from "node:url";
import { describe, it, expect } from "vitest";
import { makeSnapshot } from "./lib.mjs";
import { buildDomains, DOMAIN_SPECS } from "./domains.mjs";

const ROOT = fileURLToPath(new URL("../../", import.meta.url));

// 反証レシピは**部分集合への絞り込みに限らない**。単一ディレクトリだけで構成される母集団
// （`adrFiles` / `ruleDocs`）では、腕を引くと空集合にしかならず「空でしか倒れない錨」と区別が
// 付かなくなる——しかしそれらの錨は前方一致ではなく**直下**を見ているので、**全件を下層へ移す**
// 形では倒れる。これは `|P| > 0` には無い強さであり、絞り込みだけをレシピの形にすると測れない。
// ゆえにレシピは「members をどう変えれば倒れるか」を返す（空でない・元と違う、が下の要求）。
const withoutPrefix = (p) => (m) => m.filter((f) => !f.startsWith(p));
const withoutExactDir = (d) => (m) => m.filter((f) => f.slice(0, f.lastIndexOf("/")) !== d);
const withoutMatch = (re) => (m) => m.filter((f) => !re.test(f));
/** 全件を 1 階層下へ移す（ディレクトリごと引っ越した形）。直下を見る錨だけが倒れる。 */
const movedUnder = (d) => (m) => m.map((f) => f.replace(`${d}/`, `${d}/moved/`));

/** `"<ドメイン名>#<錨のラベル>"` → 反証レシピ（members を変えて錨を倒す関数）。
 *  **全錨がここに載っていること**は下の完全性 assert が縛る。 */
const FALSIFIERS = new Map([
  // 固定名を名指す錨——名指された側を 1 つ引く
  ["governanceDocs#ルートの AGENTS.md と CLAUDE.md", withoutMatch(/^AGENTS\.md$/)],
  // 別 SSOT（Cargo.toml）からの every——crate の CLAUDE.md を 1 つ引く
  ["governanceDocs#CLAUDE.md を持つ workspace member のすべて", (m) => m.filter((f) => !/^[^/]+\/CLAUDE\.md$/.test(f))],
  ["governanceDocs#docs/ の腕", withoutPrefix("docs/")],
  ["governanceDocs#.claude/rules/ の腕", withoutPrefix(".claude/rules/")],
  ["governanceDocs#.claude/skills/ の腕", withoutPrefix(".claude/skills/")],

  ["headingRefDocs#docs/ 配下の md", withoutPrefix("docs/")],
  ["headingRefSourceDocs#CLAUDE.md を持つ crate の src 配下の .rs", withoutMatch(/\/src\//)],

  ["headingRefCommentDocs#scripts/governance/checks/ 直下", withoutExactDir("scripts/governance/checks")],
  ["headingRefCommentDocs#scripts/governance/ 直下", withoutExactDir("scripts/governance")],
  ["headingRefCommentDocs#.claude/hooks/ 直下", withoutExactDir(".claude/hooks")],
  ["headingRefCommentDocs#ps 族（.ps1/.psm1/.psd1）の腕", withoutMatch(/\.(ps1|psm1|psd1)$/i)],

  ["allHeadingRefDocs#md の腕", withoutMatch(/\.md$/)],
  ["allHeadingRefDocs#.rs の腕", withoutMatch(/\.rs$/)],
  ["allHeadingRefDocs#スクリプトの腕", withoutMatch(/\.(mjs|ps1|psm1)$/)],

  ["staleIdentifierDocs#.claude/skills/ の腕", withoutPrefix(".claude/skills/")],
  ["staleIdentifierDocs#.claude/rules/ の腕", withoutPrefix(".claude/rules/")],
  ["staleIdentifierGuideDocs#docs/ 直下", withoutExactDir("docs")],
  ["staleIdentifierTargets#.claude/ の腕（staleIdentifierDocs）", withoutPrefix(".claude/")],
  ["staleIdentifierTargets#docs/ の腕（staleIdentifierGuideDocs）", withoutPrefix("docs/")],

  // 母集団が単一ディレクトリだけで構成されるので、引くと空にしかならない。移設で倒す
  ["adrFiles#docs/adr/ 直下", movedUnder("docs/adr")],
  ["ruleDocs#.claude/rules/ 直下", movedUnder(".claude/rules")],

  // メンバーは crate ディレクトリ名。**引いても真のまま**なので（残った側で every が成立する）、
  // 引くのではなく「宣言に在るが Cargo.toml を持たないディレクトリ」を 1 つ混ぜて倒す
  // ——この錨が守っている当の失敗（宣言と実体の食い違い）そのものである。
  ["workspaceMemberDirs#全メンバーのディレクトリに Cargo.toml が実在する", (m) => [...m.slice(1), "does-not-exist"]],
  // 逆向き（実在 → 宣言）は 1 つ引けば倒れる
  ["workspaceMemberDirs#Cargo.toml を持つ直下ディレクトリがすべてメンバーに居る", (m) => m.slice(1)],

  ["crateSources#全 workspace member の src/ 直下に .rs が居る", withoutExactDir("snotra-core/src")],
  ["moduleIndexSources#MODULE_INDEX_CRATES の全 crate の src/ 直下に索引対象が居る", withoutExactDir("snotra-core/src")],
  ["skillDocs#.claude/skills/ 直下の全ディレクトリが SKILL.md を持つ", (m) => m.slice(1)],

  ["judgingScripts#scripts/ 直下", withoutExactDir("scripts")],
  ["judgingScripts#scripts/governance/ 直下", withoutExactDir("scripts/governance")],
  ["judgingScripts#scripts/governance/checks/ 直下", withoutExactDir("scripts/governance/checks")],
  ["judgingScripts#scripts/lib/ 直下", withoutExactDir("scripts/lib")],
  ["judgingScripts#.claude/hooks/ 直下", withoutExactDir(".claude/hooks")],
  ["judgingScripts#ps 族（.ps1/.psm1）の腕", withoutMatch(/\.(ps1|psm1)$/i)],
]);

describe("buildDomains", () => {
  it("実ツリーで全ドメインのメンバーが非空である", () => {
    const domains = buildDomains(makeSnapshot(ROOT));
    expect(domains.size).toBe(DOMAIN_SPECS.length);
    for (const d of domains.values()) {
      expect(d.members.length, `ドメイン ${d.name} のメンバーが 0 件`).toBeGreaterThan(0);
    }
  });

  it("実ツリーで全ドメインの錨が成立する", () => {
    const snapshot = makeSnapshot(ROOT);
    const domains = buildDomains(snapshot);
    for (const d of domains.values()) {
      for (const a of d.anchors) {
        expect(a.holds(d.members, snapshot), `ドメイン ${d.name} の錨が成立しない: ${a.label}`).toBe(true);
      }
    }
  });

  it("錨はメンバーが縮むと成立しなくなる（錨が空虚でないことの検算）", () => {
    const snapshot = makeSnapshot(ROOT);
    for (const d of buildDomains(snapshot).values()) {
      for (const a of d.anchors) {
        expect(a.holds([], snapshot), `ドメイン ${d.name} の錨 ${a.label} は空の母集団でも成立する＝空虚`).toBe(false);
      }
    }
  });

  // 「両者の食い違いは #701 の母集団カナリアが捕まえる」は**偽だった**（2026-08-20 実測:
  // `MODULE_INDEX_CRATES` へ `excludeTest` を 1 行足すと両者は食い違い、`governance:check` も
  // `npm test` も緑のまま）。カナリアが固定するのは crate 一覧の片方向だけである。
  // ここで**成り立たねばならない向き**を縛る: 索引の母集団は workspace member の `src/` の外へ出ない。
  // **赤になるのは「外へ出た」形だけである**——`MODULE_INDEX_CRATES` へ Cargo.toml に無い crate を
  // 足しても、その `src/` に実ファイルが無ければメンバーが増えないのでここは緑である（そちらは錨と
  // `checkModuleIndex` が鳴らす）。**等号は要求しない**——`excludeTest` による縮小は正当な設定である。
  it("moduleIndexSources は crateSources の部分集合である", () => {
    const d = buildDomains(makeSnapshot(ROOT));
    const inCrates = new Set(d.get("crateSources").members);
    const outside = d.get("moduleIndexSources").members.filter((f) => !inCrates.has(f));
    expect(outside, `索引の母集団が workspace member の src/ の外へ出ている: ${outside.join(", ")}`).toEqual([]);
  });

  it("adrFiles ドメインは docs/adr/ 直下の md である", () => {
    const m = buildDomains(makeSnapshot(ROOT)).get("adrFiles").members;
    expect(m.length).toBeGreaterThan(0);
    expect(m.every((f) => /^docs\/adr\/[^/]+\.md$/.test(f))).toBe(true);
  });

  it("錨のラベルはドメイン内で一意（写像のキーが衝突しない）", () => {
    for (const spec of DOMAIN_SPECS) {
      const labels = spec.anchors.map((a) => a.label);
      expect(new Set(labels).size, `ドメイン ${spec.name} の錨ラベルが重複: ${labels.join(" / ")}`).toBe(labels.length);
    }
  });

  // --- 錨の反証レシピ（腕ごとの発火を測る） ---------------------------------
  //
  // `holds([], snapshot)` の合成 [] では、腕を丸ごと足し忘れても「非空虚」テストが黙って通る。
  // ここでは**腕だけを引いた集合**を渡し、対応する錨が実際に倒れることを実ツリーで測る。
  //
  // **腕の同定は自動導出できない。** 2026-08-20 に腕を「1 ディレクトリ」で近似して測ったところ、
  // `docs/**`（複数ディレクトリに跨る）と ps 族（拡張子で決まる）の腕を誤分類した。錨の意味を
  // 知っているのは錨を書いた人だけなので、錨ごとに宣言する。
  //
  // **この層は `npm test` に閉じる。** 本番の錨オブジェクトへ持たせれば `G-domain-anchors` が
  // 実行時にも見られたが、この置き方では見られない。得ているのは「レシピ無しの錨を足せない」
  // という構造保証だけであり、評価される層は本番配置と同じではない。
  it("錨の反証レシピが全錨ぶん揃っている（レシピ無しの錨を足せない）", () => {
    const missing = [];
    for (const spec of DOMAIN_SPECS) {
      for (const a of spec.anchors) if (!FALSIFIERS.has(`${spec.name}#${a.label}`)) missing.push(`${spec.name}#${a.label}`);
    }
    expect(missing, `反証レシピの無い錨: ${missing.join(" / ")}（FALSIFIERS へ足すこと）`).toEqual([]);
  });

  it("錨は対応する腕を引くと倒れる（空虚な錨を機構で落とす）", () => {
    const snapshot = makeSnapshot(ROOT);
    for (const spec of DOMAIN_SPECS) {
      const full = spec.members(snapshot);
      for (const a of spec.anchors) {
        const key = `${spec.name}#${a.label}`;
        const falsify = FALSIFIERS.get(key);
        expect(a.holds(full, snapshot), `${key}: 実ツリーで成立していない`).toBe(true);
        const mutated = falsify(full, snapshot);
        // 何も変えていないレシピは「倒れない」を「腕が無い」と取り違えさせる
        expect(mutated, `${key}: レシピが members を変えていない（腕を外している）`).not.toEqual(full);
        // 空集合での失敗は上の別テストが既に見ている——それは `|P| > 0` と同じ強さの証明にしかならない
        expect(mutated.length, `${key}: レシピが空集合を返した`).toBeGreaterThan(0);
        expect(a.holds(mutated, snapshot), `${key}: レシピを当てても成立している＝沈黙する`).toBe(false);
      }
    }
  });
});
