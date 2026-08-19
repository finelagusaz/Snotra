//! 編集したファイルに帰属する governance findings を、編集直後に報告する（#1139）。
//!
//! **合否を持たない。** `checks/` の外に在ることが「検査ではない」の担保である
//! （`dependents.mjs` / `instrument.mjs` と同じ理由——registry は `checks/` 直下だけを走査するので、
//! ここに在る限り「検査 N 件」には数えられない）。**`checks/` へ置いてはならない**——
//! `registry.mjs` の `checkModulesFrom` が `id` / `run` の export を要求して throw し、
//! `governance:check` 自体が起動しなくなる。
//!
//! **判定を再実装しない。** `checkModuleIndex` / `checkReferences` をそのまま呼ぶ。索引整合と参照実在の
//! SSOT は各検査であり、ここが持つのは**母集団の絞り込みと帰属の判定**だけである。
//!
//! **CI を置き換えない。** ここが見るのは「今編集した 1 ファイルに帰属する分」に限られる。
//! **射程の穴の一覧はここに写さない**——`docs/hooks.md`「検査ではない reminder」が正本であり、
//! 4 枚の規範文書もそちらを指している（写しを持つと、増えたときに片方だけが知っている状態になる。
//! 実際この `//!` は 3 つ挙げて正本の 4 つとずれていた）。全体の照合は PR CI の
//! `governance-check` job が引き続き担う。
//!
//! **CLI として hook から subprocess で呼ばれる。** import ではないのは、静的 import が
//! `post-edit.mjs` の `try { main() } catch` の**外**で走り、解決に失敗すると JSON エンベロープを
//! 出さずにプロセスごと落ちるからである（`.rs` の fmt / clippy / test まで含めて全編集で hook が沈黙する）。

import path from "node:path";
import { fileURLToPath } from "node:url";
import { makeSnapshot, governanceDocs, gitIgnoredPaths } from "./lib.mjs";
import { checkModuleIndex, MODULE_INDEX_CRATES } from "./checks/G-module-index.mjs";
import { checkReferences } from "./checks/G-references.mjs";

/**
 * `rel` が属する索引照合の対象（`{ name, whole }`）。無ければ null。
 *
 * **`cfg.src` と `cfg.exts` で判定するのは費用の最適化であって、正しさの門ではない。**
 * `G-module-index` の逆方向が名指すのは `cfg.src` 配下のファイルだけなので、`<crate>/tests/*.rs` の
 * `rel` はそもそも findings のメッセージに現れず、**crate 名の前方一致へ退化させても後段の
 * `attributesTo` が 0 件へ落とす**。実測（2026-08-19・実ツリーの `.rs` 101 件と、`tests/` 配下へ
 * 索引を持たない `.rs` を注入した対照）: 出力の差分 0 件で、変わるのは `checkModuleIndex` を呼ぶ
 * 回数だけだった（95 → 101）。**ゆえにこの判定を守る検知器は無い**——変えても誰も赤くしない。
 * それでもここを `cfg.src` に保つのは、(1) 呼ばずに済む分を呼ばないため、(2) `attributesTo` を
 * 将来ゆるめたときに 2 段目として効くためである。
 *
 * `whole` は「その crate の索引を全件返すか」。`<crate>/CLAUDE.md` を編集したときは順方向
 * （索引に実在しないファイル名）も**その文書に帰属する**ので絞らない。
 */
function moduleIndexCrateOf(rel) {
  for (const [name, cfg] of Object.entries(MODULE_INDEX_CRATES)) {
    if (rel === `${name}/CLAUDE.md`) return { name, whole: true };
    if (rel.startsWith(cfg.src) && cfg.exts.test(rel)) return { name, whole: false };
  }
  return null;
}

