# governance:check のドメインと錨（Phase 1）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `governance:check` の母集団に名前と錨を与え、母集団が**空でないまま縮む**沈黙を機構で赤くする。

**Architecture:** `buildChecks` が既に中央で組んでいる 8 個の母集団を `domains.mjs` へドメインとして格上げし、各ドメインに構造的な錨を宣言する。メタ検査 1 本が「錨 ⊆ メンバー」と「メンバー非空」を照合する。`registry.mjs` は各検査に「消費するドメイン名の配列」か「未移行マーカー」の宣言を要求し、宣言の無い検査は登録できない（起動時に throw）。未移行の残数は evidence 行に出てラチェットとして働く。

**Tech Stack:** Node.js（標準モジュールのみ）、vitest。`governance-check.mjs` の契約（依存ゼロ・決定的）を継承する。

**Spec:** `docs/superpowers/specs/2026-08-19-governance-population-anchors-design.md`

## Global Constraints

- **前提**: PR #1145（issue #1143）がマージ済みであること。本計画は `G-rules-script-coverage` が存在する木を前提に書かれている。
- **依存ゼロ・決定的**: Node 標準モジュールのみ。ネットワーク・時刻・環境変数に依存しない。
- **メンバーは文字列の配列**。ファイル一覧に限らない（禁止メソッド名・表の行なども母集団になりうる）。
- **錨は構造を名指す。単一ファイルを名指さない。件数を錨にしない。** 単一ファイルを錨にすると、そのファイルの移設だけで赤くなり、しかもメッセージが原因から目を逸らさせる（#1143 で実測）。
- **変異は複製へ当てる。稼働中のガードを弱めない**（`.claude/rules/safety-nets.md`）。複製はリポジトリ直下に置かない——`.mjs` が母集団に入って測定を汚す（#1143 で実測）。スクラッチパッドを使う。
- **`main` へ直接コミットしない。** `feat/governance-domains` を作って作業する。
- 各タスクの終わりに `node scripts/governance-check.mjs`（exit 0）と `npx vitest run`（全緑）を確認する。

---

### Task 1: ドメインの定義（8 個）と錨

**Files:**
- Create: `scripts/governance/domains.mjs`
- Create: `scripts/governance/domains.test.mjs`

**Interfaces:**
- Consumes: `scripts/governance/lib.mjs` の `governanceDocs` / `headingRefDocs` / `headingRefSourceDocs` / `headingRefCommentDocs` / `allHeadingRefDocs` / `staleIdentifierDocs` / `staleIdentifierGuideDocs` / `staleIdentifierTargets` / `workspaceMembers`
- Produces: `DOMAIN_SPECS`（配列）と `buildDomains(snapshot) -> Map<string, {name, members: string[], anchors: {label, holds}[]}>`

- [ ] **Step 1: 失敗するテストを書く**

`scripts/governance/domains.test.mjs`:

```js
import { fileURLToPath } from "node:url";
import { describe, it, expect } from "vitest";
import { makeSnapshot } from "./lib.mjs";
import { buildDomains, DOMAIN_SPECS } from "./domains.mjs";

const ROOT = fileURLToPath(new URL("../../", import.meta.url));

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
});
```

- [ ] **Step 2: テストが落ちることを確認する**

Run: `npx vitest run scripts/governance/domains.test.mjs`
Expected: FAIL — `Cannot find module './domains.mjs'`

- [ ] **Step 3: `domains.mjs` を実装する**

