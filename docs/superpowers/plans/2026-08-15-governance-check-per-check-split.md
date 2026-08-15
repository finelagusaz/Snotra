# governance:check の per-check 分割 — 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 19 検査を `scripts/governance/checks/` へ 1 検査 1 ファイルで移し、registry を `readdirSync` から導出することで、**忘れうる登録行そのものを消す**（#1088 の構造的解消）。

**Architecture:** `scripts/governance-check.mjs`（2193 行）を 4 層へ分ける——共有基盤 `scripts/governance/lib.mjs`、1 検査 1 ファイルの `scripts/governance/checks/G-*.mjs`、`readdirSync` + top-level await の `scripts/governance/registry.mjs`、そして `buildChecks` / `runAll` / CLI と公開 API を保つ facade（`governance-check.mjs` のまま）。**母集団の計算・0 件検知・evidence の組み立て・CLI は facade に残す**——移すのは検査の本体だけであり、再設計はしない。

**Tech Stack:** Node.js 22（標準モジュールのみ）、vitest。

**Spec:** `docs/superpowers/specs/2026-08-14-governance-check-scope-design.md`（§5 が本計画の対象）

## Global Constraints

- **`main` へ直接コミット・プッシュしない。** ブランチ `chore/governance-per-check-split` を `main` から切る。
- **移すコードは逐語で移す。** 移送中にコメントの言い回し・変数名・実装を変えない（例外は下の「契約コメント」1 か所だけ）。4000 行規模の差分がレビュー可能なのは「そのまま移った」ことを検算できるからであり、ついでの改善はその性質を壊す。
- **evidence 行を 1 バイトも変えない。** これが本計画で最強の不変条件である（Task 1 で凍結し、以降の各コミットで byte 比較する）。**実行時に測定・訂正: この形は強すぎた。** 新設した source file が ADR を正当に引用すれば、その引用自体が `ADR の短縮引用` の件数を動かす——本計画自身が `registry.mjs` と `instrument.mjs` にそうしたヘッダを書くよう指示しており、実際に両者で 1 件ずつ動いた（予測どおりの 2 か所）。運用として実際に効いたのは:**ADR 短縮引用の件数を除く全フィールドは 1 バイトも変わらない。その件数だけは、新設 source file での正当な新規引用に限り動いてよく、動かすときは原因を名指しして baseline を撮り直す。それ以外の動きは欠陥である。**
- **`scripts/governance-check.mjs` の契約を壊さない**（ファイル冒頭の「契約」コメント）: 依存ゼロ（Node 標準のみ）・決定的（ネットワーク・時刻・環境変数に非依存）・各検査はスナップショット注入の純関数・findings ゼロなら exit 0 と母集団の件数を印字・空母集団は明示 fail。
- **公開 API を壊さない。** `scripts/governance-manifest.mjs` が `makeSnapshot` / `buildChecks` / `governanceDocs` を、`scripts/governance-check.test.mjs` が 50 個の名前を `./governance-check.mjs` から import している。**facade がすべて re-export する**——壊せば、A を守るために作った B が同じ PR で壊れる。
- **`checkNormativeAreaInstrument` を registry に入れてはならない。** 面積に合否は無く（`ADR-retire-area-budget`）、入れれば manifest 差分に `+G-area-instrument` として現れる。`checks/` の外（`scripts/governance/instrument.mjs`）へ置き、**ディレクトリ境界が「検査であること」の定義になる**。
- **この PR の manifest delta は 0 件が受け入れ条件である。** 検査 ID 19 件・対象文書・rules・skills のいずれも変わらない。**非ゼロなら分割のバグ**であり、PR 2 で入れた `governance manifest delta` step が A の harness として実際に働く。
- **フォールトインジェクションは稼働中のガードを弱めず、複製に変異を当てる**（`.claude/rules/safety-nets.md`）。
- **訳語**: coverage は「カバレッジ」、window (GUI) は「ウィンドウ」。原語の字面をそのまま漢語へ置き換えない（ルート `CLAUDE.md` の訳語規則）。
- **各タスクの検証**: `npm test`・`npm run governance:check`・evidence の byte 比較の 3 点。`.md` の編集には PostToolUse hook が検査を割り当てない——**沈黙は「何も走らなかった」**であり、`governance:check` は手で打つ。

---

## 実測（この計画の前提。実装者は再測不要）

2026-08-15 に scratchpad の使い捨て probe で測った。

| 測ったこと | 結果 |
|---|---|
| `readdirSync` + top-level await の registry を同期 `import` した先から使えるか | **node・vitest とも成功** |
| `id` を持たないモジュールを `checks/` へ置いたとき | **exit 1・ファイル名を名指しして throw**（対照は exit 0） |
| `scripts/governance/checks/*.test.mjs` が `npm test` に入るか | **入る**（`vitest.config.mjs` の include が `scripts/**/*.test.mjs`） |

**registry は `import.meta.url` 起点で走査する。`process.cwd()` 起点にしてはならない**——PR 2 の CI step は base 側のコードを `/tmp/gov-base` を cwd にして走らせる。cwd 起点だと「読むコードと読む木がずれる」（PR 2 の H1 と同じ型）。

---

## ファイル構成

| ファイル | 責務 |
|---|---|
| `scripts/governance/lib.mjs` | 2 つ以上の移送先が使う基盤——`makeSnapshot`・`finding`・走査除外・`linesOutsideFences`・`gitIgnoredPaths`・母集団関数（`governanceDocs` 等）・見出しアンカーの解決 |
| `scripts/governance/checks/G-<id>.mjs` | 1 検査 1 ファイル。`id`（string）と `run(snapshot, ctx)`（`Finding[]` を返す）を export。**ファイル名の stem は `id` と一致させる**——`readdirSync` の一覧がそのまま ID の一覧になる |
| `scripts/governance/registry.mjs` | `checks/` を `readdirSync` + `sort` し、top-level await で import して形を検証する。**忘れうる登録行はここに存在しない** |
| `scripts/governance/instrument.mjs` | `checkNormativeAreaInstrument`・`normativeArea`・`skillDescriptionArea`・`ALWAYS_LOADED_FILES`。**`checks/` の外に在ることが「検査ではない」の担保** |
| `scripts/governance-check.mjs`（facade） | `buildChecks` / `runAll` / CLI / 公開 API の re-export。母集団の計算・0 件検知・evidence の組み立てはここに残る |

**検査モジュールの契約:**

```javascript
export const id = "G-example";
/** @param snapshot makeSnapshot の返り値  @param ctx buildChecks が組む共有母集団 */
export function run(snapshot, ctx) {
  return []; // Finding[]
}
```

`ctx` が運ぶもの（facade が組む。**今の `buildChecks` が既に計算しているものをそのまま渡すだけ**）:

| キー | 中身 | 使う検査 |
|---|---|---|
| `docs` | `governanceDocs(snapshot)` | G-references / G-spec-sections / G-adr-citations |
| `allRefDocs` | `headingRefDocs` と `headingRefSourceDocs` を連結したもの | G-heading-refs / G-near-heading-refs |
| `staleTargets` | `staleIdentifierTargets(snapshot)` | G-stale-identifiers |
| `gitIgnoredPaths` | `lib.mjs` の同名関数 | G-references |
| `record(key, {checked, findings})` | evidence 用の件数を facade の sink へ書き、findings を返す | G-adr-citations / G-heading-refs / G-near-heading-refs / G-stale-identifiers |

**evidence 専用の導出は facade が検査モジュールから名指しで import する** —— `workspaceMembers`（G-workspace-lints）・`clippyDisallowedCount`（G-clippy-disallowed）・`adrFiles`（G-adr-file-names）・`normativeArea`（instrument）。登録行と違い、**ファイルが消えれば import が失敗して鳴る**（fail-closed）ので、沈黙する写しにはならない。

**検査 → ファイルの割り当て（19 件・母集団は現行 `buildChecks` の配列）:**

