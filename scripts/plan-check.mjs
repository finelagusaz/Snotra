// plan:check — `/implement`「1a. workspace/plan.md の由来を確かめる」の入口判定（未確定ゼロ・承認の形）を決定的に行う（#1552）。
// 判定の述語の SSOT は本ファイルである。承認の行の形と 3 基準は `/start-issue`「5c. 承認」が定め、
// 判定の結果をどう読むか（1b の行き先）は `/implement` が持つ。
// shebang を置かない — CI の Windows checkout（autocrlf=true）で CRLF 化された shebang 行は
// vitest の transform を SyntaxError で落とす（PR #592 で実測。起動は常に `node scripts/...` 経由）。
//
// 契約:
// - 依存は Node 標準と、未チェック項目の数え方を共有する `.claude/hooks/pre-bash.mjs` の `countUnchecked` だけ。
//   判定はファイルの中身だけで決まる（git・時刻・ネットワークに依らない）
// - 終了コード: 0 = 成立 / 3 = 不成立（理由を全件、直し方つきで出す） / 2 = 判定できない（使用法の誤り・
//   ファイルが無い・読めない）。**0 と 3 以外はすべて「判定できない」と読む**——node がロードの前に落ちると 1 になるので、
//   不成立に 1 を使うと区別できない
// - fail-closed: 確かめられなければ成立にしない。見出しが無い・節が空・行の形が違う・引用が空は、どれも不成立
// - コードフェンスは非対称に扱う。成立の証拠（見出し・承認の行・check スキルの行）はフェンスの外だけで数え、
//   不成立の証拠（未チェック項目・（裁定）・`SPEC.md` の項目・`/persistence-check`）はフェンスの中でも数える。
//   雛形を引用した計画を通さないため
import fs from "node:fs";
import path from "node:path";
import { countUnchecked, findUp } from "../.claude/hooks/pre-bash.mjs";

export const USAGE = `使い方: npm run plan:check -- [--plan <plan.md>]
  --plan  判定する計画（既定: cwd から最も近い .git へ遡ったルートの workspace/plan.md）
終了コード: 0 = 成立 / 3 = 不成立 / 2 = 判定できない`;

export class UsageError extends Error {}

export function parseArgs(argv) {
  const out = { plan: null };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--plan") {
      if (i + 1 >= argv.length) throw new UsageError("--plan に値が無い");
      out.plan = argv[++i];
    } else {
      throw new UsageError(`未知の引数: ${a}`);
    }
  }
  return out;
}

/**
 * 計画の本文を判定する。`{ ok, kind, reasons }` を返す。`kind` は `"human"` / `"self"` / `null`（承認の行が無い）。
 * `reasons` は不成立の理由を打ち切らずにすべて並べる（直し方を添える）。
 */
export function judgePlan(text) {
  const lines = withFences(text);
  const reasons = [];

  const unresolved = section(lines, (t) => t.startsWith("未確定（実装前に潰す）"));
  if (unresolved === null) {
    reasons.push(
      "未確定欄の見出し（`## 未確定（実装前に潰す）`）が無い。見出しの無い計画は件数ゼロとして通ってしまう（#821）。" +
        "`/start-issue`「Step 4」の雛形どおりに節を置く",
    );
  } else {
    const n = countUnchecked(joined(unresolved));
    if (n > 0) reasons.push(`未確定欄に未チェックの項目が ${n} 件残っている。\`/start-issue\`「5a」へ戻って潰す`);
  }

  const approval = section(lines, (t) => t === "承認");
  let kind = null;
  if (approval === null) {
    reasons.push("承認欄の見出し（`## 承認`）が無い。`/start-issue`「5c. 承認」の形で記録する");
  } else {
    const outside = approval.filter((l) => !l.inFence).map((l) => l.text);
    if (outside.some((t) => HUMAN_LINE.test(t))) kind = "human";
    else if (outside.some((t) => SELF_LINE.test(t))) kind = "self";
    else {
      reasons.push(
        "承認欄に承認の行が無い。人間承認か自己承認の行を、`/start-issue`「5c. 承認」の形のまま書く" +
          "（`[x]` は小文字、区切りのダッシュは `—`。人間承認の問い・回答の引用は空にしない。自己承認の 3 項目はすべて「なし」）",
      );
    }
  }

  if (kind === "self") reasons.push(...selfApprovalReasons(lines, unresolved));
  return { ok: reasons.length === 0, kind, reasons };
}

// 成立の証拠（承認の行）は狭く取る: 小文字の `[x]` と U+2014 のダッシュだけ（#1552 の調査で、実例の人間承認 61 件・
// 自己承認 49 件はすべてこの形だった）。引用は空を許さない
const HUMAN_LINE = /^- \[x\] 承認済み — \S+ \/ 問い: ".+" \/ 回答: ".+"\s*$/;
const SELF_LINE = /^- \[x\] 自己承認 — \S+ \/ 仕様変更: なし — .+ \/ 戻せない変更: なし — .+ \/ 人の裁定: なし\s*$/;
// 不成立の証拠は広く取る: 半角の `(裁定)` も印として数える。印はチェックボックスの直後に置く（`/start-issue`「Step 4」）
const RULING_MARK = /^\s*[-*]\s+\[[ xX]\]\s*[（(]裁定[）)]/;
const LIST_ITEM = /^\s*(?:[-*]|\d+\.)\s+(.*)$/;
const CHECK_SKILL_LINE = /^\s*[-*]\s+check スキル/;

