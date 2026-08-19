//! G-rules-script-coverage — 判定を持つスクリプトが `.claude/rules/` の `paths` に覆われているかの照合。
//!
//! **`G-rules-globs` の逆向きである。** あちらは glob → 実ファイルが 0 件、こちらは実ファイル → glob が 0 件。
//! 向きが要る理由: #1088/#1093 で検査の判定がディレクトリごと `scripts/governance/` へ移り、
//! `scripts/*.mjs`（`*` は `/` を跨がない）の外へ出たが、**facade が今もマッチするので
//! 「マッチ 0 件」にはならず、あちらは緑のままだった**（#1143）。ファイル側から見れば黙らない。
//!
//! **宣言する死角**——ここが見ないもの:
//! - **対象外の拡張子**（`.sh` 等。`scripts/run-codex.sh` は判定に入らない）
//! - **harness が実際に配送するか**は見ない。`globToRegex` は documented な glob 意味論の近似であり
//!   （正本は `lib.mjs` の doc）、言えるのは「その意味論で覆われている」までである
//! - **意味的にセーフティネットかは判定しない**。母集団は拡張子で取り、誤配送は受け入れる側へ倒す
//!   （#837 が `clean-worktrees.mjs` について引き受けた先例と同じ）
//! - **母集団は `makeSnapshot` 由来**ゆえ git 未追跡ファイルも入る
//! - **`COVERAGE` に載っていない rule は見ない**。母集団を持つのは下の表の 2 本だけで、他の rules が
//!   何を覆っていようと（覆っていまいと）この検査は黙る。**表に足すのは手作業である**
//! - **走査が広がったときは赤側へ倒れる**——`lib.mjs` のヘッダが名指しする将来（2 つ目の npm パッケージで
//!   `ui/node_modules` が走査に入る形）が来ると vendor の `.mjs` への被覆要求で赤くなる。向きは沈黙ではなく
//!   loud なので安全側である（**未被覆は切り詰めず全件名指すので、その赤は長い**）。そのときは
//!   `WALK_EXCLUDE_PATHS` へ 1 行足すか下の `narrow` を絞る——同ヘッダが言うとおり、向きを決めるのは
//!   走査器ではなく呼び出し点の述語であり、それがこの検査である。
//!
//! **母集団を狭める道は 2 つあり、縛っている検知器が別々である**（どちらも実ツリーが全件被覆である限り
//! 判定結果を変えないので、**放っておけばどの層も赤にしない**・2026-08-19 実測）:
//! - `SCRIPT_EXT`（`.mjs` へ狭める等）→ `G-rules-script-coverage.test.mjs`「母集団の下界」
//! - **`lib.mjs` の `WALK_EXCLUDE_PATHS`（走査そのものを狭める）** → 同「実ツリーの母集団」。
//!   fixture のテストはこちらに反応できない——`makeSnapshot` を呼ばないためである。実測では、
//!   `WALK_EXCLUDE_PATHS` へ `scripts/governance` を足すと #1093 の再発形の検知が **106 件 → 0 件**へ落ち、
//!   `governance:check` も manifest delta も沈黙した（manifest のファイル名の列はどれも `scripts/` を
//!   見ておらず、`checks` 列は `readdirSync` 由来でスナップショットを経由しない）。
//! **どちらを狭めるときも、対応する側のテストを同じ変更で直すこと。**
//! **どちらの狭窄も、`judgingScripts` ドメインの錨が `governance:check` の実行時にも見る**（Phase 2 で
//! 足した層）——上の 2 行は `npm test` でしか走らないので、テストを飛ばした経路が残っていた。
import { finding, globToRegex, rulePathPatterns } from "../lib.mjs";

export const id = "G-rules-script-coverage";
export const domains = ["ruleDocs", "judgingScripts"];

/** 判定を持つスクリプトの拡張子。`commentFamilyOf` と違い**配送の母集団**を決めるので別概念である。 */
const SCRIPT_EXT = /\.(mjs|ps1|psm1)$/;

/** `judgingScripts` ドメインのメンバー——判定を持つスクリプトの全体。
 *  **腕ごとの下界は `domains.mjs` の錨が `governance:check` の実行時に見る**（この検査の外側の層）。 */
export function judgingScripts(snapshot) {
  return snapshot.files.filter((f) => SCRIPT_EXT.test(f));
}

/** 縛る rule と、その rule が覆うべき母集団——**`judgingScripts` のどこを取るか**。
 *  **この表が SSOT である。**
 *
 *  `governance-docs.md` の母集団を `scripts/` 部分木へ限るのは、あちらの `paths` が `.claude/hooks/` を
 *  持たない＝射程がそもそも違うためである。**#837 が価値を置いた「2 rules の配送対象の一致」は
 *  `scripts/` 部分木について保つ**——片方だけが古びる形をそこで塞ぐ。
 *
 *  述語が `snapshot.files` 全体ではなく**ドメインのメンバーに当たる**ことに注意する（拡張子の判定は
 *  `judgingScripts` が既に済ませている）。名前を `inPopulation` から替えたのは、意味が変わった述語に
 *  同じ名前を残すと呼び出し側が黙って古い意味のまま通るためである。 */
export const COVERAGE = [
  { rule: ".claude/rules/safety-nets.md", narrow: () => true },
  { rule: ".claude/rules/governance-docs.md", narrow: (f) => f.startsWith("scripts/") },
];

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return checkRulesScriptCoverage(snapshot);
}

export function checkRulesScriptCoverage(snapshot) {
  const findings = [];
  const members = judgingScripts(snapshot);
  for (const { rule, narrow } of COVERAGE) {
    const text = snapshot.read(rule);
    // 下界の canary 3 種。**被覆形の述語は母集団が縮む側で沈黙する**ので、縮み方ごとに赤を置く
    // （`docs/development-principles.md`「検証の層と、層と層の隙間」）。
    if (text === null) {
      findings.push(finding(rule, 1, "rule が snapshot に無い（G-rules-script-coverage 母集団の欠落）"));
      continue;
    }
    const patterns = rulePathPatterns(text);
    if (patterns.length === 0) {
      // 1 件で打ち切る——ここで全件を名指すと、原因 1 つに対して母集団の数だけ finding が出る
      findings.push(finding(rule, 1, "frontmatter に paths パターンが 1 件も無い"));
      continue;
    }
    const population = members.filter(narrow);
    if (population.length === 0) {
      findings.push(finding(rule, 1, "覆うべきスクリプトの母集団が 0 件（走査の欠落）"));
      continue;
    }
    const regexes = patterns.map(globToRegex);
    // **切り詰めない。** 未被覆は 1 ファイル 1 finding で全部名指す——壊れているときだけ長くなる形である
    // （#1093 の再発形なら 51 件 × 2 rules）。切り詰めると「どこまでが漏れか」が読めなくなる。
    for (const f of population) {
      if (!regexes.some((re) => re.test(f))) findings.push(finding(rule, 1, `paths がこのスクリプトを覆わない: ${f}`));
    }
  }
  return findings;
}
