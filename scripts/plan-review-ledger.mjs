// plan:ledger — /plan-review「Step 2 — 並列サブエージェントで検証」の台帳ディレクトリの新鮮化と、
// 「Step 3 — 結果の統合と報告」の双方向照合を決定的に行う（#826 サイクル）。
// shebang を置かない — CI の Windows checkout（autocrlf=true）で CRLF 化された shebang 行は
// vitest の transform を SyntaxError で落とす（PR #592 で実測。起動は常に `node scripts/...` 経由）。
//
// なぜスクリプトなのか: 台帳の新鮮化・スラグの一意性・ディスクとの双方向照合は、いずれも
// 「決定的な手順」であって判断ではない。散文で書き続けた結果 SKILL.md が 10 倍へ膨らみ、
// 硬化のたびに厚くなる面がここだった（`docs/adr/ADR-race-check-simplification.md`:
// 「塞ぐほど忠実な読者の詰まる箇所が増え、手を抜く読者の突破はほぼ減らなかった」）。
// 機構へ移すと腐りは本テストが捕まえ、SKILL 本文には呼び出し 1 行が残る。
//
// 契約:
// - **呼び出し側が渡した内容を、このスクリプトは決して書かない。** 受け取るのはスラグだけで、
//   `init` はディレクトリを作り、`verify` は読んで報告する。`--write` / `--report-to` の類を
//   足してはならない——`/plan-review` のオーケストレーターが `Write` を持たないのは
//   「返り値で受けた内容を自分で成果物ファイルへ転記でき、実在確認も中身の検査も
//   自作自演で通ってしまう」ためであり（SKILL 本文）、本スクリプトに書き込み口を開けると
//   その性質を静かに失う
// - 依存ゼロ（Node 標準のみ）・決定的（ネットワーク・時刻に非依存）
// - スラグ 0 件は exit 2。台帳が空のまま照合を通す経路を塞ぐ（母集団の欠落）
// - `init` の作成は**非冪等**（`mkdirSync` を `recursive` 無しで呼ぶ）。削除が silent fail して
//   いれば EEXIST で止まる——削除は成否を返さないので、これが唯一の観測点である。
//   Node の `rmSync` + `mkdirSync` は POSIX / Windows で同じ意味を持つため、
//   旧手順が必要としたシェル依存の注意（PowerShell では `rm -rf` が失敗する）は消える
// - `verify` は不着（未生成・空・スタブ・命名逸脱）が 1 件でもあれば exit 1。
//   **「ディスクにあって台帳に無い」も不着として数える**（命名逸脱で 1 体分が消え、
//   N 件中 N 件実在のまま照合を通る経路）
// - 判定本体は入力注入の純関数（scripts/plan-review-ledger.test.mjs がフィクスチャで検証する）
import fs from "node:fs";
import path from "node:path";

/** 台帳ディレクトリ（リポジトリルート相対）。`/start-issue`「5a」のループが毎ラウンド作り直す。 */
export const LEDGER_DIR = "workspace/plan-review";

/** スラグの形。`/plan-review` Step 2 が定める `[a-z0-9-]+` をそのまま機械判定にする。 */
export const SLUG_RE = /^[a-z0-9]+(-[a-z0-9]+)*$/;

/**
 * 成果物が「実在」を名乗れる最小文字数（改行・空白を除く）。
 * **これは品質の判定ではなく、途中終了の検出器である。** `/plan-review` Step 2 が要求する
 * 出力（3 分類 + 第 4 分類「未検証（理由）」・各項目に `file:line`）は、最小限の
 * 「問題なし 1 件 + 未検証 1 件」でもこの値を超える。下回るものは書き手が落ちた形であり、
 * 中身の妥当性はオーケストレーター（Opus）が読んで判断する——ここでは判断しない。
 */
export const MIN_CHARS = 120;

/** 分類値。**2 値である**（実在 / 不着）。「当ラウンド対象外」は #826 で廃止した——
 *  台帳も成果物も毎ラウンド削除されるため「前ラウンドで検証済み」を証す面が残らず、
 *  規則は既定で全起動へ倒れていた（義務だけが残る形）。 */
export const PRESENT = "実在";
export const MISSING = "不着";

/** 不着の理由。報告で「未生成」と「空」を畳まない——落ち方が違えば次の手も違う。 */
export const REASON = {
  absent: "未生成",
  empty: "空",
  stub: "スタブ（途中終了）",
  unexpected: "命名逸脱（台帳に無い）",
};

/**
 * argv からスラグ列を取り出す。`--slug <name>` の繰り返しのみを受ける。
 * 内容を受け取る口（`--write` 等）を**意図的に持たない**（先頭の契約）。
 */
export function parseArgs(argv) {
  const slugs = [];
  let command = null;
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "init" || a === "verify") command = a;
    else if (a === "--slug") {
      const v = argv[++i];
      if (v == null) return { error: "--slug に値がない" };
      slugs.push(v);
    } else return { error: `未知の引数: ${a}` };
  }
  if (!command) return { error: "サブコマンドが無い（init | verify）" };
  return { command, slugs };
}

