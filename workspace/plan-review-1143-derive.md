# #1143 独立導出 — `.claude/rules/` の `paths` が `scripts/governance/` を覆っていない

対象 issue: **#1143**
導出者: 独立枠（`workspace/` 未読・走査は `.claude/` `scripts/` `docs/` `AGENTS.md` `CLAUDE.md` `.githooks/` `.github/` へ限定）
基準ツリー: `fix/rules-paths-scripts-coverage`（`git log main..HEAD` は 0 件＝main と同一。作業ツリーは `?? workspace/` のみ）

---

## 0. 一次証拠（実測）

### 0.1 現状の被覆（`globToRegex` を直接呼んだ実測）

```
node --input-type=module -e "…globToRegex + makeSnapshot…"
scripts/*.mjs   => 10
scripts/*.ps1   => 12
scripts/lib/**  => 10
scripts/**      => 84
scripts/ total 84 covered 32
UNCOVERED: 52 件（scripts/governance/** の 51 件 + scripts/run-codex.sh）
```

51 件の内訳は `scripts/governance/checks/G-*.mjs`（19 検査 × 2＝38）と
`scripts/governance/{lib,registry,dependents,edit-findings,evidence,instrument,test-helpers}.mjs`（+ 各 `.test.mjs`）。

### 0.2 害の量（規範が届いていない相手が、その規範を実際に書いている）

```
grep -c "」" scripts/governance/*.mjs scripts/governance/checks/*.mjs
→ files with 「」: 40 / total occurrences: 288
```

`governance-docs.md` が教える正準形 `` `<対象>`「<見出し>」 `` は #1141 以降 **スクリプトのコメントも
G-heading-refs の走査元**である（`scripts/governance-check.mjs` の evidence 行:
「見出し参照 278 件を md 47 件 + .rs 101 件 + **スクリプトのコメント 107 件**から照合」）。
その走査元の 51 ファイルへ、書き方の規範が配送されていない。

### 0.3 「配送されている」と書いてある当のファイルが配送されていない

`scripts/governance/lib.mjs:532-534`:

> **規範はすでにここへ配送されている。** `.claude/rules/governance-docs.md` の frontmatter は
> `scripts/*.mjs` / `scripts/*.ps1` / `scripts/lib/**` を含む——正準形で書けと言いながら検めていない
> 状態だった（`.rs` の非対称はこの逆で、検めるが規範を配送しない）。

**この逐語の写しは 2 か所だけである**（走査を閉じた実測。最初は
`docs/ AGENTS.md CLAUDE.md .claude/ .github/` しか掃いておらず `scripts/` と `.githooks/` が
母集団の外に居たので、掃き直した）:

```
grep -rn 'scripts/\*\.mjs\|scripts/\*\.ps1\|scripts/lib/\*\*' docs/ AGENTS.md CLAUDE.md .claude/ .github/
  → docs/adr/ADR-canonical-heading-references.md:37（現在形の引用）
  → docs/adr/ADR-plan-ledger-population-persistence.md:29,45（決定当時の記述・凍結）
  → .claude/rules/{governance-docs,safety-nets}.md（frontmatter 本体）
  → .github/workflows/e2e.yml:26（workflow の paths フィルタ。rules とは無関係）
grep -rn 'scripts/\*\.mjs\|scripts/\*\.ps1\|scripts/lib/\*\*' scripts/ .githooks/
  → scripts/governance/lib.mjs:533 のみ
```

この JSDoc は `headingRefCommentDocs`（＝**コメント記法を持つ全ファイル**を母集団にする関数）に付いており、
その母集団には `scripts/governance/**` が丸ごと入る。**この一文が書かれているファイル自身が
`paths` の外にある**（0.1 の UNCOVERED に `scripts/governance/lib.mjs` が在る）。live なコードコメントの
偽であり、#1143 の修正と同じ差分で直す対象である。

### 0.4 ベースライン

```
node scripts/governance-check.mjs
→ governance:check — 全検査 passed（検査 19 件 / 対象文書 35 件 / rules 8 件 / skills 12 件 / …）
```