| 検査 ID | 現行の行範囲 | 移送先 |
|---|---|---|
| G-module-index | 106-158 | `checks/G-module-index.mjs` |
| G-module-linkage | 159-385 | `checks/G-module-linkage.mjs` |
| G-architecture-table | 386-401 | `checks/G-architecture-table.mjs` |
| G-references | 402-499 | `checks/G-references.mjs`（`gitIgnoredPaths` は lib へ） |
| G-spec-sections | 500-552 | `checks/G-spec-sections.mjs` |
| G-workspace-lints | 553-714 | `checks/G-workspace-lints.mjs` |
| G-clippy-disallowed | 715-918 | `checks/G-clippy-disallowed.mjs` |
| G-build-commands | 919-956 | `checks/G-build-commands.mjs` |
| G-ci-table | 957-1023 | `checks/G-ci-table.mjs` |
| G-rules-globs | 1024-1087 | `checks/G-rules-globs.mjs` |
| G-skill-table | 1088-1147 | `checks/G-skill-table.mjs` |
| G-hook-commands | 1148-1204 | `checks/G-hook-commands.mjs` |
| G-hook-fires | 1205-1333 | `checks/G-hook-fires.mjs`（`selectChecks` の import を保存する） |
| （計器） | 1334-1441 | `instrument.mjs`——**`checks/` ではない** |
| G-heading-refs | 1442-1534 | `checks/G-heading-refs.mjs` |
| G-near-heading-refs | 1535-1612 | `checks/G-near-heading-refs.mjs` |
| （母集団） | 1613-1706 | `lib.mjs` |
| G-stale-identifiers | 1707-1920 | `checks/G-stale-identifiers.mjs` |
| G-check-skill-enumeration | 1921-1993 | `checks/G-check-skill-enumeration.mjs` |
| G-adr-file-names | 1994-2044 | `checks/G-adr-file-names.mjs` |
| G-adr-citations | 2045-2110 | `checks/G-adr-citations.mjs` |

---

### Task 1: baseline を凍結する

**Files:**
- Create: `<SDD workspace>/baseline-evidence.txt`（git 管理外）
- Create: `<SDD workspace>/baseline-ids.txt`（git 管理外）

**Interfaces:**
- Produces: 以降のすべてのタスクが比較する基準ファイル 2 本と、その比較コマンド。

**背景（実装者向け）:** この分割は「挙動を変えない移送」である。挙動不変を主張する根拠は、**evidence 行が 1 バイトも変わらないこと**と**検査 ID の集合が変わらないこと**の 2 点で足りる。evidence は 12 個の母集団の件数を 1 行へ畳んだ文字列なので、どれか 1 つでも取りこぼせば必ず変わる。

⚠ **これをテストへ焼き付けてはならない。** evidence の数値は文書を 1 枚足すだけで動くので、pin するテストを書くと本計画と無関係な PR が赤くなる。比較は**この PR の中だけ**の手作業として行う。

- [ ] **Step 1: baseline を取る**

Run:
```bash
node scripts/governance-check.mjs > "<workspace>/baseline-evidence.txt"
node --input-type=module -e "import { makeSnapshot, buildChecks } from './scripts/governance-check.mjs'; console.log(buildChecks(makeSnapshot(process.cwd()), {}).map((c) => c.id).sort().join('\n'));" > "<workspace>/baseline-ids.txt"
```

- [ ] **Step 2: 中身を目視で確かめる**

Run: `cat "<workspace>/baseline-evidence.txt"; wc -l "<workspace>/baseline-ids.txt"`
Expected: evidence は `governance:check — 全検査 passed（検査 19 件 / …）` の 1 行。ID は 19 行。

**2026-08-15 時点の evidence（参考。実行時の値が正本）:**

```
governance:check — 全検査 passed（検査 19 件 / 対象文書 35 件 / rules 8 件 / skills 12 件 / 恒久規範 常時ロード 15595 字・rules 11702 字 / 見出し参照 209 件を md 47 件 + .rs 101 件から照合 / workspace member 4 件の lints opt-in / clippy 禁止 8 件 / 散文の識別子 370 件を 33 文書から照合 / 近傍の見出し参照 13 件 / ADR 50 本の名前 / ADR の短縮引用 250 件）
```

- [ ] **Step 3: 以降の全タスクで使う検証コマンドを控える**

```bash
node scripts/governance-check.mjs > /tmp/now-evidence.txt && diff "<workspace>/baseline-evidence.txt" /tmp/now-evidence.txt && echo "evidence 一致"
```

⚠ **`diff` が無音で終わることを「一致」と読まない。** 上のように `&& echo` を付け、**その行が出ることを確認する**（`diff` が走らなかった場合と区別できない）。

- [ ] **Step 4: コミットしない**

baseline は SDD の作業領域（git 管理外）に置く。リポジトリへ入れない——この PR が閉じれば無意味になる紙片であり、撤去の合図を持たない足場を残さない（`AGENTS.md`「調査・測定のための一時的な足場」）。

---

### Task 2: `scripts/governance/lib.mjs` を切り出す

**Files:**
- Create: `scripts/governance/lib.mjs`
- Modify: `scripts/governance-check.mjs`（該当宣言を削除し、lib から import して re-export）

**Interfaces:**
- Produces: `lib.mjs` が以下を export する。名前・引数・実装は現行と**逐語で同一**。
  - `makeSnapshot(root)` / `finding(file, line, message)` / `linesOutsideFences(text)`
  - `gitIgnoredPaths(paths, root = process.cwd())`
  - `governanceDocs(snapshot)` / `headingRefDocs(snapshot)` / `headingRefSourceDocs(snapshot)`
  - `staleIdentifierDocs(snapshot)` / `staleIdentifierGuideDocs(snapshot)` / `staleIdentifierTargets(snapshot)`
  - `collectAnchors(text)` / `resolveRefTarget(snapshot, doc, target)`
  - 上記が使う private な定数・関数（`REF_EXTENSIONS`・`WALK_EXCLUDE_NAMES`・`WALK_EXCLUDE_PATHS`・`normAnchor`・`stripRustComments`・`productionOnly`・`STALE_EXTRA_DOCS` 等）
- Consumes: なし（最下層）。

**背景（実装者向け）:** lib に入れる基準は **「2 つ以上の移送先が使うか、facade が母集団・0 件検知・evidence のために使うか」** である。1 つの検査しか使わない helper は lib へ入れず、その検査のファイルへ一緒に移す。

- [ ] **Step 1: 移送対象を実測で確定する**

Run:
```bash
for f in linesOutsideFences collectAnchors resolveRefTarget normAnchor stripRustComments productionOnly staleTarget; do printf "%-24s " "$f"; grep -on "\b$f\b" scripts/governance-check.mjs | awk -F: '{print $1}' | tr '\n' ' '; echo; done
```

各行の使用位置を「検査 → ファイル」表の行範囲へ当て、**2 つ以上の移送先にまたがるものだけ**を lib へ入れる。2026-08-15 の実測では `linesOutsideFences`（8 か所・多数の検査）・`collectAnchors`・`resolveRefTarget`・`normAnchor`（G-heading-refs と G-near-heading-refs の 2 つ）がまたがっていた。`stripRustComments` と `productionOnly` は**自分で測って決める**——定義位置は母集団ブロックだが、使用位置が 1 つの検査に閉じているなら lib ではなくその検査へ移す。

- [ ] **Step 2: `lib.mjs` を作り、宣言を逐語で移す**

`scripts/governance/lib.mjs` の冒頭へ:

```javascript
//! governance:check の共有基盤。**2 つ以上の移送先が使うものだけ**を置く——
//! 1 つの検査しか使わない helper はその検査のファイルへ置く（`scripts/governance/checks/`）。
//! 依存は Node 標準モジュールのみ（`governance-check.mjs` の契約を継承する）。
```

以下、現行 `governance-check.mjs` から Step 1 で確定した宣言を**逐語で**移す。コメントも含めて 1 文字も変えない。

- [ ] **Step 3: facade から re-export する**

`scripts/governance-check.mjs` の該当宣言を削除し、冒頭の import 群の直後へ:

