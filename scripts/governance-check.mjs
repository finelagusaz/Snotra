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
import { skillFiles, modelHiddenSkills, checkSkillTable } from "./governance/checks/G-skill-table.mjs";
import { checkHookCommands } from "./governance/checks/G-hook-commands.mjs";
import { checkHookFires } from "./governance/checks/G-hook-fires.mjs";
import { checkCheckSkillEnumeration } from "./governance/checks/G-check-skill-enumeration.mjs";
import { checkModuleLinkage, declaredModuleFiles } from "./governance/checks/G-module-linkage.mjs";
import { checkReferences } from "./governance/checks/G-references.mjs";
import { checkHeadingRefs, scanHeadingRefs } from "./governance/checks/G-heading-refs.mjs";
import { checkNearHeadingRefs, scanNearHeadingRefs } from "./governance/checks/G-near-heading-refs.mjs";
import { checkStaleIdentifiers, scanStaleIdentifiers, currentVocabulary } from "./governance/checks/G-stale-identifiers.mjs";
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
  STALE_EXTRA_DOCS,
  workspaceMembers,
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
};

// ---------------------------------------------------------------------------
// G-area-instrument — 恒久規範の面積の計器（合否を持たない）。ADR-retire-area-budget。
// **上限判定は廃止した。** 一次規範は「書く約束」（`.claude/rules/governance-docs.md`: かぶりなく・
// 必要なことだけ・古い情報を残さない）であり、数字はその代役になれない。ratchet 期の 3 発火は
// すべて正当な追記に対するもので（#894 の実績調査）、続く火災報知器期は 8 日で両面が +30% 育つ間
// 一度も鳴らなかった——通算 4 観測点で欠陥検出はゼロである。残るのは実測値の報告だけで、
// 推移は `governance:check` の成功行と git 履歴が運ぶ。
//
// **合否を持たない道具でも、母集団の欠落だけは判定する。** 読めない入力の上で出した数字は
// 静かに誤り、判定が無いぶん誰も気づかない（`check:colors` がロック画面を撮る形と同型）。
// ゆえにこの検査が残すのは「計器が入力を読めているか」だけである。
//
// 指標は**文字数（コードポイント・CR 除く）**——行数は「改行を消す」で読む量を減らさず数字だけ
// 下がる（ADR-area-metric-characters に実測）。CR を除くのは CRLF checkout 対策（#587/#589）。
// 常時ロード面には skill の description を含める（毎セッション注入される面）。skills 本文・
// モジュール CLAUDE.md・docs・ADR は対象外——「その作業に入った者だけが読む面」への退去は
// #593 が推奨する経路であり、課税すれば登ってほしい階梯を登る側が罰せられる。
// 二面（常時ロード / rules）を分けて報告するのは、面替えによる片面の肥大が合計では見えないため。
// ---------------------------------------------------------------------------

/** 常時ロードされる恒久規範ファイル（ルート直下の 2 文書。ほかに skill description が同じ面に載る）。
 *  **保証は狭い**——常時ロード面にファイルが増えてもここへ足さなければ、その面積は報告に
 *  一度も算入されない（2026-08-09 実測: 5000 字の文書を新設して `CLAUDE.md` から `@` で読み込ませても、
 *  計上が動いたのは `CLAUDE.md` 側の 1 行分だけ・#1008）。足し忘れを知るのはファイルシステムであって
 *  この検査ではない。 */
export const ALWAYS_LOADED_FILES = ["CLAUDE.md", "AGENTS.md"];

/** コードポイント数（CR は除く）。読めなければ null（母集団欠落を上位で検知） */
function countChars(text) {
  return text == null ? null : [...text.replace(/\r/g, "")].length;
}

/** 指定ファイル群の総文字数。読めないファイルは finding に積み、面積へは算入しない */
function sumChars(snapshot, files, gLabel) {
  let total = 0;
  const findings = [];
  for (const f of files) {
    const c = countChars(snapshot.read(f));
    if (c == null) findings.push(finding(f, 1, `${f} が読めない（${gLabel} 母集団の欠落）`));
    else total += c;
  }
  return { total, findings };
}

/**
 * 毎セッション注入される skill の `description` の総文字数。
 * 複数行スカラー（`|` / `>`）と欠落は数えられないので finding に倒す（沈黙経路の閉塞）。
 * **`disable-model-invocation: true` の skill は除く** — ADR-area-metric-characters が description を常時ロード面へ
 * 算入した根拠は「毎セッション注入されるのに ratchet から見えていない」であり、roster に載らない
 * skill はその前提を満たさない（載らないものを数えれば、実際には注入されていない字に課税する）。
 * `count` は母集団の存在確認用なので、除外前の全 skill 数を返す。
 */