---

## 1. 変更が必要なファイルの完全な一覧と、触るシンボル

推奨案（下の §5 candidate **B**）を採ったときの一覧。candidate A を採る場合の差分は §5 に書く。

| # | パス | 種別 | 触るシンボル |
|---|---|---|---|
| 1 | `.claude/rules/safety-nets.md` | 変更 | frontmatter `paths`（`scripts/*.mjs` / `scripts/*.ps1` / `scripts/lib/**` の 3 行 → `scripts/**` の 1 行）。本文は触らない |
| 2 | `.claude/rules/governance-docs.md` | 変更 | 同上 |
| 3 | `scripts/governance/lib.mjs` | 変更 | **新規 export**: `globToRegex`（`G-rules-globs.mjs` から移送）・`rulePathPatterns(snapshot, ruleFile)`。**既存**: `headingRefCommentDocs` の JSDoc（532-534 行の「規範はすでにここへ配送されている」段落）を実態へ合わせて書き直す |
| 4 | `scripts/governance/checks/G-rules-globs.mjs` | 変更 | ローカル定義の `globToRegex` を削除して `../lib.mjs` から import。`checkRulesGlobs` の frontmatter 自前パースを `rulePathPatterns` の呼び出しへ置換（パターン集合の SSOT 化） |
| 5 | `scripts/governance/checks/G-rules-globs.test.mjs` | 変更 | 3 行目の import を `import { globToRegex } from "../lib.mjs"` / `import { checkRulesGlobs } from "./G-rules-globs.mjs"` へ分ける（`globToRegex` の意味論固定テスト 6 ケースはそのまま） |
| 6 | `scripts/governance/checks/G-rules-paths-depth.mjs` | **新規** | export: `id`（= `"G-rules-paths-depth"`）・`run(snapshot, ctx)`・`deepenPattern(pattern)`・`checkRulesPathsDepth(snapshot)`・`COVERED_ANCHORS` |
| 7 | `scripts/governance/checks/G-rules-paths-depth.test.mjs` | **新規** | `deepenPattern` の代表入力・`checkRulesPathsDepth` の red/green/母集団欠落・アンカーの vacuous green 閉塞 |
| 8 | **PR 本文** | 変更 | `+G-rules-paths-depth` を**逐語で**書く（`scripts/governance-manifest.mjs` の `undeclared` は書式を強制せず本文への逐語出現だけを見る。書かなければ CI の `governance manifest delta` step が落ちる） |

任意（§4 で根拠を述べる。必須ではない）:

- `docs/adr/ADR-canonical-heading-references.md:37` — 3 パターンを逐語で引く現在形の記述。candidate B では現況と食い違うが、`ADR-adr-frozen-history`（ADR 本文は決定日時点で凍結）に従い**本文は書き換えず**、必要なら追記で足す（`ADR-plan-ledger-population-persistence.md:45` が先例の形）
- 新規 ADR — §5 の A/B と「G-rules-globs を拡張せず別検査にした」判断は否定の知識を持つ。`AGENTS.md`「ドキュメント参照」の基準（否定の知識が生じた決定のみ）には当たるが、PR 本文で足りる規模とも読める

**変更不要と確かめたもの**（方法は §4）: `scripts/governance/registry.mjs`・`vitest.config.ts`・
`.github/workflows/ci.yml`・`package.json`・`AGENTS.md`・ルート `CLAUDE.md`・`docs/hooks.md`・
`scripts/governance-check.mjs`・`scripts/governance-manifest.mjs` とその 3 テスト。

---

## 2. 検出器の実装

### 2.1 置き場所（既存構造を読んだ上での決定）

- **1 検査 1 ファイル**: `registry.mjs` は `checks/` を `readdirSync` して `id` / `run` を持つ `.mjs` を
  そのまま検査にする（`scripts/governance/registry.mjs:20-40`）。**登録行は存在しない**ので、
  `checks/G-rules-paths-depth.mjs` を置くだけで登録が完了する。ファイル名の stem と `id` の食い違いは
  `checkModulesFrom` が throw で拒む。
