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
// - 差分と未追跡がともに空なら exit 3（沈黙経路の閉塞。SKILL 段 1 の「2 コマンドの出力が
//   両方とも空なら『変更が無い』ではなく『基準が誤っている』と読む」を機構化したもの）。
//   **merge-base が HEAD と一致すること自体は異常ではない**——この検査は `/implement`「3a. 委譲の前に」から
//   コミットより前に呼ばれ、初コミット前の feature ブランチでは常にそうなる（実測）
// - 追跡差分の patch と numstat が食い違えば exit 3（git の設定が出力の形を変えて行を黙って落とす形の閉塞・`readTrackedRows`）
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

/** unified diff（`diff-index -p -U0`）を [{ file, sign, text }] へ分解する。
 *  `+++ b/<path>` をファイル名の起点にする（`--- a/<path>` は削除専用ファイルでも同じ値を持つ）。
 *  hunk の本文は `@@ -a,b +c,d @@` が宣言した行数だけ読む——接頭辞で見分けると、`++ ` で始まる追加行
 *  （patch では `+++ …`）をヘッダと誤ってファイル名を切り替えた（委譲レビューが本物の git で実測）。 */
export function parseDiff(diffText) {
  const rows = [];
  let file = null;
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
    } else if (line.startsWith("+++ ")) {
      const t = line.slice(4).trim();
      file = t === "/dev/null" ? null : t.replace(/^b\//, "");
    }
  }
  return rows;
}

/** `diff-index -M --numstat -z` を Map(新しいパス → { add, del }) へ分解する。バイナリ（`-`）は null。
 *  一時リポジトリで実測した形は、通常 `<add>\t<del>\t<path>\0` と、rename の `<add>\t<del>\t\0<old>\0<new>\0` だった。 */
export function parseNumstatZ(out) {
  const map = new Map();
  const fields = out.split("\0");
  for (let i = 0; i < fields.length; i++) {
    if (fields[i] === "" && i === fields.length - 1) break;
    const m = /^(\d+|-)\t(\d+|-)\t(.*)$/s.exec(fields[i]);
    if (!m) throw new Error(`numstat の項目を読めない: ${JSON.stringify(fields[i])}`);
    let file = m[3];
    if (file === "") {
      file = fields[i + 2];
      i += 2;
      if (file === undefined || file === "") throw new Error("numstat の rename に新しいパスが無い");
    }
    map.set(file, m[1] === "-" ? null : { add: Number(m[1]), del: Number(m[2]) });
  }
  return map;
}

/** patch から解いた行と numstat の行数の食い違い（空なら一致）。別経路の 2 つの出力を突き合わせるので、
 *  ヘッダの形が変わって名前が化けた・行が落ちた形を、形の種類を列挙せずに捕まえる。
 *  削除されたファイル（`+++ /dev/null`）は patch 側が行を生まないので、追加 0 の項目は欠落を問わない。
 *  バイナリ扱いのソース（`-diff` 属性など）は中身を見られないので食い違いとして出す（委譲レビューが実測）。 */
export function diffMismatches(rows, numstat) {
  const parsed = new Map();
  for (const r of rows) {
    const c = parsed.get(r.file) ?? { add: 0, del: 0 };
    if (r.sign === "+") c.add++;
    else c.del++;
    parsed.set(r.file, c);
  }
  const out = [];
  for (const [file, c] of parsed) {
    const n = numstat.get(file);
    if (n === undefined) out.push(`patch の名前が numstat に無い: ${file}`);
    else if (n !== null && (n.add !== c.add || n.del !== c.del)) {
      out.push(`${file}: patch +${c.add}/-${c.del} 行 / numstat +${n.add}/-${n.del} 行`);
    }
  }
  for (const [file, n] of numstat) {
    if (n === null && SOURCE_EXT.test(file)) out.push(`${file}: バイナリ扱いで中身を見られない（.gitattributes の -diff 等）`);
    else if (n !== null && n.add > 0 && !parsed.has(file)) out.push(`${file}: numstat の +${n.add} 行が patch に無い`);
  }
  return out;
}

/** 照合が食い違った（設定や git の版が出力の形を変えた）。main が exit 3 にする。 */
export class DiffShapeError extends Error {}

/** 追跡ファイルの差分行（作業ツリー対 mergeBase）。porcelain の `git diff` は出力の形が設定と cwd で変わり
 *  （quotePath・relative・mnemonicPrefix・noprefix・color・external）、6 形とも行が**黙って**落ちた（#1300 の作業中に
 *  一時リポジトリで実測）。plumbing の `diff-index` は残る quotePath 以外を無視する。照合の相手も plumbing にする——
 *  porcelain の `--numstat` は `diff.relative` で patch と同じだけ狭まり、黙って一致した（実測）。 */
export function readTrackedRows(git, mergeBase) {
  const rows = parseDiff(git("-c", "core.quotePath=false", "diff-index", "-M", "-p", "-U0", mergeBase));
  const numstat = parseNumstatZ(git("-c", "core.quotePath=false", "diff-index", "-M", "--numstat", "-z", mergeBase));
  const bad = diffMismatches(rows, numstat);
  if (bad.length > 0) throw new DiffShapeError(bad.join("\n"));
  return rows;
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
 *  削除行は既定で除く——段 2 の主題は「**渡っている**ものを挙げる」であり追加側が主だからである。
 *  廃止・移行を検査するとき（SKILL 段 1 の最後の段落）だけ includeRemoved を立てる。 */
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
  // 未コミット分が落ちる。`main...HEAD` を使わないのも同じ理由である（SKILL 段 1・実測 183 行 vs 139 行）。
  let rows;
  try {
    rows = readTrackedRows(git, mergeBase);
  } catch (e) {
    if (!(e instanceof DiffShapeError)) throw e;
    console.error(
      `差分の 2 経路（patch と numstat）が食い違う——母集団を欠いたまま報告しない:\n${e.message}\n` +
        `git の設定・属性・版が出力の形を変えたか、2 回の読み取りの間に作業ツリーが動いた（再実行で消えるなら後者）。`,
    );
    process.exit(3);
  }
  const untracked = listUntracked(git);
  const readFile = readerFor(git("rev-parse", "--show-toplevel").trim());
  const all = [...rows, ...untrackedRows(untracked, readFile)];

  // SKILL 段 1: 「2 コマンドの出力が両方とも空なら、それは『変更が無い』ではなく『基準が誤っている』と読む」。
  // 空の母集団のまま先へ進ませない——0 件の報告と「そもそも何も見ていない」を区別できなくなる。
  if (rows.length === 0 && untracked.length === 0) {
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
      changedFiles: new Set(rows.map((r) => r.file)).size,
      untrackedFiles: untracked.length,
      populationLines: population.length,
      includeRemoved: args.includeRemoved,
    }),
  );
}

if (process.argv[1] && process.argv[1].endsWith("race-boundaries.mjs")) main();
