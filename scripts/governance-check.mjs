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
// 越境だが、**唯一の検出器であり移す先が無い**ため意図してここに置く（#1088 で帰属見直しを却下）。
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
  normAnchor,
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

/** 実在検査の対象と見なすソース系拡張子（G-references）。ランタイム生成物（.bin/.bak 等）は含めない。
 *  **保証は狭い**——バッククォート内のパス様参照は、拡張子がここに無ければ（`/` を含んでいても）
 *  静かにスキップされる（2026-08-09 実測: `.psm1` の実在しないパスが素通り・#1008）。 */
const REF_EXTENSIONS = /\.(md|rs|ts|tsx|mjs|json|toml|yml|ps1|html|css)$/;

// ---------------------------------------------------------------------------
// G-references — ガバナンス文書群の参照実在（Markdown リンク + バッククォート内パス様参照）。
// バッククォート参照の検査述語（受容する偽陰性はスクリプトコメントとテストで固定）:
//   `/` を含む・glob（* ? {）なし・<> なし・% なし・URL なし・拡張子が REF_EXTENSIONS・
//   workspace/ 配下でない・`\` を含まない。
//   → ベア名（`SPEC.md` 等）とランタイム生成物（`config.toml`・`*.bin`・`*.bak`）は構造的に対象外。
// ---------------------------------------------------------------------------