```js
//! ドメイン — 名前つきの母集団と、その**錨**（ここには必ずこれが居る、という構造的事実）。
//!
//! **錨は構造を名指す。単一ファイルを名指さない。件数も錨にしない。**
//! 単一ファイルを錨にすると、そのファイルの移設だけで赤くなり、メッセージが原因から目を逸らさせる
//! （#1143 で実測）。件数を錨にすると、文書が 1 枚増えるたびに赤くなり、無視されるゲートに化ける
//! （`ADR-retire-area-budget` が面積 ratchet について通った道と同じ）。
//!
//! **`|P| > 0` では足りない**——#1143 のとき母集団は空ではなく、facade が 1 件マッチし続けたために
//! 「マッチ 0 件」を見る検査が緑のままだった。錨は「空でないこと」ではなく
//! 「守りたい対象が実際に入っていること」を言う。
import {
  governanceDocs,
  headingRefDocs,
  headingRefSourceDocs,
  headingRefCommentDocs,
  allHeadingRefDocs,
  staleIdentifierDocs,
  staleIdentifierGuideDocs,
  staleIdentifierTargets,
  workspaceMembers,
} from "./lib.mjs";

/** そのディレクトリ**直下**に 1 件以上（前方一致にしない——配下が在れば真になり、
 *  中間層が消えても沈黙する。#1143 で実測した形）。 */
const hasDirectChild = (members, dir) => members.some((f) => f.slice(0, f.lastIndexOf("/")) === dir);

/** `CLAUDE.md` を持つ workspace member（#701 のカナリアと同じ導出。正本は `Cargo.toml`）。 */
const cratesWithClaudeMd = (snapshot) =>
  workspaceMembers(snapshot).members.filter((c) => snapshot.read(`${c}/CLAUDE.md`) !== null);

export const DOMAIN_SPECS = [
  {
    name: "governanceDocs",
    members: governanceDocs,
    anchors: [
      { label: "ルートの AGENTS.md と CLAUDE.md", holds: (m) => m.includes("AGENTS.md") && m.includes("CLAUDE.md") },
      {
        label: "CLAUDE.md を持つ workspace member のすべて",
        holds: (m, s) => {
          const crates = cratesWithClaudeMd(s);
          return crates.length > 0 && crates.every((c) => m.includes(`${c}/CLAUDE.md`));
        },
      },
    ],
  },
  {
    name: "headingRefDocs",
    members: headingRefDocs,
    anchors: [{ label: "docs/ 配下の md", holds: (m) => m.some((f) => f.startsWith("docs/")) }],
  },
  {
    name: "headingRefSourceDocs",
    members: headingRefSourceDocs,
    anchors: [
      {
        label: "CLAUDE.md を持つ crate の src 配下の .rs",
        holds: (m, s) => cratesWithClaudeMd(s).some((c) => m.some((f) => f.startsWith(`${c}/src/`))),
      },
    ],
  },
  {
    name: "headingRefCommentDocs",
    members: headingRefCommentDocs,
    anchors: [
      // #1143 の教訓——判定を持つスクリプトがこの母集団から落ちると、コメントの参照が沈黙で腐る
      { label: "scripts/governance/checks/ 直下", holds: (m) => hasDirectChild(m, "scripts/governance/checks") },
      { label: "scripts/governance/ 直下", holds: (m) => hasDirectChild(m, "scripts/governance") },
      { label: ".claude/hooks/ 直下", holds: (m) => hasDirectChild(m, ".claude/hooks") },
    ],
  },
  {
    name: "allHeadingRefDocs",
    members: allHeadingRefDocs,
    anchors: [
      // 和は 3 腕から成る。腕ごとに 1 つ錨を置く——束ねた長さは他の腕の消滅を隠す
      { label: "md の腕", holds: (m) => m.some((f) => f.endsWith(".md")) },
      { label: ".rs の腕", holds: (m) => m.some((f) => f.endsWith(".rs")) },
      { label: "スクリプトの腕", holds: (m) => m.some((f) => f.endsWith(".mjs") || f.endsWith(".ps1") || f.endsWith(".psm1")) },
    ],
  },
  {
    name: "staleIdentifierDocs",
    members: staleIdentifierDocs,
    anchors: [{ label: ".claude/ 配下の md", holds: (m) => m.some((f) => f.startsWith(".claude/")) }],
  },
  {
    name: "staleIdentifierGuideDocs",
    members: staleIdentifierGuideDocs,
    anchors: [{ label: "docs/ 配下の開発ガイド", holds: (m) => m.some((f) => f.startsWith("docs/")) }],
  },
  {
    name: "staleIdentifierTargets",
    members: staleIdentifierTargets,
    anchors: [{ label: "対象が 1 件以上", holds: (m) => m.length > 0 }],
  },
];

/** 名前 → { name, members, anchors } の Map。`members` はここで 1 度だけ導出する。 */
export function buildDomains(snapshot) {
  const out = new Map();
  for (const spec of DOMAIN_SPECS) {
    out.set(spec.name, { name: spec.name, members: spec.members(snapshot), anchors: spec.anchors });
  }
  return out;
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `npx vitest run scripts/governance/domains.test.mjs`
Expected: PASS（3 件）

- [ ] **Step 5: 錨が空虚でないことを、実ツリーへの変異で 1 件だけ確かめる**

スクラッチパッドへ `lib.mjs` の複製を作り、`WALK_EXCLUDE_PATHS` へ `"scripts/governance"` を足したものから `headingRefCommentDocs` を取り、`hasDirectChild(m, "scripts/governance/checks")` が **false** になることを確認する。稼働中のファイルは触らない。

Expected: 変異あり → false（＝錨が発火する）／変異なし → true

- [ ] **Step 6: コミット**

```bash
git add scripts/governance/domains.mjs scripts/governance/domains.test.mjs
git commit -m "feat(governance): 母集団をドメインとして名づけ、構造的な錨を宣言する"
```

---

### Task 2: メタ検査 `G-domain-anchors`

**Files:**
- Create: `scripts/governance/checks/G-domain-anchors.mjs`
- Create: `scripts/governance/checks/G-domain-anchors.test.mjs`
- Modify: `scripts/governance-check.mjs`（`buildChecks` で `ctx.domains` を組む）

**Interfaces:**
- Consumes: Task 1 の `buildDomains`
- Produces: `checkDomainAnchors(domains, snapshot) -> finding[]`、および `ctx.domains`（Map）

- [ ] **Step 1: 失敗するテストを書く**

`scripts/governance/checks/G-domain-anchors.test.mjs`:

```js
import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkDomainAnchors } from "./G-domain-anchors.mjs";

