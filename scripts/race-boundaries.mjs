// race:boundaries — `/race-check`「母集団」を決定的に取る（#805・#593。#894 の圧縮以降、
// 種別 ①〜⑧ の定義・検出パターン・出力仕様の SSOT は本ファイルである）。
// shebang を置かない — CI の Windows checkout（autocrlf=true）で CRLF 化された shebang 行は
// vitest の transform を SyntaxError で落とす（PR #592 で実測。起動は常に `node scripts/...` 経由）。
//
// なぜスクリプトなのか: `docs/check-skill-skeleton-design.md`「必須 1 — 母集団」の要石は
// 「母集団を得る手順が決定的でないなら、その検査は結論を持たない」である。種別 ①〜⑧ の手がかりは
// 既に確定した正規表現であり、決定的な手順とは定義上スクリプトで書けるものを指す。
//
// 契約:
// - 依存ゼロ（Node 標準のみ）・決定的（ネットワーク・時刻に非依存）
// - `--base` は必須。省略は使用法 + exit 2（リビジョン未固定が同型欠陥の本体・設計書「背景」）
// - 変更されたファイルと未追跡のソースがともに無ければ exit 3（沈黙経路の閉塞。「両方とも空なら
//   『変更が無い』ではなく『基準が誤っている』と読む」を機構化したもの）。
//   **merge-base が HEAD と一致すること自体は異常ではない**——この検査は `/implement`「3a. 委譲の前に」から
//   コミットより前に呼ばれ、初コミット前の feature ブランチでは常にそうなる（実測）
// - patch のファイル名が変更ファイルの一覧と食い違えば exit 3（git の設定が出力の形を変えて行を黙って落とす形の閉塞・`readTrackedRows`）
// - **出力は常に ①〜⑧ の全種**。0 件の種別も件数とパターンを印字する——これで
//   「1 件見つけたら残り 7 種を素通り」という経路が構造的に消える（#800）
// - 分類本体は入力注入の純関数（scripts/race-boundaries.test.mjs がフィクスチャで検証する）
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

/** 種別表（序数・名称・パターンの SSOT。#894 以前は SKILL.md 側に写しの表があった）。
 *  **序数 ①〜⑧ は書き換えない** — `docs/adr/ADR-race-check-predicate-and-norm-hardening.md` `docs/adr/ADR-check-skill-skeleton.md`
 *  `docs/check-skill-skeleton-design.md` から参照され、うち 2 つは status: Accepted の ADR である
 *  （`docs/adr/ADR-race-check-simplification.md` 却下 4）。 */
export const KINDS = [
  { ord: "①", name: "worker の spawn", patterns: ["thread::spawn", "async_runtime::spawn"] },
  { ord: "②", name: "channel への send", patterns: ["\\.send\\(", "Sender", "channel\\("] },
  { ord: "③", name: "フレームでの drain", patterns: ["try_recv", "Receiver"] },
  { ord: "④", name: "managed state の読み書き", patterns: ["try_state", "Mutex", "Atomic", "\\.lock\\("] },
  { ord: "⑤", name: "Tauri listener / emit", patterns: ["\\.listen\\(", "\\.emit\\(", "events::"] },
  // ⑥ は定義上 ① の部分集合で、独立した grep を原理的に書けない（ADR-race-check-simplification 却下 4）。
  // 行を落とさず「① のヒットを送信の有無で分類する」ことを出力へ明示する。
  { ord: "⑥", name: "channel を経由しない worker", patterns: null },
  { ord: "⑦", name: ".await 地点", patterns: ["\\.await"] },
  { ord: "⑧", name: "paint より後に走るもの", patterns: ["request_repaint_after", "dispatch"] },
];

/** 母集団に含めるソース拡張子。`.md` を含めない理由は #805 の着手前ゲートで実測した——
 *  `b7c7ae9` の ⑧ は 2 件ヒットしたが両方とも設計文書の散文で、実コードには 1 件も無かった。 */
export const SOURCE_EXT = /\.(rs|ts|tsx|mjs|ps1)$/;