export function checkReferences(snapshot, docs, filterIgnored = () => new Set()) {
  const findings = [];
  const fileSet = new Set(snapshot.files);
  // 実在判定（exists）と ignore 照合（下の candidates）が同じ「文書ディレクトリ基準の正規化」を
  // 使う——exists はここを呼ぶ（独立した式を 2 つ持つと、片方だけ変えたときに実在判定と
  // 免除照合がずれ、偽の赤か偽の緑になる）。
  const docRelative = (doc, ref) => path.posix.normalize(path.posix.join(path.posix.dirname(doc), ref));
  const exists = (doc, ref, { allowSuffix = false } = {}) => {
    const norm = (p) => path.posix.normalize(p);
    if (fileSet.has(norm(ref))) return true; // リポジトリルート基準
    if (fileSet.has(docRelative(doc, ref))) return true; // 文書ディレクトリ基準
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

// ---------------------------------------------------------------------------
// G-spec-sections — SPEC.md 番号連続性 + SPEC 前置の §N(.x) 参照の実在（旧 Check 4 + #587 新規）。
// 裸の `§N` は各文書自身の節参照でありうるため対象外（不混入はテストで固定）。
// ---------------------------------------------------------------------------
export function checkSpecSections(snapshot, docs) {
  const findings = [];
  const spec = snapshot.read("SPEC.md");
  if (spec == null) return [finding("SPEC.md", 1, "SPEC.md が読めない")];
  const sections = new Set();
  let prevTop = null;
  let prevSub = null;
  for (const [lineNo, line] of linesOutsideFences(spec)) {
    const top = line.match(/^## (\d+)\. /);
    if (top) {
      const n = Number(top[1]);
      if (prevTop != null && n !== prevTop + 1) {
        findings.push(finding("SPEC.md", lineNo, `セクション番号が連続しない: ## ${prevTop}. の次が ## ${n}.`));
      }
      prevTop = n;
      prevSub = 0;
      sections.add(`${n}`);
      continue;
    }
    const sub = line.match(/^### (\d+)\.(\d+) /);
    if (sub) {
      const [n, x] = [Number(sub[1]), Number(sub[2])];
      if (n !== prevTop) {
        findings.push(finding("SPEC.md", lineNo, `子セクション ### ${n}.${x} が親 ## ${prevTop}. と不一致`));
      } else if (x !== prevSub + 1) {
        findings.push(finding("SPEC.md", lineNo, `子セクション番号が連続しない: ${n}.${prevSub} の次が ${n}.${x}`));
      }
      prevSub = x;
      sections.add(`${n}.${x}`);
    }
  }
  if (sections.size === 0) findings.push(finding("SPEC.md", 1, "セクション見出し（## N.）が 1 件も無い（G-spec-sections 母集団の欠落）"));
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) continue; // 母集団欠落は G-references が報告する
    for (const [lineNo, line] of linesOutsideFences(text)) {
      for (const m of line.matchAll(/SPEC(?:\.md)?`?(?: の)? ?§(\d+(?:\.\d+)?)/g)) {
        if (!sections.has(m[1])) {
          findings.push(finding(doc, lineNo, `SPEC §${m[1]} が SPEC.md に実在しない`));
        }
      }
    }
  }
  return findings;
}

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
// G-heading-refs — 見出し参照の実在（正準形 `<対象>`「<見出し>」）。
// 参照に構文を与えて機械照合可能にする。これが `.claude/rules/governance-docs.md` の
// 「改変前に参照側を名前と序数で数え上げる」手作業を置き換える機構である。
// アンカーは ATX 見出し・番号付きリスト項目・太字リードの 3 種（この repo の参照実態に合わせた。
// 節ではなく箇条書きのリード文を指す参照が実在する: `src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」）。
// 照合は正規化（`**`・バッククォート・「」・空白の除去）後の**前方一致**——見出しが後置の
// 括弧注記（「…の不変条件（#532 SU5）」「条件別チェック（トリガー → 参照先）」）を持つため。
// 受容する偽陰性: 「ルート `CLAUDE.md` のフック節」のような散文形の参照は見ない。
// 検査されるのは正準形に書かれたものだけであり、**この検査は規範の完全な代替ではない**。
// ---------------------------------------------------------------------------

/** 見出し参照の正準形。対象は `<path>.md` か `/skill-name`。
 *  `§` には節番号を伴ってよい（`SPEC.md` §11「見た目の規範」）——番号を許さないと、
 *  節番号つきの参照は正準形へ直しても照合されず、G-near-heading-refs が「直せない指摘」を出し続ける（#727 で実測）。 */
const HEADING_REF = /`([^`\n]+)`\s*(?:§\s*[\d.]*\s*)?「([^「」\n]+)」/g;

/** findings に加えて照合件数を返す（「差分ゼロ」と「照合していない」を区別する証跡・#497） */
export function scanHeadingRefs(snapshot, docs) {
  const findings = [];
  let checked = 0;
  const anchorCache = new Map();
  const anchorsOf = (p) => {
    if (!anchorCache.has(p)) {
      const t = snapshot.read(p);
      anchorCache.set(p, t == null ? null : collectAnchors(t).map(normAnchor));
    }
    return anchorCache.get(p);
  };
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) {
      findings.push(finding(doc, 1, "対象文書が読めない（G-heading-refs 母集団の欠落）"));
      continue;
    }
    for (const [lineNo, line] of linesOutsideFences(text)) {
      for (const m of line.matchAll(HEADING_REF)) {
        const [, target, label] = m;
        if (!target.endsWith(".md") && !/^\/[a-z0-9-]+$/.test(target)) continue;
        checked += 1;
        const p = resolveRefTarget(snapshot, doc, target);
        if (p == null) {
          findings.push(finding(doc, lineNo, `見出し参照の対象が解決できない: \`${target}\`「${label}」`));
          continue;
        }
        const anchors = anchorsOf(p);
        if (anchors == null) {
          findings.push(finding(doc, lineNo, `見出し参照の対象が読めない: ${p}`));
          continue;
        }
        if (!anchors.some((a) => a.startsWith(normAnchor(label)))) {
          findings.push(
            finding(doc, lineNo, `見出し参照が着地しない: \`${target}\`「${label}」（${p} に該当する見出し・リード文が無い）`),
          );
        }
      }
    }
  }
  return { findings, checked };
}

export function checkHeadingRefs(snapshot, docs) {
  return scanHeadingRefs(snapshot, docs).findings;
}

