# governance:check の剪定と、母集団 manifest 差分 — 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `G-references` の gitignore 誤爆を断ち、判定を持たない計器を検査配列から外し、構造母集団の縮小を CI で捕まえる。

**Architecture:** 2 つの PR に分ける。PR 1（C）は `governance-check.mjs` 内の局所変更——`git check-ignore` の判定を注入して参照の実在検査に 3 分類を入れ、`G-area-instrument` を検査配列の外へ移す。PR 2（B）は新スクリプト `scripts/governance-manifest.mjs` と CI step——構造母集団の集合を吐き、main との差分を PR 本文の宣言と突き合わせる。**per-check 分割（A）はこの計画に含まれない**（下の「この計画に含まれないもの」を見よ）。

**Tech Stack:** Node.js 22（標準モジュールのみ）、vitest、GitHub Actions。

**Spec:** `docs/superpowers/specs/2026-08-14-governance-check-scope-design.md`

## Global Constraints

- **`main` へ直接コミット・プッシュしない。** PR 1 は既存ブランチ `chore/governance-check-scope`（spec のコミットが載っている）で続行する。PR 2 は `chore/governance-manifest-delta` を新たに切る。
- **`scripts/governance-check.mjs` の契約を壊さない**（ファイル冒頭の「契約」コメント）: 依存ゼロ（Node 標準のみ）・決定的（ネットワーク・時刻・環境変数に非依存）・各検査はスナップショット注入の純関数・findings ゼロなら exit 0 と母集団の件数を印字・空母集団は明示 fail。
- **PR 本文の読取や `main` の checkout を `governance-check.mjs` へ入れてはならない**（決定的性の契約が壊れる）。PR 2 の成果物は別スクリプト + CI step に閉じる。
- **フォールトインジェクションは稼働中のガードを弱めず、複製に変異を当てる**（`.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」）。
- **訳語**: coverage は「カバレッジ」、window (GUI) は「ウィンドウ」。原語の字面をそのまま漢語へ置き換えない（ルート `CLAUDE.md` の訳語規則）。
- **各タスクの検証**: `npm test`（vitest）と `npm run governance:check` の両方。`.md` の編集には PostToolUse hook が検査を割り当てない——**沈黙は「何も走らなかった」**であり、`governance:check` は手で打つ。

---

# PR 1（C）— 剪定

ブランチ: `chore/governance-check-scope`（継続）

### Task 1: `gitIgnoredPaths` — `git check-ignore` の薄い包み

**Files:**
- Modify: `scripts/governance-check.mjs`（import 追加 + 新関数を `checkReferences` の直前に置く）
- Test: `scripts/governance-check.test.mjs`（`describe("G-references checkReferences"` の直前に新 describe）

**Interfaces:**
- Produces: `gitIgnoredPaths(paths: string[], root?: string) => Set<string>` — 与えたパスのうち gitignore 対象のものだけを含む集合を返す。**パスの実在を要求しない**。

**背景（実装者向け）:** `git check-ignore` はパス名だけでパターン照合するので、ファイルが存在しなくても判定できる。2026-08-14 の実測で確認済み——不在の `test-results/never-created.json` は `.gitignore:34` に当たり、`docs/nonexistent-typo.md` は当たらなかった。これが「CI のチェックアウトには存在しない生成物の名前を、散文にバッククォートで書けない」という歪みを解く鍵である。

- [ ] **Step 1: 失敗するテストを書く**

`scripts/governance-check.test.mjs` の import 一覧（`checkReferences` の隣）へ `gitIgnoredPaths` を足し、`describe("G-references checkReferences"` の直前へ:

```javascript
// 実リポジトリの `.gitignore` に対するカナリア（fixture では repo が無く判定できない）。
// `test-results/` と `.claude/settings.local.json` は実際に ignore されている（2026-08-14 実測）。
describe("gitIgnoredPaths（存在に依らずパス名で判定する・#1088）", () => {
  it("ignore 対象は不在でも返り、非 ignore は返らない", () => {
    const got = gitIgnoredPaths([
      "test-results/never-created.json",
      ".claude/settings.local.json",
      "docs/nonexistent-typo.md",
    ]);
    expect(got.has("test-results/never-created.json")).toBe(true);
    expect(got.has(".claude/settings.local.json")).toBe(true);
    expect(got.has("docs/nonexistent-typo.md"), "非 ignore の typo が緑に化けている").toBe(false);
  });
  it("該当なし（git の exit 1）は失敗ではなく空集合", () => {
    expect(gitIgnoredPaths(["docs/nonexistent-typo.md"])).toEqual(new Set());
  });
  it("空入力では spawn しない", () => {
    expect(gitIgnoredPaths([])).toEqual(new Set());
  });
});
```

- [ ] **Step 2: 落ちることを確認する**

Run: `npx vitest run scripts/governance-check.test.mjs -t "gitIgnoredPaths"`
Expected: FAIL — `gitIgnoredPaths is not a function`（import が解決しない）

- [ ] **Step 3: 最小の実装を書く**

`scripts/governance-check.mjs` の import に足す（`node:url` の次の行）:

```javascript
import { spawnSync } from "node:child_process";
```

`checkReferences` の直前（`// G-references —` のコメントブロックの後）へ:

```javascript
/** `git check-ignore` は**ファイルの存在に依らずパス名だけで判定する**（2026-08-14 実測: 不在の
 *  `test-results/never-created.json` が当たり、`docs/nonexistent-typo.md` は当たらない）。ゆえに
 *  CI のチェックアウトでも手元と同じ判定が出る——これが「CI に存在しない生成物の名前を散文へ
 *  バッククォートで書けない」という表記の歪みを解く（#1088）。
 *  **exit 1 は「該当なし」であって失敗ではない**（失敗は 128）。git が無い・repo でない場合は
 *  空集合を返す＝何も免除しない側へ倒す（誤爆より見落としを避ける）。
 *  **決定的性**: 読むのは同じチェックアウトの `.gitignore` だけで、ネットワーク・時刻・環境変数に依らない。 */
export function gitIgnoredPaths(paths, root = process.cwd()) {
  if (paths.length === 0) return new Set();
  const r = spawnSync("git", ["check-ignore", "--stdin", "-z"], {
    cwd: root,
    input: paths.join("\0"),
    encoding: "utf8",
  });
  if (r.error || r.status === 128) return new Set();
  return new Set(r.stdout.split("\0").filter(Boolean));
}
```

- [ ] **Step 4: 通ることを確認する**

Run: `npx vitest run scripts/governance-check.test.mjs -t "gitIgnoredPaths"`
Expected: PASS

- [ ] **Step 5: コミット**

```bash
git add scripts/governance-check.mjs scripts/governance-check.test.mjs
git commit -m "feat: git check-ignore の判定を gitIgnoredPaths として取り出す (#1088)"
```

---

### Task 2: `checkReferences` に 3 分類を入れる

**Files:**
- Modify: `scripts/governance-check.mjs:402-450`（`checkReferences`）
- Test: `scripts/governance-check.test.mjs`（`describe("G-references checkReferences"` の末尾へ追加）

**Interfaces:**
- Consumes: Task 1 の `gitIgnoredPaths`（ここでは**直接呼ばない**——注入点だけを作る）
- Produces: `checkReferences(snapshot, docs, filterIgnored = () => new Set())` — 第 3 引数は `(paths: string[]) => Set<string>`。**既定は何も免除しない**（現行挙動と同一）。

**背景（実装者向け）:** 純関数の契約を守るため、`checkReferences` は git を呼ばない。実物を渡すのは Task 3 の `buildChecks` である。既定引数を「何も免除しない」にするのは、fixture で走る既存テスト 10 件を現行の期待のまま通すためと、注入を忘れた経路が**緑ではなく赤**へ倒れるようにするため。

判定は 2 段になる: (1) 実在しなかった参照を `pending` へ貯める、(2) 全候補を 1 回で `filterIgnored` へ渡し、免除されなかったものだけを findings にする。**spawn を 1 回に束ねるための構造**であり、findings の順序は `pending` の順序がそのまま保つ。

- [ ] **Step 1: 失敗するテストを書く**

`describe("G-references checkReferences"` の中、最後の `it` の後ろへ:

```javascript
  // --- gitignore の 3 分類（#1088）---
  // 実在する → 緑 / 実在しないが ignore 対象 → 緑 / どちらでもない → 赤
  it("実在しないが ignore 対象なら緑（生成物・ローカル設定を意図して指している）", () => {
    const s = snap({ "AGENTS.md": "実行の記録は `test-results/.last-run.json` に出る\n" });
    const ignored = () => new Set(["test-results/.last-run.json"]);
    expect(checkReferences(s, ["AGENTS.md"], ignored)).toEqual([]);
  });
  it("実在せず ignore 対象でもなければ赤のまま（typo の検出という本来の目的）", () => {
    const s = snap({ "AGENTS.md": "`docs/typo-nonexistent.md` を見よ\n" });
    const f = checkReferences(s, ["AGENTS.md"], () => new Set());
    expect(f.some((x) => x.message.includes("docs/typo-nonexistent.md"))).toBe(true);
  });
  it("既定引数は何も免除しない（注入を忘れた経路は緑でなく赤へ倒れる）", () => {
    const s = snap({ "AGENTS.md": "`test-results/.last-run.json` を見よ\n" });
    expect(checkReferences(s, ["AGENTS.md"])).toHaveLength(1);
  });
  it("文書ディレクトリ基準の候補も判定へ渡る", () => {
    const s = snap({ "docs/a.md": "`gen/out.json` に出る\n" });
    const seen = [];
    const ignored = (paths) => {
      seen.push(...paths);
      return new Set(paths.filter((p) => p === "docs/gen/out.json"));
    };
    expect(checkReferences(s, ["docs/a.md"], ignored)).toEqual([]);
    expect(seen, "ルート基準の候補も渡っていない").toContain("gen/out.json");
  });
  it("Markdown リンクにも同じ 3 分類が当たる", () => {
    const s = snap({ "AGENTS.md": "[記録](test-results/.last-run.json)\n" });
    expect(checkReferences(s, ["AGENTS.md"], () => new Set(["test-results/.last-run.json"]))).toEqual([]);
  });
  it("filterIgnored の呼び出しは 1 回（spawn を束ねる構造の固定）", () => {
    const s = snap({ "AGENTS.md": "`docs/x1.md` と `docs/x2.md` と [y](docs/x3.md)\n" });
    let calls = 0;
    checkReferences(s, ["AGENTS.md"], (paths) => {
      calls += 1;
      return new Set(paths);
    });
    expect(calls).toBe(1);
  });
```

- [ ] **Step 2: 落ちることを確認する**

Run: `npx vitest run scripts/governance-check.test.mjs -t "G-references"`
Expected: FAIL — 「実在しないが ignore 対象なら緑」が 1 件の finding を返す（第 3 引数が無視されている）

- [ ] **Step 3: 実装する**

`scripts/governance-check.mjs:402` からの `checkReferences` を次で置き換える（`exists` の中身は現行のまま・変えるのは findings の作り方だけ）:

```javascript
export function checkReferences(snapshot, docs, filterIgnored = () => new Set()) {
  const findings = [];
  const fileSet = new Set(snapshot.files);
  const exists = (doc, ref, { allowSuffix = false } = {}) => {
    const norm = (p) => path.posix.normalize(p);
    if (fileSet.has(norm(ref))) return true; // リポジトリルート基準
    const rel = norm(path.posix.join(path.posix.dirname(doc), ref)); // 文書ディレクトリ基準
    if (fileSet.has(rel)) return true;
    // crate 内相対参照（`lib/types.ts` = ui/src/lib/types.ts、`commands/launch.rs` =
    // src-tauri/src/commands/launch.rs 等）はサフィックス一致で解決する（意図的な近似）。
    // バッククォート参照（`/` 必須の述語 = 2 セグメント以上）に限る——Markdown リンクへ
    // 適用すると、壊れた相対リンクが同 basename の別ファイルで偽陰性になる
    if (!allowSuffix) return false;
    const suffix = `/${norm(ref)}`;
    return !suffix.includes("..") && snapshot.files.some((f) => f.endsWith(suffix));
  };
  // 実在しなかった参照は**いったん保留する**——ignore 判定を 1 回の spawn に束ねるため（#1088）。
  // findings の順序は pending の順序がそのまま保つ。
  const pending = [];
  const docRelative = (doc, ref) => path.posix.normalize(path.posix.join(path.posix.dirname(doc), ref));
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) {
      findings.push(finding(doc, 1, "対象文書が読めない（G-references 母集団の欠落）"));
      continue;
    }
    for (const [lineNo, line] of linesOutsideFences(text)) {
      // (i) Markdown リンク
      for (const m of line.matchAll(/\[[^\]]*\]\(([^()\s]+)\)/g)) {
        let target = m[1];
        if (/^[a-z]+:/.test(target)) continue; // https: / mailto: 等
        target = target.split("#")[0];
        if (!target) continue; // 純アンカー
        if (!exists(doc, target)) {
          pending.push({ doc, lineNo, ref: target, message: `Markdown リンク先が実在しない: ${m[1]}` });
        }
      }
      // (ii) バッククォート内パス様参照
      for (const m of line.matchAll(/`([^`\n]+)`/g)) {
        const t = m[1];
        if (!t.includes("/")) continue;
        if (/[*?{<>%\\]/.test(t)) continue;
        if (t.includes("://") || t.includes(" ")) continue;
        if (!REF_EXTENSIONS.test(t)) continue;
        if (t.startsWith("workspace/") || t.startsWith("~")) continue;
        if (!exists(doc, t, { allowSuffix: true })) {
          pending.push({ doc, lineNo, ref: t, message: `バッククォート参照のパスが実在しない: ${t}` });
        }
      }
    }
  }
  // ルート基準と文書ディレクトリ基準の**両方**を候補に出す（散文がどちらの形で書くかは選べない）
  const candidates = pending.flatMap((p) => [p.ref, docRelative(p.doc, p.ref)]);
  const ignored = filterIgnored([...new Set(candidates)]);
  for (const p of pending) {
    if (ignored.has(p.ref) || ignored.has(docRelative(p.doc, p.ref))) continue;
    findings.push(finding(p.doc, p.lineNo, p.message));
  }
  return findings;
}
```

- [ ] **Step 4: 通ることを確認する**

Run: `npx vitest run scripts/governance-check.test.mjs -t "G-references"`
Expected: PASS（既存の it も含めて全件）

- [ ] **Step 5: コミット**

```bash
git add scripts/governance-check.mjs scripts/governance-check.test.mjs
git commit -m "feat: G-references に gitignore の 3 分類を入れる (#1088)"
```

---

### Task 3: `buildChecks` で実物を注入し、配線をカナリアで縛る

**Files:**
- Modify: `scripts/governance-check.mjs:2087`（`G-references` の登録行）
- Test: `scripts/governance-check.test.mjs`（新 describe を `describe("G-stale-identifiers の配線` の直前へ）

**Interfaces:**
- Consumes: Task 1 の `gitIgnoredPaths`、Task 2 の `checkReferences` 第 3 引数

**背景（実装者向け）:** Task 2 の既定引数は「何も免除しない」なので、**注入を忘れると誤爆が戻る**。その忘れを検知するのがこのカナリアである。`G-hook-fires` の「既定引数は実物の `selectChecks` である」テストと同じ役割を、ここでは `buildChecks` 経由で果たす。

- [ ] **Step 1: 失敗するテストを書く**

import 一覧に `buildChecks` があることを確認し（既にある）、`describe("G-stale-identifiers の配線` の直前へ:

```javascript
// `checkReferences` の既定引数は「何も免除しない」ので、`buildChecks` が実物を渡し忘れると
// gitignore 済みファイルの誤爆が戻る（#1088 で解いた当の欠陥）。この describe だけがそれを縛る。
describe("G-references の配線（buildChecks が gitignore 判定を渡す）", () => {
  const wired = (contents) => buildChecks(snap(contents), {}).find((c) => c.id === "G-references").run();
  it("ignore 対象の不在パスは buildChecks 経由で緑になる", () => {
    // `AGENTS.md` は governanceDocs の母集団に入る（固定パス）
    const f = wired({ "AGENTS.md": "記録は `test-results/.last-run.json` に出る\n" });
    expect(f.filter((x) => x.message.includes("test-results/.last-run.json"))).toEqual([]);
  });
  it("非 ignore の typo は buildChecks 経由でも赤（免除が広がっていない）", () => {
    const f = wired({ "AGENTS.md": "`docs/typo-nonexistent.md` を見よ\n" });
    expect(f.some((x) => x.message.includes("docs/typo-nonexistent.md"))).toBe(true);
  });
});
```

- [ ] **Step 2: 落ちることを確認する**

Run: `npx vitest run scripts/governance-check.test.mjs -t "G-references の配線"`
Expected: FAIL — 1 件目が `test-results/.last-run.json` の finding を返す

- [ ] **Step 3: 登録行を書き換える**

`scripts/governance-check.mjs:2087` を:

```javascript
    { id: "G-references", run: () => checkReferences(snapshot, docs, gitIgnoredPaths) },
```

- [ ] **Step 4: 通ることを確認する**

Run: `npx vitest run scripts/governance-check.test.mjs -t "G-references の配線"`
Expected: PASS

- [ ] **Step 5: 実リポジトリで、過去に赤くなった形が緑になることを測る**

`docs/hooks.md:99` は現在「settings.local.json（gitignore 済みゆえバッククォートで参照しない）」とバッククォートを剥がして書かれている。**複製に変異を当てて**（稼働中の文書を書き換えず）、2026-08-14 に CI を赤くした形が緑になることを確かめる:

Run:
```bash
node -e '
import("./scripts/governance-check.mjs").then((m) => {
  const s = m.makeSnapshot(process.cwd());
  const orig = s.read("docs/hooks.md");
  const mutated = { ...s, read: (rel) => (rel === "docs/hooks.md"
    ? orig.replace("settings.local.json（gitignore 済みゆえ", "`.claude/settings.local.json`（gitignore 済みゆえ")
    : s.read(rel)) };
  const f = m.checkReferences(mutated, ["docs/hooks.md"], m.gitIgnoredPaths);
  console.log("findings =", JSON.stringify(f));
});'
```
Expected: `findings = []`（2026-08-14 の CI ではこの形が 2 件の赤だった）

- [ ] **Step 6: 全体が緑であることを確認する**

Run: `npm test && npm run governance:check`
Expected: どちらも PASS。`governance:check` の evidence 行は「検査 20 件」のまま（件数が変わるのは Task 4）

- [ ] **Step 7: コミット**

```bash
git add scripts/governance-check.mjs scripts/governance-check.test.mjs
git commit -m "feat: buildChecks が G-references へ gitignore 判定を渡す (#1088)"
```

---

### Task 4: `G-area-instrument` を検査配列の外へ移す

**Files:**
- Modify: `scripts/governance-check.mjs:2097`（登録行を削除）、`scripts/governance-check.mjs:2122` 付近（`runAll` へ移設）
- Test: `scripts/governance-check.test.mjs`（`describe("runAll（空母集団の明示 fail` の中へ追加）

**背景（実装者向け）:** 面積に合否は無い（`ADR-retire-area-budget`）。`.claude/rules/governance-docs.md` は既に「`governance:check` は実測値を報告するだけで、判定はこの約束が持つ」と書いており、**機構を規範の記述へ揃える**変更である。ただし `checkNormativeAreaInstrument` が返す finding は「入力が読めない／空」だけであり、**これは失ってはならない**——母集団が欠ければ evidence が嘘になる。ゆえに削除ではなく、`runAll` の空母集団検知と同じ位置（検査配列の外）へ移す。

- [ ] **Step 1: 失敗するテストを書く**

`describe("runAll（空母集団の明示 fail = 沈黙経路の閉塞）"` の中へ:

```javascript
  it("計器（G-area-instrument）は検査配列に無い——面積に合否は無い（ADR-retire-area-budget）", () => {
    const ids = buildChecks(snap({}), {}).map((c) => c.id);
    expect(ids).not.toContain("G-area-instrument");
  });
  it("それでも計器の母集団欠落は runAll の findings に残る（検査配列の外でも沈黙しない）", () => {
    const { findings } = runAll(snap({}));
    expect(findings.some((f) => f.message.includes("G-area-instrument 母集団の欠落"))).toBe(true);
  });
```

- [ ] **Step 2: 落ちることを確認する**

Run: `npx vitest run scripts/governance-check.test.mjs -t "計器"`
Expected: FAIL — 1 件目が `G-area-instrument` を含む配列を見つける

- [ ] **Step 3: 登録行を削除し、`runAll` へ移す**

`scripts/governance-check.mjs:2097` の次の行を**削除**:

```javascript
    { id: "G-area-instrument", run: () => checkNormativeAreaInstrument(snapshot) },
```

`runAll` の `for (const c of checks) findings.push(...c.run());` の**直後**へ:

```javascript
  // 計器は検査ではない——面積に合否は無い（`ADR-retire-area-budget`）ので「検査 N 件」に数えない。
  // ただし母集団が欠ければ下の evidence が嘘になるため、入力の健全性だけは findings に残す
  // （空母集団の明示 fail と同じ役割・検査配列の外に置く理由がこれである）。
  findings.push(...checkNormativeAreaInstrument(snapshot));
```

- [ ] **Step 4: 通ることを確認する**

Run: `npx vitest run scripts/governance-check.test.mjs && npm run governance:check`
Expected: vitest PASS、`governance:check` の evidence が「**検査 19 件**」に変わる（面積の実測値は従来どおり印字される）

- [ ] **Step 5: コミット**

```bash
git add scripts/governance-check.mjs scripts/governance-check.test.mjs
git commit -m "refactor: 判定を持たない計器を検査配列から外す (#1088)"
```

---

### Task 5: 文書の同期（傷跡を戻す）

**Files:**
- Modify: `docs/hooks.md:99`（バッククォートを戻し、剥がした理由の記述を消す）
- Modify: `scripts/governance-check.mjs:6-9`（冒頭の契約コメントへ 1 行）

**背景（実装者向け）:** Task 2〜3 が入ると「gitignore 済みゆえバッククォートで参照しない——CI のチェックアウトに存在せず `G-references` が赤くなる」は**偽になる**。同じ差分で直さなければ、規範を守る読者を誤りへ導く記述が残る。**`docs/adr/ADR-stale-identifier-detector-scope.md` の同種の記述は戻さない**——凍結された歴史である（`ADR-adr-frozen-history`）。

- [ ] **Step 1: `docs/hooks.md:99` を書き換える**

現行:
```markdown
- settings.local.json（gitignore 済みゆえバッククォートで参照しない——CI のチェックアウトに存在せず `G-references` が赤くなる）は project より**優先順位が高い**ため、
```

これを:
```markdown
- `.claude/settings.local.json`（gitignore 済み。実在検査は ignore 対象を免除するので参照してよい・#1088）は project より**優先順位が高い**ため、
```

- [ ] **Step 2: 冒頭の契約コメントを、Task 3 が変えた事実へ合わせる**

**これは飾りではない。** `scripts/governance-check.mjs:18` は「**例外は G-hook-fires ただ 1 つ**」と
全称で書いており、Task 3 で `buildChecks` が `gitIgnoredPaths` を渡した時点で**偽になる**——
G-references も外部の `git` と cwd の `.gitignore` に依存する第 2 の例外になるからである。
全称表現は実装より強い主張になった瞬間に嘘になり、規範を守る読者を誤りへ導く（`AGENTS.md`
「検証の作法（全タスク共通）」）。**触った節の隣にある主張が今も真か見る**のがこの Step の役目である。

現行（`scripts/governance-check.mjs:18-21`）:

```javascript
//   - **例外は G-hook-fires ただ 1 つ**: 判定の再実装を避けるため `.claude/hooks/post-edit.mjs` の
//     `selectChecks` を import し、既定引数として注入する（理由は同検査のコメント）。ゆえに
//     **snapshot の root（cwd）と import 元（スクリプト相対）が同じツリーであること**を前提とする——
//     `npm run governance:check` 経由では常に成り立つが、別ツリーのスクリプトを叩けば崩れる
```

これを:

```javascript
//   - **例外は 2 つある。** (1) G-hook-fires: 判定の再実装を避けるため `.claude/hooks/post-edit.mjs` の
//     `selectChecks` を import し、既定引数として注入する（理由は同検査のコメント）。ゆえに
//     **snapshot の root（cwd）と import 元（スクリプト相対）が同じツリーであること**を前提とする——
//     `npm run governance:check` 経由では常に成り立つが、別ツリーのスクリプトを叩けば崩れる。
//     (2) G-references: `gitIgnoredPaths` が同じチェックアウトの `.gitignore` を外部の `git` で読む
//     （#1088）。注入するのは `buildChecks` で、**既定引数は何も免除しない**ため純関数としての
//     テストは fixture のまま走る。決定的性は保たれる——読むのは同じチェックアウトの `.gitignore` だけで、
//     ネットワーク・時刻・環境変数に依らない（「依存ゼロ」は npm 依存の話であり、`git` は
//     チェックアウトが在る以上どちらの環境にも在る）
```

続けて「意味判断（責務の妥当性・…）は `/health-check` に残る」の行の後ろへ:

```javascript
// なお `G-workspace-lints` / `G-clippy-disallowed` は文書ではなくリポジトリ規約を見る。責務としては
// 越境だが、**唯一の検出器であり移す先が無い**ため意図してここに置く（#1088 で帰属見直しを却下）。
```

- [ ] **Step 3: 検証**

Run: `npm run governance:check`
Expected: 全検査 passed（`.claude/settings.local.json` がバッククォート参照に戻ったが、ignore 対象なので緑）

**この 1 回が Task 2〜3 の end-to-end の実測である**——実リポジトリの散文に、CI では存在しないファイル名がバッククォートで書かれ、それが緑になった。

- [ ] **Step 4: コミット**

```bash
git add docs/hooks.md scripts/governance-check.mjs
git commit -m "docs: gitignore 済みファイルの参照を戻し、規約検査の帰属を契約コメントへ書く (#1088)"
```

- [ ] **Step 5: PR を作る**

PR 本文は**ファイルへ書いてから渡す**（PowerShell の here-string を Bash ツールで打つと `@` 行が本文へ混入する）。`gh pr create` は未 push だと pre-bash hook が拒むので `&&` で繋ぐ:

```bash
git push -u origin HEAD && gh pr create --title "chore: governance:check の gitignore 誤爆を断ち、計器を検査配列から外す (#1088)" --body-file <本文を書いたファイルのパス>
```

PR 本文に必ず入れる項目（`.claude/rules/safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」——CI の実測は PR が在って初めて行える）:

```markdown
- [ ] CI の governance-check job が緑（`.claude/settings.local.json` の参照が ubuntu のチェックアウトで免除されること＝手元でしか測れなかった経路の実測）
```

---

# PR 2（B）— 構造母集団の manifest 差分

ブランチ: `chore/governance-manifest-delta`（PR 1 のマージ後に `main` から切る）

### Task 6: manifest の生成

**Files:**
- Create: `scripts/governance-manifest.mjs`
- Create: `scripts/governance-manifest.test.mjs`
- Modify: `package.json`（`scripts` へ 1 行）

**Interfaces:**
- Consumes: `scripts/governance-check.mjs` の `makeSnapshot`・`buildChecks`・`governanceDocs`
- Produces: `manifest(snapshot) => { checks: string[], docs: string[], rules: string[], skills: string[] }` — 4 列すべて sorted。`diffManifest(base, head) => string[]`（`+G-foo` / `-docs/x.md` の形）。

**背景（実装者向け）:** **件数ではなく集合**を持つ理由は、件数だと「1 消して 1 足す」が沈黙するため。構造母集団だけを対象にするのは実測に基づく——直近 20 コミットで構造（検査・対象文書・rules・skills）は 0〜1 回しか動かないのに対し、散文（見出し参照・文字数）は 11 回と 6 回動いた。散文まで対象にすると承認がゴム印化する。

- [ ] **Step 1: 失敗するテストを書く**

`scripts/governance-manifest.test.mjs` を作る:

```javascript
import { describe, it, expect } from "vitest";
import { makeSnapshot } from "./governance-check.mjs";
import { manifest, diffManifest, undeclared } from "./governance-manifest.mjs";

describe("manifest（構造母集団の集合）", () => {
  it("実リポジトリで 4 列すべてが非空", () => {
    const m = manifest(makeSnapshot(process.cwd()));
    for (const key of ["checks", "docs", "rules", "skills"]) {
      expect(m[key].length, `${key} が空（母集団の欠落）`).toBeGreaterThan(0);
    }
  });
  it("各列は sorted（readdir 順の揺れが差分に化けない）", () => {
    const m = manifest(makeSnapshot(process.cwd()));
    for (const key of ["checks", "docs", "rules", "skills"]) {
      expect(m[key], `${key} が sorted でない`).toEqual([...m[key]].sort());
    }
  });
  it("検査 ID を含む", () => {
    expect(manifest(makeSnapshot(process.cwd())).checks).toContain("G-references");
  });
});

describe("diffManifest（件数ではなく集合を比べる）", () => {
  const base = { checks: ["G-a", "G-b"], docs: [], rules: [], skills: [] };
  it("同一なら空", () => {
    expect(diffManifest(base, base)).toEqual([]);
  });
  it("追加と削除の両方を出す", () => {
    const head = { checks: ["G-a", "G-c"], docs: [], rules: [], skills: [] };
    expect(diffManifest(base, head).sort()).toEqual(["+G-c", "-G-b"]);
  });
  it("1 消して 1 足す入れ替えを沈黙させない（件数では捕まらない形）", () => {
    const head = { checks: ["G-a", "G-z"], docs: [], rules: [], skills: [] };
    expect(diffManifest(base, head).length).toBe(2);
  });
});

describe("undeclared（PR 本文に逐語で現れない delta を返す）", () => {
  it("すべて宣言されていれば空", () => {
    const body = "## governance manifest delta\n- checks: +G-c, -G-b\n";
    expect(undeclared(["+G-c", "-G-b"], body)).toEqual([]);
  });
  it("宣言が無ければ全件返る（宣言なし PR で diff が在れば赤）", () => {
    expect(undeclared(["+G-c"], "ふつうの PR 本文")).toEqual(["+G-c"]);
  });
  it("diff が空なら宣言が無くても空（既定の経路を赤にしない）", () => {
    expect(undeclared([], "ふつうの PR 本文")).toEqual([]);
  });
  it("本文が null でも落ちない", () => {
    expect(undeclared(["+G-c"], null)).toEqual(["+G-c"]);
  });
});
```

- [ ] **Step 2: 落ちることを確認する**

Run: `npx vitest run scripts/governance-manifest.test.mjs`
Expected: FAIL — `Cannot find module './governance-manifest.mjs'`

- [ ] **Step 3: 実装する**

`scripts/governance-manifest.mjs`:

```javascript
// governance manifest — 構造母集団の集合を吐き、main との差分を PR 本文の宣言と突き合わせる（#1088）。
//
// **なぜ構造だけか**: 直近 20 コミット（2026-08-12〜08-14）の実測で、構造母集団（検査・対象文書・
// rules・skills）の変動は 0〜1 回、散文母集団（見出し参照 11 回・文字数 6 回）とは桁が違った。
// 散文まで対象にすると毎 PR で承認が要り、ゴム印化する。
//
// **なぜ集合か**: 件数では「1 消して 1 足す」が沈黙する。集合なら diff がそのまま承認の材料になる。
//
// **なぜ governance-check.mjs の外か**: あちらは「依存ゼロ・決定的（ネットワーク・時刻・環境変数に
// 非依存）」を契約に持つ。PR 本文の読取と main の checkout はその契約の外にある。
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { makeSnapshot, buildChecks, governanceDocs } from "./governance-check.mjs";

/** 構造母集団の 4 列。すべて sorted——`readdirSync` の順序は ext4 で不定であり、
 *  揃えないと CI と手元で差分に化ける。 */
export function manifest(snapshot) {
  const files = (re) => snapshot.files.filter((f) => re.test(f)).sort();
  return {
    checks: buildChecks(snapshot, {})
      .map((c) => c.id)
      .sort(),
    docs: [...governanceDocs(snapshot)].sort(),
    rules: files(/^\.claude\/rules\/[^/]+\.md$/),
    skills: files(/^\.claude\/skills\/[^/]+\/SKILL\.md$/),
  };
}

const KEYS = ["checks", "docs", "rules", "skills"];

/** `+<name>` / `-<name>` の列。列をまたいで平坦化する（名前が一意なので衝突しない）。 */
export function diffManifest(base, head) {
  const out = [];
  for (const key of KEYS) {
    const b = new Set(base[key] ?? []);
    const h = new Set(head[key] ?? []);
    for (const x of h) if (!b.has(x)) out.push(`+${x}`);
    for (const x of b) if (!h.has(x)) out.push(`-${x}`);
  }
  return out;
}

/** 宣言されていない delta を返す。**書式は強制しない**——本文に逐語で現れるかだけを見る。
 *  書式を決めるとその書式が腐る側になり、ゴム印を押す欄になる。実際に `+G-foo` と打つ手間が
 *  「気づいて書いた」ことの証拠になる。 */
export function undeclared(deltas, body) {
  const text = body ?? "";
  return deltas.filter((d) => !text.includes(d));
}

// fileURLToPath を使う — URL.pathname は空白等を percent-encode するため resolve と一致せず、
// 「何もせず exit 0」という沈黙経路になる（`governance-check.mjs` の同じ行が持つ実測に倣う）
const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const args = process.argv.slice(2);
  const m = manifest(makeSnapshot(process.cwd()));
  if (args[0] === "--compare") {
    const base = JSON.parse(fs.readFileSync(args[1], "utf8"));
    const deltas = diffManifest(base, m);
    const missing = undeclared(deltas, process.env.PR_BODY);
    if (deltas.length === 0) {
      console.log("governance manifest — 構造母集団に差分なし");
    } else if (missing.length === 0) {
      console.log(`governance manifest — 差分 ${deltas.length} 件はすべて PR 本文で宣言済み: ${deltas.join(" ")}`);
    } else {
      console.error(`governance manifest — PR 本文で宣言されていない差分 ${missing.length} 件:`);
      for (const d of missing) console.error(`  ${d}`);
      console.error("PR 本文へ次の行を足してください（逐語で照合します）:");
      console.error(`  ## governance manifest delta\n  ${missing.join(", ")}`);
      process.exitCode = 1;
    }
  } else {
    console.log(JSON.stringify(m, null, 2));
  }
}
```

- [ ] **Step 4: 通ることを確認する**

Run: `npx vitest run scripts/governance-manifest.test.mjs`
Expected: PASS

- [ ] **Step 5: `package.json` へ script を足す**

`"governance:check"` の次の行へ:

```json
    "governance:manifest": "node scripts/governance-manifest.mjs",