```javascript
import {
  makeSnapshot,
  finding,
  linesOutsideFences,
  gitIgnoredPaths,
  governanceDocs,
  headingRefDocs,
  headingRefSourceDocs,
  staleIdentifierDocs,
  staleIdentifierGuideDocs,
  staleIdentifierTargets,
  collectAnchors,
  resolveRefTarget,
} from "./governance/lib.mjs";

// 既存の import 元（`governance-manifest.mjs` と `governance-check.test.mjs`）を壊さないための再輸出。
// **`export *` にしない**——公開する名前を明示的に持つことで、意図しない露出が起きない。
export {
  makeSnapshot,
  gitIgnoredPaths,
  governanceDocs,
  headingRefDocs,
  headingRefSourceDocs,
  staleIdentifierDocs,
  staleIdentifierGuideDocs,
  staleIdentifierTargets,
  collectAnchors,
  resolveRefTarget,
};
```

⚠ `finding` と `linesOutsideFences` は現行も export されていない。**export を増やさない**——公開面を広げるのは移送ではない。

- [ ] **Step 4: 検証する**

Run: `npm test`
Expected: PASS（テストは `./governance-check.mjs` から import したままなので、再輸出が効いていれば 1 件も落ちない）

Run: `npm run governance:check`
Expected: 全検査 passed

Run: `node scripts/governance-check.mjs > /tmp/now-evidence.txt && diff "<workspace>/baseline-evidence.txt" /tmp/now-evidence.txt && echo "evidence 一致"`
Expected: `evidence 一致` が出る

- [ ] **Step 5: コミット**

```bash
git add scripts/governance/lib.mjs scripts/governance-check.mjs
git commit -F <メッセージを書いたファイル>
```

メッセージ: `refactor: governance:check の共有基盤を scripts/governance/lib.mjs へ切り出す (#1088)`

⚠ コミットメッセージは `git commit -F <path>` で渡す。bash の HEREDOC も PowerShell の here-string も、この環境では本文が壊れる（実測）。直後に `git log -1 --format=%B` で読み直す。

---

### Task 3: registry と検査モジュールの契約（G-architecture-table を移して型を決める）

**Files:**
- Create: `scripts/governance/registry.mjs`
- Create: `scripts/governance/checks/G-architecture-table.mjs`
- Create: `scripts/governance/registry.test.mjs`
- Modify: `scripts/governance-check.mjs`（`buildChecks` を registry ベースへ）

**Interfaces:**
- Consumes: Task 2 の `lib.mjs`。
- Produces:
  - `registry.mjs` が `CHECK_MODULES: Array<{id: string, run: Function}>`（ID 昇順）と `checkModulesFrom(dir): Promise<Array>`（テスト用にディレクトリを差し替えられる形）を export する。
  - `checks/G-architecture-table.mjs` が `id = "G-architecture-table"` と `run(snapshot, ctx)` を export する。

**背景（実装者向け）:** G-architecture-table は 16 行と最小で、`ctx` も使わない。**型を決めるためだけに 1 本目として選んでいる**——ここで決めた形を残り 18 本がなぞる。

`checkModulesFrom(dir)` を分けるのは、**フォールトインジェクションでライブの `checks/` を汚さないため**である（`.claude/rules/safety-nets.md`「稼働中のガードを弱めない——複製に変異を当てる」）。Task 11 が使い捨てディレクトリを渡す。

- [ ] **Step 1: 失敗するテストを書く**

`scripts/governance/registry.test.mjs`:

```javascript
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, it, expect } from "vitest";
import { CHECK_MODULES, checkModulesFrom } from "./registry.mjs";

describe("registry（checks/ の走査から導出する・#1088）", () => {
  it("検査モジュールが 1 本以上あり、すべて id と run を持つ", () => {
    expect(CHECK_MODULES.length).toBeGreaterThan(0);
    for (const m of CHECK_MODULES) {
      expect(typeof m.id, `${m.id} の id が string でない`).toBe("string");
      expect(typeof m.run, `${m.id} の run が function でない`).toBe("function");
    }
  });
  it("id は昇順（readdir 順の揺れが出力順に出ない）", () => {
    const ids = CHECK_MODULES.map((m) => m.id);
    expect(ids).toEqual([...ids].sort());
  });
  it("id が重複しない", () => {
    const ids = CHECK_MODULES.map((m) => m.id);
    expect(new Set(ids).size, `重複: ${ids.join(",")}`).toBe(ids.length);
  });
});

describe("registry の形の検証（複製に変異を当てる）", () => {
  const withDir = async (files, fn) => {
    const dir = mkdtempSync(path.join(tmpdir(), "gov-registry-"));
    try {
      for (const [name, body] of Object.entries(files)) writeFileSync(path.join(dir, name), body);
      return await fn(dir);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  };
  it("id を持たないモジュールがあれば throw する（沈黙して落とさない）", async () => {
    await withDir({ "G-a.mjs": 'export const id = "G-a";\nexport function run() { return []; }\n', "G-bad.mjs": "export function run() { return []; }\n" }, async (dir) => {
      await expect(checkModulesFrom(dir)).rejects.toThrow(/G-bad\.mjs/);
    });
  });
  it("run を持たないモジュールがあれば throw する", async () => {
    await withDir({ "G-bad.mjs": 'export const id = "G-bad";\n' }, async (dir) => {
      await expect(checkModulesFrom(dir)).rejects.toThrow(/G-bad\.mjs/);
    });
  });
  it("ファイル名の stem と id が食い違えば throw する（一覧が ID の一覧であることの担保）", async () => {
    await withDir({ "G-a.mjs": 'export const id = "G-different";\nexport function run() { return []; }\n' }, async (dir) => {
      await expect(checkModulesFrom(dir)).rejects.toThrow(/G-a\.mjs/);
    });
  });
  it("`.test.mjs` は検査として読まない", async () => {
    await withDir({ "G-a.mjs": 'export const id = "G-a";\nexport function run() { return []; }\n', "G-a.test.mjs": "export const nothing = 1;\n" }, async (dir) => {
      expect((await checkModulesFrom(dir)).map((m) => m.id)).toEqual(["G-a"]);
    });
  });
});
```

- [ ] **Step 2: 落ちることを確認する**

Run: `npx vitest run scripts/governance/registry.test.mjs`
Expected: FAIL — `Cannot find module './registry.mjs'`

- [ ] **Step 3: `registry.mjs` を書く**

```javascript
//! 検査の登録は**ディレクトリの走査から導出する**——`checks/` に置いたファイルがそのまま検査になり、
//! 忘れうる登録行が存在しない（#1088 が問うた欠陥の構造的な解消）。
//! **`checks/` の外にあるものは検査ではない**——合否を持たない計器は `instrument.mjs` に置く
//! （`ADR-retire-area-budget`）。
import { readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// 走査元は `import.meta.url` 起点である。**`process.cwd()` 起点にしてはならない**——CI の
// manifest 差分は base 側のコードを別ツリーを cwd にして走らせるため、cwd 起点だと
// 「読むコードと読む木がずれる」（#1092 の H1 と同じ型）。
const CHECKS_DIR = path.join(path.dirname(fileURLToPath(import.meta.url)), "checks");

/** `dir` 直下の検査モジュールを id 昇順で返す。形が不正なら**そのファイル名を名指しして throw する**
 *  ——沈黙して 1 本落とすと #1088 の欠陥がそのまま戻る。テストが使い捨てディレクトリを渡せるよう
 *  引数に取る（稼働中の `checks/` へ変異を当てないため・`.claude/rules/safety-nets.md`）。 */
export async function checkModulesFrom(dir) {
  const files = readdirSync(dir)
    .filter((f) => f.endsWith(".mjs") && !f.endsWith(".test.mjs"))
    .sort();
  const mods = [];
  for (const f of files) {
    // `import()` へ素の Windows パスを渡すと ERR_UNSUPPORTED_ESM_URL_SCHEME になる。
    // 自前で組み立てず `pathToFileURL` を使う——区切り・ドライブレター・percent-encode の
    // 扱いを自作すると、動く機体と動かない機体が生まれる
    const m = await import(pathToFileURL(path.join(dir, f)).href);
    if (typeof m.id !== "string") throw new Error(`検査モジュールが id を export していない: ${f}`);
    if (typeof m.run !== "function") throw new Error(`検査モジュールが run を export していない: ${f}`);
    if (m.id !== path.basename(f, ".mjs")) throw new Error(`ファイル名と id が食い違う: ${f}（id=${m.id}）`);
    mods.push(m);
  }
  return mods;
}

export const CHECK_MODULES = await checkModulesFrom(CHECKS_DIR);
```