// ---------------------------------------------------------------------------
// G-near-heading-refs — 正準形に見えて隣接していない見出し参照（#727）。
//
// G-heading-refs が見るのはバッククォートを閉じた直後（`§` と空白のみを挟む）に `「` が続く形だけで、
// **助詞が 1 つ挟まると検査対象から外れる**。人の目には同じ参照に見える:
//   `/start-issue`「Step 6 — …」      ← G-heading-refs が見る
//   `/start-issue` は「Step 6 — …」   ← 見ない
// #725 では Claude 自身が書いた 3 件がこの形で、しかも `/implement` の入口判定の中核推論を
// 支えていた（`/start-issue` が改番されれば黙って壊れる）。
//
// **判定の要は窓幅ではなく「引用が実際に着地するか」である。** 近傍に `「…」` があるだけでは
// 参照と散文の引用（「`SPEC.md`（…）は「何を実現すべきか」を記す」）を分けられない。実測:
//
// | 窓幅 | 着地する（＝正準形へ直せる真の参照） | 着地しない（＝散文の引用） |
// |---|---|---|
// | 2 | 5 | 8 |
// | 4 | 7 | 12 |
// | 8 | **8** | 28 |
// | 12 | 8 | 34 |
//
// 着地条件を課すと、窓を広げても真の参照は 8 件で頭打ちになり、増えるのは無視する側だけである。
// ゆえに**窓幅 8・着地必須**とした。この形なら誤爆の代償は「散文の引用がたまたま見出しと同名」
// に限られ、そのときは正準形へ直すのが正しい（G-heading-refs の保護下へ入る）。
//
// **受容する残余**: 着地しない非隣接参照は見ない。腐った参照（消滅した節を指す散文形）は
// この検査では捕まらない——歴史記述と区別できないためである（`.claude/rules/governance-docs.md`
// 「既に消滅した節の名前を正準形で書かない」が規範として担う）。
// ---------------------------------------------------------------------------

/** 閉じバッククォートから `「` までに挟まってよい最大文字数（実測で頭打ちになる値） */
const NEAR_REF_GAP = 8;
/** 非隣接の近傍参照。gap は最短一致で取る */
const NEAR_REF = new RegExp("`([^`\\n]+)`([^`\\n]{1," + NEAR_REF_GAP + "}?)「([^「」\\n]+)」", "g");
/** G-heading-refs が既に見ている隣接形（`§` + 節番号と空白のみを挟む）。HEADING_REF と同じ前提を持つ */
const ADJACENT_REF = /`[^`\n]+`\s*(?:§\s*[\d.]*\s*)?「/;

export function scanNearHeadingRefs(snapshot, docs) {
  const findings = [];
  let checked = 0;
  const anchorCache = new Map();
  const anchorsOf = (p) => {
    if (!anchorCache.has(p)) {
      const t = snapshot.read(p);
      anchorCache.set(p, t == null ? null : collectAnchors(t).map(normAnchor));
    }
    return anchorCache.get(p);
  };
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) continue; // 読めない文書は G-heading-refs が母集団の欠落として報告済み
    for (const [lineNo, line] of linesOutsideFences(text)) {
      for (const m of line.matchAll(NEAR_REF)) {
        const [, target, gap, label] = m;
        if (!target.endsWith(".md") && !/^\/[a-z0-9-]+$/.test(target)) continue;
        if (ADJACENT_REF.test(m[0])) continue;
        const p = resolveRefTarget(snapshot, doc, target);
        if (p == null) continue;
        const anchors = anchorsOf(p);
        if (anchors == null) continue;
        checked += 1;
        if (anchors.some((a) => a.startsWith(normAnchor(label)))) {
          // 節番号は正準形が許すので、直し方の提示から落とさない
          const section = (gap.match(/§\s*[\d.]+/) ?? [""])[0];
          const canonical = `\`${target}\`${section ? ` ${section}` : ""}「${label}」`;
          findings.push(
            finding(doc, lineNo, `見出し参照が正準形でない（G-heading-refs の視界外）: \`${target}\`【${gap}】「${label}」— ${canonical}と書く`),
          );
        }
      }
    }
  }
  return { findings, checked };
}

export function checkNearHeadingRefs(snapshot, docs) {
  return scanNearHeadingRefs(snapshot, docs).findings;
}

// ---------------------------------------------------------------------------
// 実行
// ---------------------------------------------------------------------------