```

- [ ] **Step 6: 手で走らせて出力を見る**

Run: `npm run governance:manifest`
Expected: 4 列の JSON。`checks` は 19 件（PR 1 の Task 4 で計器を外した後）

- [ ] **Step 7: コミット**

```bash
git add scripts/governance-manifest.mjs scripts/governance-manifest.test.mjs package.json
git commit -m "feat: 構造母集団の manifest と差分・宣言照合を足す (#1088)"
```

---

### Task 7: CI step

**Files:**
- Modify: `.github/workflows/ci.yml`（`governance-check` job へ step を 2 つ追加）

**背景（実装者向け）:** base 側（`main`）には `governance-manifest.mjs` がまだ存在しない——この PR で新設するのだから当然である。**base にスクリプトが無ければ比較を飛ばす**（初回だけ緑）。これを忘れると、この PR 自身が永久に赤くなって入らない。

- [ ] **Step 1: `ci.yml` の `governance-check` job へ step を足す**

`- name: governance check` の**後ろ**へ:

```yaml
      # #1088: 構造母集団（検査・対象文書・rules・skills）の集合を main と比べ、
      # 差分が在るのに PR 本文で宣言されていなければ落とす。散文母集団（見出し参照・文字数）は
      # 変動が多すぎて承認がゴム印化するので対象にしない（実測は
      # docs/superpowers/specs/2026-08-14-governance-check-scope-design.md）。
      # main 側にスクリプトが無い場合（この機構を導入する PR 自身）は比較を飛ばす。
      - name: governance manifest delta
        if: github.event_name == 'pull_request'
        env:
          PR_BODY: ${{ github.event.pull_request.body }}
        run: |
          git fetch --depth=1 origin main
          if git cat-file -e origin/main:scripts/governance-manifest.mjs 2>/dev/null; then
            git worktree add --detach /tmp/gov-base origin/main
            (cd /tmp/gov-base && node scripts/governance-manifest.mjs) > /tmp/base.json
            node scripts/governance-manifest.mjs --compare /tmp/base.json
          else
            echo "governance manifest — main 側にスクリプトが無いので比較を飛ばす（導入 PR）"
          fi
