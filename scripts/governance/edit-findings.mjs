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
//! 規範文書もそちらを指している（写しを持つと、増えたときに片方だけが知っている状態になる。
//! 実際この `//!` は自前の列挙を持っていて正本とずれた）。全体の照合は PR CI の
//! `governance-check` job が引き続き担う。
//!
//! **CLI として hook から subprocess で呼ばれる。** import ではないのは、静的 import が
//! `post-edit.mjs` の `try { main() } catch` の**外**で走り、解決に失敗すると JSON エンベロープを
//! 出さずにプロセスごと落ちるからである（`.rs` の fmt / clippy / test まで含めて全編集で hook が沈黙する）。

import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  makeSnapshot,
  governanceDocs,
  gitIgnoredPaths,
  allHeadingRefDocs,
  headingRefSourceDocs,
  staleIdentifierTargets,
} from "./lib.mjs";
import { checkModuleIndex, MODULE_INDEX_CRATES } from "./checks/G-module-index.mjs";
import { checkReferences } from "./checks/G-references.mjs";
import { checkHeadingRefs } from "./checks/G-heading-refs.mjs";
import { checkNearHeadingRefs } from "./checks/G-near-heading-refs.mjs";
import { checkFoldedHeadingRefs } from "./checks/G-folded-heading-refs.mjs";
import { checkFoldedCodeSpans } from "./checks/G-folded-code-spans.mjs";
import { checkFullwidthDocLinkBrackets } from "./checks/G-fullwidth-doc-link-bracket.mjs";
import { checkStaleIdentifiers } from "./checks/G-stale-identifiers.mjs";
import { checkAdrFileNames } from "./checks/G-adr-file-names.mjs";

/**
 * 走査元（参照・識別子が**書かれている**側）を 1 枚へ絞れる検査。
 *
 * **帰属の作り方が索引側と違う。** `G-module-index` は finding のメッセージへ文字列で結合して
 * 帰属を判定する（`attributesTo`）が、こちらは**母集団そのものを `[rel]` へ絞る**ので帰属が
 * 構造的に決まる——メッセージ書式が変わっても静かに 0 件へ倒れる経路が無い。
 *
 * **着地先は snapshot 全体のままである。** アンカー（`collectAnchors`）も語彙
 * （`currentVocabulary`）も全ファイルから導くので、**判定の強さは CI と同じ**であり、
 * 違うのは「どのファイルに書かれた違反を報告するか」だけである。
 *
 * **母集団の述語を写さない**——各検査が読む母集団は `lib.mjs` の導出関数が正本で、ここは
 * その `includes` を取るだけである（写すと、母集団が動いたとき片方だけが知っている状態になる）。
 */
/**
 * **export しているのは `G-edit-findings-table` が照合するためである。**
 * あちらは `docs/hooks.md`「検査ではない reminder（発火一覧に現れない）」の表が
 * この配列と釣り合っているかを見る——**判定を再実装せず、この配列そのものを読む**
 * （`G-hook-fires` が `selectChecks` を import して呼ぶのと同じ理由。抽出で近似すると、
 * 閉じたい写しを一段下で作り直すことになる）。**要素を足すときは表にも行を足す。**
 * **`G-edit-findings-table.test.mjs` の `ALL` にも判定名を足す**——あちらは fixture の表を組むための
 * 手書きの一覧で、足し忘れると 9 本が赤になる（機構が捕まえるので沈黙はしないが、#1172 では
 * 敵対的調査も独立導出も「検査 ID の列挙は無い」と結論していた——検査 ID で grep しており、
 * 判定**関数名**の写しは別の綴りだった）
 */
export const SCAN_SCOPED = [
  { population: allHeadingRefDocs, check: checkHeadingRefs },
  { population: allHeadingRefDocs, check: checkNearHeadingRefs },
  { population: allHeadingRefDocs, check: checkFoldedHeadingRefs },
  // **着地先を持たない判定である。** 上の 3 本は「参照が着地するか」を snapshot 全体へ問うが、
  // こちらは編集した 1 枚の中で完結する——前倒しの条件（`ADR-edit-time-check-scope`「決定」）を
  // 既存より強く満たす形であり、**書いた瞬間に鳴ることが #992 の動機そのものである**
  { population: allHeadingRefDocs, check: checkFoldedCodeSpans },
  // 同じく編集した 1 枚の中で完結する。母集団は検査の射程（`.rs` の doc 行）と一致させて `.rs` 全件にする（#1172）
  { population: headingRefSourceDocs, check: checkFullwidthDocLinkBrackets },
  { population: staleIdentifierTargets, check: checkStaleIdentifiers },
];

/**
 * `rel` が属する索引照合の対象（`{ name, whole }`）。無ければ null。
 *
 * **判定は `cfg.src` と `cfg.exts` で行う。** 確かに効くのは費用で、`checkModuleIndex` を呼ばずに済む
 * 分を呼ばない（実測 2026-08-19: 実ツリーの `.rs` 101 件に対し呼び出しが 95 対 101）。
 *
 * **正しさの側では後段の `attributesTo` と重なっている。** 逆方向の findings が名指すのは `cfg.src`
 * 配下のファイルだけなので、`<crate>/tests/*.rs` の `rel` は帰属フィルタでも落ちる——**実ツリーの
 * `.rs` 101 件を crate 名の前方一致版と突き合わせて出力の差分 0 件を実測した**（測ったのはこの母集団である）。
 * **重なりには切れ目がある**: 順方向の finding は索引に書かれた token をそのままメッセージへ載せるので、
 * **`rel` がツリーに実在しない呼び出し**では前方一致版だけが誤って帰属させうる（実測: 索引が
 * `<crate>/tests/ghost.rs` を持ち、実ファイルも `rel` も無いとき、現行 0 件 / 前方一致版 1 件）。
 * hook 経由の `rel` は編集直後の実在ファイルなのでこの経路には来ない。
 *
 * **この判定の形を固定する検知器は無い**——`moduleIndexCrateOf` に外部消費者は無く、`checks/` にも
 * 無いので registry の母集団外である。変えても誰も赤くしない。
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
  for (const { population, check } of SCAN_SCOPED) {
    if (population(snapshot).includes(rel)) findings.push(...check(snapshot, [rel]));
  }
  // ADR の命名だけは走査元を絞れない——母集団が**ファイル名の一覧そのもの**で、「どこに書かれたか」が
  // 存在しない。ゆえに finding の `file` で帰属させる。**空母集団の finding は落ちない**——この分岐へ
  // 入る時点で `rel` 自身が `docs/adr/` 直下に在り、`adrFiles` は空になりえないからである。
  if (/^docs\/adr\/[^/]+\.md$/.test(rel)) {
    findings.push(...checkAdrFileNames(snapshot).filter((f) => f.file === rel));
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