/**
 * スラグ集合の健全性を検査する。**空集合を通さない**——台帳が空なら照合は
 * 「0 件中 0 件実在」で自動成立し、起動しなかったことが問題なしへ化ける。
 */
export function validateSlugs(slugs) {
  const errors = [];
  if (slugs.length === 0) errors.push("スラグが 0 件（台帳が空のまま照合は通せない）");
  const seen = new Set();
  for (const s of slugs) {
    if (!SLUG_RE.test(s)) errors.push(`スラグの形が不正: ${JSON.stringify(s)}（${SLUG_RE} に一致しない）`);
    if (seen.has(s)) errors.push(`スラグが重複: ${s}（後勝ちで 1 体分の成果物が消える）`);
    seen.add(s);
  }
  return errors;
}

/**
 * 台帳（スラグ列）とディスク上のファイル列を**双方向**で突き合わせる。
 * `files` は [{ name, chars }]（`chars` は空白・改行を除いた文字数）。
 * 戻り値は台帳順の行 + 命名逸脱の行。
 */
export function classifyEntries(slugs, files) {
  const byName = new Map(files.map((f) => [f.name, f]));
  const rows = slugs.map((slug) => {
    const f = byName.get(`${slug}.md`);
    if (!f) return { slug, status: MISSING, reason: REASON.absent, chars: 0 };
    if (f.chars === 0) return { slug, status: MISSING, reason: REASON.empty, chars: 0 };
    if (f.chars < MIN_CHARS) return { slug, status: MISSING, reason: REASON.stub, chars: f.chars };
    return { slug, status: PRESENT, reason: null, chars: f.chars };
  });
  const expected = new Set(slugs.map((s) => `${s}.md`));
  for (const f of files) {
    if (!expected.has(f.name)) {
      rows.push({ slug: f.name.replace(/\.md$/, ""), status: MISSING, reason: REASON.unexpected, chars: f.chars });
    }
  }
  return rows;
}

/** 報告表。`/plan-review`「Step 3 — 結果の統合と報告」の配送欄へそのまま貼れる形で出す。 */
export function formatReport(rows) {
  const present = rows.filter((r) => r.status === PRESENT).length;
  const out = [`### 配送（台帳 ${rows.length} 件中 ${present} 件が実在）`];
  for (const r of rows) {
    const detail = r.status === PRESENT ? `実在（${r.chars} 字）` : `不着（${r.reason}）— 独立レビュー不成立`;
    out.push(`- ${r.slug} → ${LEDGER_DIR}/${r.slug}.md: ${detail}`);
  }
  if (present < rows.length) {
    out.push("");
    out.push("**不着のエントリは検証されていない**（問題が無かったのではない）。同じ指示で 1 度だけ再起動し、");
    out.push("それでも不着なら報告の配送欄へ不成立として書く。completeness は「高」にできない。");
  }
  return out.join("\n");
}

/** ディレクトリ内の `*.md` を [{ name, chars }] で列挙する。読めないファイルは 0 字として不着へ倒す。 */
export function readLedgerDir(dir, io = fs) {
  if (!io.existsSync(dir)) return [];
  return io
    .readdirSync(dir)
    .filter((n) => n.endsWith(".md"))
    .sort()
    .map((name) => {
      let chars = 0;
      try {
        chars = io.readFileSync(path.join(dir, name), "utf8").replace(/\s/g, "").length;
      } catch {
        chars = 0;
      }
      return { name, chars };
    });
}

function usage() {
  return [
    "使い方:",
    "  node scripts/plan-review-ledger.mjs init   --slug <name> [--slug <name> ...]",
    "  node scripts/plan-review-ledger.mjs verify --slug <name> [--slug <name> ...]",
    "",
    "init   … 台帳ディレクトリを削除して作り直し、割り当てる絶対パスを印字する",
    "verify … 台帳とディスクを双方向照合し、不着があれば exit 1",
  ].join("\n");
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.error) {
    console.error(`${args.error}\n\n${usage()}`);
    process.exit(2);
  }
  const errors = validateSlugs(args.slugs);
  if (errors.length > 0) {
    console.error(errors.map((e) => `- ${e}`).join("\n"));
    process.exit(2);
  }

  const dir = path.resolve(process.cwd(), LEDGER_DIR);

  if (args.command === "init") {
    fs.rmSync(dir, { recursive: true, force: true });
    fs.mkdirSync(path.dirname(dir), { recursive: true });
    // recursive を付けない。削除が silent fail していれば EEXIST で止まる（唯一の観測点）。
    fs.mkdirSync(dir);
    console.log(`台帳ディレクトリを新鮮化した: ${dir}`);
    console.log("");
    console.log("各スカウトへ渡す絶対パス（返り値に依存させず、このパスへ書かせる）:");
    for (const s of args.slugs) console.log(`- ${s}: ${path.join(dir, `${s}.md`)}`);
    return;
  }

  const rows = classifyEntries(args.slugs, readLedgerDir(dir));
  console.log(formatReport(rows));
  if (rows.some((r) => r.status === MISSING)) process.exit(1);
}

if (process.argv[1] && process.argv[1].endsWith("plan-review-ledger.mjs")) main();