function selfApprovalReasons(lines, unresolved) {
  const reasons = [];
  if (unresolved !== null && unresolved.some((l) => RULING_MARK.test(l.text))) {
    reasons.push(
      "自己承認なのに、未確定欄に「（裁定）」の印の付いた項目がある。人の裁定を含む計画は人間承認にする（`/start-issue`「5c. 承認」）",
    );
  }
  const files = section(lines, (t) => t.startsWith("変更するファイル"));
  if (files === null) {
    reasons.push("自己承認なのに「変更するファイル」の節（`## 変更するファイル`）が無い。`/start-issue`「Step 4」の形で置く");
  } else if (files.some((l) => listsSpec(l.text))) {
    reasons.push(
      "自己承認なのに「変更するファイル」の節に SPEC.md の項目がある。仕様変更なら人間承認にする（`/start-issue`「5c. 承認」）。" +
        "変えないなら、節に書かない（`/start-issue`「Step 4」）",
    );
  }
  const review = section(lines, (t) => t.startsWith("セルフレビュー"));
  if (review === null || !review.some((l) => !l.inFence && CHECK_SKILL_LINE.test(l.text))) {
    reasons.push("自己承認なのに、セルフレビューの節（`## セルフレビュー`）に「check スキル」の行が無い（`/start-issue`「5b」）");
  }
  if (review !== null && review.some((l) => l.text.includes("/persistence-check"))) {
    reasons.push(
      "自己承認なのに、セルフレビューの節に /persistence-check がある。永続形式に触れる計画は人間承認にする（`/start-issue`「5c. 承認」）",
    );
  }
  return reasons;
}

/** リスト項目で、最初のコードスパン（無ければ項目の先頭の語）が `SPEC.md` か。散文の行（「`SPEC.md` は変えない。」）は数えない */
function listsSpec(line) {
  const m = LIST_ITEM.exec(line);
  if (!m) return false;
  const code = /`([^`]*)`/.exec(m[1]);
  const first = code ? code[1] : m[1].split(/\s/)[0];
  return first === "SPEC.md";
}

const FENCE_LINE = /^\s*(?:```|~~~)/;
const HEADING = /^(#{1,2})\s+(.*?)\s*$/;

/** 各行に、コードフェンスの中かどうかを添える（フェンスの開き・閉じの行も中として扱う） */
function withFences(text) {
  let inFence = false;
  return text.split(/\r?\n/).map((line) => {
    if (FENCE_LINE.test(line)) {
      inFence = !inFence;
      return { text: line, inFence: true };
    }
    return { text: line, inFence };
  });
}

/**
 * フェンスの外のレベル 2 の見出しのうち、見出しの文字列が `match` に当たる最初のものの節（次のレベル 1・2 の見出しの前まで）。
 * 当たる見出しが無ければ null
 */
function section(lines, match) {
  const start = lines.findIndex((l) => {
    if (l.inFence) return false;
    const h = HEADING.exec(l.text);
    return h !== null && h[1].length === 2 && match(h[2]);
  });
  if (start < 0) return null;
  const out = [];
  for (const l of lines.slice(start + 1)) {
    if (!l.inFence && HEADING.test(l.text)) break;
    out.push(l);
  }
  return out;
}

const joined = (sectionLines) => sectionLines.map((l) => l.text).join("\n");

/**
 * cwd から最も近い `.git`（ディレクトリでもファイルでもよい）へ遡ったルート。見つからなければ cwd。
 * PR 前ゲート（`readPlanState`）と同じ決め方にして、2 つが同じ plan.md を読むようにする
 */
export function findRoot(cwd) {
  return findUp(cwd, ".git") ?? path.resolve(cwd);
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
  const planPath = args.plan ?? path.join(findRoot(process.cwd()), "workspace", "plan.md");
  let text;
  try {
    text = fs.readFileSync(planPath, "utf8");
  } catch (e) {
    console.error(`計画を読めない: ${planPath.replaceAll("\\", "/")}（${e.code ?? e.message}）\n${USAGE}`);
    process.exit(2);
  }
  const r = judgePlan(text);
  if (r.ok) {
    console.log(`plan:check — 成立（${r.kind === "human" ? "人間承認" : "自己承認"}）`);
    process.exit(0);
  }
  console.log(`plan:check — 不成立（${r.reasons.length} 件）`);
  for (const reason of r.reasons) console.log(`- ${reason}`);
  process.exit(3);
}

if (process.argv[1] && process.argv[1].endsWith("plan-check.mjs")) {
  try {
    main();
  } catch (e) {
    console.error(`plan:check — 判定できない: ${e?.stack ?? String(e)}`);
    process.exit(2);
  }
}