/** コメント行の判定。#805 の実測では `5b5aa8e` の ⑧ 6 件が**すべて**日本語コメント中の
 *  「dispatch」という語だった（実コードのシンボルではない）。#736 と同型の、散文中の識別子である。
 *  ブロックコメントの継続は `*` の直後に空白か `/` が来る形だけを見る——`*ptr = x` のような
 *  逆参照の代入を落とさないため。 */
export function isCommentLine(text) {
  return /^\s*(\/\/|\/\*|\*[\s/]|#)/.test(text);
}

/** patch のファイル名が変更ファイルの一覧と食い違った（設定や git の版が出力の形を変えた）。main が exit 3 にする。 */
export class DiffShapeError extends Error {}

/** unified diff（`diff-index -p -U0`）を [{ file, sign, text }] へ分解する。
 *  ファイル名は `+++ b/<path>`、削除されたファイル（`+++ /dev/null`）だけ `--- a/<path>` から取る——
 *  消えた行を母集団へ入れるかは `selectPopulation` の `--include-removed` だけが決める。
 *  以前は削除されたファイルの行を捨てており、ファイルごと消した worker が廃止の検査に映らなかった（セルフレビューが実測）。
 *  hunk の本文は `@@ -a,b +c,d @@` が宣言した行数だけ読む——接頭辞で見分けると、`++ ` で始まる追加行
 *  （patch では `+++ …`）をヘッダと誤ってファイル名を切り替えた（委譲レビューが本物の git で実測）。 */
export function parseDiff(diffText) {
  const rows = [];
  let file = null;
  let oldFile = null;
  let oldLeft = 0;
  let newLeft = 0;
  for (const line of diffText.split("\n")) {
    if (oldLeft > 0 || newLeft > 0) {
      if (line.startsWith("+")) {
        newLeft--;
        if (file) rows.push({ file, sign: "+", text: line.slice(1) });
      } else if (line.startsWith("-")) {
        oldLeft--;
        if (file) rows.push({ file, sign: "-", text: line.slice(1) });
      } else if (line.startsWith(" ")) {
        oldLeft--;
        newLeft--;
      } // `\ No newline at end of file` は数えない
      continue;
    }
    const hunk = /^@@ -\d+(?:,(\d+))? \+\d+(?:,(\d+))? @@/.exec(line);
    if (hunk) {
      oldLeft = hunk[1] === undefined ? 1 : Number(hunk[1]);
      newLeft = hunk[2] === undefined ? 1 : Number(hunk[2]);
    } else if (line.startsWith("--- ")) {
      const t = line.slice(4).trim();
      oldFile = t === "/dev/null" ? null : t.replace(/^a\//, "");
    } else if (line.startsWith("+++ ")) {
      const t = line.slice(4).trim();
      file = t === "/dev/null" ? oldFile : t.replace(/^b\//, "");
    }
  }
  return rows;
}

/** 追跡ファイルの差分行（作業ツリー対 mergeBase）と変更ファイルの数。porcelain の `git diff` は出力の形が
 *  設定と cwd で変わり、行が**黙って**落ちた（形を変えた設定はテストの `CONFIGS` が持つ・#1300 の作業中に実測）。
 *  plumbing の `diff-index` は quotePath 以外を無視する。
 *  patch から解いた名前を、`-z` の一覧（quote されない）と突き合わせる——観測した壊れ方はどれも名前が化ける形で
 *  あり、化けた名前の行はソース拡張子に合わず黙って落ちた。行数までは照合しない（ADR-race-boundaries-git-plumbing）。 */
export function readTrackedRows(git, mergeBase) {
  const rows = parseDiff(git("-c", "core.quotePath=false", "diff-index", "-M", "-p", "-U0", mergeBase));
  const names = new Set(
    git("-c", "core.quotePath=false", "diff-index", "-M", "--name-only", "-z", mergeBase)
      .split("\0")
      .filter((p) => p !== ""),
  );
  const unknown = [...new Set(rows.map((r) => r.file))].filter((f) => !names.has(f));
  if (unknown.length > 0) throw new DiffShapeError(`patch の名前が変更ファイルの一覧に無い: ${unknown.join(", ")}`);
  return { rows, changedFiles: names.size };
}

/** 未追跡のソースファイルを 1 件ずつ挙げる。`--untracked-files=all` が無いと新しいディレクトリが
 *  `?? dir/` の 1 行に畳まれ、`-z` が無いと `core.quotePath=true`（git の既定）で日本語パスが quote される——
 *  どちらも SOURCE_EXT に合わず、中のファイルが**黙って**落ちる（#1300 の作業中に実測）。 */
export function listUntracked(git) {
  return git("status", "--porcelain", "-z", "--untracked-files=all")
    .split("\0")
    .filter((e) => e.startsWith("?? "))
    .map((e) => e.slice(3))
    .filter((p) => SOURCE_EXT.test(p));
}

/** `listUntracked` のパスを読む関数。`--porcelain` のパスは cwd に依らずルート相対である（`--short` は
 *  cwd 相対だった）——cwd 相対で開くと、サブディレクトリから打ったとき件数だけ数えて中身が落ちる（実測）。 */
export function readerFor(top) {
  return (f) => {
    try {
      return fs.readFileSync(path.join(top, f), "utf8");
    } catch {
      return null;
    }
  };
}

/** 未追跡ファイルの全行を「追加行」として母集団へ入れる。
 *  `listUntracked` が挙げたファイルは、パスではなく中身が対象である。 */
export function untrackedRows(files, readFile) {
  const rows = [];
  for (const file of files) {
    const text = readFile(file);
    if (text == null) continue;
    for (const line of text.split("\n")) rows.push({ file, sign: "+", text: line });
  }
  return rows;
}

/** 母集団のフィルタ。既定は「追加行・ソース拡張子・コメントでない」。
 *  削除行は既定で除く——境界の候補として挙げたいのは変更後に**渡っている**ものであり、追加側が主だからである。
 *  廃止・移行を検査するとき（`docs/build-commands.md`「Windows/macOS/Linux で実行可能」）だけ includeRemoved を立てる。 */
export function selectPopulation(rows, { includeRemoved = false } = {}) {
  return rows.filter(
    (r) => (includeRemoved || r.sign === "+") && SOURCE_EXT.test(r.file) && !isCommentLine(r.text),
  );
}

/** 種別ごとに候補を分類する。1 行が複数種別に当たるなら**そのすべてに立てる**——
 *  SKILL「1 つの worker が複数の種別を持つなら、種別ごとに別に立てる」に対応する。 */
export function classify(population) {
  const compiled = KINDS.map((k) => ({ ...k, res: k.patterns?.map((p) => new RegExp(p)) ?? null }));
  return compiled.map((k) => {
    if (!k.res) return { ord: k.ord, name: k.name, patterns: null, hits: [] };
    const hits = [];
    for (const r of population) {
      const i = k.res.findIndex((re) => re.test(r.text));
      if (i >= 0) hits.push({ file: r.file, sign: r.sign, pattern: k.patterns[i], text: r.text.trim() });
    }
    return { ord: k.ord, name: k.name, patterns: k.patterns, hits };
  });
}

/** 報告本文。**全種を必ず印字する**（0 件も件数とパターンつきで）。
 *  行番号は出さない——SKILL「行番号で位置を断定しない。挿入でずれる」。 */
export function formatReport(kinds, meta) {
  const out = [];
  out.push(`# 並行境界の候補（race:boundaries）`);
  out.push(``);
  out.push(`基準: \`git merge-base ${meta.base} HEAD\` = ${meta.mergeBase.slice(0, 8)}`);
  out.push(`母集団: 変更ファイル ${meta.changedFiles} 件 / 未追跡 ${meta.untrackedFiles} 件 / 判定対象 ${meta.populationLines} 行`);
  out.push(`除外: ソース拡張子（${SOURCE_EXT.source}）以外・コメント行${meta.includeRemoved ? "" : "・削除行"}`);
  out.push(``);
  for (const k of kinds) {
    if (!k.patterns) {
      out.push(`${k.ord} ${k.name}: 独立した grep は無い——① のヒットを送信の有無で分類し、送信を持たないものがこれである`);
      out.push(``);
      continue;
    }
    out.push(`${k.ord} ${k.name}: ${k.hits.length} 件  [パターン: ${k.patterns.join(", ")}]`);
    for (const h of k.hits) out.push(`    ${h.file}  /${h.pattern}/  ${h.text.slice(0, 100)}`);
    out.push(``);
  }
  out.push(`**手がかりは入口であって網羅ではない。** 別名 import・再エクスポート・マクロ生成・\`Builder::new().spawn\` は当たらない。`);
  out.push(`0 件は「この手がかりでは見つからなかった」であって「無い」ではない。`);
  return out.join("\n");
}

// ---------------------------------------------------------------------------
// 実行
// ---------------------------------------------------------------------------

const USAGE = `usage: node scripts/race-boundaries.mjs --base <ref> [--include-removed]

  --base <ref>        分岐元。merge-base をこの ref と HEAD から取る（必須）
  --include-removed   削除行も母集団へ入れる（廃止・移行を検査するとき）

--base は省略できない。比較対象を書いて初めて手順になる
（docs/check-skill-skeleton-design.md「必須 1 — 母集団」）。`;

export function parseArgs(argv) {
  const out = { base: null, includeRemoved: false };
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === "--base") {
      out.base = argv[i + 1] ?? null;
      i += 1;
    } else if (argv[i] === "--include-removed") {
      out.includeRemoved = true;
    }
  }
  return out;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (!args.base) {
    console.error(USAGE);
    process.exit(2);
  }

  const git = (...a) => execFileSync("git", a, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });

  let mergeBase;
  try {
    mergeBase = git("merge-base", args.base, "HEAD").trim();
  } catch {
    console.error(`基準を解決できない: \`git merge-base ${args.base} HEAD\` が失敗した`);
    process.exit(3);
  }

  // `diff-index <mergeBase>`（`--cached` 無し）は作業ツリーまでを含む——`<mergeBase> HEAD` の 2 端点にすると
  // 未コミット分が落ちる。`main...HEAD` を使わないのも同じ理由である（実測 183 行 vs 139 行）。
  let rows;
  let changedFiles;
  try {
    ({ rows, changedFiles } = readTrackedRows(git, mergeBase));
  } catch (e) {
    if (!(e instanceof DiffShapeError)) throw e;
    console.error(
      `差分のファイル名を読めない——母集団を欠いたまま報告しない:\n${e.message}\n` +
        `git の設定・版が出力の形を変えたか、2 回の読み取りの間に作業ツリーが動いた（再実行で消えるなら後者）。`,
    );
    process.exit(3);
  }
  const untracked = listUntracked(git);
  const readFile = readerFor(git("rev-parse", "--show-toplevel").trim());
  const all = [...rows, ...untrackedRows(untracked, readFile)];

  // 変更されたファイルも未追跡のソースも無いなら、それは「変更が無い」ではなく「基準が誤っている」と読む。
  // 空の母集団のまま先へ進ませない——0 件の報告と「そもそも何も見ていない」を区別できなくなる。
  // 行の数で判定しない——バイナリを消しただけ・中身の変わらない rename だけのブランチは行を生まない（委譲レビューが実測）
  if (changedFiles === 0 && untracked.length === 0) {
    console.error(
      `基準が誤っている: \`git diff-index ${mergeBase.slice(0, 8)}\` と未追跡のソースファイル（\`git status --porcelain --untracked-files=all\`）がともに空。\n` +
        `--base に渡した \`${args.base}\` が分岐元でない可能性がある（merge-base = ${mergeBase.slice(0, 8)}）。分岐元のブランチ名を渡し直す。`,
    );
    process.exit(3);
  }

  const population = selectPopulation(all, { includeRemoved: args.includeRemoved });
  const kinds = classify(population);
  console.log(
    formatReport(kinds, {
      base: args.base,
      mergeBase,
      changedFiles,
      untrackedFiles: untracked.length,
      populationLines: population.length,
      includeRemoved: args.includeRemoved,
    }),
  );
}

if (process.argv[1] && process.argv[1].endsWith("race-boundaries.mjs")) main();
