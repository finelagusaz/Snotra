// prose:sweep — `/plan-review`「Step 1 — plan.md の読み込み」の「変更で偽になる散文」の母集団を、切り詰めずに全件出す（#1300）。
// 出力仕様（面の分類・0 件の明示・★未掲載）の SSOT は本ファイルであり、読み方は `/plan-review` が持つ。
// shebang を置かない — CI の Windows checkout（autocrlf=true）で CRLF 化された shebang 行は
// vitest の transform を SyntaxError で落とす（PR #592 で実測。起動は常に `node scripts/...` 経由）。
//
// なぜスクリプトなのか: 独立レビュー 58 回の実測で Step 2 の要対処の最多は網羅漏れであり、
// 主の grep の切り詰め（`head` / `--include`）が原因として記録された例がある（#1142・#1182）。
// 切り詰めない手順は、手で打つ grep ではなく決定的な道具に置く。
//
// 契約（`scripts/race-boundaries.mjs` と同じ殻。母集団は差分ではなく全コーパスなので中身は共有しない）:
// - 依存ゼロ（Node 標準のみ）・決定的
// - 語が無い・空語・未知のフラグ・`--plan` が読めない → 使用法 + exit 2
// - `git grep` 自体の失敗 → exit 1。**失敗を「0 件」として印字しない**
// - **語ごとの件数は 0 件も印字する**——概念ラベルの 0 件は「無い」ではなく「ラベルが外れている」の合図
// - **行の種類（コメントか否か）で落とさない**。コメント行に絞ると行末コメントと文字列中の散文が落ち、
//   道具の中に切り詰めを再導入する（#1300 の敵対的調査）。代わりに面で分類して出す
// - 分類本体は入力注入の純関数（scripts/prose-sweep.test.mjs がフィクスチャで検証する）
import { spawnSync } from "node:child_process";
import fs from "node:fs";

export const USAGE = `使い方: npm run prose:sweep -- [--plan <plan.md>] <語>...
  <語>    識別子と、その変更を指す概念ラベル（固定文字列・大文字小文字を区別）
  --plan  ヒットのあったファイルのうち、パスがこの計画に現れないものへ ★未掲載 を付ける
  --      以降はフラグに見えても語として扱う`;

export class UsageError extends Error {}

export function parseArgs(argv) {
  const out = { plan: null, terms: [] };
  let rest = false;
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!rest && a === "--") {
      rest = true;
    } else if (!rest && a === "--plan") {
      if (i + 1 >= argv.length) throw new UsageError("--plan に値が無い");
      out.plan = argv[++i];
    } else if (!rest && a.startsWith("-")) {
      throw new UsageError(`未知のフラグ: ${a}`);
    } else {
      out.terms.push(a);
    }
  }
  if (out.terms.length === 0) throw new UsageError("語が無い");
  // `-e ""` は全行に一致する——空語を通すと母集団がコーパス全体になる
  if (out.terms.some((t) => t === "")) throw new UsageError("空の語は渡せない");
  return out;
}

/** `git grep -n -z` の出力（`<path>\0<line>\0<text>`）を分解する。`-z` は `:` を含む本文と
 *  日本語パスの quote を避けるため。NUL 区切りでない行（`-I` を外したときの `Binary file X matches`）は
 *  黙って捨てずに throw する——捨てると母集団の取り違えが件数の減少として静かに現れる。 */
export function parseGrepZ(stdout) {
  const hits = [];
  for (const raw of stdout.split("\n")) {
    if (raw === "") continue;
    const parts = raw.split("\0");
    // `Number("")` は 0 になるので、数値化の前に桁だけであることを見る
    const line = /^\d+$/.test(parts[1] ?? "") ? Number(parts[1]) : NaN;
    if (parts.length < 3 || !Number.isInteger(line)) {
      throw new Error(`git grep の出力を解析できない: ${raw}`);
    }
    hits.push({ file: parts[0], line, text: parts.slice(2).join("\0").replace(/\r$/, "") });
  }
  return hits;
}

/** 面の一覧（出力順）。凍結された歴史（ADR・superpowers）は書き換えない前提の読者向けに分けるだけで、落とさない。 */
export const SURFACES = ["常時ロード規範", "skills・rules・hooks", "生きた文書", "凍結された歴史", "コード"];

