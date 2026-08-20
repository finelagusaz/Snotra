import { fileURLToPath } from "node:url";
import { describe, it, expect } from "vitest";
import { makeSnapshot, workspaceMembers } from "./lib.mjs";
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
/** 全件を 1 階層下へ移す（ディレクトリごと引っ越した形）。直下を見る錨だけが倒れる。
 *  **接頭辞の位置で切る**——`String.replace` は最初の 1 出現を置換するので、パスの途中に同じ綴りが
 *  現れる母集団へ流用したときに静かにずれる。 */
const movedUnder = (d) => (m) => m.map((f) => (f.startsWith(`${d}/`) ? `${d}/moved/${f.slice(d.length + 1)}` : f));

/** **`every` 形の錨には「下界にしている母集団と同じ述語で 1 件だけ引く」レシピを当てる。**
 *
 *  「1 件だけ引く」だけでは足りない——腕を丸ごと引くレシピだと `every → some` の弱化が沈黙するが
 *  （`some` でも腕ごと引けば倒れる）、**述語がずれた「1 件」は誤検出を生む**。下界の外に居る
 *  ファイルを引くと錨は成立したままで、テストは「錨が空虚」という**原因から最も遠いメッセージ**で
 *  赤くなる（`.claude/rules/<dir>/notes.md` や `docs/CLAUDE.md` を置いた実測でレビューが再現）。
 *  それは `domains.mjs` 冒頭が #1143 で禁じた症状そのものである。
 *
 *  ⚠ `findIndex` ゆえ**走査順に依存する**。今日の用途は `every` 形だけなので結果は変わらないが、
 *  `some` 形へ流用すると引く 1 件が順序で変わる。 */
const withoutFirstMatch = (re) => (m) => {
  const i = m.findIndex((f) => re.test(f));
  return m.filter((_, j) => j !== i);
};

/** `"<ドメイン名>#<錨のラベル>"` → 反証レシピ（members を変えて錨を倒す関数）。
 *  **全錨がここに載っていること**は下の完全性 assert が縛る。 */