⚠ **CI は ubuntu、開発機は Windows である。** `pathToFileURL` を使うのはその両方で同じ形が動くためで、パスの組み立てを自作してはならない。Step 6 の `npm test` が Windows 側の実測、PR の `governance-check` job が ubuntu 側の実測になる。

- [ ] **Step 4: 1 本目の検査を逐語で移す**

`scripts/governance/checks/G-architecture-table.mjs`:

```javascript
//! G-architecture-table — `docs/architecture.md` の対応表 ↔ 実ファイル。
import { finding, linesOutsideFences } from "../lib.mjs";

export const id = "G-architecture-table";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return checkArchitectureTable(snapshot);
}

export function checkArchitectureTable(snapshot) {
  // ← 現行 governance-check.mjs:389-401 の本体を**逐語で**貼る
}
```

現行 `governance-check.mjs` から `checkArchitectureTable` とその直上のコメントブロック（386-401）を削除する。

- [ ] **Step 5: facade の `buildChecks` を registry ベースへ切り替える**

`scripts/governance-check.mjs` の `buildChecks` を次の形にする。**母集団の計算・`sink` への記録・`record` は現行のまま残す**——変えるのは `return [...]` の配列だけである。

```javascript
import { CHECK_MODULES } from "./governance/registry.mjs";
import { checkArchitectureTable } from "./governance/checks/G-architecture-table.mjs";

export function buildChecks(snapshot, sink = {}) {
  // …（母集団の計算と sink への記録は現行のまま）…
  const ctx = { docs, allRefDocs, staleTargets, gitIgnoredPaths, record };
  const moved = new Set(CHECK_MODULES.map((m) => m.id));
  const legacy = [
    // 未移送の検査は現行の登録行のまま残す。移送が済んだものはここから消す——
    // **移送の途中でも 19 件が揃うことを、この 2 本の連結が保つ**
    { id: "G-module-index", run: () => checkModuleIndex(snapshot) },
    // …（G-architecture-table 以外の 18 行をそのまま）…
  ].filter((c) => !moved.has(c.id));
  return [...CHECK_MODULES.map((m) => ({ id: m.id, run: () => m.run(snapshot, ctx) })), ...legacy];
}
```

⚠ `.filter((c) => !moved.has(c.id))` は**移送のたびに登録行を消し忘れても二重登録にならない**ための保険である。**Task 7 の終わりに `legacy` は空配列になり、その時点で配列ごと削除する**（残せば、それ自体が忘れうる登録行として生き残る）。

- [ ] **Step 6: 検証する**

Run: `npx vitest run scripts/governance/registry.test.mjs`
Expected: PASS（7 件）

Run: `npm test`
Expected: PASS

Run: `npm run governance:check`
Expected: 全検査 passed

Run: `node scripts/governance-check.mjs > /tmp/now-evidence.txt && diff "<workspace>/baseline-evidence.txt" /tmp/now-evidence.txt && echo "evidence 一致"`
Expected: `evidence 一致` が出る

Run:
```bash
node --input-type=module -e "import { makeSnapshot, buildChecks } from './scripts/governance-check.mjs'; console.log(buildChecks(makeSnapshot(process.cwd()), {}).map((c) => c.id).sort().join('\n'));" > /tmp/now-ids.txt
diff "<workspace>/baseline-ids.txt" /tmp/now-ids.txt && echo "ID 集合 一致"
```
Expected: `ID 集合 一致` が出る（19 件のまま）

- [ ] **Step 7: コミット**

メッセージ: `refactor: 検査の登録を checks/ の走査から導出する registry を入れる (#1088)`

---

### Task 4: 移送バッチ 1 — `snapshot` だけを使う 5 本

**Files:**
- Create: `scripts/governance/checks/G-module-index.mjs` / `G-build-commands.mjs` / `G-ci-table.mjs` / `G-rules-globs.mjs` / `G-skill-table.mjs`
- Modify: `scripts/governance-check.mjs`（該当宣言と `legacy` の行を削除、evidence 用の import は無し）

**Interfaces:**
- Consumes: Task 3 が決めた `id` / `run(snapshot, ctx)` の契約、Task 2 の `lib.mjs`。
- Produces: 5 本の検査モジュール。いずれも `ctx` を使わない。

**背景（実装者向け）:** 同じ形の作業なので 1 タスクへ束ねる。5 本とも `ctx` を必要とせず、private helper も自分の行範囲に閉じている（`G-rules-globs` の `globToRegex`、`G-skill-table` の `SKILL_FILE_RE` / `skillFiles` / `modelHiddenSkills` は一緒に移す）。

- [ ] **Step 1: 5 本を逐語で移す**

各ファイルの雛形（`G-module-index` の例。他の 4 本も同型）:

```javascript
//! G-module-index — 各 crate の CLAUDE.md モジュール構成 ↔ 実ファイル。
import { finding, linesOutsideFences } from "../lib.mjs";

export const id = "G-module-index";

export function run(snapshot, ctx) {
  return checkModuleIndex(snapshot);
}

// ← 現行 governance-check.mjs から MODULE_INDEX_CRATES と checkModuleIndex を**逐語で**移す
```

**移すもの（現行の行範囲は「ファイル構成」の表を見よ）:**

| 移送先 | 一緒に移す宣言 | facade が re-export し続ける名前 |
|---|---|---|
| `G-module-index.mjs` | `MODULE_INDEX_CRATES`・`checkModuleIndex` | `MODULE_INDEX_CRATES`・`checkModuleIndex` |
| `G-build-commands.mjs` | `checkBuildCommands` | `checkBuildCommands` |
| `G-ci-table.mjs` | `checkCiTable` | `checkCiTable` |
| `G-rules-globs.mjs` | `globToRegex`・`checkRulesGlobs` | `globToRegex`・`checkRulesGlobs` |
| `G-skill-table.mjs` | `SKILL_FILE_RE`・`skillFiles`・`modelHiddenSkills`・`checkSkillTable` | `modelHiddenSkills`・`checkSkillTable` |

- [ ] **Step 2: facade から re-export する**

`scripts/governance-check.mjs` へ、上の表の右列を各検査モジュールから import して `export { … }` に足す。`legacy` 配列から該当 5 行を削除する。

- [ ] **Step 3: 検証する**

Run: `npm test`／`npm run governance:check`／evidence の diff／ID 集合の diff（Task 3 Step 6 と同じ 4 本）
Expected: すべて green、`evidence 一致` と `ID 集合 一致` が出る

- [ ] **Step 4: 「逐語で移った」ことを自分で検算する**

Run:
```bash
git show HEAD:scripts/governance-check.mjs > /tmp/before.mjs
for f in scripts/governance/checks/G-module-index.mjs scripts/governance/checks/G-build-commands.mjs scripts/governance/checks/G-ci-table.mjs scripts/governance/checks/G-rules-globs.mjs scripts/governance/checks/G-skill-table.mjs; do
  echo "--- $f ---"
  grep -c "" "$f"
done
```

そのうえで、各ファイルの本体（`//!` と `id` と `run` を除いた部分）が `/tmp/before.mjs` の該当行範囲と**行単位で一致する**ことを目視で確かめる。**一致しない行があれば、それは移送ではなく変更である**——戻す。

- [ ] **Step 5: コミット**

メッセージ: `refactor: 検査 5 本（module-index/build-commands/ci-table/rules-globs/skill-table）を checks/ へ移す (#1088)`

---

### Task 5: 移送バッチ 2 — 4 本

**Files:**
- Create: `scripts/governance/checks/G-hook-commands.mjs` / `G-hook-fires.mjs` / `G-check-skill-enumeration.mjs` / `G-adr-file-names.mjs`
- Modify: `scripts/governance-check.mjs`