export function surfaceOf(file) {
  if (file === "CLAUDE.md" || file === "AGENTS.md") return "常時ロード規範";
  if (/^\.(claude|githooks|github)\//.test(file)) return "skills・rules・hooks";
  if (/^docs\/(adr|superpowers)\//.test(file)) return "凍結された歴史";
  if (file.endsWith(".md")) return "生きた文書";
  return "コード";
}

/** パスが計画の**どこかに**現れるか（変更ファイル一覧に限らない——「触らない」と名指したファイルも掲載に数える。
 *  ★未掲載が意味するのは「計画が一度も名指していない」である）。前後をパス境界で区切る——素の部分一致だと
 *  ルート `CLAUDE.md` が `snotra-core/CLAUDE.md`（`\` 区切りでも）の部分文字列として偽の掲載になる（#1300 の plan-review）。 */
export function isListedInPlan(file, planText) {
  const esc = file.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`(^|[^A-Za-z0-9_./\\\\-])${esc}(?![A-Za-z0-9_/\\\\-])`, "m").test(planText);
}

/** `git grep` の母集団の引数（語を除く）。見出し行もここから作る——写しを置くと引数だけが変わる。 */
export const GREP_FLAGS = ["-I", "--untracked", "-n", "-z", "-F", "--no-color"];
export const GREP_PATHSPEC = [".", ":!workspace"];

/** 1 語ぶんの `git grep`。cwd はリポジトリルートを渡す（pathspec `.` は cwd 相対で、サブディレクトリから
 *  打つと母集団が縮む）。status 0 = 一致あり・1 = 一致なし、**それ以外はすべて失敗**
 *  （`error` = git 不在・`status: null` = maxBuffer 超過かシグナル）。 */
export function runGrep(term, { cwd, spawn = spawnSync }) {
  const args = ["grep", ...GREP_FLAGS, "-e", term, "--", ...GREP_PATHSPEC];
  const r = spawn("git", args, { cwd, encoding: "utf8", maxBuffer: 256 * 1024 * 1024 });
  if (r.error) throw new Error(`git grep を起動できない: ${r.error.message}`);
  if (r.status === 1) return [];
  if (r.status !== 0) throw new Error(`git grep が失敗した（status ${r.status}）: ${r.stderr ?? ""}`);
  return parseGrepZ(r.stdout);
}

/** @param {{ terms: string[], hitsByTerm: {file:string,line:number,text:string}[][], planText?: string|null, planPath?: string|null }} p */
export function buildReport({ terms, hitsByTerm, planText = null, planPath = null }) {
  // (file, line) で束ね、当たった語を集める。総件数は重複を除き、語ごとの件数は除かない
  const byKey = new Map();
  hitsByTerm.forEach((hits, ti) => {
    for (const h of hits) {
      const key = `${h.file}\0${h.line}`;
      if (!byKey.has(key)) byKey.set(key, { ...h, terms: [] });
      byKey.get(key).terms.push(terms[ti]);
    }
  });
  const byFile = new Map();
  for (const h of byKey.values()) {
    if (!byFile.has(h.file)) byFile.set(h.file, []);
    byFile.get(h.file).push(h);
  }
  const files = [...byFile.keys()].sort();
  const listed = (f) => planText == null || isListedInPlan(f, planText);
  const unlisted = files.filter((f) => !listed(f)).length;

  const out = [];
  out.push(
    `prose:sweep — 語 ${terms.length} / 総件数 ${byKey.size} 行 / ファイル ${files.length} / ` +
      (planText == null ? "★未掲載 —（--plan なし）" : `★未掲載 ${unlisted}（--plan ${planPath ?? ""}）`),
  );
  out.push(`母集団: git grep ${GREP_FLAGS.join(" ")} -- ${GREP_PATHSPEC.join(" ")}（.gitignore 尊重・行の種類で落とさない）`);
  out.push("", "## 語ごとの件数");
  terms.forEach((t, ti) => {
    const n = hitsByTerm[ti].length;
    out.push(`- "${t}": ${n}` + (n === 0 ? "  ← 0 件: 言い換えて打ち直す（ラベルの外れ・物理改行の跨ぎでも 0 になる）" : ""));
  });
  for (const s of SURFACES) {
    const fs_ = files.filter((f) => surfaceOf(f) === s);
    const n = fs_.reduce((acc, f) => acc + byFile.get(f).length, 0);
    out.push("", `## ${s}（${n} 行）`);
    for (const f of fs_) {
      const hs = byFile.get(f).sort((a, b) => a.line - b.line);
      out.push(`### ${f}（${hs.length} 行）${listed(f) ? "" : " ★未掲載"}`);
      for (const h of hs) {
        out.push(`${h.line}${terms.length > 1 ? ` [${h.terms.join(", ")}]` : ""}: ${h.text}`);
      }
    }
  }
  return { text: out.join("\n"), total: byKey.size, unlisted };
}

function main() {
  let args;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (e) {
    if (!(e instanceof UsageError)) throw e;
    console.error(`${e.message}\n${USAGE}`);
    process.exit(2);
  }
  let planText = null;
  if (args.plan != null) {
    try {
      planText = fs.readFileSync(args.plan, "utf8");
    } catch (e) {
      console.error(`--plan を読めない: ${args.plan}（${e.code ?? e.message}）\n${USAGE}`);
      process.exit(2);
    }
  }
  try {
    const top = spawnSync("git", ["rev-parse", "--show-toplevel"], { encoding: "utf8" });
    if (top.error || top.status !== 0) throw new Error(`リポジトリのルートを解決できない: ${top.stderr ?? top.error}`);
    const cwd = top.stdout.trim();
    const hitsByTerm = args.terms.map((t) => runGrep(t, { cwd }));
    console.log(buildReport({ terms: args.terms, hitsByTerm, planText, planPath: args.plan }).text);
  } catch (e) {
    console.error(e.message);
    process.exit(1);
  }
}

if (process.argv[1] && process.argv[1].endsWith("prose-sweep.mjs")) main();