- **`lib.mjs` に置いてよいもの**: 同ファイル冒頭が SSOT——「helper を置く前に、まずその検査のファイルへ
  移せないかを問う。…ここに残ってよいのは…**複数の検査ファイルが import する**…だけである」。
  `globToRegex` と frontmatter パースは今回 2 検査（G-rules-globs / G-rules-paths-depth）が読むので、
  この条件に**新たに**当たる。#1088 の分割時に `globToRegex` を検査へ寄せた判断
  （当時「private helper も自分の行範囲に閉じている」）を覆すのではなく、置き場所の条件が変わっただけである。
- **G-rules-globs を拡張しない理由**（否定の知識）:
  1. 当該検査は自ら「harness の配送判定の再現ではなく**「マッチ 0 件の検知」に限定した近似**」と
     宣言している（`scripts/governance/checks/G-rules-globs.mjs:12-14`）。命題を足すとこの宣言が偽になる。
  2. `docs/adr/ADR-comment-guideline-delivery-by-pointer.md:35` が「`G-rules-globs` は…逆向き（全 `.rs` が
     覆われているか）を見ない」と現在形で書いている。拡張すると**この ADR 行が偽になり、凍結規約と衝突する**。
     別検査なら、あちらの命題は逐語で真のまま残る（新検査は crate 丸ごとの取りこぼしを見ないため・§3 S7）。
  3. 新しい `id` が `governance-manifest` の `checks` 列に `+G-rules-paths-depth` として現れ、
     **PR 本文での宣言が強制される**。拡張ではこの構造信号が出ない。

### 2.2 母集団

`.claude/rules/*.md`（`/^\.claude\/rules\/[^/]+\.md$/`）× 各 rule の frontmatter `paths` パターン全件 ×
`snapshot.files`。**G-rules-globs と同じ `rulePathPatterns` から取る**——別々にパースすると
「片方だけが見ているパターン」が作れる。

### 2.3 判定（2 本立て。∀ と ∃ を組にする）

**(∀) 深さの取りこぼし** — `docs/development-principles.md:130`「全称条件だけの検査は、集合が縮んだときに
空振りする。守りたい要素は名指しする」の前半に当たる部分。

```
deepenPattern(P):
  segs = P.split("/")
  各 i について: segs[i] が "*" を含み、かつ segs[i] !== "**" かつ segs[i-1] !== "**" なら
    その前に "**" を挿入
  例: scripts/*.mjs        -> scripts/**/*.mjs      （変化する＝判定対象）
      snotra-core/**/*.rs  -> 変化しない            （前が ** なので挿入しない＝対象外）
      docs/adr/**          -> 変化しない
      AGENTS.md            -> 変化しない
```

`deepenPattern(P) !== P` の各パターンについて、`globToRegex(deepenPattern(P))` にマッチし、
かつ**その rule のどのパターンにもマッチしない**ファイルを列挙。1 件以上あれば finding
（パターン名・件数・代表 3 件を message に入れる）。

**(∃) アンカー** — 「行ごと消す」壊れ方を ∀ は見ないので組にする。

```js
export const COVERED_ANCHORS = {
  ".claude/rules/safety-nets.md": [
    "scripts/governance/edit-findings.mjs",   // PostToolUse から subprocess で呼ばれる判定（post-edit.mjs:234）
    "scripts/governance/dependents.mjs",      // 同（post-edit.mjs:197）
  ],
  ".claude/rules/governance-docs.md": [
    "scripts/governance/lib.mjs",             // 見出し参照の走査元の定義そのもの（§0.3）
    "scripts/governance/checks/G-rules-globs.mjs",
  ],
};
```

アンカーは **2 部**で判定する:
- (a) アンカーのパスが `snapshot.files` に実在するか（改名・移動で **vacuous green** になるのを閉じる）
- (b) その rule のパターンのどれかにマッチするか

どちらが欠けても finding。「カナリアが『消えたら困る識別子』そのものを持つのは写しではなく設計である」
（`docs/development-principles.md:130`）に従い、パスの逐語保持は意図的である。