/**
 * finding のメッセージが `rel` を**パスとして完結した形**で名指しているか。
 *
 * **素の `includes` では接頭辞で誤爆する**——`実ファイル <path>/a.rs.bak.rs が…` は
 * `<path>/a.rs` を部分文字列として含む。直後の 1 文字がパスの継続でないことを要求して境界を作る。
 *
 * **この結合は文字列であって契約ではない。** `finding` は `{file, line, message}` で、`file` は
 * **文書側**（`<crate>/CLAUDE.md`）を指すため、編集ファイルとの結合は message にしか無い。
 * ゆえに `G-module-index` のメッセージ書式が変われば**静かに 0 件へ倒れる**（沈黙側）。
 * 検知器は `edit-findings.test.mjs` が実物の `checkModuleIndex` を呼んでいることそのものである
 * （モックへ替えると、この結合は誰にも見られなくなる）。
 */
function attributesTo(message, rel) {
  for (let i = message.indexOf(rel); i !== -1; i = message.indexOf(rel, i + 1)) {
    const next = message[i + rel.length];
    if (next === undefined || !/[A-Za-z0-9._/\\-]/.test(next)) return true;
  }
  return false;
}

/**
 * 編集された 1 ファイルに帰属する findings。
 *
 * **索引と参照実在は排他ではない**——`<crate>/CLAUDE.md` は `MODULE_INDEX_CRATES` の索引対象であり、
 * かつ `governanceDocs()` にも含まれるので、両方が帰属する。
 *
 * **母集団はスナップショット全体が要る**（差分では判定できない）。`checkModuleIndex` は
 * `allBasenames`（全ファイル）と `cfg.src` 配下の全 `.rs` を、`checkReferences` は実在判定に
 * `snapshot.files` 全体を引く。呼び出し側が `makeSnapshot` を組んで渡すのはそのためである。
 */
export function scopedFindings(snapshot, rel, filterIgnored = gitIgnoredPaths) {
  const findings = [];
  const crate = moduleIndexCrateOf(rel);
  if (crate) {
    const all = checkModuleIndex(snapshot, [crate.name]);
    findings.push(...(crate.whole ? all : all.filter((f) => attributesTo(f.message, rel))));
  }
  if (rel.endsWith(".md") && governanceDocs(snapshot).includes(rel)) {
    findings.push(...checkReferences(snapshot, [rel], filterIgnored));
  }
  return findings;
}

/** 報告に並べる件数の上限。**`dependents.mjs` の `LISTED` と共有しない**——参照の一覧と索引の一覧では
 *  読みやすい件数が違い、片方だけが変わる将来を挙げられる（＝別概念である）。 */
const LISTED = 3;

/** hook が読む 1 行。**帰属する findings が無ければ空文字**（呼び出し側は空なら何も出さない）。 */
export function reportFor(snapshot, rel, filterIgnored = gitIgnoredPaths) {
  const findings = scopedFindings(snapshot, rel, filterIgnored);
  if (findings.length === 0) return "";
  const head = findings.slice(0, LISTED).map((f) => f.message).join(" / ");
  const rest = findings.length > LISTED ? ` ほか ${findings.length - LISTED} 件` : "";
  return (
    `WARN: ${rel} の編集に帰属するガバナンスの不整合が ${findings.length} 件あります` +
    `（${head}${rest}）。**この報告は編集したファイルに帰属する分だけである**——` +
    "全体の照合は `npm run governance:check`（PR では CI の governance-check job）が担う。"
  );
}

// ---------------------------------------------------------------------------
// CLI: `node scripts/governance/edit-findings.mjs <rel>` — cwd をツリー根として、
// そのファイルに帰属する報告を stdout へ出す。**exit code は常に 0 である**
// （合否を持たない計器なので、赤にする資格がない）。
// ---------------------------------------------------------------------------

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const rel = process.argv[2];
  if (!rel) {
    console.error("usage: node scripts/governance/edit-findings.mjs <path>");
  } else {
    const line = reportFor(makeSnapshot(process.cwd()), rel.replaceAll("\\", "/"));
    if (line) console.log(line);
  }
  process.exitCode = 0;
}
