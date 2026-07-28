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
// - **このスクリプトが書くのは、自分が検証した識別子だけである。** `init` はスラグ集合を
//   `ledger.json` へ書き、`verify` は読んで報告する。**呼び出し側が渡したバイト列
//   （＝レビュー成果物の中身）は決して書かない**——`--write` / `--report-to` の類を
//   足してはならない。`/plan-review` のオーケストレーターが `Write` を持たないのは
//   「返り値で受けた内容を自分で成果物ファイルへ転記でき、実在確認も中身の検査も
//   自作自演で通ってしまう」ためであり（SKILL 本文）、本スクリプトに書き込み口を開けると
//   その性質を静かに失う。識別子と内容の線引きが決定の中身であることは
//   `docs/adr/ADR-plan-ledger-population-persistence.md` が持つ（#831）
// - 依存ゼロ（Node 標準のみ）・決定的（ネットワーク・時刻に非依存）
// - **母集団を得られない全形が exit 2**: 引数の異常（スラグ 0 件・重複・形不正・`verify` への
//   `--slug`）と台帳の異常（不在・parse 不能・shape 不正）。**空集合へ倒さない**——
//   「0 件中 0 件実在」で緑になる経路を作れば、#831 の穴が別の顔で復活する
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

/**
 * 母集団の実体ファイル。**`.md` にしてはならない**——`readLedgerDir` は `.md` を成果物として
 * 列挙するため、台帳が `.md` だと `classifyEntries` が毎回「命名逸脱」で不着 1 件を立て、
 * `verify` が常に赤になる。ドットファイルにもしない: 永続化の目的の一つは PR 差分での
 * 可読な監査であり、レビュアーが開かない名前にする理由が無い。
 */
export const LEDGER_FILE = "ledger.json";

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
 * argv からサブコマンドとスラグ列を取り出す。
 *
 * **`--slug` を受けるのは `init` だけである**（#831）。`verify` の母集団は `init` が書いた
 * 台帳であって argv ではない——ここで `--slug` を**黙って無視すると**、古い手癖・古い写し・
 * 古いトランスクリプトが沈黙で通り、#831 の穴が「引数が効かないだけ」の形で生き延びる。
 * ゆえに拒否する（復帰手順は usage が持つ）。
 *
 * 内容を受け取る口（`--write` / `--report-to` 等）は**意図的に持たない**（先頭の契約）。
 */
export function parseArgs(argv) {
  const slugs = [];
  let command = null;
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "init" || a === "verify") {
      // 後勝ちを許すと `verify init --slug a` が破壊分岐（rmSync）へ到達する。
      // 読み取りのつもりの入力から消去へ落ちる口を塞ぐ。
      if (command) return { error: `サブコマンドが 2 つある: ${command} と ${a}` };
      command = a;
    }
    else if (a === "--slug") {
      if (command === "verify") {
        return {
          error: "verify は --slug を受け付けない（母集団は init が書いた台帳から読む）。レイヤーを変えるなら init からやり直す",
        };
      }
      const v = argv[++i];
      if (v == null) return { error: "--slug に値がない" };
      slugs.push(v);
    } else return { error: `未知の引数: ${a}` };
  }
  if (!command) return { error: "サブコマンドが無い（init | verify）" };
  if (command === "verify" && slugs.length > 0) {
    // サブコマンドが --slug より後に来た形（`--slug x verify`）を拾う。
    return { error: "verify は --slug を受け付けない（母集団は init が書いた台帳から読む）" };
  }
  return { command, slugs };
}

/**
 * 台帳を書く。`init` が**検証済みの識別子だけ**を書く（先頭の契約）。
 * `flag: "wx"` は「このファイルはまだ存在しないはず」という意図の表明である——
 * **観測点ではない**。`main` は `rmSync` → `mkdirSync`（recursive 無し）の順で呼ぶため、
 * ディレクトリが残っていれば `mkdirSync` が先に EEXIST で落ち、ここへは到達しない。
 */
export function writeLedger(dir, slugs, io = fs) {
  io.writeFileSync(path.join(dir, LEDGER_FILE), `${JSON.stringify({ slugs }, null, 2)}\n`, {
    encoding: "utf8",
    flag: "wx",
  });
}

/**
 * 台帳を読む。戻り値は `{ slugs }` か `{ error }`。
 *
 * **失敗を空配列へ倒さない。** 同じファイルの `readLedgerDir` は「読めなければ 0 字」へ倒すが、
 * あれは**成果物側**の fail-closed（不着へ倒れる）である。**母集団側で同じ形を書くと
 * fail-open になる**——0 件中 0 件実在で緑が出て、#831 の穴が別経路で復活する。
 * 例外は種別で絞らず全部捕まえる（不在・権限・エンコーディングのどれも母集団の欠落である）。
 */