**Interfaces:**
- Consumes: Task 3 の契約、Task 2 の `lib.mjs`。
- Produces: 4 本。`G-adr-file-names.mjs` は `adrFiles(snapshot)` も export する——**facade が evidence（`ADR ${adrFiles(snapshot).length} 本の名前`）のために名指しで import する**。

**背景（実装者向け）:** `G-hook-fires` は `.claude/hooks/post-edit.mjs` の `selectChecks` を import する**唯一の例外**であり、spec §5 が「保存する」と明記している。import の相対パスが `scripts/governance/checks/` から見て 1 段深くなるので、**そこだけは書き換えが要る**（逐語ではない箇所として報告に明記すること）。

- [ ] **Step 1: 4 本を逐語で移す**

| 移送先 | 一緒に移す宣言 | facade が re-export し続ける名前 |
|---|---|---|
| `G-hook-commands.mjs` | `OUTPUT_ONLY_FLAGS`・`checkHookCommands` | `checkHookCommands` |
| `G-hook-fires.mjs` | `HOOK_FIRES_HEADER`・`checkHookFires` | `checkHookFires` |
| `G-check-skill-enumeration.mjs` | `CHECK_SKILL_REF`・`sectionOf`・`checkCheckSkillEnumeration` | `checkCheckSkillEnumeration` |
| `G-adr-file-names.mjs` | `ADR_FILE_NAME`・`adrFiles`・`checkAdrFileNames` | `adrFiles`・`checkAdrFileNames` |

- [ ] **Step 2: `selectChecks` の import パスを直す**

現行 `governance-check.mjs` の該当 import 行を `G-hook-fires.mjs` へ移し、相対パスを 1 段深い位置から解決できる形へ直す。

Run: `node --input-type=module -e "import { CHECK_MODULES } from './scripts/governance/registry.mjs'; console.log(CHECK_MODULES.map((m)=>m.id).join(','));"`
Expected: エラーを出さずに ID が並ぶ（import が解決できなければここで落ちる）

- [ ] **Step 3: facade へ evidence 用の import を足す**

```javascript
// evidence 専用の導出は、その検査のファイルから名指しで取る。**登録行と違い、
// ファイルが消えれば import が失敗して鳴る**（沈黙する写しにはならない）。
import { adrFiles } from "./governance/checks/G-adr-file-names.mjs";
```

`legacy` 配列から該当 4 行を削除する。

- [ ] **Step 4: 検証する**

Run: Task 3 Step 6 と同じ 4 本
Expected: すべて green、`evidence 一致` と `ID 集合 一致` が出る

- [ ] **Step 5: コミット**

メッセージ: `refactor: 検査 4 本（hook-commands/hook-fires/check-skill-enumeration/adr-file-names）を checks/ へ移す (#1088)`

---

### Task 6: 移送バッチ 3 — Rust / Cargo 系 3 本

**Files:**
- Create: `scripts/governance/checks/G-module-linkage.mjs` / `G-workspace-lints.mjs` / `G-clippy-disallowed.mjs`
- Modify: `scripts/governance-check.mjs`

**Interfaces:**
- Consumes: Task 3 の契約、Task 2 の `lib.mjs`。
- Produces: 3 本。`G-workspace-lints.mjs` は `workspaceMembers(snapshot)` を、`G-clippy-disallowed.mjs` は `clippyDisallowedCount(snapshot)` も export する——**facade が evidence のために名指しで import する**。

**背景（実装者向け）:** 3 本で 590 行と本計画で最大の塊である。`G-module-linkage` の Rust パーサ（`blankRustNonCode`・`RAW_STRING_PREFIX`・`moduleChildDir`・`declaredModuleFiles`）と `G-workspace-lints` の TOML ヘルパ（`stripTomlComment`・`tomlLine`・`lintLevel`・`tomlInt`・`lintPriority`）は、それぞれ 1 つの検査に閉じているので一緒に移す。

⚠ `workspaceMembers` は `G-module-linkage`（173・338 行）と `G-build-commands`（936・939 行）からも使われている（Task 2 Step 1 と同じ実測をこの 3 本に対しても行うこと）。**2 つ以上の移送先が使うなら `lib.mjs` へ入れる**——その場合は facade の evidence も lib から取る。実測してから決める。

- [ ] **Step 1: `workspaceMembers` の使用位置を実測する**

Run: `grep -n "workspaceMembers" scripts/governance-check.mjs scripts/governance/checks/*.mjs`

複数の移送先にまたがるなら `lib.mjs` へ移し、facade と各検査は lib から import する。1 つに閉じているなら `G-workspace-lints.mjs` へ置き、facade は名指しで import する。**どちらにしたかを報告に書くこと。**

- [ ] **Step 2: 3 本を逐語で移す**

| 移送先 | 一緒に移す宣言 | facade が re-export し続ける名前 |
|---|---|---|
| `G-module-linkage.mjs` | `moduleChildDir`・`RAW_STRING_PREFIX`・`blankRustNonCode`・`declaredModuleFiles`・`checkModuleLinkage` | `declaredModuleFiles`・`checkModuleLinkage` |
| `G-workspace-lints.mjs` | `stripTomlComment`・`tomlLine`・`lintLevel`・`tomlInt`・`lintPriority`・`REQUIRED_RUSTDOC_LINTS`・`hasWorkspaceLintsOptIn`・`rustdocLintsAreDenied`・`checkWorkspaceLints`（＋ Step 1 の結論次第で `workspaceMembers`） | `REQUIRED_RUSTDOC_LINTS`・`hasWorkspaceLintsOptIn`・`rustdocLintsAreDenied`・`checkWorkspaceLints`・`workspaceMembers` |
| `G-clippy-disallowed.mjs` | `REQUIRED_DISALLOWED_METHODS`・`CLIPPY_TOML`・`SRC_TAURI_MANIFEST`・`disallowedMethodPaths`・`declaresEguiDependency`・`DISALLOWED_METHODS_GROUPS`・`clippyMethodsDenied`・`clippyDisallowedCount`・`checkClippyDisallowed` | `REQUIRED_DISALLOWED_METHODS`・`disallowedMethodPaths`・`declaresEguiDependency`・`clippyMethodsDenied`・`clippyDisallowedCount`・`checkClippyDisallowed` |

- [ ] **Step 3: facade の evidence 用 import を足し、`legacy` から 3 行を削除する**

- [ ] **Step 4: 検証する**

Run: Task 3 Step 6 と同じ 4 本
Expected: すべて green、`evidence 一致` と `ID 集合 一致` が出る

⚠ evidence には `workspace member N 件の lints opt-in` と `clippy 禁止 N 件` が含まれる。**この 2 つの数が変われば、移送で導出が壊れている。** diff が一致することがその検算である。

- [ ] **Step 5: コミット**

メッセージ: `refactor: 検査 3 本（module-linkage/workspace-lints/clippy-disallowed）を checks/ へ移す (#1088)`

---

### Task 7: 移送バッチ 4 — `ctx` を使う 6 本

**Files:**
- Create: `scripts/governance/checks/G-references.mjs` / `G-spec-sections.mjs` / `G-heading-refs.mjs` / `G-near-heading-refs.mjs` / `G-stale-identifiers.mjs` / `G-adr-citations.mjs`
- Modify: `scripts/governance-check.mjs`（`legacy` 配列ごと削除）

**Interfaces:**
- Consumes: Task 3 の契約、Task 2 の `lib.mjs`、`ctx` の 5 キー。
- Produces: 6 本。これで 19 本すべてが `checks/` に揃い、**`legacy` 配列が空になるので配列ごと削除する**。

**背景（実装者向け）:** この 6 本だけが `ctx` を必要とする。`run` の中身は、**現行 `buildChecks` の登録行に書かれている呼び出しをそのまま `ctx` 経由へ移した形**になる。