const dom = (name, members, anchors) => new Map([[name, { name, members, anchors }]]);
const always = { label: "常に成立", holds: () => true };

describe("G-domain-anchors checkDomainAnchors", () => {
  it("緑: 錨が成立し、メンバーが非空", () => {
    expect(checkDomainAnchors(dom("d", ["a.md"], [always]), snap({}))).toEqual([]);
  });

  it("赤: 錨が成立しないとき、ドメイン名と錨の label を名指す", () => {
    const f = checkDomainAnchors(dom("d", ["a.md"], [{ label: "b.md が居る", holds: (m) => m.includes("b.md") }]), snap({}));
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("d");
    expect(f[0].message).toContain("b.md が居る");
  });

  it("赤: メンバーが 0 件（走査の欠落）", () => {
    const f = checkDomainAnchors(dom("d", [], [always]), snap({}));
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("メンバーが 0 件");
  });

  it("赤: ドメインが 1 つも無い（メタ検査自身の母集団の欠落）", () => {
    const f = checkDomainAnchors(new Map(), snap({}));
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("ドメインが 0 件");
  });
});
```

- [ ] **Step 2: テストが落ちることを確認する**

Run: `npx vitest run scripts/governance/checks/G-domain-anchors.test.mjs`
Expected: FAIL — `Cannot find module './G-domain-anchors.mjs'`

- [ ] **Step 3: メタ検査を実装する**

```js
//! G-domain-anchors — ドメインのメンバーに錨が居ることの照合。
//! **これは「検査を検査する層」である**——各検査が見る母集団そのものが縮む向きを、ここ 1 か所で赤くする。
//! 言えるのは「錨が居る」までであり、母集団が正しい・十分であることは言わない（受容する残余）。
import { finding } from "../lib.mjs";

export const id = "G-domain-anchors";
export const domains = ["*"]; // このメタ検査は全ドメインを見る（Task 3 の宣言要求を満たす）

const SELF = "scripts/governance/domains.mjs";

/** @param {object} snapshot  @param {object} ctx `ctx.domains` を使う */
export function run(snapshot, ctx) {
  return checkDomainAnchors(ctx.domains, snapshot);
}