```

- [ ] **Step 2: シェルの分岐を手元で測る**

`git cat-file -e` の判定は、CI と同じ形で手元でも測れる:

Run: `git cat-file -e origin/main:scripts/governance-manifest.mjs 2>/dev/null && echo "在る" || echo "無い"`
Expected: `無い`（この PR がマージされるまでは）

- [ ] **Step 3: base 側の worktree 生成を手元で測る**

Run:
```bash
git worktree add --detach /tmp/gov-base origin/main && (cd /tmp/gov-base && node scripts/governance-check.mjs | head -1); git worktree remove --force /tmp/gov-base
```
Expected: base 側で `governance:check` が走り、evidence 行が出る（**worktree で走ることの確認**——`.claude/hooks/post-edit.mjs` を import する経路が別ツリーでも解決することを見る）

- [ ] **Step 4: コミット**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: 構造母集団の manifest 差分を PR 本文の宣言と突き合わせる (#1088)"
```

---

### Task 8: フォールトインジェクション — 検知器が発火しうることを測る

**Files:**
- Test: `scripts/governance-manifest.test.mjs`（末尾へ describe を追加）

**背景（実装者向け）:** #1088 が求めたのは「その検知器が発火しうるかを先に測る」ことだった。**登録配列から 1 本消したときに manifest 差分が発火する**——これがその実測である。稼働中のガードは弱めない: 実ファイルを書き換えず、`manifest()` の返り値の複製に変異を当てる。