**(0 件検知)** — rules 0 件、いずれかの rule のパターン 0 件は、それぞれ明示 finding
（`governance-check.mjs` の契約「空母集団は明示 fail」に合わせる）。

**実装時の注意**: #1141 以降、`.mjs` のコメントも G-heading-refs の走査元である。
新設する `G-rules-paths-depth.mjs` / `.test.mjs` のコメントに正準形 `` `<対象>`「<見出し>」 `` を
書くなら、実在する見出しに着地させること（着地しなければ `governance:check` が自分自身で赤くなる）。

### 2.4 実測（プロトタイプを現行ツリーで走らせた結果）

```
.claude/rules/governance-docs.md | scripts/*.mjs -> scripts/**/*.mjs | uncovered 51
.claude/rules/governance-docs.md | scripts/*.ps1 -> scripts/**/*.ps1 | uncovered 0
.claude/rules/safety-nets.md     | scripts/*.mjs -> scripts/**/*.mjs | uncovered 51
.claude/rules/safety-nets.md     | scripts/*.ps1 -> scripts/**/*.ps1 | uncovered 0
```

**発火は 2 件・誤検出 0 件**。残り 6 rules（`comments.md` / `snotra-core.md` / `snotra-core-search.md` /
`snotra-settings.md` / `spec.md` / `src-tauri.md`）は、単一 `*` の前が必ず `**` か、パターンが literal
なので**構造的に対象外**であり、しきい値ではなく形で外れている。

---

## 3. この検出器が沈黙しうる経路と歯止め

| # | 壊れ方 | 緑のまま通るか | 歯止め |
|---|---|---|---|
| S1 | frontmatter の書式が変わる（引用符なし・YAML 記法変更）でパターンが 0 件に見える | ∀ が対象を失う | `rulePathPatterns` を両検査で共有し、0 件は**明示 finding**。G-rules-globs も同じ結果を見るので「片方だけが見える」状態が作れない |
| S2 | `.claude/rules/*.md` が 0 件 | 全ループが空回り | 明示 finding（G-rules-globs と同型） |
| S3 | rule から `scripts/**` の行ごと消える | ∀ は**鳴らない**（パターンが無ければ deepened も無い） | **∃ アンカー（2.3 (b)）が受ける** |
| S4 | アンカーのファイルが改名・移動 | (b) だけなら vacuous green | **(a) の実在検査**が赤にする |
| S5 | `scripts/**` を再び `scripts/*.mjs` へ狭める（今回の修正の回帰） | — | ∀ が即座に赤（2.4 と同じ形で 51 件） |
| S6 | 検査ファイルごと削除 | `registry.mjs` は readdir 由来なので `governance:check` は**沈黙する** | `governance-manifest.mjs --compare` の `-G-rules-paths-depth` が PR 本文の宣言を要求。**ただし base 側に manifest が無い PR では比較を飛ばす**（`.github/workflows/ci.yml:128`）——ここは受容する残余 |
| S7 | crate / ディレクトリを丸ごと `paths` へ足し忘れる（`comments.md` に新 crate が入らない形） | 鳴らない | **閉じない。** `ADR-comment-guideline-delivery-by-pointer.md:35` が名指しする残余はそのまま残る（この検査の射程は「既に在るパターンの深さ」だけである） |
| S8 | `globToRegex` が harness の実配送と食い違う | 近似上は緑・実配送は穴 | **閉じない。** 検出器は近似の上で測る。実配送は新鮮な context のサブエージェントで別途実測する（方法は `ADR-comment-guideline-delivery-by-pointer.md:21`——重複排除はファイル単位で 1 セッション 1 回なので、**測れる glob は 1 context につき 1 本**） |
| S9 | 新テストが編集時に走らない | 沈黙 | `selectChecks`（`.claude/hooks/post-edit.mjs:132-178`）は `scripts/**/*.mjs` に検査を割り当てず、`CHECK_DEFINITION` も 4 件のみ（`post-edit.mjs:77-82`）。**手動で `npx vitest run scripts/governance` を打つか CI の `npm test` に委ねる**。ルート `CLAUDE.md` が既に「`scripts/` 配下の非 TS ファイルの沈黙は『何も走らなかった』である」と述べており、新規の穴ではない |