export function skillDescriptionArea(snapshot) {
  const all = skillFiles(snapshot);
  const hidden = modelHiddenSkills(snapshot);
  const files = all.filter((f) => !hidden.has(f.split("/")[2]));
  let total = 0;
  const findings = [];
  for (const f of files) {
    const text = snapshot.read(f);
    if (text == null) {
      findings.push(finding(f, 1, `${f} が読めない（G-area-instrument 母集団の欠落）`));
      continue;
    }
    const m = text.match(/^description:[ \t]*(.*)$/m);
    const v = m ? m[1].trim() : "";
    if (!m || v === "" || v.startsWith("|") || v.startsWith(">")) {
      findings.push(finding(f, 1, "description が 1 行スカラーでない（G-area-instrument が面積を数えられない）"));
      continue;
    }
    total += [...v.replace(/^["']/, "").replace(/["']$/, "")].length;
  }
  return { total, findings, count: all.length };
}

/**
 * 計器の母集団だけを見る（面積の大小は判定しない・ADR-retire-area-budget）。
 * 返す finding はすべて「入力が読めない／空」であり、面積が大きいことは finding にならない。
 */
export function checkNormativeAreaInstrument(snapshot) {
  const findings = [];

  const docs = sumChars(snapshot, ALWAYS_LOADED_FILES, "G-area-instrument");
  const desc = skillDescriptionArea(snapshot);
  findings.push(...docs.findings, ...desc.findings);
  if (desc.count === 0) findings.push(finding(".claude/skills", 1, "skills が 0 件（G-area-instrument 母集団の欠落）"));

  const ruleFiles = snapshot.files.filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f));
  if (ruleFiles.length === 0) {
    findings.push(finding(".claude/rules", 1, "rules が 0 件（G-area-instrument 母集団の欠落）"));
  } else {
    findings.push(...sumChars(snapshot, ruleFiles, "G-area-instrument").findings);
  }
  return findings;
}

/** evidence 用の実測（検査と同じ母集団・同じ数え方であることを型で担保するための共有関数） */
export function normativeArea(snapshot) {
  const always =
    (sumChars(snapshot, ALWAYS_LOADED_FILES, "G-area-instrument").total ?? 0) + skillDescriptionArea(snapshot).total;
  const rules = sumChars(
    snapshot,
    snapshot.files.filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f)),
    "G-area-instrument",
  ).total;
  return { always, rules };
}

// ---------------------------------------------------------------------------
// 実行
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// G-adr-citations — `ADR-<slug>` の短縮引用が実在の ADR を指すか（#812 の A）。
//
// **連番だった頃、この検査は書けなかった。** `ADR-0007` はファイル名の一部でしかなく、
// 引用文字列とファイル名 stem が別物だったためである（`0007-results-presentation-two-stage.md`）。
// `ADR-<slug>.md` へ移して stem = 引用文字列にしたことで、初めて機械照合できるようになった
// ——`docs/adr/ADR-canonical-heading-references.md` が見出し参照に正準形を与えて照合可能にしたのと同じ手。
//
// **母集団はコードコメントを含む。** 製品コードの 5 箇所（`view.rs` ほか）が ADR を短縮名で呼んでおり、
// そこは今日まで検出器を 1 つも持っていなかった。
// **テストファイル（`*.test.mjs`）は母集団外である**——フィクスチャは赤経路を測るために
// 意図的に実在しない名前を持つ（実測: 本検査の初回実行で 5 件すべてが自分のフィクスチャだった）。
// md のコードフェンスを見ないのと同じ理由で、構造的に外す。
// **受容する残余**: `docs/superpowers/` は歴史資料（#589 で非規範化）ゆえ母集団外である。
// 旧番号のパスが残るが、その時点の事実の記録であり、書き換えると当時を偽ることになる。
// ---------------------------------------------------------------------------

/** 短縮引用の形。`ADR-` + kebab slug */
const ADR_CITATION = /\bADR-([a-z][a-z0-9]*(?:-[a-z0-9]+)*)\b/g;

/** G-adr-citations の母集団: ガバナンス文書 + skills + 製品ソース（コメントに引用が在る） */
export function adrCitationDocs(snapshot, docs) {
  return [
    ...docs,
    // 凍結された歴史も**実在の辺だけ**は守る——ADR → ADR の短縮引用は、指す側が凍結でも
    // 指される側の削除で壊れる。`docs`（governanceDocs）は docs/adr/ を含まないため明示的に足す。
    // この 1 行が落ちると ADR→ADR の実在検査が沈黙で消える（母集団カナリアがテストで膜を張る）
    ...snapshot.files.filter((f) => /^docs\/adr\/[^/]+\.md$/.test(f)),
    ...snapshot.files.filter((f) => /^\.claude\/skills\/.*\.md$/.test(f)),
    // 非 docs のソース。**見るのは直下の正規表現が挙げる拡張子だけである**——`.ts` / `.tsx` /
    // `.ps1` / `.psm1` に書いた ADR の短縮引用は実在照合を素通りする（2026-08-09 実測・#1008）。
    ...snapshot.files.filter((f) => /\.(rs|mjs)$/.test(f) && !f.startsWith("docs/") && !f.endsWith(".test.mjs")),
  ];
}

export function scanAdrCitations(snapshot, docs) {
  const findings = [];
  let checked = 0;
  const exists = (slug) => snapshot.files.includes(`docs/adr/ADR-${slug}.md`);
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) continue;
    const isMd = doc.endsWith(".md");
    const lines = isMd ? linesOutsideFences(text) : text.split("\n").map((l, i) => [i + 1, l]);
    for (const [lineNo, line] of lines) {
      for (const m of line.matchAll(ADR_CITATION)) {
        checked += 1;
        if (!exists(m[1])) {
          findings.push(finding(doc, lineNo, `ADR の短縮引用が実在しない: \`${m[0]}\`（docs/adr/ADR-${m[1]}.md が無い）`));
        }
      }
    }
  }
  return { findings, checked };
}

export function checkAdrCitations(snapshot, docs) {
  return scanAdrCitations(snapshot, docs).findings;
}

/** 検査の登録表。**ここが検査 ID の SSOT である**——サマリ行の件数もこの配列から計算するので、
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
  const moved = new Set(CHECK_MODULES.map((m) => m.id));
  const legacy = [
    // 未移送の検査は現行の登録行のまま残す。移送が済んだものはここから消す——
    // **移送の途中でも 19 件が揃うことを、この 2 本の連結が保つ**
    { id: "G-adr-citations", run: () => record("adrCitations", scanAdrCitations(snapshot, adrCitationDocs(snapshot, docs))) },
  ].filter((c) => !moved.has(c.id));
  return [...CHECK_MODULES.map((m) => ({ id: m.id, run: () => m.run(snapshot, ctx) })), ...legacy];
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