```javascript
// G-references.mjs
export function run(snapshot, ctx) {
  return checkReferences(snapshot, ctx.docs, ctx.gitIgnoredPaths);
}

// G-spec-sections.mjs
export function run(snapshot, ctx) {
  return checkSpecSections(snapshot, ctx.docs);
}

// G-heading-refs.mjs
export function run(snapshot, ctx) {
  return ctx.record("headingRefs", scanHeadingRefs(snapshot, ctx.allRefDocs));
}

// G-near-heading-refs.mjs
export function run(snapshot, ctx) {
  return ctx.record("nearRefs", scanNearHeadingRefs(snapshot, ctx.allRefDocs));
}

// G-stale-identifiers.mjs
export function run(snapshot, ctx) {
  return ctx.record("stale", scanStaleIdentifiers(snapshot, ctx.staleTargets));
}

// G-adr-citations.mjs
export function run(snapshot, ctx) {
  return ctx.record("adrCitations", scanAdrCitations(snapshot, adrCitationDocs(snapshot, ctx.docs)));
}
```

⚠ `adrCitationDocs` は `G-adr-citations.mjs` の中に置く（現行は facade の `buildChecks` が呼んでいるが、**呼ぶのは 1 つの検査だけ**なので検査の側が持つのが正しい）。ただし `governance-check.test.mjs` が import しているので facade は re-export し続ける。

- [ ] **Step 1: 6 本を逐語で移す**

| 移送先 | 一緒に移す宣言 | facade が re-export し続ける名前 |
|---|---|---|
| `G-references.mjs` | `checkReferences` | `checkReferences` |
| `G-spec-sections.mjs` | `checkSpecSections` | `checkSpecSections` |
| `G-heading-refs.mjs` | `HEADING_REF`・`scanHeadingRefs`・`checkHeadingRefs` | `scanHeadingRefs`・`checkHeadingRefs` |
| `G-near-heading-refs.mjs` | `NEAR_REF_GAP`・`NEAR_REF`・`ADJACENT_REF`・`scanNearHeadingRefs`・`checkNearHeadingRefs` | `scanNearHeadingRefs`・`checkNearHeadingRefs` |
| `G-stale-identifiers.mjs` | `VOCAB_SOURCE_EXT`・`VOCAB_TEST_FILE`・`STALE_EXTRA_DOCS`・`STALE_IDENT`・`STALE_SNAKE_IDENT`・`STALE_LOWER_SNAKE_IDENT`・`staleTarget`・`currentVocabulary`・`scanStaleIdentifiers`・`checkStaleIdentifiers` | `STALE_EXTRA_DOCS`・`currentVocabulary`・`scanStaleIdentifiers`・`checkStaleIdentifiers` |
| `G-adr-citations.mjs` | `ADR_CITATION`・`adrCitationDocs`・`scanAdrCitations`・`checkAdrCitations` | `adrCitationDocs`・`scanAdrCitations`・`checkAdrCitations` |

⚠ `STALE_EXTRA_DOCS` は `lib.mjs` の `staleIdentifierTargets` も使う。**2 つの移送先が使うなら `lib.mjs` に置き、`G-stale-identifiers.mjs` は lib から import する**（Task 2 Step 1 の基準と同じ）。実測して決め、報告に書くこと。

- [ ] **Step 2: `legacy` 配列を削除する**

`buildChecks` から `legacy` の宣言と `.filter(...)` と連結を消し、次の形にする:

```javascript
export function buildChecks(snapshot, sink = {}) {
  // …（母集団の計算と sink への記録は現行のまま）…
  const ctx = { docs, allRefDocs, staleTargets, gitIgnoredPaths, record };
  return CHECK_MODULES.map((m) => ({ id: m.id, run: () => m.run(snapshot, ctx) }));
}
```

⚠ **`legacy` を「念のため」残してはならない。** 空配列でも、それは忘れうる登録行が生き残るということであり、#1088 が構造的に消えない。

- [ ] **Step 3: 検証する**

Run: Task 3 Step 6 と同じ 4 本
Expected: すべて green、`evidence 一致` と `ID 集合 一致` が出る

Run: `grep -c "export function check\|export function scan" scripts/governance-check.mjs`
Expected: `1`（実行時に測定・訂正）——面積の計器（`checkNormativeAreaInstrument` 等）が本 Task 時点ではまだ facade に残っており `0` にならない。`0` になるのは Task 8 で計器を `instrument.mjs` へ移した後である。この期待値は Task 8 に属する。

- [ ] **Step 4: コミット**

メッセージ: `refactor: 残る検査 6 本を checks/ へ移し、登録配列を削除する (#1088)`

---

### Task 8: 計器を `instrument.mjs` へ切り出す

**Files:**
- Create: `scripts/governance/instrument.mjs`
- Modify: `scripts/governance-check.mjs`

**Interfaces:**
- Consumes: Task 2 の `lib.mjs`。
- Produces: `instrument.mjs` が `ALWAYS_LOADED_FILES`・`skillDescriptionArea`・`checkNormativeAreaInstrument`・`normativeArea` を export する。

**背景（実装者向け）:** 計器は検査ではない——面積に合否は無い（`ADR-retire-area-budget`）。PR #1091 で登録配列の外へ出し、`runAll` が直接 push する形にした。**`checks/` の外に置くことで、その区別がディレクトリ境界になる**。

⚠ **`checks/` へ入れてはならない。** 入れれば registry が拾い、`checks` 列に `G-area-instrument` が現れて manifest 差分が非ゼロになる（＝この PR の受け入れ条件を破る）。

- [ ] **Step 1: 逐語で移す**

`countChars`・`sumChars`・`ALWAYS_LOADED_FILES`・`skillDescriptionArea`・`checkNormativeAreaInstrument`・`normativeArea` を現行 1334-1441 から `scripts/governance/instrument.mjs` へ移す。冒頭へ:

```javascript
//! 合否を持たない計器。**`checks/` の外に在ることが「検査ではない」の担保である**——
//! registry は `checks/` だけを走査するので、ここに在る限り「検査 N 件」には数えられない
//! （`ADR-retire-area-budget`）。母集団が欠ければ evidence が嘘になるため、
//! 入力の健全性だけは findings に残す。
```

- [ ] **Step 2: facade から import して re-export する**

`runAll` の `findings.push(...checkNormativeAreaInstrument(snapshot));` とその直上のコメントは**現行のまま残す**。

- [ ] **Step 3: 検証する**

Run: Task 3 Step 6 と同じ 4 本
Expected: すべて green

Run: `node --input-type=module -e "import { CHECK_MODULES } from './scripts/governance/registry.mjs'; console.log(CHECK_MODULES.some((m) => m.id === 'G-area-instrument'));"`
Expected: `false`（計器が registry に入っていない）

- [ ] **Step 4: コミット**

メッセージ: `refactor: 合否を持たない計器を scripts/governance/instrument.mjs へ移す (#1088)`

---

### Task 9: テストの再配置 1 — per-check テストファイル（12 本）

**Files:**
- Create: `scripts/governance/checks/G-module-index.test.mjs` ほか 11 本
- Modify: `scripts/governance-check.test.mjs`（移した describe を削除）

**Interfaces:**
- Consumes: Task 4〜7 の検査モジュール。
- Produces: 検査ごとのテストファイル。**import 元は facade ではなく当の検査モジュール**である。

**背景（実装者向け）:** テストは facade 経由で import したままでも通る（再輸出があるため）。**それでも検査の隣へ置くのは、検査ファイルを消したときにテストも一緒に消え、「テストだけ残って何も検査していない」状態が生まれないようにするため**である。

現行 `scripts/governance-check.test.mjs` は 34 の `describe` を持つ。**移すのは 1 検査に閉じた describe だけ**であり、横断的なものは Task 10 で扱う。

`snap()` ヘルパ（現行 60-66 行）は各テストファイルが必要とする。**`scripts/governance/test-helpers.mjs` へ切り出して共有する**——`.test.mjs` でないので registry には拾われない。

- [ ] **Step 1: `test-helpers.mjs` を作る**

```javascript
//! テスト専用の最小スナップショット。**`.test.mjs` でないため registry には拾われない**
//! （registry は `checks/` 直下だけを走査するので、そもそもこの位置は対象外である）。

/** 最小スナップショット: files はリポジトリ相対（"/" 区切り）、contents は path → 本文 */
export function snap(contents, extraFiles = []) {
  const files = [...Object.keys(contents), ...extraFiles];
  return { files, read: (p) => contents[p] ?? null };
}
```