export function checkDomainAnchors(domains, snapshot) {
  if (!domains || domains.size === 0) return [finding(SELF, 1, "ドメインが 0 件（G-domain-anchors 母集団の欠落）")];
  const findings = [];
  for (const d of domains.values()) {
    if (d.members.length === 0) {
      findings.push(finding(SELF, 1, `ドメイン ${d.name} のメンバーが 0 件（走査の欠落）`));
      continue;
    }
    for (const a of d.anchors) {
      if (!a.holds(d.members, snapshot)) {
        findings.push(finding(SELF, 1, `ドメイン ${d.name} の錨が母集団に居ない: ${a.label}`));
      }
    }
  }
  return findings;
}
```

- [ ] **Step 4: `buildChecks` で `ctx.domains` を組む**

`scripts/governance-check.mjs` の `buildChecks` に追加（`import { buildDomains } from "./governance/domains.mjs";` も足す）:

```js
  const domains = buildDomains(snapshot);
  sink.domains = domains;
```

そして `const ctx = { docs, allRefDocs, staleTargets, gitIgnoredPaths, record };` へ `domains` を足す。

- [ ] **Step 5: テストと全体が通ることを確認する**

Run: `npx vitest run scripts/governance/checks/G-domain-anchors.test.mjs && node scripts/governance-check.mjs`
Expected: テスト 4 件 PASS。`governance:check` は exit 0 で、検査の件数が 1 つ増えている

- [ ] **Step 6: コミット**

```bash
git add scripts/governance/checks/G-domain-anchors.mjs scripts/governance/checks/G-domain-anchors.test.mjs scripts/governance-check.mjs
git commit -m "feat(governance): ドメインの錨を照合するメタ検査を置く"
```

---

### Task 3: registry の宣言要求と、未移行のラチェット

**Files:**
- Modify: `scripts/governance/registry.mjs`
- Modify: `scripts/governance/registry.test.mjs`
- Modify: すべての `scripts/governance/checks/G-*.mjs`（宣言 1 行ずつ）
- Modify: `scripts/governance-check.mjs`（未移行の残数を evidence へ）
- Modify: `scripts/governance/evidence.mjs`（evidence テンプレートへ 1 項目）

**Interfaces:**
- Consumes: Task 2 の `ctx.domains`
- Produces: 各検査の `export const domains`（ドメイン名の配列、`["*"]`、または `"unmigrated"`）

- [ ] **Step 1: 失敗するテストを書く**

`scripts/governance/registry.test.mjs` へ追加:

```js
  it("domains を宣言していない検査モジュールはファイル名を名指して throw する", async () => {
    const dir = mkdtempSync(path.join(tmpdir(), "checks-"));
    writeFileSync(path.join(dir, "G-x.mjs"), 'export const id = "G-x";\nexport function run() { return []; }\n');
    await expect(checkModulesFrom(dir)).rejects.toThrow(/G-x\.mjs/);
    rmSync(dir, { recursive: true, force: true });
  });
```

- [ ] **Step 2: テストが落ちることを確認する**

Run: `npx vitest run scripts/governance/registry.test.mjs`
Expected: FAIL — throw されず解決してしまう

- [ ] **Step 3: registry に要求を足す**

`checkModulesFrom` の検証列へ 1 行追加:

```js
    if (!Array.isArray(m.domains) && m.domains !== "unmigrated") {
      throw new Error(`検査モジュールが domains を宣言していない（ドメイン名の配列か "unmigrated"）: ${f}`);
    }
```

- [ ] **Step 4: 全検査へ宣言を 1 行ずつ足す**

`ctx` 経由でドメインを消費している 6 本には、実際に使うドメイン名を書く。

| 検査 | 宣言 |
|---|---|
| `G-adr-citations` | `export const domains = ["governanceDocs"];` |
| `G-heading-refs` | `export const domains = ["allHeadingRefDocs"];` |
| `G-near-heading-refs` | `export const domains = ["allHeadingRefDocs"];` |
| `G-references` | `export const domains = ["governanceDocs"];` |
| `G-spec-sections` | `export const domains = ["governanceDocs"];` |
| `G-stale-identifiers` | `export const domains = ["staleIdentifierTargets"];` |

残りの検査（自前 filter の 11 本と固定パスの 3 本）には `export const domains = "unmigrated";` を書く。

- [ ] **Step 5: 未移行の残数を evidence へ出す**

`scripts/governance-check.mjs` の `runAll` で、`CHECK_MODULES` から未移行を数えて袋へ入れる:

```js
  const unmigrated = CHECK_MODULES.filter((m) => m.domains === "unmigrated").map((m) => m.id);