**新しく生まれる拘束（宣言）**: この検査の後、「深い所に同形のファイルが在るのに、意図的にルート直下だけを
単一 `*` で拾う」書き方が**表現不能**になる。免除注記の機構は置かない（`governance-check.mjs` の契約）。
逃げ道は literal パスで名指しすること（`.claude/settings.json` と同じ形）。
`.claude/rules/safety-nets.md`「検知器は必要な分だけ縛る」に照らし、**塞げない S7 / S8 は上表で名指しして止める**。

---

## 4. 検査を 1 本増やすことの波及（実際に確かめた結果）

| 面 | 更新要否 | 確かめ方 |
|---|---|---|
| 検査の登録 | **不要** | `scripts/governance/registry.mjs:11-40` — `readdirSync(CHECKS_DIR)` 由来。登録行が存在しない |
| vitest の対象 | **不要** | `vitest.config.ts:8-12` の include に `scripts/**/*.test.mjs` が在る |
| evidence 行の件数 | **自動** | `governance-check.mjs` の `runAll` が `checkCount: checks.length` を組む。19 → 20 になる（ベースラインは §0.4） |
| 件数を書いた文書 | **不要** | `grep -rnE "[0-9]+ *(件\|本)? *検査\|全 ?[0-9]+ ?検査" docs/ AGENTS.md CLAUDE.md .claude/ --include=*.md` の live ヒットは 0。ヒットは `docs/superpowers/plans/`・`docs/superpowers/specs/` の**歴史記録**のみ（#589 で非規範化済み） |
| 件数を書いたテスト | **不要** | `grep -rnE "検査 ?[0-9]+\|checkCount\|CHECK_MODULES\|checkModulesFrom" scripts/governance-check.test.mjs scripts/governance-manifest.test.mjs scripts/governance/registry.test.mjs scripts/governance/lib.test.mjs` → ヒットは `registry.test.mjs` のみで、判定は `toBeGreaterThan(0)`（9 行目）と使い捨てディレクトリ。固定件数の assert は 0 件 |
| 検査 ID の網羅一覧を持つ文書 | **存在しない** | live 文書（`docs/*.md` `docs/adr/*.md` `AGENTS.md` `CLAUDE.md` `.claude/rules/*.md` `.claude/skills/*/SKILL.md` `.github/workflows/*.yml`）の `G-[a-z-]*` 出現を数えたところ、23 種の文脈的言及があるだけで、19 本を並べた一覧は 1 か所も無い。**逆に `G-area-budget` / `G-config-reachability` / `G-lsp-config` / `G-area-instrument` は文書に在るが `checks/` に無い**（既存の状態であり本件の範囲外） |
| `governance-manifest` | **PR 本文が必要** | `manifest()` の `checks` 列に新 ID が入り、`diffManifest` が `+G-rules-paths-depth` を出す。`undeclared()` は PR 本文への**逐語出現**だけを見る（書式は強制しない）。CI: `.github/workflows/ci.yml:111` |
| `AGENTS.md` 条件別チェック表 | **不要** | ガバナンス文書の行が既に `npm run governance:check` へ振っており、検査単位の行を持たない |
| `docs/build-commands.md:163` の散文列挙 | **不要（軽微）** | 「参照実在・モジュール索引・スキル表・SPEC 番号・**rules glob**・コマンド写像…」——新検査は既存の "rules glob" の枠に収まる。厳密を期すなら「rules glob の深さ」を足せるが、`.claude/rules/governance-docs.md`「機構の実装の詳細を散文へ写さない」に照らすと**足さない方が正しい** |
| `docs/hooks.md` の発火一覧 / `G-hook-fires` | **不要** | あの表は PostToolUse の検査 id（fmt / clippy / …）であって governance の検査は載らない。`selectChecks` を触らない限り無関係 |
| `.github/workflows/ci.yml` の governance-check job | **不要** | `run: node scripts/governance-check.mjs` の 1 行のみ（74 行目） |
| `G-stale-identifiers` | **副作用なし** | 新 ID を文書に書けば実在照合の対象になるが、ファイルが在るので緑 |
| 面積の計器（`instrument.mjs` / `areaRules`） | **数値が動くだけ** | candidate B は rules の frontmatter を 3 行 → 1 行に減らすので rules 面積が微減。**面積に合否は無い**（`ADR-retire-area-budget`） |
| `docs/adr/ADR-comment-guideline-delivery-by-pointer.md:35` | **不要** | 「`G-rules-globs` は…逆向きを見ない」は**別検査を新設する限り逐語で真のまま**（§2.1 の 2） |