- [ ] **Step 1: テストを書く**

```javascript
describe("フォールトインジェクション — #1088 が求めた「発火しうるか」の実測", () => {
  it("登録配列から検査が 1 本落ちれば差分が発火する（#1088 の当の欠陥）", () => {
    const base = manifest(makeSnapshot(process.cwd()));
    // 稼働中のガードは弱めない——返り値の複製に変異を当てる（.claude/rules/safety-nets.md）
    const mutated = { ...base, checks: base.checks.filter((id) => id !== "G-references") };
    expect(diffManifest(base, mutated)).toEqual(["-G-references"]);
    expect(undeclared(diffManifest(base, mutated), "宣言のない PR 本文")).toEqual(["-G-references"]);
  });
  it("走査の母集団が黙って縮んでも発火する（WALK_EXCLUDE_PATHS へ 1 行足した形）", () => {
    const base = manifest(makeSnapshot(process.cwd()));
    const mutated = { ...base, rules: base.rules.slice(1) };
    expect(diffManifest(base, mutated)).toEqual([`-${base.rules[0]}`]);
  });
  it("変異が無ければ発火しない（常に赤いゲートはゲートが無いのと同じ）", () => {
    const base = manifest(makeSnapshot(process.cwd()));
    expect(diffManifest(base, manifest(makeSnapshot(process.cwd())))).toEqual([]);
  });
});
```