```

袋へ `unmigrated: unmigrated.length` を入れ、`scripts/governance/evidence.mjs` の `assembleEvidence` のテンプレートへ ` / ドメイン未移行 ${ev.unmigrated} 本` を足す。**未記録の読みは finding になる**ので、供給を外せば赤くなる（#1098）。

- [ ] **Step 6: 全体が通ることを確認する**

Run: `npx vitest run && node scripts/governance-check.mjs`
Expected: すべて緑。evidence 行の末尾に「ドメイン未移行 14 本」（6 本が移行済みなので、検査数 21 − メタ検査 1 − 移行済み 6 = 14）

- [ ] **Step 7: 宣言を落とす変異で、起動時に落ちることを確かめる**

スクラッチパッドの複製で `G-skill-table.mjs` の `domains` 行を消し、`node <複製>/governance-check.mjs` が **`G-skill-table.mjs` を名指して throw する**ことを確認する。

- [ ] **Step 8: コミット**

```bash
git add scripts/governance/registry.mjs scripts/governance/registry.test.mjs scripts/governance/checks/ scripts/governance-check.mjs scripts/governance/evidence.mjs
git commit -m "feat(governance): 検査にドメインの宣言を要求し、未移行の残数を evidence へ出す"
```

---

### Task 4: 同一メンバーのドメインを報告する計器

**Files:**
- Modify: `scripts/governance/instrument.mjs`
- Modify: `scripts/governance/instrument.test.mjs`

**Interfaces:**
- Consumes: Task 1 の `buildDomains`
- Produces: `duplicateDomains(domains) -> string[][]`（同一メンバー集合を持つドメイン名の組）

- [ ] **Step 1: 失敗するテストを書く**

```js
  it("同一メンバーのドメインの組を返す（順序に依らない）", () => {
    const d = new Map([
      ["a", { name: "a", members: ["x", "y"], anchors: [] }],
      ["b", { name: "b", members: ["y", "x"], anchors: [] }],
      ["c", { name: "c", members: ["z"], anchors: [] }],
    ]);
    expect(duplicateDomains(d)).toEqual([["a", "b"]]);
  });

  it("重複が無ければ空", () => {
    const d = new Map([["a", { name: "a", members: ["x"], anchors: [] }]]);
    expect(duplicateDomains(d)).toEqual([]);
  });
```

- [ ] **Step 2: テストが落ちることを確認する**

Run: `npx vitest run scripts/governance/instrument.test.mjs`
Expected: FAIL — `duplicateDomains is not a function`

- [ ] **Step 3: 計器を実装する**

```js
/** 同一メンバーのドメインの組を返す。**合否を持たない**——このリポジトリの「重複に見える」対は
 *  すべて意図的で、畳むと片方向が沈黙する実測記録を持つ（heading-refs↔near /
 *  module-index↔module-linkage / adr-citations↔adr-file-names / rules-globs↔rules-script-coverage）。
 *  ゆえに gate にせず、報告だけを行い判断は人に残す（`ADR-retire-area-budget` と同じ向き）。 */