---

## 5. `paths` の書き換え案と、被覆を減らさないことの確認

### candidate B（推奨）— 3 パターンを `scripts/**` の 1 本へ

```yaml
paths:
  # …他はそのまま…
  - "scripts/**"
```

### candidate A（代替）— 3 パターンを残し `scripts/governance/**` を足す

```yaml
  - "scripts/*.mjs"
  - "scripts/*.ps1"
  - "scripts/lib/**"
  - "scripts/governance/**"
```

### 実測比較（`globToRegex` を直接呼んで集合で比較した）

```
base 32  A 83  B 84
A superset of base: true
B superset of base: true
A \ base: 51 件   B \ A: [ 'scripts/run-codex.sh' ]
検出器の残 findings:  A 0 件 / B 0 件
G-rules-globs（1 件以上マッチ）: scripts/governance/** true / scripts/** true
```

**現在の被覆集合を減らさないことの確認方法**（そのまま再実行できる）:

```bash
node --input-type=module -e "
import { globToRegex } from './scripts/governance/checks/G-rules-globs.mjs';
import { makeSnapshot } from './scripts/governance/lib.mjs';
const snap = makeSnapshot(process.cwd());
const cov = (pats) => { const s = new Set();
  for (const p of pats) { const re = globToRegex(p); for (const f of snap.files) if (re.test(f)) s.add(f); }
  return s; };
const base = cov(['scripts/*.mjs','scripts/*.ps1','scripts/lib/**']);
const after = cov(['scripts/**']);              // ← 採用案のパターン集合
console.log('superset:', [...base].every(f => after.has(f)), base.size, '->', after.size);
"
```

判定は**件数ではなく包含**で見る（件数だけでは「1 消して 1 足す」が沈黙する。
`governance-manifest.mjs` が集合を採った理由と同じ）。

### B を推す理由と、A を採る場合の差分

- B は `scripts/` について**穴を表現不能にする**（`prefer-structural-over-documented-contract` /
  `AGENTS.md`「検証の層と、層と層の隙間」）。A は「次も同じ形が起きうるが検出器が赤くする」に留まる。
- B で検出器が無意味になるわけではない——**狭める向きの回帰（S5）を捕まえ**、残り 6 rules と将来の
  単一 `*` パターンを守り続ける。
- 誤配送の増分は B − A = `scripts/run-codex.sh` の **1 件だけ**。#837 は既に
  `clean-worktrees.mjs` への誤配送を「意図的なトレードオフ」として受け入れており、判断の型は同じ。
- **A を採る場合の一覧差分**: §1 の #1 / #2 の内容が「3 行を残して 1 行足す」に変わる。他の 6 ファイルは同じ。
- **B を採る場合の追加確認**: `docs/adr/ADR-canonical-heading-references.md:37` が 3 パターンを現在形で
  引いている（§1 の任意項目）。`ADR-adr-frozen-history` に従い本文は書き換えず、必要なら追記で足す。

---

## 所見（3 分類）

### 要対処

1. **`.claude/rules/safety-nets.md` / `governance-docs.md` の `paths` が `scripts/governance/**` の 51 件を
   覆っていない**（§0.1 実測）。`scripts/*.mjs` の `*` は `/` を跨がない。
   → §5 candidate B（`scripts/**`）で修正。被覆 32 → 84、既存被覆の真の上位集合であることを実測済み。