const FALSIFIERS = new Map([
  // 固定名を名指す錨——名指された側を 1 つ引く
  ["governanceDocs#ルートの AGENTS.md と CLAUDE.md", withoutMatch(/^AGENTS\.md$/)],
  // 別 SSOT（Cargo.toml）からの every——**crate の** CLAUDE.md を 1 つだけ引く。
  // `/^[^/]+\/CLAUDE\.md$/` では足りない: `docs/CLAUDE.md` のような crate でない CLAUDE.md が
  // 走査順で先に来ると、下界の外を引いて錨が成立したまま赤くなる（誤検出・レビューが実測）
  [
    "governanceDocs#CLAUDE.md を持つ workspace member のすべて",
    (m, s) => {
      const crates = workspaceMembers(s).members;
      const i = m.findIndex((f) => crates.some((c) => f === `${c}/CLAUDE.md`));
      return m.filter((_, j) => j !== i);
    },
  ],
  ["governanceDocs#docs/ の腕", withoutPrefix("docs/")],
  ["governanceDocs#.claude/rules/ の腕", withoutPrefix(".claude/rules/")],
  ["governanceDocs#.claude/skills/ の腕", withoutPrefix(".claude/skills/")],

  ["headingRefDocs#docs/ 配下の md", withoutPrefix("docs/")],
  // 述語は `ruleDocs`（`lib.mjs` の `RULE_FILE_RE`）と同じ形にする——前方一致だと
  // `.claude/rules/<dir>/notes.md` のような**下界の外**を引いて誤検出になる（レビューが実測）
  ["headingRefDocs#ruleDocs の全メンバー", withoutFirstMatch(/^\.claude\/rules\/[^/]+\.md$/)],
  ["headingRefDocs#skillDocs の全メンバー", withoutFirstMatch(/^\.claude\/skills\/[^/]+\/SKILL\.md$/)],
  ["headingRefDocs#ルートの AGENTS.md と CLAUDE.md", withoutMatch(/^AGENTS\.md$/)],
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

  // SKILL.md を 1 本引く（先頭が references/ の md でありうるので、SKILL.md を名指しで探す）
  [
    "skillTreeDocs#.claude/skills/ 直下の全ディレクトリが SKILL.md を持つ",
    (m) => {
      const i = m.findIndex((f) => f.endsWith("/SKILL.md"));
      return m.filter((_, j) => j !== i);
    },
  ],
  ["nonDocSources#全 workspace member から .rs が居る", withoutPrefix("snotra-core/")],
  ["nonDocSources#scripts/ 直下", withoutExactDir("scripts")],
  ["nonDocSources#.claude/hooks/ 直下", withoutExactDir(".claude/hooks")],

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
  // **レシピの候補は機械的に生成できる**（前方一致・直下・拡張子・固定名・移設の総当たりで、
  // 現在の錨のほとんどに当たる候補が得られることをレビューが実測した）。**決められないのは
  // 「どれがその錨の意味する腕か」である**——総当たりは ps 族の錨へ `scripts/` の前方一致を返し、
  // 主エージェントが腕を「1 ディレクトリ」で近似したときと同型の誤分類をした。
  // 錨の意味を知っているのは錨を書いた人だけなので、錨ごとに宣言する。
  //
  // **この層は `npm test` に閉じる。** 本番の錨オブジェクトへ持たせれば `G-domain-anchors` が
  // 実行時にも見られたが、この置き方では見られない。得ているのは「レシピ無しの錨を足せない」
  // という構造保証だけであり、評価される層は本番配置と同じではない。
  it("錨の反証レシピが全錨ぶん揃っている（レシピ無しの錨を足せない）", () => {
    const missing = [];
    for (const spec of DOMAIN_SPECS) {
      for (const a of spec.anchors) if (!FALSIFIERS.has(`${spec.name}#${a.label}`)) missing.push(`${spec.name}#${a.label}`);
    }
    expect(
      missing,
      `反証レシピの無い錨: ${missing.join(" / ")}（FALSIFIERS へ足すこと）。` +
        "レシピが書けないと感じたら、まず**その錨が `|P| > 0` と同じ強さでないか**を疑うこと" +
        "——レシピは絞り込みに限らず、移設（movedUnder）や偽メンバーの混入でも倒せる。",
    ).toEqual([]);
  });

  // 逆向き。錨を消した／改名したときに写像へ古いキーが残ると、**それ自身は何も測っていないのに
  // 「レシピが在る」という見た目だけが残る**（写しが腐る形）。両向きで一致を要求する。
  it("反証レシピに、対応する錨の無いキーが残っていない", () => {
    const live = new Set(DOMAIN_SPECS.flatMap((spec) => spec.anchors.map((a) => `${spec.name}#${a.label}`)));
    const stale = [...FALSIFIERS.keys()].filter((k) => !live.has(k));
    expect(stale, `対応する錨の無い反証レシピ: ${stale.join(" / ")}（FALSIFIERS から消すこと）`).toEqual([]);
  });

  // **切り詰めず全件を挙げる。** 最初の 1 本で止めると、複数の錨を同時に弱める変更に対して
  // 「1 本だけ直せば緑になる」と読める（完全性 assert は全件名指しなので、そちらとの非対称も消す）。
  it("錨は対応する腕を引くと倒れる（空虚な錨を機構で落とす）", () => {
    const snapshot = makeSnapshot(ROOT);
    const failures = [];
    for (const spec of DOMAIN_SPECS) {
      const full = spec.members(snapshot);
      for (const a of spec.anchors) {
        const key = `${spec.name}#${a.label}`;
        const falsify = FALSIFIERS.get(key);
        // レシピ欠落は完全性 assert の担当だが、**素通りさせない**——完全性 assert が骨抜きに
        // されたとき、ここまで沈黙すると錨がレシピ無しで入る（冗長な検知点として残す）。
        // 関数として呼べば TypeError にはなるが、それは診断にならない赤である。
        if (typeof falsify !== "function") {
          failures.push(`${key}: 反証レシピが無い（FALSIFIERS へ足すこと）`);
          continue;
        }
        if (!a.holds(full, snapshot)) {
          failures.push(`${key}: 実ツリーで成立していない`);
          continue;
        }
        const mutated = falsify(full, snapshot);
        // 何も変えていないレシピは「倒れない」を「腕が無い」と取り違えさせる
        if (JSON.stringify(mutated) === JSON.stringify(full)) {
          failures.push(`${key}: レシピが members を変えていない（腕を外している）`);
          continue;
        }
        // 空集合での失敗は上の別テストが既に見ている——`|P| > 0` と同じ強さの証明にしかならない
        if (mutated.length === 0) {
          failures.push(`${key}: レシピが空集合を返した`);
          continue;
        }
        if (a.holds(mutated, snapshot)) failures.push(`${key}: レシピを当てても成立している＝沈黙する`);
      }
    }
    // 見出しは「空虚な錨」に限らない——「実ツリーで成立していない」は空虚とは逆の故障であり、
    // レシピ自体の欠陥（何も変えない・空を返す・欠落）も同じ列に並ぶ
    expect(failures, `錨と反証レシピの不整合:\n  ${failures.join("\n  ")}`).toEqual([]);
  });
});