export function duplicateDomains(domains) {
  const byKey = new Map();
  for (const d of domains.values()) {
    const key = JSON.stringify([...d.members].sort());
    byKey.set(key, [...(byKey.get(key) ?? []), d.name]);
  }
  return [...byKey.values()].filter((names) => names.length > 1).map((names) => names.sort());
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `npx vitest run scripts/governance/instrument.test.mjs`
Expected: PASS

- [ ] **Step 5: 実ツリーの結果を計器として出す**

`runAll` の計器の並びへ、`duplicateDomains` の結果が空でないときだけ 1 行を印字する（**findings には積まない**）。

- [ ] **Step 6: コミット**

```bash
git add scripts/governance/instrument.mjs scripts/governance/instrument.test.mjs scripts/governance-check.mjs
git commit -m "feat(governance): 同一メンバーのドメインを計器として報告する（合否は持たない）"
```

---

### Task 5: 最初の移行 — `adrFiles` ドメイン

`docs/adr/[^/]+\.md` は `G-adr-file-names.mjs:30` と `G-adr-citations.mjs:38` に**同じ正規表現で 2 回**書かれている。extract-on-second-use の 2 本目が既に居るので、最初の移行はここから始める。

**Files:**
- Modify: `scripts/governance/domains.mjs`（`adrFiles` ドメインを足す）
- Modify: `scripts/governance/checks/G-adr-file-names.mjs`
- Modify: `scripts/governance/checks/G-adr-citations.mjs`
- Modify: `scripts/governance/domains.test.mjs`

**Interfaces:**
- Consumes: Task 1 の `DOMAIN_SPECS`
- Produces: ドメイン `adrFiles`（メンバー = `docs/adr/*.md`）

- [ ] **Step 1: 移送前の母集団を凍結する（オラクル）**

Run: `node --input-type=module -e 'import { makeSnapshot } from "./scripts/governance/lib.mjs"; import { adrFiles } from "./scripts/governance/checks/G-adr-file-names.mjs"; console.log(JSON.stringify(adrFiles(makeSnapshot(process.cwd())).sort()))' > adr-oracle.json`

この JSON は**このコミットの中でだけ使う足場**である。コミットしない。

- [ ] **Step 2: 失敗するテストを書く**

`domains.test.mjs` へ追加:

```js
  it("adrFiles ドメインは docs/adr/ 直下の md である", () => {
    const m = buildDomains(makeSnapshot(ROOT)).get("adrFiles").members;
    expect(m.length).toBeGreaterThan(0);
    expect(m.every((f) => /^docs\/adr\/[^/]+\.md$/.test(f))).toBe(true);
  });
```

- [ ] **Step 3: テストが落ちることを確認する**

Run: `npx vitest run scripts/governance/domains.test.mjs`
Expected: FAIL — `Cannot read properties of undefined (reading 'members')`

- [ ] **Step 4: ドメインを足し、2 本の検査を寄せる**

`DOMAIN_SPECS` へ:

```js
  {
    name: "adrFiles",
    members: adrFiles,
    anchors: [{ label: "docs/adr/ 直下", holds: (m) => hasDirectChild(m, "docs/adr") }],
  },
```

（この `members: adrFiles` は Step 4 の帰結を先取りして書いてある。実際に `adrFiles` を import して渡す形は下の「依存の向きに注意する」で導く——この時点ではまだ import していないので、コードを逐語で動かすなら Step 4 の下の指示まで読んでから書くこと。）

**依存の向きに注意する。** `adrFiles` は facade（`governance-check.mjs:52`）が evidence のために**静的 import している**——`ADR-facade-evidence-static-imports` が「ファイルが消えれば import が失敗して鳴る」性質として意図的に残したものであり、**壊してはならない**。

ゆえに `G-adr-file-names.mjs` の `adrFiles(snapshot)` は**そのまま残し**、`domains.mjs` が**それを import してドメインの `members` に据える**（向きを逆にする）。こうすると定義は 1 つ、facade の静的 import も無傷になる。

```js
// domains.mjs へ追加する import
import { adrFiles } from "./checks/G-adr-file-names.mjs";
```

`DOMAIN_SPECS` の `members` は `adrFiles` をそのまま渡す。`G-adr-citations.mjs:38` の `snapshot.files.filter(...)` を `ctx.domains.get("adrFiles").members` へ差し替える（**写しが消えるのはこちら側である**）。`domains` 宣言は `G-adr-file-names` が `["adrFiles"]`、`G-adr-citations` が `["governanceDocs", "adrFiles"]`。

**循環参照にならないことを確かめる**: `checks/` → `lib.mjs`、`domains.mjs` → `lib.mjs` + `checks/G-adr-file-names.mjs`、`governance-check.mjs` → `domains.mjs`。`G-adr-citations` は `domains.mjs` を import せず `ctx` 経由で受け取るので、閉路は生じない。

- [ ] **Step 5: 移送前後で母集団が集合として同一であることを測る**

Run: `node --input-type=module -e 'import fs from "node:fs"; import { makeSnapshot } from "./scripts/governance/lib.mjs"; import { buildDomains } from "./scripts/governance/domains.mjs"; const now = buildDomains(makeSnapshot(process.cwd())).get("adrFiles").members.slice().sort(); const before = JSON.parse(fs.readFileSync("adr-oracle.json","utf8")); console.log("同一:", JSON.stringify(now) === JSON.stringify(before), now.length)'`

Expected: `同一: true`

- [ ] **Step 6: 全体が通ることを確認し、足場を消す**

Run: `npx vitest run && node scripts/governance-check.mjs && rm adr-oracle.json`
Expected: すべて緑。evidence の「ドメイン未移行」が 14 → 13 本へ減る

- [ ] **Step 7: コミット**

```bash
git add scripts/governance/domains.mjs scripts/governance/domains.test.mjs scripts/governance/checks/G-adr-file-names.mjs scripts/governance/checks/G-adr-citations.mjs
git commit -m "refactor(governance): docs/adr の母集団を adrFiles ドメインへ寄せる（写しを 1 つ畳む）"
```

---

## この計画の範囲外（後続の計画）

- **Phase 2 の残り**（自前 filter の 10 本の移行）。1 クラスタ 1 コミットで進める。クラスタの形は移行しながら決まるので、コミット数は事前に確定しない。
- **Phase 3**（固定パスの 3 本へ錨を付ける）。
- **Phase 4**（未移行マーカーの削除と、`WALK_EXCLUDE_PATHS` のドメイン単位への移動）。**未移行が 0 になるまで着手できない。**

## Phase 2 への申し送り（Phase 1 の全体レビューが実測したもの）

Phase 1 は完了したが、**この Phase が自ら掲げた約束のうち 2 つは Phase 2 の最初のコミットへ送った**。
どちらも数行で閉じられる。

- **I3: `DOMAIN_SPECS` からドメインを 1 つ消しても、どの層も見ない。** `governance-manifest.mjs` の 4 列は
  ドメインを持たず、`domains.test.mjs` の `domains.size === DOMAIN_SPECS.length` は両辺が同時に減るので
  見えない（名前の衝突は捕まえる）。`adrFiles` だけは消費側が `TypeError` で鳴るが、他は無言で消える。
  **「錨が 0 本は赤、ドメインごと消えるのは緑」という非対称**であり、manifest へ `domains`（名前の集合）を
  5 列目として足すのが自然。設計の却下理由は**件数**を列にする案に向けられたもので、名前の集合は
  ドメインを増減させたときだけ動くため当たらない。
- **I4: ラチェットは逆向きの機構を持たない。** 未移行の残数は成功時に印字されるだけで、増えても何も落ちない。
  Phase 2 の入口で、現在の未移行 id を凍結して「未移行 ⊆ 凍結集合」を検めるテストを 1 件置けば機構になる。
- **Phase 3 は設計どおりには進めない。** 固定パスの 3 本は members が literal 配列になるため、その錨は
  `holds([], s)` テストを通りながら実質的に空虚になる。**完了判定を「腕ごとの絞り込みで発火を測る」形へ
  書き換えてから着手する**こと。
- **Phase 4 の着手条件**（未移行が 0）は、`adrCitationDocs` の省略可能な第 3 引数（同じ母集団への 2 本目の
  経路）と併せて扱う——`WALK_EXCLUDE_PATHS` をドメイン単位へ移した時点で、単体呼び出しが本番と別の
  母集団を見る。
- **未着手のまま残した沈黙**: `headingRefDocs` の `.claude/` 腕と、ルート直下の腕。I2 と同クラスだが、
  この母集団は単一の filter（除外句の列）から出ており腕の切り分けが自明でないため、錨を足すと
  正当な変更まで赤にする側へ倒れうる。除外句が増える将来の見積もりとセットで判断する。

## Self-Review

- **Spec coverage**: §2.1 錨 → Task 1 / §2.2 登録時強制とラチェット → Task 3 / §2.3 メタ検査 → Task 2 / §3 Phase 1 → Task 1〜4 / §3 Phase 2 の入口 → Task 5 / §6「同一メンバーは gate にしない」→ Task 4。**§4 の「メンバーは文字列の配列」は Task 1 の型で満たす。** §3 Phase 3・4 は本計画の範囲外として明示した。
- **Placeholder scan**: 「適切に」「必要に応じて」の類は無し。全ステップに実コードか実コマンドを置いた。
- **Type consistency**: `buildDomains` は Map を返し、要素は `{name, members, anchors}`。`anchors` の要素は `{label, holds(members, snapshot)}`。Task 2・4・5 はこの形だけを読む。検査の宣言は `export const domains`（配列 / `["*"]` / `"unmigrated"`）で、registry の検証と Task 3 の表が一致している。
