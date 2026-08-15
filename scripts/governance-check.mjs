// governance:check — ガバナンス文書の決定的検査（#587）。
// shebang を置かない — CI の Windows checkout（autocrlf=true）で CRLF 化された
// shebang 行は vitest の transform を SyntaxError で落とす（PR #592 で実測。
// 他の *.mjs も同じ理由で shebang なし。起動は常に `node scripts/...` 経由）。
//
// PostToolUse hook は `.md`・rules・skills に検査を割り当てない（#497 で受容した残余）。
// 本スクリプトはその残余のうち決定的に照合できる項目を PR CI（governance-check job）と
// `npm run governance:check` で引き取る。意味判断（責務の妥当性・npm 系ラッパーの等価判断・
// メモリ整合）は `/health-check` に残る（cargo フラグ照合は G-hook-commands が機械化済み・#589）。
// なお `G-workspace-lints` / `G-clippy-disallowed` は文書ではなくリポジトリ規約を見る。責務としては
// 越境だが意図的な選択であり、帰属の作り直し（他の責務分担への割り当て直し）は #1088 で却下された。
//
// 契約:
// - 依存ゼロ（Node 標準のみ）・決定的（ネットワーク・時刻・環境変数に非依存）
// - findings ゼロ → exit 0 + 照合母集団の件数を印字（根拠の接地）
// - findings あり → exit 1 + `file:line` 付き全件列挙。免除注記の機構は設けない
// - 空母集団（対象文書 0 件・rules 0 件・skills 0 件）は明示 fail（沈黙経路の閉塞）
// - 各検査はスナップショット注入の純関数（scripts/governance-check.test.mjs がフィクスチャで
//   フォールトインジェクション red / 正常 green / 判定対象外の不混入を検証する）
//   - **例外は 2 つある。** (1) G-hook-fires: 判定の再実装を避けるため `.claude/hooks/post-edit.mjs` の
//     `selectChecks` を import し、既定引数として注入する（理由は同検査のコメント）。ゆえに
//     **snapshot の root（cwd）と import 元（スクリプト相対）が同じツリーであること**を前提とする——
//     `npm run governance:check` 経由では常に成り立つが、別ツリーのスクリプトを叩けば崩れる。
//     (2) G-references: `gitIgnoredPaths` が外部の `git` でチェックアウトの gitignore 設定を読む
//     （#1088）。注入するのは `buildChecks` で、**既定引数は何も免除しない**ため純関数としての
//     テストは fixture のまま走る。読む入力の内訳・機体間の乖離の向きは `gitIgnoredPaths` の JSDoc が
//     正本（「依存ゼロ」は npm 依存の話であり、`git` はチェックアウトが在る以上どちらの環境にも在る）
import path from "node:path";
import { fileURLToPath } from "node:url";
import { CHECK_MODULES } from "./governance/registry.mjs";
import { checkArchitectureTable } from "./governance/checks/G-architecture-table.mjs";
import { MODULE_INDEX_CRATES, checkModuleIndex } from "./governance/checks/G-module-index.mjs";
import { checkBuildCommands } from "./governance/checks/G-build-commands.mjs";
import { checkCiTable } from "./governance/checks/G-ci-table.mjs";
import { globToRegex, checkRulesGlobs } from "./governance/checks/G-rules-globs.mjs";
import { modelHiddenSkills, checkSkillTable } from "./governance/checks/G-skill-table.mjs";
import { checkHookCommands } from "./governance/checks/G-hook-commands.mjs";
import { checkHookFires } from "./governance/checks/G-hook-fires.mjs";
import { checkCheckSkillEnumeration } from "./governance/checks/G-check-skill-enumeration.mjs";
import { checkModuleLinkage, declaredModuleFiles } from "./governance/checks/G-module-linkage.mjs";
import { checkReferences } from "./governance/checks/G-references.mjs";
import { checkHeadingRefs, scanHeadingRefs } from "./governance/checks/G-heading-refs.mjs";
import { checkNearHeadingRefs, scanNearHeadingRefs } from "./governance/checks/G-near-heading-refs.mjs";
import { checkStaleIdentifiers, scanStaleIdentifiers, currentVocabulary } from "./governance/checks/G-stale-identifiers.mjs";
import { checkAdrCitations, scanAdrCitations, adrCitationDocs } from "./governance/checks/G-adr-citations.mjs";
import { checkSpecSections } from "./governance/checks/G-spec-sections.mjs";
import { checkWorkspaceLints, REQUIRED_RUSTDOC_LINTS, hasWorkspaceLintsOptIn, rustdocLintsAreDenied } from "./governance/checks/G-workspace-lints.mjs";
import {
  checkClippyDisallowed,
  clippyDisallowedCount,
  disallowedMethodPaths,
  declaresEguiDependency,
  clippyMethodsDenied,
  REQUIRED_DISALLOWED_METHODS,
} from "./governance/checks/G-clippy-disallowed.mjs";
// evidence 専用の導出は、その検査のファイルから名指しで取る。**登録行と違い、
// ファイルが消えれば import が失敗して鳴る**（沈黙する写しにはならない）。
import { adrFiles, checkAdrFileNames } from "./governance/checks/G-adr-file-names.mjs";
import {
  makeSnapshot,
  finding,
  gitIgnoredPaths,
  governanceDocs,
  headingRefDocs,
  headingRefSourceDocs,
  staleIdentifierDocs,
  staleIdentifierGuideDocs,
  staleIdentifierTargets,
  collectAnchors,
  resolveRefTarget,
  STALE_EXTRA_DOCS,
  workspaceMembers,
} from "./governance/lib.mjs";
import {
  ALWAYS_LOADED_FILES,
  skillDescriptionArea,
  checkNormativeAreaInstrument,
  normativeArea,
} from "./governance/instrument.mjs";

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
  STALE_EXTRA_DOCS,
  workspaceMembers,
  checkArchitectureTable,
  MODULE_INDEX_CRATES,
  checkModuleIndex,
  checkBuildCommands,
  checkCiTable,
  globToRegex,
  checkRulesGlobs,
  modelHiddenSkills,
  checkSkillTable,
  checkHookCommands,
  checkHookFires,
  checkCheckSkillEnumeration,
  adrFiles,
  checkAdrFileNames,
  checkModuleLinkage,
  declaredModuleFiles,
  checkReferences,
  checkHeadingRefs,
  scanHeadingRefs,
  checkNearHeadingRefs,
  scanNearHeadingRefs,
  checkStaleIdentifiers,
  scanStaleIdentifiers,
  currentVocabulary,
  checkAdrCitations,
  scanAdrCitations,
  adrCitationDocs,
  checkSpecSections,
  checkWorkspaceLints,
  REQUIRED_RUSTDOC_LINTS,
  hasWorkspaceLintsOptIn,
  rustdocLintsAreDenied,
  checkClippyDisallowed,
  clippyDisallowedCount,
  disallowedMethodPaths,
  declaresEguiDependency,
  clippyMethodsDenied,
  REQUIRED_DISALLOWED_METHODS,
  ALWAYS_LOADED_FILES,
  skillDescriptionArea,
  checkNormativeAreaInstrument,
  normativeArea,
};