2. **`scripts/governance/lib.mjs:532-534` の「規範はすでにここへ配送されている」が偽である。**
   この一文が付いている `headingRefCommentDocs` の母集団には `scripts/governance/**` が丸ごと入るのに、
   その 51 件（うち 40 件が `「」` を計 288 個持つ）は `paths` の外にある。**同じ差分で直す。**
3. **検出器 `G-rules-paths-depth` を新設する**（`scripts/governance/checks/` に 1 検査 1 ファイル）。
   ∀（単一 `*` の深さ取りこぼし）と ∃（アンカー 2 部判定）を組にする。現行ツリーで**発火 2 件・誤検出 0 件**を実測。
4. **`globToRegex` と frontmatter パースを `scripts/governance/lib.mjs` へ移す。**
   2 検査が読むため `lib.mjs` 冒頭の掲載条件に当たる。パターン集合を 1 か所から取ることが S1 の歯止めになる。
5. **PR 本文へ `+G-rules-paths-depth` を逐語で書く。** `governance-manifest.mjs` の `undeclared` は
   本文中の逐語出現だけを見る。書かなければ CI の `governance manifest delta` step が落ちる。

### 軽微

6. `scripts/run-codex.sh` は candidate A では覆われないまま残る（検出器も `.sh` の単一 `*` パターンが
   無いので鳴らない）。B なら自動で覆われる。実害は小さく、A/B の決め手にはならない。
7. `docs/adr/ADR-canonical-heading-references.md:37` が 3 パターンを**現在形**で引く。B では現況と食い違うが、
   ADR 本文の凍結規約により書き換えず、追記で足すか、そのまま残すのが repo の型。
8. `docs/build-commands.md:163` の散文列挙は "rules glob" のままで足りる。実装の詳細を散文へ写さない規範
   （`governance-docs.md`）に照らすと**足さない方が正しい**。
9. `selectChecks` は `scripts/**/*.mjs` に検査を割り当てない（`post-edit.mjs:132-178`・`CHECK_DEFINITION` は
   4 件）。新テストは編集時に自動では走らない。**`selectChecks` を触るのは `G-hook-fires` と
   `docs/hooks.md` の発火一覧へ波及するため本件の射程外**とし、手動 vitest / CI に委ねる。
10. live 文書に `G-area-budget` / `G-config-reachability` / `G-lsp-config` / `G-area-instrument` の言及が
    在るが `checks/` に対応ファイルは無い（撤去済み・計器）。既存の状態で、本件の範囲外。

### 未検証

11. **harness の実配送**（`scripts/**` や `scripts/governance/**` で rules が実際に届くか）は測っていない。
    `globToRegex` は自ら「harness の配送判定の再現ではない近似」と宣言しており
    （`G-rules-globs.mjs:12-14`）、検出器も修正案もその近似の上で緑になっただけである。
    測る方法は `ADR-comment-guideline-delivery-by-pointer.md:21` の型——新鮮な context のサブエージェントに
    対象ファイルを 1 枚読ませ、rule が同時配送されるかを見る（**重複排除がファイル単位で 1 セッション 1 回
    なので、1 context で測れるのは 1 パターンだけ**）。#837 も同じ残余を明示して受容している。
12. **`G-rules-paths-depth` のフォールトインジェクション**（`.claude/rules/safety-nets.md` の
    `scripts/**` を `scripts/*.mjs` へ戻して赤くなること、アンカーのパスを 1 文字変えて赤くなること）は
    **実装後に必ず実測する**（`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで
    一度は実測する」）。プロトタイプでの発火 2 件は現行ツリーの実態を測っただけで、注入の実測ではない。
13. **`checkRulesGlobs` の frontmatter パースを `rulePathPatterns` へ差し替えたとき、G-rules-globs の
    判定が逐語で不変であること**は未確認。既存 2 テスト（緑・赤）に加え、`npm run governance:check` の
    evidence 行が rules 8 件のまま変わらないことで接地する。