/** Rust のコメントを落とす。落とさないと `preset` のような普通の英単語が doc コメントに埋もれる（実測）。
 *  文字列リテラル内の `//` 以降も落ちるが、向きは赤側（読みが消える）ゆえ沈黙しない */
function stripRustComments(src) {
  return src.replace(/\/\*[\s\S]*?\*\//g, " ").replace(/\/\/.*$/gm, " ");
}

/** `#[cfg(test)]` 以降を落とす。**母集団と読み手の両方に適用する**——読み手側で落とさないと
 *  「テストだけが読む」フィールドが読まれている側へ落ちる（`visible_rows` で実測） */
function productionOnly(src) {
  return src.split(/^#\[cfg\(test\)\]/m)[0];
}

// ---------------------------------------------------------------------------
// G-stale-identifiers — 規範の散文に残る、現行語彙に無い識別子（腐り）の検出（#736 の同クラス）。
//
// #698 が述べた「述語だけが書かれた間接参照」は概念での再導出でしか拾えなかったが、
// **識別子として書かれた腐りは機械で拾える**。G-references が見るのはパスの実在までで、
// 識別子の実在は誰も見ていなかった。
//
// **自称スコープ**（#891 で広げた。射程の内訳は `ADR-stale-identifier-detector-scope` の追記節）。
// 見るのは次の 3 群の中の**バッククォート内 camelCase / SCREAMING_SNAKE / lowercase snake_case
// 識別子**だけである（型で修飾した形は末尾セグメントを見る・#993。判定の正本は `staleTarget`）:
// - `.claude/**` の規範の散文（`staleIdentifierDocs`）
// - **開発ガイド `docs/**`**（`staleIdentifierGuideDocs`。設計原則・ビルド手順・フック契約・
//   アーキ説明という性質の違うものが混在する）から**歴史記録 2 種を除いたもの**
// - 固定パスの `STALE_EXTRA_DOCS`（意図の SSOT・常時ロードの規範・設定 UI のデザイン規約）
//
// **母集団から外す基準は「日付を持つか」ではなく「もう成り立たないことを書く場所か」である。**
// `docs/adr/` は却下案（＝もう存在しない案）、`docs/superpowers/` は #589 で非規範化された当時の設計。
// 一方 `docs/design/` は日付スラグを持つが `status: Agreed` で `docs/architecture.md` が現在形で
// 指す先ゆえ**含める**——外すと G-references が守るポインタの**指し先だけが黙って腐る**。
// `docs/adr/` の除外はかつてこの検査だけの非対称だったが、`ADR-adr-frozen-history` で
// 全検査へ揃った——**凍結された歴史は語彙も供給せず、精度の照合もされない**（残るのは実在の辺のみ）。
// 実測でも `docs/adr/` を検査対象に入れると finding の 8 割が ADR 自身の却下記録で、
// **この検出器の ADR がこの検出器を赤にする**。
// **モジュール `CLAUDE.md` は入れない**——ラップ対象の外部 API（Win32 / tao / TTC）を語る場所ゆえ
// 外部語彙の**密度**が高い（実測 真の腐り 1 : 外部語彙 3。`WM_SETCURSOR` 等は語彙源をどう広げても免罪できない）。
//
// **述語の外に在るもの**は依然として多い。frontmatter の文字列・素の表テキスト・
// 日本語散文（「リアクティブ制約」等）は構造的に対象外で、#736 が挙げた 10 件のうちこの述語が
// 届くのは 0 件である（実測）。PascalCase・ドット区切り・式で書かれた腐りも述語の外にある
// ——**修飾形で見るのは末尾セグメントだけ**なので、型が改名されメンバ名が残った形も鳴らない。
// **「文書の腐りが機構で捕まる」とは言えない**——言えるのは
// **「`.md` の散文に camelCase / SCREAMING_SNAKE / lowercase snake_case で書かれた再発は捕まる
// （型で修飾されていてもよい）」**までである。**`.rs` の doc コメントは母集団外**ゆえ、そこに書かれた腐りは捕まらない
// （#975 で `.rs` を足す案を測って却下した。理由は外部 API の密度・`ADR-stale-identifier-detector-scope`
// 「その後（#975・述語へ lowercase snake_case を足し、`.rs` への母集団拡大は却下した）」）。
// **この検査は #736 の代替ではない**——同 issue は手作業で閉じ、G-stale-identifiers が引き受けるのは再発防止だけである。
//
// 判定: 識別子が「現行語彙」に 1 度も現れないなら finding。**現行語彙の正本は
// 「production のソースの非コメント本文」ただ 1 つである**（`stripRustComments` + `*.test.*` の除外）。
// この母集団を狭める 2 つは、どちらも同じ 1 つの原則から出ている——
// **語彙を寄付してよいのは「現に動いている実装」だけである**:
// - **コメントを外す**。含めると `resetForShow` のような由来注記（「〜相当」「parity」）が
//   語彙に化け、腐りが原理的に検出できない（実測 11 件）
// - **テストコードを外す**。含めると検出器自身のフィクスチャが偽陰性を作る——
//   `createObjectURL`（本検査が守りたい対象として `governance-check.test.mjs` に名指しで書いた語）が、
//   同ファイルに書かれているという理由だけで実リポジトリでは永久に検出されなかった（実測）
//
// **`SPEC.md` は語彙源ではなく検査対象である**（`ADR-stale-identifier-detector-scope`
// 「却下 4: 現行語彙をソースだけから作る（`SPEC.md` を入れない）」の失効注記が経緯を持つ）。
// 語彙に置いていた頃は、SPEC 内の候補が**自分自身に一致して自分を免罪していた**。
// 語彙から外すと同時に検査対象へ入れることで、SSOT と写しが**同時に鳴る**——
// 「どちらを先に直すか」という向きの問いが構造的に消える。
//
// **外部ツールの語彙は空白の規則が外す**: コマンドを書いた span（`gh pr view <PR> --json
// closingIssuesReferences` 等）は空白を含むので、そもそも判定対象にならない。かつては行に
// コマンドが在れば**その行ごと**捨てていたが、#993 で撤去した——日本語の長い段落にコマンドを
// 1 つ書いただけで段落の識別子が全滅する**沈黙経路**であり、しかも空白の規則と役目が重複していた
// （実測: コマンド span 153 件のすべてが空白を含み、行の述語を置く／置かないで結果が完全一致）。
// #984 の腐りを隠していたのはこの経路である。
//
// **受容する残余**:
// - 単語 1 つの識別子（`Glob` `expand` `plain`）は対象外である。こぶを 1 つ以上要求しないと、
//   harness のツール名と散文の語彙が大量に混じる（実測 53 件中 40 件弱）。SCREAMING_SNAKE 側が
//   `_` を 1 つ以上要求するのも同じ構造である（`CI` `TODO` `README` は対象外）
// - **`.yml` は GitHub 提供の語彙を寄付する**（`GITHUB_ENV` / `GITHUB_OUTPUT` / `GITHUB_TOKEN` /
//   `TAG_NAME` / `TAURI_SIGNING_PRIVATE_KEY`）ほか、`'` で分断された日付書式の断片
//   （`ddTHH` `ssZ` `yyyyMMddHHmm`）も語彙に化ける。同名の識別子が散文に書かれれば誤って免罪する（今日 0 件）
// - **Rust のテストコードは今も語彙を寄付しうる。** `VOCAB_TEST_FILE` が当たるのはファイル名の
//   `.test.<ext>` という形だけで、Rust 側の 3 つの形——`#[cfg(test)] mod` の中身・
//   `<crate>/tests/*.rs` の統合テスト・`src/**/tests/*.rs` へ分けたテストファイル——はどれも外れる。
//   `productionOnly` を通しても落ちるのは 1 つ目だけである。現時点でこの穴に落ちた finding は
//   1 件も無く（測定の全セルで 0 件）、測って動かなかったものを入れないだけの理由で開けてある
// - **`.json` は語彙源ではない**（`VOCAB_SOURCE_EXT`）。設定キーが JSON にしか無い語は偽陽性になりうる
//   ——`docs/hooks.md` の `${CLAUDE_PROJECT_DIR}` はこの残余を避けて**文書側の記述を正確化**して外した。
//   `.json` を入れれば免罪できるが、生成物（`src-tauri/gen/schemas/`）・依存メタデータ
//   （`package-lock.json` の integrity 断片）・gitignore 済みで CI に存在しないファイルを同時に招き、
//   **除外リスト無しには分離できない**（ファイル冒頭の「免除注記の機構を設けない」契約に当たる）
// - **テストコードにしか無い識別子も偽陽性になりうる**——上の「テストコードを外す」の裏返しで、
//   語彙源を狭めた側が新しく作った残余である（今日 0 件）
// ---------------------------------------------------------------------------

/** 現行語彙の正本になるソース拡張子。`.yml` が入るのは `.github/workflows/**` が
 *  **追跡され・人が書き・CI が実際に実行する**＝「現に動いている実装」だからである
 *  （`.yaml` はリポジトリに 1 本も無い）。**`.json` は入れない**——生成物・依存メタデータ・
 *  gitignore 済みファイルを同時に招き入れ、除外リスト無しには分離できない
 *  （`ADR-stale-identifier-detector-scope` の追記節） */
const VOCAB_SOURCE_EXT = /\.(rs|ts|tsx|mjs|ps1|toml|yml)$/;
/** 語彙源から外すテストコード。見るのは `.test.<ext>` という**ファイル名の形**だけで、
 *  拡張子は `VOCAB_SOURCE_EXT` の **JS/TS 系だけ**を採る（`rs|ps1|toml` は含まない——
 *  Rust 側の穴は上の「受容する残余」） */
const VOCAB_TEST_FILE = /\.test\.(mjs|ts|tsx)$/;
/** バッククォート内で腐りを問う形: camelCase（こぶ 1 つ以上）・末尾 `()` は任意 */
const STALE_IDENT = /^([a-z][a-z0-9]*(?:[A-Z][a-z0-9]*)+)(\(\))?$/;
/** 同じく SCREAMING_SNAKE（`_` 1 つ以上）。camelCase 側が「こぶを 1 つ以上要求する」のと同じ構造で、
 *  単語 1 つの識別子を受容する残余から外さない。**2 述語は先頭文字で相互排他**ゆえ、
 *  どちらが当たっても `scanStaleIdentifiers` の照合件数は 1 しか進まない */
const STALE_SNAKE_IDENT = /^([A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+)(\(\))?$/;
/** 同じく lowercase snake_case（`_` 1 つ以上・#975）。**このリポジトリの主要言語の語彙はここに居る**
 *  ——Rust の関数名・テスト名・フィールド名はすべて lowercase snake_case であり、上の 2 述語は
 *  そのどれにも当たらなかった（`index_tree.rs` の doc が存在しないテスト名を引いたまま素通りした）。
 *  **3 述語は先頭文字と字種で相互排他である**（camelCase は `_` を含まず、SCREAMING は先頭が大文字）
 *  ゆえ、どれが当たっても `scanStaleIdentifiers` の照合件数は 1 しか進まない */
const STALE_LOWER_SNAKE_IDENT = /^([a-z][a-z0-9]*(?:_[a-z0-9]+)+)(\(\))?$/;
/** 判定対象の識別子を取り出す。無ければ `null`。
 *  **修飾形（`::`）は末尾セグメントだけを見る**——型セグメント（PascalCase）を見ない理由は
 *  「単語 1 つの識別子は対象外」と同じで、外部の型名は語彙源をどう広げても免罪できない
 *  （`ADR-stale-identifier-detector-scope` の #993 の追記節に測定表がある）。
 *  **`.` の除外はトークン全体ではなくセグメントへ当てる**——先に当てると
 *  `icon.rs::encode_batch_binary` の形が素通りする（実測で唯一の真の腐りがこの形だった）。
 *  **捕獲群を読まない**のは 3 述語と同じ理由である（`scanStaleIdentifiers` のコメント）。 */
function staleTarget(raw) {
  const bare = raw.replace(/\(\)$/, "");
  const seg = bare.includes("::") ? bare.slice(bare.lastIndexOf("::") + 2) : bare;
  if (seg.includes(".")) return null;
  if (!STALE_IDENT.test(seg) && !STALE_SNAKE_IDENT.test(seg) && !STALE_LOWER_SNAKE_IDENT.test(seg)) return null;
  return seg.replace(/\(\)$/, "");
}

/** 現行語彙。production のソースだけを集め、コメントを落とす
 *  （`#` コメントの言語は行頭・行中とも落とす） */
export function currentVocabulary(snapshot) {
  const parts = [];
  for (const f of snapshot.files) {
    if (!VOCAB_SOURCE_EXT.test(f) || VOCAB_TEST_FILE.test(f)) continue;
    const src = snapshot.read(f);
    if (src == null) continue;
    // コメント除去の振り分け。**`VOCAB_SOURCE_EXT` へ `#` コメントの言語を足したら、この正規表現へも
    // 同時に足すこと**——足し忘れるとその言語のコメントが生のまま語彙へ入り、由来注記に書かれた
    // 識別子が「現行語彙」に化ける。その識別子が文書で腐っていても免罪されて検出されない
    // （2026-08-09 実測: `.psm1` のコメント語が実際に赤を緑へ変えた・#1008。上の「受容する残余」が
    // 記録する失敗形の再演）。この対応を強制する機構は無い。
    parts.push(/\.(ps1|toml|yml)$/.test(f) ? src.replace(/#.*$/gm, " ") : stripRustComments(src));
  }
  return parts.join("\n");
}

/** findings に加えて照合件数を返す（「腐りゼロ」と「照合していない」を区別する証跡・#497） */
export function scanStaleIdentifiers(snapshot, docs) {
  const findings = [];
  let checked = 0;
  const vocab = currentVocabulary(snapshot);
  const seen = new Map();
  const inVocab = (id) => {
    if (!seen.has(id)) seen.set(id, new RegExp(`\\b${id}\\b`).test(vocab));
    return seen.get(id);
  };
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) {
      findings.push(finding(doc, 1, "対象文書が読めない（G-stale-identifiers 母集団の欠落）"));
      continue;
    }
    for (const [lineNo, line] of linesOutsideFences(text)) {
      for (const m of line.matchAll(/`([^`\n]+)`/g)) {
        const raw = m[1];
        if (raw.includes("/") || raw.includes(" ")) continue;
        // **捕獲群を読まない**——`staleTarget` は `test` で当てて `()` は自分で落とす。マッチ結果の
        // `[1]` を読む形だと、2 述語を `|` で 1 本へ畳んだ瞬間に群がずれて `inVocab(undefined)` になり、
        // しかも実語彙は `undefined` を含むので**赤が出ないまま沈黙する**（複製への変異で実測）。
        // 読まなければ畳もうが分けようが結果が変わらず、「畳むな」という文書契約自体が要らなくなる
        const target = staleTarget(raw);
        if (target == null) continue;
        checked += 1;
        if (!inVocab(target)) {
          findings.push(
            finding(doc, lineNo, `散文に、現行語彙に無い識別子が残っている: \`${raw}\`（production のソースの非コメント本文に無い）`),
          );
        }
      }
    }
  }
  return { findings, checked };
}

export function checkStaleIdentifiers(snapshot, docs) {
  return scanStaleIdentifiers(snapshot, docs).findings;
}

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
    { id: "G-references", run: () => checkReferences(snapshot, docs, gitIgnoredPaths) },
    { id: "G-spec-sections", run: () => checkSpecSections(snapshot, docs) },
    { id: "G-adr-citations", run: () => record("adrCitations", scanAdrCitations(snapshot, adrCitationDocs(snapshot, docs))) },
    { id: "G-heading-refs", run: () => record("headingRefs", scanHeadingRefs(snapshot, allRefDocs)) },
    { id: "G-stale-identifiers", run: () => record("stale", scanStaleIdentifiers(snapshot, staleTargets)) },
    { id: "G-near-heading-refs", run: () => record("nearRefs", scanNearHeadingRefs(snapshot, allRefDocs)) },
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