// ---------------------------------------------------------------------------
// 実行
// ---------------------------------------------------------------------------

/** 検査の登録表を組む。**検査 ID の SSOT は `checks/` ディレクトリの一覧である**——ファイルを
 *  置けばそのまま検査になり、ファイル名が `id` と一致することは `registry.mjs` の
 *  `checkModulesFrom` が強制する。サマリ行の件数もこの配列から計算するので、
 *  「G1..G15 passed」のような範囲を手で書く面が存在しない（範囲は黙って腐る。実例が
 *  `docs/build-commands.md` に「G1〜G12」と残っていた・#812）。
 *  ID は `G-<name>` 形で連番を持たない——連番は「いま空いている最大値 + 1」をマージの瞬間に
 *  確定させるため、並行する 2 本の PR が同じ値を見る（`.claude/rules/governance-docs.md`
 *  「序数で他を指してはならない」）。 */
export function buildChecks(snapshot, sink = {}) {
  const docs = governanceDocs(snapshot);
  const refDocs = headingRefDocs(snapshot);
  const refSourceDocs = headingRefSourceDocs(snapshot);
  // 2 つの腕は検査へ渡すときだけ束ねる。母集団としては別々に持つ——`runAll` の 0 件検知が
  // 腕ごとに 1 本ずつ要るためである（束ねた長さは片方の消滅を隠す）
  const allRefDocs = [...refDocs, ...refSourceDocs];
  const staleDocs = staleIdentifierDocs(snapshot);
  const staleGuides = staleIdentifierGuideDocs(snapshot);
  const staleTargets = staleIdentifierTargets(snapshot);
  sink.docs = docs;
  sink.refDocs = refDocs;
  sink.refSourceDocs = refSourceDocs;
  sink.staleDocs = staleDocs;
  sink.staleGuides = staleGuides;
  sink.staleTargets = staleTargets;
  const record = (key, r) => {
    sink[key] = r.checked;
    return r.findings;
  };
  const ctx = { docs, allRefDocs, staleTargets, gitIgnoredPaths, record };
  return CHECK_MODULES.map((m) => ({ id: m.id, run: () => m.run(snapshot, ctx) }));
}