- [ ] **Step 2: 走らせる**

Run: `npx vitest run scripts/governance-manifest.test.mjs`
Expected: PASS

- [ ] **Step 3: コミット**

```bash
git add scripts/governance-manifest.test.mjs
git commit -m "test: manifest 差分が登録の欠落と母集団の縮小で発火することを測る (#1088)"
```

- [ ] **Step 4: PR を作る**

```bash
git push -u origin HEAD && gh pr create --title "chore: 構造母集団の manifest 差分を PR 本文の宣言と突き合わせる (#1088)" --body-file <本文を書いたファイルのパス>
```

PR 本文のチェックリスト（CI の実測は PR が在って初めて行える）:

```markdown
- [ ] CI の `governance manifest delta` step が「main 側にスクリプトが無いので比較を飛ばす」を出して緑
- [ ] マージ後、次の PR で差分照合が実際に動くことを確認する（この PR では原理的に測れない）
```

---

## この計画に含まれないもの

**A（per-check 分割）は別の計画とする。** spec §5 が規定する `scripts/governance/checks/*.mjs` への分割は、20 ファイルの移設 + 115KB のテスト分割 + registry の導入からなり、C・B とは独立に完結する大きさを持つ。**C と B がマージされた後に別 plan を起こす**——spec §6 が「途中で止めても C と B は残る」順序を選んだ理由がこれである。

なお A の完了後に「検査ファイルごと削除された」場合を捕まえるのは Task 6 の manifest である（A 単独では沈黙する残余）。**B を先に入れる順序がその備えになっている。**

## Self-Review — spec カバレッジ

| spec の節 | 実装するタスク |
|---|---|
| §3.1 gitignore の 3 分類 | Task 1・2・3 |
| §3.2 計器を検査配列から外す | Task 4 |
| §3.3 `docs/hooks.md` の同期 | Task 5 Step 1 |
| §3.4 帰属見直しの却下（契約コメント 1 行） | Task 5 Step 2 |
| §4.1 構造だけを対象にする | Task 6（`manifest` の 4 列） |
| §4.2 件数ではなく集合 | Task 6（`diffManifest` と入れ替えのテスト） |
| §4.3 承認チャネルと fail-closed の向き | Task 6（`undeclared`）・Task 7（CI step） |
| §5 per-check 分割 | **別 plan**（上記） |
| §6 各段のフォールトインジェクション | Task 3 Step 5・Task 8 |
