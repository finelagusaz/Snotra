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
import { DOMAIN_SPECS } from "./governance/domains.mjs";

/** 構造母集団の列。すべて sorted——`readdirSync` の順序は ext4 で不定であり、
 *  揃えないと CI と手元で差分に化ける。
 *
 *  `checks` は `buildChecks` が積む検査 ID のみを見る——`G-area-instrument` は合否を持たない
 *  計器で `buildChecks` を経由せず `runAll` へ直接 push されるため、この列には現れない
 *  （その push 行を消しても manifest は沈黙する。歯止めは `governance-check.test.mjs` の
 *  カナリアテストの側にある）。
 *
 *  `domains` は `DOMAIN_SPECS` の**名前の集合**である（I3）。これが無いと、ドメインを 1 つ
 *  丸ごと消しても `G-domain-anchors` は残ったドメインだけを見て緑を返し、`domains.test.mjs` の
 *  `domains.size === DOMAIN_SPECS.length` も両辺が同時に減るので沈黙する——「錨が 0 本は赤、
 *  ドメインごと消えるのは緑」という非対称になっていた。**件数ではなく名前の集合**にするのは、
 *  件数だと入れ替えが沈黙するのと、ドメインの増減以外では動かない値にするためである
 *  （設計が件数の列を却下したのはこの前者の理由による）。他の列と違い snapshot を読まない
 *  ——ドメインの一覧はファイル走査ではなくソースの宣言から出る。 */
export function manifest(snapshot) {
  const files = (re) => snapshot.files.filter((f) => re.test(f)).sort();
  return {
    checks: buildChecks(snapshot, {})
      .map((c) => c.id)
      .sort(),
    docs: [...governanceDocs(snapshot)].sort(),
    rules: files(/^\.claude\/rules\/[^/]+\.md$/),
    skills: files(/^\.claude\/skills\/[^/]+\/SKILL\.md$/),
    domains: DOMAIN_SPECS.map((s) => s.name).sort(),
  };
}

/** 列の一覧の SSOT。テストもここを読む——列を足したとき「非空」「sorted」の検算から
 *  漏れる（＝新しい列だけが無検査になる）形を構造的に消すため。 */
export const KEYS = ["checks", "docs", "rules", "skills", "domains"];

/** 格下げ中の列（`ADR-governance-meta-demotion`）。**`manifest()` は出し続ける**——数字が取れなく
 *  なると、戻す/撤去するの判定材料が消える。差分だけを取らない。 */
export const META_KEYS = ["domains"];

/** 差分を取る列。監査モード（`SNOTRA_GOV_META_AUDIT=1`）では格下げ中の列も戻す。 */
export const diffKeys = () =>
  process.env.SNOTRA_GOV_META_AUDIT === "1" ? KEYS : KEYS.filter((k) => !META_KEYS.includes(k));

/** `+<name>` / `-<name>` の列。
 *
 *  ファイル名の列は構造的に重なる——`governanceDocs` の定義そのものが `.claude/rules/` 配下の md と
 *  `.claude/skills/` 配下の SKILL.md の glob を自分の腕として持つため（`governance-check.mjs`
 *  `governanceDocs` 実測）、`rules`/`skills` の要素は必ず `docs` にも現れる。それでも別の列を
 *  残すのは、`docs` が `governanceDocs()` の定義から出るのに対し `rules`/`skills` はファイル
 *  走査だけから独立に導出されるためで、この二重導出こそが母集団の裏取りになる——重なりは
 *  消すのではなく許容する。一方 diff は「名前の集合」であり、同じ名前が複数列から来ても
 *  意味は 1 つなので、返す前に重複を畳む。 */
export function diffManifest(base, head) {
  const out = new Set();
  for (const key of diffKeys()) {
    const b = new Set(base[key] ?? []);
    const h = new Set(head[key] ?? []);
    for (const x of h) if (!b.has(x)) out.add(`+${x}`);
    for (const x of b) if (!h.has(x)) out.add(`-${x}`);
  }
  return [...out];
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
      console.error("本文を編集したら、この governance-check job を re-run してください（push は不要です）。");
      process.exitCode = 1;
    }
  } else {
    console.log(JSON.stringify(m, null, 2));
  }
}
