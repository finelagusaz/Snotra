// governance:check — ガバナンス文書の決定的検査（#587）。
// shebang を置かない — CI の Windows checkout（autocrlf=true）で CRLF 化された
// shebang 行は vitest の transform を SyntaxError で落とす（PR #592 で実測。
// 他の *.mjs も同じ理由で shebang なし。起動は常に `node scripts/...` 経由）。
//
// 文書の決定的に照合できる項目を PR CI（governance-check job）と `npm run governance:check` で見る。意味判断（責務の妥当性・メモリ整合）は
// `/health-check` に残る。
// なお `G-workspace-lints` / `G-clippy-disallowed` は文書ではなくリポジトリ規約を見る。責務としては
// 越境だが意図的な選択であり、帰属の作り直し（他の責務分担への割り当て直し）は #1088 で却下された。
//
// 契約:
// - 依存ゼロ（Node 標準のみ）・決定的（ネットワーク・時刻・環境変数に非依存）——facade だけでなく
//   `checks/` 配下の各検査・`registry.mjs` も含む全層が同じ制約を負う
// - findings ゼロ → exit 0 + 検査の本数を印字
// - findings あり → exit 1 + `file:line` 付き全件列挙。免除注記の機構は設けない
// - 空母集団（対象文書 0 件・rules 0 件・skills 0 件）は明示 fail（沈黙経路の閉塞）
// - 検査の登録は `scripts/governance/checks/` の走査から導出される（`registry.mjs`）——ファイルを
//   置けばそのまま検査になり、忘れうる登録行が無い。ファイル名と export した `id` の食い違いは
//   `registry.mjs` が throw で拒む（#1088 が問うた「検査が沈黙で 1 本落ちる」構造の解消）
// - 各検査は自分が読む母集団を自分で導く。宣言（ドメイン）も、その縮みを見張る層も持たない
//   ——錨の層ごと撤去した経緯は `ADR-governance-anchor-layer-discarded`
// - facade（本ファイル）が持つのは母集団の算出・0 件検知・CLI 起動であり、
//   各検査の判定ロジックそのものは `checks/` 側にある
// - 各検査はスナップショット注入の純関数が既定であり、それぞれ隣の `*.test.mjs` が
//   フォールトインジェクション red / 正常 green / 判定対象外の不混入を検証する
//   - **既定の純関数から外れる検査もある。少なくとも次を含み、増えてもこの記述は偽にならない——
//     偽になるのは、ここに名指した検査自身が外れなくなったときである。**
//     G-references: `gitIgnoredPaths` が外部の `git` でチェックアウトの gitignore 設定を読む
//       （#1088）。注入するのは `buildChecks` で、**既定引数は何も免除しない**ため純関数としての
//       テストは fixture のまま走る。読む入力の内訳・機体間の乖離の向きは `gitIgnoredPaths` の JSDoc が
//       正本（「依存ゼロ」は npm 依存の話であり、`git` はチェックアウトが在る以上どちらの環境にも在る）
import path from "node:path";
import { fileURLToPath } from "node:url";
import { CHECK_MODULES } from "./governance/registry.mjs";
import {
  makeSnapshot,
  finding,
  gitIgnoredPaths,
  governanceDocs,
  allHeadingRefDocs,
  headingRefCommentDocs,
  headingRefDocs,
  headingRefSourceDocs,
  staleIdentifierDocs,
  staleIdentifierGuideDocs,
  staleIdentifierTargets,
} from "./governance/lib.mjs";

// `lib.mjs` の 2 名を、facade 経由で読むテストのために再輸出する。
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
 *  確定させるため、並行する 2 本の PR が同じ値を見る（`.claude/rules/governance-docs.md`「序数で他を指してはならない」）。 */
export function buildChecks(snapshot, sink = {}) {
  const docs = governanceDocs(snapshot);
  const refDocs = headingRefDocs(snapshot);
  const refSourceDocs = headingRefSourceDocs(snapshot);
  const refCommentDocs = headingRefCommentDocs(snapshot);
  // 3 つの腕は検査へ渡すときだけ束ねる。母集団としては別々に持つ——`runAll` の 0 件検知が
  // 腕ごとに 1 本ずつ要るためである（束ねた長さは他の腕の消滅を隠す）。
  const allRefDocs = allHeadingRefDocs(snapshot);
  const staleDocs = staleIdentifierDocs(snapshot);
  const staleGuides = staleIdentifierGuideDocs(snapshot);
  const staleTargets = staleIdentifierTargets(snapshot);
  sink.docs = docs;
  sink.refDocs = refDocs;
  sink.refSourceDocs = refSourceDocs;
  sink.refCommentDocs = refCommentDocs;
  sink.staleDocs = staleDocs;
  sink.staleGuides = staleGuides;
  sink.staleTargets = staleTargets;
  const ctx = { docs, allRefDocs, staleTargets, gitIgnoredPaths };
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
  if (ctx.refCommentDocs.length === 0) findings.push(finding(".", 1, "G-heading-refs の対象スクリプト（コメント記法を持つファイル）が 0 件（母集団の欠落）"));
  // `staleTargets` ではなく `staleDocs` を見る——`STALE_EXTRA_DOCS` が常に長さを埋めるため、
  // targets 側で判定すると `.claude/**` が 1 枚残らず消えてもこの検知が沈黙する。
  // **グロブ由来の母集団ごとに 1 本ずつ要る**——束ねると片方が埋めた長さで他方の消滅が隠れる。
  // 固定パスの `STALE_EXTRA_DOCS` はここに要らない（読めなければ scanStaleIdentifiers が鳴る）
  if (ctx.staleDocs.length === 0) findings.push(finding(".", 1, "G-stale-identifiers の対象 md が 0 件（母集団の欠落）"));
  if (ctx.staleGuides.length === 0) findings.push(finding(".", 1, "G-stale-identifiers の開発ガイド（docs/**）が 0 件（母集団の欠落）"));
  for (const c of checks) findings.push(...c.run());
  return { findings, checkCount: checks.length };
}

// fileURLToPath を使う — URL.pathname は空白等を percent-encode するため resolve と一致せず、
// 「検査ゼロ件のまま exit 0」という沈黙経路になる（レビュー H1 で実測）
const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const { findings, checkCount } = runAll(makeSnapshot(process.cwd()));
  if (findings.length > 0) {
    console.error(`governance:check — ${findings.length} 件の不整合:`);
    for (const f of findings) console.error(`  ${f.file}:${f.line}  ${f.message}`);
    process.exitCode = 1;
  } else {
    console.log(`governance:check — 全検査 passed（検査 ${checkCount} 件）`);
  }
}