- [ ] **Step 2: 12 本の describe を移す**

| 移送先 | 移す describe（現行の行） |
|---|---|
| `G-module-index.test.mjs` | 128 |
| `G-module-linkage.test.mjs` | 161 |
| `G-architecture-table.test.mjs` | 344 |
| `G-spec-sections.test.mjs` | 480 |
| `G-build-commands.test.mjs` | 511 |
| `G-workspace-lints.test.mjs` | 548 |
| `G-clippy-disallowed.test.mjs` | 663 |
| `G-ci-table.test.mjs` | 849 |
| `G-rules-globs.test.mjs` | 67（`globToRegex`）・895 |
| `G-skill-table.test.mjs` | 907 |
| `G-hook-commands.test.mjs` | 946 |
| `G-hook-fires.test.mjs` | 1006 |

各ファイルの import は当の検査モジュールから取る:

```javascript
import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkModuleIndex, MODULE_INDEX_CRATES } from "./G-module-index.mjs";
```

describe の本文は**逐語で移す**。

- [ ] **Step 3: 検証する**

Run: `npm test`
Expected: PASS。**テスト総数が減っていないこと**を確かめる——移送前の件数を控えておき、同じ数であることを見る。

⚠ **「PASS した」だけでは足りない。** describe を移し忘れると、そのテストは実行されないまま `npm test` は緑になる。**件数の一致が唯一の検算である。**

Run: Task 3 Step 6 と同じ 4 本
Expected: すべて green

- [ ] **Step 4: コミット**

メッセージ: `test: 1 検査に閉じたテスト 12 本を検査の隣へ移す (#1088)`

---

### Task 10: テストの再配置 2 — 残りと横断テスト

**Files:**
- Create: `scripts/governance/checks/G-references.test.mjs` ほか 6 本、`scripts/governance/lib.test.mjs`、`scripts/governance/instrument.test.mjs`
- Modify: `scripts/governance-check.test.mjs`（横断テストだけが残る）

**Interfaces:**
- Consumes: Task 9 の `test-helpers.mjs`。
- Produces: すべての describe が「1 検査に閉じたものは検査の隣」「横断的なものは facade のテスト」へ分かれた状態。

**背景（実装者向け）:** 残りの describe には**複数の検査にまたがるもの**と**facade そのものを見るもの**が混ざっている。前者は「どちらか一方の隣」ではなく、見ている対象の場所（`lib.mjs` / facade）へ置く。

**割り当て（34 の describe すべて。母集団は `grep -n "^describe(" scripts/governance-check.test.mjs`）:**

| 移送先 | 移す describe（現行の行） | 理由 |
|---|---|---|
| `G-references.test.mjs` | 379・1694（配線） | 1694 は「buildChecks が gitignore 判定を渡す」——`ctx.gitIgnoredPaths` の配線カナリア |
| `G-heading-refs.test.mjs` | 1250 | |
| `G-near-heading-refs.test.mjs` | 1782 | |
| `G-stale-identifiers.test.mjs` | 1446・1646・1676・1711・1742 | 1711/1742 は配線カナリア（`ctx.staleTargets` の中身） |
| `G-check-skill-enumeration.test.mjs` | 1830 | |
| `G-adr-file-names.test.mjs` | 1898 | |
| `G-adr-citations.test.mjs` | 1940 | |
| `lib.mjs` のテスト（`scripts/governance/lib.test.mjs`） | 360（`gitIgnoredPaths`）・1325（見出し参照のソースの腕）・1402（凍結された歴史）・1879（`makeSnapshot` の走査除外） | いずれも lib の関数か、2 つの検査にまたがる母集団を見ている |
| `instrument.test.mjs` | 1164 | 計器 |
| `governance-check.test.mjs`（残す） | 95（母集団カナリア #701）・1148（`runAll` の空母集団）・1991（検査 ID の形）・2009（実リポジトリ スモーク） | facade そのものの振る舞い。**実行時に測定・訂正: 5 本目として「facade の公開面（export の凍結）」も残る**（凍結一覧は `governance-check.mjs` の export そのものを見るため、他のどの検査の隣にも属さない）。**この 5 本目は本表にも元の 34 の母集団にも一度も載っていない**——ブランチ途中で追加され、移送の割り当て判断を必要としなかったため数え落とされていた。表自体が自分の残り母集団を過小に見積もっていた、という 2 つ目の原因である |

⚠ **実行時に測定・訂正:** 1991 の「検査 ID の形」describe は 3 つの assertion を持つ。`buildChecks` を呼ぶこと自体は移送先を決める理由にならない——`buildChecks` は `CHECK_MODULES` の id を 1:1 でそのまま通す passthrough であり、フィルタも変換もしないため。assertion ごとに答えは割れる: 1 件目（id が `G-<kebab>` 形）は `buildChecks` を経由する必要が無く、2 件目（id が重複しない）は `registry.test.mjs` に既にある検査と機能的に重複する。**移せるのはこの 2 件だけで、3 件目（`runAll(...)` の evidence 文字列が「検査 N 件」を含むこと）は facade の summary 出力そのものを見ており、他のどこにも書けない。** 1 つの atomic な describe を割って 2 件だけ移す益は、まとまりを壊すコストに見合わないため、**describe ごと `governance-check.test.mjs` に残す**——3 件目が facade 固有であることだけが理由になる。

- [ ] **Step 1: 表のとおりに移す**

配線カナリア（1694・1711・1742）は `buildChecks` を呼んで `ctx` の中身を確かめる形なので、facade を import する。**検査の隣に置きつつ facade を import する**のは矛盾ではない——見ているのは「facade が正しい ctx を渡すか」であり、その検査に固有の関心である。

- [ ] **Step 2: 検証する**

Run: `npm test`
Expected: PASS。**テスト総数が Task 9 Step 3 の件数と一致すること。**

Run: `grep -c "^describe(" scripts/governance-check.test.mjs`
Expected: `5`（実行時に測定・訂正。原因は上の表に記した 2 つ——「facade の公開面（export の凍結）」describe が独立に要ることと、それがブランチ途中の追加で本表の母集団に一度も載っていなかったこと。次に読む者が `4` を再導出しないよう、値と原因の両方をここに残す）

Run: Task 3 Step 6 と同じ 4 本
Expected: すべて green

- [ ] **Step 3: コミット**

メッセージ: `test: 残るテストを検査・lib・計器・facade へ振り分ける (#1088)`

---

### Task 11: フォールトインジェクション — 分割が #1088 を実際に消したことを測る

**Files:**
- Modify: `scripts/governance/registry.test.mjs`（describe を追加）
- Test: `scripts/governance-manifest.test.mjs`（describe を追加）

**Interfaces:**
- Consumes: Task 3 の `checkModulesFrom(dir)`、`scripts/governance-manifest.mjs` の `manifest` / `diffManifest`。
- Produces: spec §6 の表の「A — 検査ファイルを 1 本削除 → B が赤にする」行の実測。

**背景（実装者向け）:** #1088 が問うたのは「登録配列から検査が落ちても沈黙する」ことだった。分割後は登録配列が存在しないので、**落ちうるのはファイルそのものである**。それが manifest 差分に現れることを測る——これが「欠陥が構造的に消えた」の証拠になる。

⚠ **稼働中の `checks/` を汚さない。** `checkModulesFrom(dir)` に使い捨てディレクトリを渡し、`manifest()` の返り値の複製に変異を当てる（`.claude/rules/safety-nets.md`）。

- [ ] **Step 1: テストを書く**

`scripts/governance-manifest.test.mjs` の末尾へ:

```javascript
describe("per-check 分割後の欠落 — 検査ファイルが消えれば manifest 差分が発火する（#1088）", () => {
  it("checks/ から 1 本消えた形は差分として現れる", () => {
    const base = manifest(makeSnapshot(process.cwd()));
    // 稼働中の checks/ は触らない——返り値の複製に変異を当てる
    const mutated = { ...base, checks: base.checks.filter((id) => id !== "G-ci-table") };
    expect(diffManifest(base, mutated)).toEqual(["-G-ci-table"]);
    expect(undeclared(diffManifest(base, mutated), "宣言のない PR 本文")).toEqual(["-G-ci-table"]);
  });
});
```