export function readLedger(dir, io = fs) {
  const file = path.join(dir, LEDGER_FILE);
  let raw;
  try {
    raw = io.readFileSync(file, "utf8");
  } catch {
    return { error: `台帳が読めない: ${file}\n先に \`npm run plan:ledger -- init --slug <name> ...\` を実行する` };
  }
  // 台帳が壊れている場合の復帰は `init` だが、**それは破壊的である**——`init` は
  // ディレクトリごと消すので、既に届いている成果物も一緒に失われる。復帰手順は
  // 費用とセットで出す（`docs/development-principles.md`「デバッグ・バグ修正」の
  // 「拒否メッセージの復帰手順が適用できない誤爆」と同じ配慮）。
  const RECOVER = "init からやり直す。**init は成果物ごと消すため、残っている *.md を退避してから実行する**";
  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return { error: `台帳が JSON として読めない: ${file}\n${RECOVER}` };
  }
  if (parsed == null || !Array.isArray(parsed.slugs) || parsed.slugs.some((s) => typeof s !== "string")) {
    return { error: `台帳の形が不正: ${file} は { "slugs": [文字列, ...] } でなければならない\n${RECOVER}` };
  }
  return { slugs: parsed.slugs };
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
  // 「台帳 N 件」の N に命名逸脱の行を混ぜない——あれは台帳に無いからこそ立つ行であり、
  // 母集団の件数の正確さが主題である以上、報告表が母集団の件数を誤って名乗ってはならない。
  const unexpected = rows.filter((r) => r.reason === REASON.unexpected).length;
  const ledgerCount = rows.length - unexpected;
  const suffix = unexpected > 0 ? ` / 台帳に無いファイル ${unexpected} 件` : "";
  const out = [`### 配送（台帳 ${ledgerCount} 件中 ${present} 件が実在${suffix}）`];
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
  let names;
  try {
    names = io.readdirSync(dir);
  } catch (e) {
    // 個々のファイルが読めないのは「不着」だが、**ディレクトリを列挙できないのは母集団の欠落**
    // である（成果物側の集合そのものが取れていない）。exit 1 へ落とすと、読み手は
    // 「スカウトを 1 度再起動」へ進んでしまう。
    throw Object.assign(new Error(`成果物ディレクトリを列挙できない: ${dir}（${e.message}）`), {
      populationFailure: true,
    });
  }
  return names
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
    "  node scripts/plan-review-ledger.mjs init --slug <name> [--slug <name> ...]",
    "  node scripts/plan-review-ledger.mjs verify",
    "",
    `init   … 台帳ディレクトリを削除して作り直し、スラグ集合を ${LEDGER_FILE} へ書き、割り当てる絶対パスを印字する`,
    `verify … ${LEDGER_FILE} を母集団として読み、ディスクと双方向照合する。不着があれば exit 1`,
    "",
    "exit 0 = 全件実在 / 1 = 不着あり / 2 = 母集団を得られない（引数・台帳の異常）",
    "  exit 2 のうち引数の異常は打ち直しで足りる。init を要するのは台帳の異常だけであり、",
    "  その init は成果物ごと消すため、残っている *.md を退避してから実行する",
  ].join("\n");
}

/** 母集団の欠落として止める。exit 2 は「検査が結論を持たない」であって「不着」ではない。 */
function failPopulation(message) {
  console.error(`${message}\n\n${usage()}`);
  process.exit(2);
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.error) failPopulation(args.error);

  const dir = path.resolve(process.cwd(), LEDGER_DIR);

  if (args.command === "init") {
    // validateSlugs は init の引数に対して行う。verify では argv に母集団が無いため、
    // ここで無条件に呼ぶと verify が「スラグが 0 件」で止まり、台帳へ到達しない。
    const errors = validateSlugs(args.slugs);
    if (errors.length > 0) failPopulation(errors.map((e) => `- ${e}`).join("\n"));

    fs.rmSync(dir, { recursive: true, force: true });
    fs.mkdirSync(path.dirname(dir), { recursive: true });
    // recursive を付けない。削除が silent fail していれば EEXIST で止まる（唯一の観測点）。
    fs.mkdirSync(dir);
    writeLedger(dir, args.slugs);
    console.log(`台帳ディレクトリを新鮮化し、母集団を ${LEDGER_FILE} へ書いた: ${dir}`);
    console.log("");
    console.log("各スカウトへ渡す絶対パス（返り値に依存させず、このパスへ書かせる）:");
    for (const s of args.slugs) console.log(`- ${s}: ${path.join(dir, `${s}.md`)}`);
    return;
  }

  // verify: 母集団は台帳から読む。読んだ内容も validateSlugs へ通す——
  // 台帳は人もエージェントも書けるファイルであり、形の保証は読み戻し時にしか無い。
  const ledger = readLedger(dir);
  if (ledger.error) failPopulation(ledger.error);
  const errors = validateSlugs(ledger.slugs);
  if (errors.length > 0) {
    failPopulation([`台帳の中身が不正: ${path.join(dir, LEDGER_FILE)}`, ...errors.map((e) => `- ${e}`)].join("\n"));
  }

  let disk;
  try {
    disk = readLedgerDir(dir);
  } catch (e) {
    if (e.populationFailure) failPopulation(e.message);
    throw e;
  }

  const rows = classifyEntries(ledger.slugs, disk);
  console.log(formatReport(rows));
  if (rows.some((r) => r.status === MISSING)) process.exit(1);
}

if (process.argv[1] && process.argv[1].endsWith("plan-review-ledger.mjs")) main();
