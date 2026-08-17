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
// - 依存ゼロ（Node 標準のみ）・決定的（ネットワーク・時刻・環境変数に非依存）——facade だけでなく
//   `checks/` 配下の各検査・`registry.mjs`・`instrument.mjs` も含む全層が同じ制約を負う
// - findings ゼロ → exit 0 + 照合母集団の件数を印字（根拠の接地）
// - findings あり → exit 1 + `file:line` 付き全件列挙。免除注記の機構は設けない
// - 空母集団（対象文書 0 件・rules 0 件・skills 0 件）は明示 fail（沈黙経路の閉塞）
// - 検査の登録は `scripts/governance/checks/` の走査から導出される（`registry.mjs`）——ファイルを
//   置けばそのまま検査になり、忘れうる登録行が無い。ファイル名と export した `id` の食い違いは
//   `registry.mjs` が throw で拒む（#1088 が問うた「検査が沈黙で 1 本落ちる」構造の解消）
// - `checks/` の外に置いたものは検査ではない——`governance/instrument.mjs`（合否を持たない計器）も
//   `governance/evidence.mjs`（evidence の組み立てと、その入力の読み取りガード・#1098）も
//   登録走査の対象外であり、「検査 N 件」に数えられない。**findings を出すかどうかとは別の軸である**
//   ——計器も evidence も入力の健全性については findings を出す（下の `checkNormativeAreaInstrument` と
//   `evidenceView` がそれで、どちらも検査配列の外に置いてある）
// - facade（本ファイル）が持つのは母集団の算出・0 件検知・evidence への入力の供給・CLI 起動であり、
//   各検査の判定ロジックそのものは `checks/` 側にある
// - 各検査はスナップショット注入の純関数が既定であり、それぞれ隣の `*.test.mjs` が
//   フォールトインジェクション red / 正常 green / 判定対象外の不混入を検証する
//   - **既定の純関数から外れる検査もある。少なくとも次を含み、増えてもこの記述は偽にならない——
//     偽になるのは、ここに名指した検査自身が外れなくなったときである。**
//     (1) G-hook-fires: 判定の再実装を避けるため `.claude/hooks/post-edit.mjs` の
//       `selectChecks` を import し、既定引数として注入する（理由は同検査のコメント）。ゆえに
//       **snapshot の root（cwd）と import 元（スクリプト相対）が同じツリーであること**を前提とする——
//       `npm run governance:check` 経由では常に成り立つが、別ツリーのスクリプトを叩けば崩れる。
//     (2) G-references: `gitIgnoredPaths` が外部の `git` でチェックアウトの gitignore 設定を読む
//       （#1088）。注入するのは `buildChecks` で、**既定引数は何も免除しない**ため純関数としての
//       テストは fixture のまま走る。読む入力の内訳・機体間の乖離の向きは `gitIgnoredPaths` の JSDoc が
//       正本（「依存ゼロ」は npm 依存の話であり、`git` はチェックアウトが在る以上どちらの環境にも在る）
import path from "node:path";
import { fileURLToPath } from "node:url";
import { CHECK_MODULES } from "./governance/registry.mjs";
// evidence 専用の導出は、その検査のファイルから名指しで取る。**登録行と違い、
// ファイルが消えれば import が失敗して鳴る**（沈黙する写しにはならない）。
// **facade から `checks/` を静的 import するのはこの 2 本だけである**（#1094 で他を落とした）。
// 意図的な非対称であり、下の再輸出ブロックの注記がその帰結を持つ。
import { clippyDisallowedCount } from "./governance/checks/G-clippy-disallowed.mjs";
import { adrFiles } from "./governance/checks/G-adr-file-names.mjs";
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
  workspaceMembers,
} from "./governance/lib.mjs";
import { checkNormativeAreaInstrument, normativeArea } from "./governance/instrument.mjs";
import { assembleEvidence, evidenceView } from "./governance/evidence.mjs";

// `lib.mjs` の 2 名を、facade 経由で読む消費者のために再輸出する（`buildChecks` / `runAll` は
// 下で `export function` として定義するのでここに要らない）。**`export *` にしない**——公開する
// 名前を明示的に持つことで、意図しない露出が起きない。
//
// **この一覧が短いことには機構上の役目がある**（#1094）。かつてここは 19 検査の関数を名指しで
// 再輸出しており、その副作用として `checks/` の全ファイルが facade へ静的 import されていた。
// ゆえに検査ファイルが消えると `buildChecks` へ到達する前に `ERR_MODULE_NOT_FOUND` で落ち、
// **#1092 の manifest 差分は消失に対して発火する機会が無かった**。再輸出を実際の消費者まで絞った
// ことで、その遮蔽が外れている。**消費者の一覧をここへ写さない**（増減しても赤くならない写しになる）
// ——母集団は次の grep が持つ（**動的 `import()` は当たらない**。今日の動的消費者は同じファイルが
// 静的 import も持つので取りこぼしは無いが、動的だけの消費者が現れれば母集団の外に居る）:
//   grep -rn 'from ".*governance-check\.mjs"' --include=*.mjs scripts/
// **射程と残余は `governance-manifest.test.mjs` のフォールトインジェクション節が正本**である。
//
// **名前を足す前に、その名前を読む消費者が実在するか確かめること。** `checks/` の関数をここへ
// 戻すと、そのファイルだけ消失の検知が manifest 差分から import エラーへ戻る。
export { makeSnapshot, governanceDocs };

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
  // evidence の入力は view 越しに読む（#1098）。検査が `ctx.record` を呼ばなくなると
  // 値が `undefined` のまま印字され、誰も赤くしないまま exit 0 になっていた（実測）。
  // view を外す形は `assembleEvidence` が brand で throw して拒む。
  // 袋は `...ctx` のスプレッドで組む——必須キーの一覧を手で持つと、それ自体が腐る写しになる。
  // 供給が消えれば読みが `undefined` になり、`evidenceView` が findings へ積む
  //
  // **袋へ入れるのは平坦な値にする**——`area` をそのまま入れると evidence 側の読みが
  // `ev.area.always`＝2 段目になり、2 段目は生のオブジェクトからの読みなのでガードを通らない
  // （`always` を落とすと findings 0 件のまま `undefined` を印字して exit 0 だった・2026-08-17 実測）
  const evidence = assembleEvidence(
    evidenceView(
      {
        ...ctx,
        checkCount: checks.length,
        rules,
        skills,
        areaAlways: area.always,
        areaRules: area.rules,
        workspaceMembers: workspaceMembers(snapshot).members.length,
        clippyDisallowed: clippyDisallowedCount(snapshot),
        adrFiles: adrFiles(snapshot).length,
      },
      findings,
    ),
  );
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