`scripts/governance/registry.test.mjs` の末尾へ:

```javascript
describe("走査が母集団である — ファイルの増減がそのまま検査の増減になる（#1088）", () => {
  it("使い捨てディレクトリからファイルを 1 本消すと、その id が registry から消える", async () => {
    const dir = mkdtempSync(path.join(tmpdir(), "gov-registry-"));
    try {
      const mod = (id) => `export const id = "${id}";\nexport function run() { return []; }\n`;
      writeFileSync(path.join(dir, "G-a.mjs"), mod("G-a"));
      writeFileSync(path.join(dir, "G-b.mjs"), mod("G-b"));
      expect((await checkModulesFrom(dir)).map((m) => m.id)).toEqual(["G-a", "G-b"]);
      rmSync(path.join(dir, "G-b.mjs"));
      // 対照との差が証拠である——緑になったこと自体は証拠ではない
      expect((await checkModulesFrom(dir)).map((m) => m.id)).toEqual(["G-a"]);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
```

⚠ `checkModulesFrom` は `import()` を使うため、**同じパスへ違う内容を書いて読み直しても ESM のキャッシュが効いて古い方が返る**。上のテストが「消す」方向だけを測るのはそのためである。**内容を差し替える変異を書くなら、毎回違うファイル名を使うこと。**

- [ ] **Step 2: 走らせる**

Run: `npx vitest run scripts/governance-manifest.test.mjs scripts/governance/registry.test.mjs`
Expected: PASS

- [ ] **Step 3: 変異が届いていることを対照で確かめる**

Run:
```bash
node --input-type=module -e "
import { manifest } from './scripts/governance-manifest.mjs';
import { makeSnapshot } from './scripts/governance-check.mjs';
const m = manifest(makeSnapshot(process.cwd()));
console.log('checks:', m.checks.length, m.checks.includes('G-ci-table') ? '(G-ci-table 在り)' : '(G-ci-table 無し)');
"
```
Expected: `checks: 19 (G-ci-table 在り)` ——**変異対象が母集団に実在することの確認**である。実在しないものを消しても差分は出ず、テストは自明に通る。

- [ ] **Step 4: コミット**

メッセージ: `test: 検査ファイルの欠落が manifest 差分として現れることを測る (#1088)`

---

### Task 12: 文書の同期

**Files:**
- Modify: `scripts/governance-check.mjs`（冒頭の契約コメント）
- Modify: `AGENTS.md`・`.claude/rules/governance-docs.md`（必要な箇所のみ）

**Interfaces:**
- Consumes: Task 3〜11 の結果。
- Produces: 機構の説明が実装と一致した状態。

**背景（実装者向け）:** これが**逐語移送の唯一の例外**である。冒頭の契約コメントは「各検査はスナップショット注入の純関数」「例外は 2 つある」という形で、`buildChecks` の登録配列を前提に書かれている。分割後はその前提が変わる。

⚠ **数え上げで書き直さない。** このリポジトリには「偽の全称を直した文がまた全称で偽になる」連鎖が記録されている（#1091 で 3 回）。「例外は N 個」ではなく**下限の主張**（「〜だけではない」「少なくとも」）で書く。書いたあとに「何が増えたらこの文は偽になるか」を 1 つ挙げ、挙がるなら数えている——書き直す。

- [ ] **Step 1: 契約コメントを直す**

現行の「例外は 2 つある」という形を、分割後の実態へ合わせる。含めるべき事実:
- 検査の登録は `checks/` の走査から導出される（忘れうる登録行が無い）
- `checks/` の外に在るものは検査ではない（計器は `instrument.mjs`）
- 母集団の計算・0 件検知・evidence の組み立て・CLI は facade に残る
- 依存ゼロ・決定的の契約は分割後も全層に効く

- [ ] **Step 2: 他の文書が壊れていないか実測する**

Run: `grep -rn "governance-check\.mjs" --include="*.md" . | grep -v "docs/superpowers/"`

ファイル名は変わらないので大半は無傷のはずである。**「はず」で済ませず、出た行を 1 つずつ読んで、分割で偽になった記述が無いか確かめる**（`.claude/rules/governance-docs.md`「(3) 古い情報を残さない——触った節の隣にある主張が今も真か見る」）。

Run: `grep -rn "buildChecks\|登録配列\|登録行" --include="*.md" .`

- [ ] **Step 3: 検証する**

Run: `npm run governance:check`
Expected: 全検査 passed（`G-heading-refs` が文書間の参照を照合するので、見出しを変えると赤くなる）

Run: Task 3 Step 6 と同じ 4 本
Expected: すべて green

- [ ] **Step 4: コミット**

メッセージ: `docs: 契約コメントを per-check 分割後の実態へ合わせる (#1088)`

---

### Task 13: PR を作る

**Files:** なし（PR 本文のみ）

**背景（実装者向け）:** **この PR は #1088 を閉じる。** C（#1091）と B（#1092）は前段であり閉じなかったが、A で登録配列そのものが消えるので欠陥が構造的に解消する。

⚠ PR 本文へ closing keyword を書くかどうかは**呼び出し側が決める**。実装者は本文の草稿を書くところまでとし、`gh pr create` は打たない。

- [ ] **Step 1: PR 本文の草稿を書く**

含めるもの:
- **manifest delta が 0 件であること**（この PR の受け入れ条件。非ゼロなら分割のバグ）
- evidence 行が baseline と byte 一致したこと
- テスト総数が移送前後で一致したこと
- 移送が逐語であること、およびその例外（`selectChecks` の import パス・契約コメント）
- PR #1092 が残した 2 項目を、この PR で実際に測ること（下の Step 2）

- [ ] **Step 2: PR #1092 の積み残しを回収する手順を本文へ書く**

```markdown
- [ ] `governance manifest delta` step が比較の枝を実際に走り、delta 0 件で緑になった
- [ ] 故意に `.claude/rules/` へ 1 枚足して赤になり、本文へ宣言して緑になり、戻して緑に戻った
```

2 つ目の手順（**CI を意図的に 1 度赤くする**）:

1. `.claude/rules/zz-manifest-probe.md` を足してコミット・push する
2. CI の `governance manifest delta` が `+.claude/rules/zz-manifest-probe.md` を未宣言として赤にすることを見る
3. PR 本文へその行を逐語で足し、**この job を re-run する**（push は不要——PR #1092 でそう作った）
4. 緑になることを見る
5. 当該コミットを revert し、本文から宣言を消し、緑に戻ることを見る

⚠ **この手順は CI を意図的に赤くする。** 呼び出し側が実行の可否を判断する。

---

## この計画に含まれないもの

- **`governance-manifest.mjs` の変更。** A は manifest の入力（`buildChecks` の返り値）を変えないので、B 側は 1 行も触らない。**触る必要が生じたら、それは公開 API を壊した合図である。**
- **検査の実装の改善。** 移送と改善を同じ差分へ混ぜない（「逐語で移す」の Global Constraint）。気づいた改善は PR 本文へ書き出し、別 issue とする。

## Self-Review — spec カバレッジ

| spec §5 の要求 | 実装するタスク |
|---|---|
| `scripts/governance/checks/*.mjs` へ 1 検査 1 ファイル | Task 3〜7 |
| 各ファイルは `id` と `run` を export | Task 3（契約）・Task 4〜7（適用） |
| registry は `readdirSync` から導出 | Task 3 |
| `readdirSync` の順序が不定ゆえ明示 `sort` | Task 3（registry.mjs と `id は昇順` テスト） |
| registry は import 時に形を検証する | Task 3（3 本の throw テスト） |
| `G-hook-fires` の `selectChecks` import を保存 | Task 5 |
| テスト側も同じ構造へ分割 | Task 9・Task 10 |
| §6 の「A — 検査ファイルを 1 本削除 → B が赤にする」 | Task 11 |