export function runAll(snapshot) {
  const ctx = {};
  const checks = buildChecks(snapshot, ctx);
  const findings = [];
  if (ctx.docs.length === 0) findings.push(finding(".", 1, "ガバナンス文書が 0 件（母集団の欠落）"));
  if (ctx.refDocs.length === 0) findings.push(finding(".", 1, "G-heading-refs の対象 md が 0 件（母集団の欠落）"));
  // 腕ごとに 1 本ずつ要る（`staleDocs` / `staleGuides` と同型）——束ねると md 側の長さが
  // `.rs` の消滅を埋め、Rust コメントの見出し参照が誰にも見られないまま緑になる
  if (ctx.refSourceDocs.length === 0) findings.push(finding(".", 1, "G-heading-refs の対象ソース（.rs）が 0 件（母集団の欠落）"));
  // `staleTargets` ではなく `staleDocs` を見る——`STALE_EXTRA_DOCS` が常に長さを埋めるため、
  // targets 側で判定すると `.claude/**` が 1 枚残らず消えてもこの検知が沈黙する。
  // **グロブ由来の母集団ごとに 1 本ずつ要る**——束ねると片方が埋めた長さで他方の消滅が隠れる。
  // 固定パスの `STALE_EXTRA_DOCS` はここに要らない（読めなければ scanStaleIdentifiers が鳴る）
  if (ctx.staleDocs.length === 0) findings.push(finding(".", 1, "G-stale-identifiers の対象 md が 0 件（母集団の欠落）"));
  if (ctx.staleGuides.length === 0) findings.push(finding(".", 1, "G-stale-identifiers の開発ガイド（docs/**）が 0 件（母集団の欠落）"));
  for (const c of checks) findings.push(...c.run());
  // 計器は検査ではない——面積に合否は無い（`ADR-retire-area-budget`）ので「検査 N 件」に数えない。
  // ただし母集団が欠ければ下の evidence が嘘になるため、入力の健全性だけは findings に残す
  // （空母集団の明示 fail と同じ役割・検査配列の外に置く理由がこれである）。
  findings.push(...checkNormativeAreaInstrument(snapshot));
  const area = normativeArea(snapshot);
  const rules = snapshot.files.filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f)).length;
  const skills = snapshot.files.filter((f) => /^\.claude\/skills\/[^/]+\/SKILL\.md$/.test(f)).length;
  const evidence = `検査 ${checks.length} 件 / 対象文書 ${ctx.docs.length} 件 / rules ${rules} 件 / skills ${skills} 件 / 恒久規範 常時ロード ${area.always} 字・rules ${area.rules} 字 / 見出し参照 ${ctx.headingRefs} 件を md ${ctx.refDocs.length} 件 + .rs ${ctx.refSourceDocs.length} 件から照合 / workspace member ${workspaceMembers(snapshot).members.length} 件の lints opt-in / clippy 禁止 ${clippyDisallowedCount(snapshot)} 件 / 散文の識別子 ${ctx.stale} 件を ${ctx.staleTargets.length} 文書から照合 / 近傍の見出し参照 ${ctx.nearRefs} 件 / ADR ${adrFiles(snapshot).length} 本の名前 / ADR の短縮引用 ${ctx.adrCitations} 件`;
  return { findings, evidence };
}

// fileURLToPath を使う — URL.pathname は空白等を percent-encode するため resolve と一致せず、
// 「検査ゼロ件のまま exit 0」という沈黙経路になる（レビュー H1 で実測）
const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const { findings, evidence } = runAll(makeSnapshot(process.cwd()));
  if (findings.length > 0) {
    console.error(`governance:check — ${findings.length} 件の不整合:`);
    for (const f of findings) console.error(`  ${f.file}:${f.line}  ${f.message}`);
    process.exitCode = 1;
  } else {
    console.log(`governance:check — 全検査 passed（${evidence}）`);
  }
}
