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
//! - **走査が広がったときは赤側へ倒れる**——`lib.mjs` のヘッダが名指しする将来（2 つ目の npm パッケージで
//!   `ui/node_modules` が走査に入る形）が来ると vendor の `.mjs` への被覆要求で赤くなる。向きは沈黙ではなく
//!   loud なので安全側である。そのときは `WALK_EXCLUDE_PATHS` へ 1 行足すか下の `inPopulation` を絞る
//!   ——同ヘッダが言うとおり、向きを決めるのは走査器ではなく呼び出し点の述語であり、それがこの検査である。
import { finding, globToRegex, rulePathPatterns } from "../lib.mjs";

export const id = "G-rules-script-coverage";

/** 判定を持つスクリプトの拡張子。`commentFamilyOf` と違い**配送の母集団**を決めるので別概念である。 */
const SCRIPT_EXT = /\.(mjs|ps1|psm1)$/;

/** 縛る rule と、その rule が覆うべき母集団。**この表が SSOT である。**
 *
 *  `governance-docs.md` の母集団を `scripts/` 部分木へ限るのは、あちらの `paths` が `.claude/hooks/` を
 *  持たない＝射程がそもそも違うためである。**#837 が価値を置いた「2 rules の配送対象の一致」は
 *  `scripts/` 部分木について保つ**——片方だけが古びる形をそこで塞ぐ。 */
const COVERAGE = [
  { rule: ".claude/rules/safety-nets.md", inPopulation: (f) => SCRIPT_EXT.test(f) },
  { rule: ".claude/rules/governance-docs.md", inPopulation: (f) => f.startsWith("scripts/") && SCRIPT_EXT.test(f) },
];

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return checkRulesScriptCoverage(snapshot);
}

export function checkRulesScriptCoverage(snapshot) {
  const findings = [];
  for (const { rule, inPopulation } of COVERAGE) {
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
    const population = snapshot.files.filter(inPopulation);
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
