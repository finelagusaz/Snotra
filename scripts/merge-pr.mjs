// merge:pr — PR を squash マージし、閉じる issue をマージの前後で照合するツール（#1312）。
// 手順の読み方（終了コードごとの次の一手）は `/merge-pr` が持ち、なぜこの形かは
// `docs/adr/ADR-squash-merge-issue-autoclose.md` が持つ。
// shebang を置かない — CI の Windows checkout（autocrlf=true）で CRLF 化された shebang 行は
// vitest の transform を SyntaxError で落とす（PR #592 で実測。起動は常に `node scripts/...` 経由）。
//
//   npm run merge:pr -- <PR> --closes <N[,N...]|owner/repo#N|none> --body-file <path> [--dry-run]
//
// 終了コード: 0 = マージ済みで事後検証もすべて緑（--dry-run では、マージ前の検査がすべて通った） / 1 = マージ前に止めた（マージしていない）
//   / 2 = 走らなかった（使用法・環境・マージ前の gh/git の失敗。マージしていない）
//   / 3 = マージを試みたあと、事後検証が赤か、確かめられなかった（人の判断が要る）。
// **3 を 0〜2 と混ぜない**——「マージしたか」を終了コードだけで読めるようにするため。
//
// 構造で消している落とし穴（手順で守っていた頃の経緯は ADR の追記）:
// - 件名（`--subject`）・`--auto`・`--delete-branch` を渡す口を持たない
// - マージ前に `closingIssuesReferences` を宣言（`--closes`）と完全一致で照合する。本文は直さない（人へ返す）
// - マージ後の検索は ISO 8601 の `mergedAt` だけを渡し、0 件を「該当なし」として受け取る経路を持たない
// - ツールの要約の照合は hook と共有する（`.claude/hooks/pre-bash.mjs` の `verifiedSummaryState`）
// - gh / git は `shell: false` で呼ぶ（`closed:>=` の `>` を shell に渡さない）
// - --dry-run は `gh pr merge` の手前で止める。マージ前の書き込みは `git fetch`（head の commit を手元へ取る。
//   GitHub 側は変えない）だけであり、照合に要るので dry-run でも行う。`--` を落とした
//   `npm run merge:pr <PR> --dry-run` では npm が `--dry-run` を自分の設定として食う（`npm_config_dry_run` だけが届く）
//   ので、それも dry-run として拾う——拾わないと本番になる
// - exit 0 の後に、そのブランチのツールの worktree と要約を片づける（#1359。どのスキルを経たかに依らず閉じるため）。
//   exit 3 では人が調べる材料として残す。片づけの失敗は終了コードを変えない
// - exit 0 の後に、本体のローカル main を origin/main へ fast-forward する（#1528。スラッシュコマンド・Skill ツール・hook は
//   本体の版を読むため）。本体に触ってよいかの判定は `fastForwardMain` が持つ。exit 3 では進めず、マージ済みの判定のときだけ手で打つコマンドを案内する
import { spawnSync } from "node:child_process";
import { lstatSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { discardBranchArtifacts, mainRoot, summaryPathFor } from "./verify-tree.mjs";
import { verifiedSummaryState } from "../.claude/hooks/pre-bash.mjs";

export const USAGE = `使い方: npm run merge:pr -- <PR> --closes <N[,N...]|owner/repo#N|none> --body-file <path>
  --closes     このマージで閉じる issue の宣言。閉じないなら none（必須）
  --body-file  squash commit の本文（必須）。closing keyword を書かない
  --dry-run    マージ前の検査だけ行い、通ればマージする内容を出して止める（gh pr merge を呼ばない）
  件名は PR タイトルから組まれる（変えるなら gh pr edit <PR> --title）。--subject / --auto / --delete-branch は受け付けない
  PowerShell からは -- を引用して npm run merge:pr '--' <PR> ... と打つ（素の -- は npm.ps1 の経路によって消える）`;

export class UsageError extends Error {}

/** 後処理の fetch の上限。認証待ちやネットワーク断で、マージ済みのツールをぶら下げないため */
export const FETCH_TIMEOUT_MS = 60_000;

/**
 * 本体のローカル `main` を `origin/main` へ fast-forward する（#1528）。スラッシュコマンド・Skill ツール・hook は
 * 本体の版を読むので、スキルや hook を変える PR のマージ後に本体が古いと、旧版が動く。`{ ok, notes }` を返し、投げない。
 *
 * 判定を `--ff-only` に任せない。`--ff-only` は detached HEAD や `origin/main` の祖先である別ブランチでも成功してそれを
 * 動かし、先行していても `Already up to date.` で成功し、無視されたファイルを黙って上書きする（どれも #1528 で実測）。
 * 止めたときは、理由を解消する操作を先に書く——`merge --ff-only` だけを案内すると、人が上の害を再現する
 */
export function fastForwardMain(root, mergeCommit, exec = gitExec) {
  // fetch を含める——判定 1・2 は fetch の手前で止まるので、本体の origin/main はマージ前のままで、
  // merge だけを打つと `Already up to date.` で終わる（#1528 の委譲レビューで実測）
  const manual = manualSyncCommand(root);
  const stop = (reason, todo) => ({ ok: false, notes: [`本体の main を進めなかった: ${reason}`, todo] });
  const retry = `時間を置いて、${manual}`;

  // symbolic-ref が detached を表すのは rc=1 だけである。128 や起動の失敗（null）は本体を読めないという別の理由
  const branch = exec(root, ["symbolic-ref", "--quiet", "--short", "HEAD"]);
  if (branch.status === 1) return stop("本体が detached HEAD である", `本体の作業を片づけて main へ戻してから、${manual}`);
  if (branch.status !== 0) return stop(`本体のブランチを読めない（${branch.stderr}）`, `本体（${root}）が git の作業ツリーとして読めるかを確かめてから、${manual}`);
  if (branch.stdout.trim() !== "main") return stop(`本体が main に居ない（${branch.stdout.trim()}）`, `本体の作業を片づけて main へ戻してから、${manual}`);

  const dirty = exec(root, ["status", "--porcelain", "--untracked-files=no"]);
  if (dirty.status !== 0) return stop(`本体の状態を読めない（${dirty.stderr}）`, manual);
  if (dirty.stdout.trim() !== "") return stop("本体の追跡ファイルに変更がある", `本体の変更をコミットか退避してから、${manual}`);

  const fetched = exec(root, ["fetch", "origin", "main"], { timeout: FETCH_TIMEOUT_MS });
  if (fetched.status !== 0) return stop(`git fetch origin main が失敗した（${fetched.stderr || "timeout"}）`, retry);

  if (!mergeCommit) return stop("マージ commit が取れなかった", retry);
  const contains = exec(root, ["merge-base", "--is-ancestor", mergeCommit, "origin/main"]);
  if (contains.status === 1) return stop("origin/main にまだマージが現れていない", retry);
  if (contains.status !== 0) return stop(`マージ commit を確かめられない（${contains.stderr}）`, retry);

  const before = exec(root, ["rev-parse", "HEAD"]).stdout.trim();
  const target = exec(root, ["rev-parse", "origin/main"]).stdout.trim();
  if (before !== target && exec(root, ["merge-base", "--is-ancestor", "origin/main", "HEAD"]).status === 0) {
    return stop("本体の main が origin/main より先にある（未 push のコミット）", `先行しているコミットを push するか捨てるかを決めてから、${manual}`);
  }

  // リネームの移動先も新しく追跡されるので `--no-renames`。非 ASCII のパスを引用させないので `-z`
  const added = exec(root, ["diff", "--no-renames", "--name-only", "-z", "--diff-filter=A", "HEAD", "origin/main"]);
  if (added.status !== 0) return stop(`マージで追跡されるパスを列挙できない（${added.stderr}）`, manual);
  const occupied = occupiedPaths(root, added.stdout.split("\0").filter((p) => p !== ""), exec);
  if (occupied === null) return stop("本体の追跡ファイルの一覧を読めない", manual);
  if (occupied.length > 0) {
    return stop(`マージで追跡されるパスに、追跡していないファイルがある（無視されたものを含む）: ${occupied.join(", ")}`, `名指したファイルを退避してから、${manual}`);
  }

  const merged = exec(root, ["merge", "--ff-only", "origin/main"]);
  if (merged.status !== 0) return stop(`git merge --ff-only が失敗した（${merged.stderr}）`, `git の出力に従って直してから、${manual}`);
  const after = exec(root, ["rev-parse", "HEAD"]).stdout.trim();
  if (after === before) return { ok: true, notes: ["本体の main は既に最新"] };
  return { ok: true, notes: [`本体の main を進めた: ${before.slice(0, 8)}..${after.slice(0, 8)}`] };
}

/**
 * 本体を手で origin/main へ進めるコマンド。fetch から打つ（本体の origin/main はマージ前のままでありうる）。
 * そのまま貼って打てる 1 行にする（`&&` は Git Bash でも PowerShell 7 でも効く）
 */
export function manualSyncCommand(root) {
  return `git -C "${root}" fetch origin main && git -C "${root}" merge --ff-only origin/main`;
}

/**
 * マージで新たに追跡されるパスのうち、本体で追跡していないものに塞がれているもの。パスそのものに加え、
 * 祖先がディレクトリでないもの（無視されたファイル `d` に対する `d/x.txt`）も数える——ff はそれを黙って消す。
 * HEAD で追跡しているものは大文字小文字を区別せずに除く（大文字小文字だけのリネームは、大文字小文字を区別しない
 * FS では移動先が「在る」と見える）。追跡の一覧を読めなければ null
 */
function occupiedPaths(root, added, exec) {
  const tracked = exec(root, ["ls-tree", "-r", "-z", "--name-only", "HEAD"]);
  if (tracked.status !== 0) return null;
  const trackedLower = new Set(tracked.stdout.split("\0").filter((p) => p !== "").map((p) => p.toLowerCase()));
  const blocked = new Set();
  for (const p of added) {
    const parts = p.split("/");
    for (let i = 1; i <= parts.length; i++) {
      const prefix = parts.slice(0, i).join("/");
      if (trackedLower.has(prefix.toLowerCase())) continue;
      // lstat で見る——stat はシンボリックリンクをたどるので、壊れたリンクを「無い」、ディレクトリへのリンクを
      // 「ディレクトリ」と見誤り、ff に黙って上書きさせる
      let st;
      try {
        st = lstatSync(path.join(root, prefix));
      } catch {
        break;
      }
      if (i === parts.length || !st.isDirectory()) {
        blocked.add(prefix);
        break;
      }
    }
  }
  return [...blocked];
}

/** `git -C <root> ...` を shell なしで呼び、終了コードで分岐できる形で返す（投げない） */
export function gitExec(root, args, { timeout } = {}) {
  const r = spawnSync("git", ["-C", root, ...args], { encoding: "utf8", shell: false, timeout });
  return { status: r.error ? null : r.status, stdout: r.stdout ?? "", stderr: `${r.error?.message ?? ""}${r.stderr ?? ""}`.trim() };
}

/** 事後の検索の上限。これに達したら打ち切りと区別できないので、検算不能として止める */
export const SEARCH_LIMIT = 200;

const REF = /^(?:[\w.-]+\/[\w.-]+)?#\d+$/;

export function parseArgs(argv) {
  const out = { pr: null, closes: null, bodyFile: null, dryRun: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--dry-run") {
      out.dryRun = true;
    } else if (a === "--closes" || a === "--body-file") {
      if (i + 1 >= argv.length) throw new UsageError(`${a} に値が無い`);
      const v = argv[++i];
      if (a === "--body-file") out.bodyFile = v;
      else if (v === "none") out.closes = [];
      else {
        out.closes = v.split(",").map((s) => (/^\d+$/.test(s) ? `#${s}` : s));
        const bad = out.closes.filter((s) => !REF.test(s));
        if (bad.length > 0) throw new UsageError(`--closes の形が違う: ${bad.join(", ")}`);
      }
    } else if (a.startsWith("-")) {
      throw new UsageError(`受け付けない引数: ${a}`);
    } else if (out.pr !== null) {
      throw new UsageError(`PR 番号は 1 つだけ（2 つ目: ${a}）`);
    } else if (/^\d+$/.test(a)) {
      out.pr = a;
    } else {
      throw new UsageError(`PR 番号の形でない: ${a}`);
    }
  }
  if (out.pr === null) throw new UsageError("PR 番号が無い");
  if (out.closes === null) throw new UsageError("--closes が無い（閉じないなら none）");
  if (out.bodyFile === null) throw new UsageError("--body-file が無い");
  return out;
}

// 9 形（close / fix / resolve とその -s / -d 形）。コロンの有無・他リポジトリ・issue URL・`GH-N` を含め、
// 大文字小文字を問わない。過剰検出の側を選ぶ（拾い損ねると一覧に現れないまま閉じる。`GH-N` が close に
// 効くかは確かめていないが、拾っても失うのは再実行 1 回である）
const KEYWORD = String.raw`\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\b`;
const TARGET = String.raw`(?:[\w.-]+\/[\w.-]+#\d+|#\d+|GH-\d+|https?:\/\/github\.com\/[\w.-]+\/[\w.-]+\/issues\/\d+)`;
const CLOSING = new RegExp(String.raw`${KEYWORD}\s*:?\s*${TARGET}`, "gi");
const TITLE_TYPE = new RegExp(String.raw`^\s*${KEYWORD}(?:\([^)]*\))?!?\s*:\s*${TARGET}`, "i");

/** 本文・タイトル中の closing keyword（GitHub が close に使う 9 形）の出現を返す */
export function findClosingKeywords(text) {
  return [...String(text).matchAll(CLOSING)].map((m) => m[0]);
}

/** タイトルが conventional commit の型名として closing keyword を使い、その直後に issue を置く形か */
export function titleClosesIssue(title) {
  return TITLE_TYPE.test(String(title));
}

/** `#N` / `owner/repo#N` を `owner/repo#N` へ正規化する */
export function normalizeRef(ref, repo) {
  return ref.startsWith("#") ? `${repo}${ref}` : ref;
}

/** 宣言と GitHub の一覧の差。どちらも正規化済みの `owner/repo#N` の配列 */
export function diffClosing(declared, actual) {
  const d = new Set(declared);
  const a = new Set(actual);
  return { extra: [...a].filter((x) => !d.has(x)), missing: [...d].filter((x) => !a.has(x)) };
}

/** gh の `--jq .mergedAt` が返す形だけを通す。崩れた時刻を渡すと GitHub の検索はエラーを出さず 0 件を返す */
export function isIsoTimestamp(s) {
  return typeof s === "string" && /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.test(s);
}

/** 走らなかった（exit 2）。マージ前にだけ投げる */
class EnvError extends Error {}

const SHA = /^[0-9a-f]{40}$/;
/** 事後検証の観測が揃うまでの待ち（close の反映と索引の遅延）。間隔はミリ秒。先頭はマージ直後の状態の引き直しにも使う */
const SEARCH_RETRIES = [5_000, 10_000, 15_000];

export const realDeps = {
  exec: (cmd, args, cwd) => spawnSync(cmd, args, { cwd, encoding: "utf8", shell: false }),
  readFile: (p) => readFileSync(p, "utf8"),
  mainRoot,
  verify: (root, branch, target) => verifiedSummaryState({ root, branch, target, summaryPathFor }),
  cleanup: discardBranchArtifacts,
  syncMain: fastForwardMain,
  sleep: (ms) => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms),
  out: (s) => console.log(s),
  err: (s) => console.error(s),
  env: process.env,
};

/** dry-run でマージする内容として見せる squash 本文の行数 */
const BODY_PREVIEW_LINES = 5;

/** gh / git を呼び、失敗なら `Fail` の型の例外にする（マージ前は EnvError、マージ後は PostError） */
function call(deps, cwd, cmd, args, Fail) {
  const r = deps.exec(cmd, args, cwd);
  if (r.error || r.status !== 0) {
    throw new Fail(`${cmd} ${args.join(" ")} が失敗した（${r.error?.message ?? `exit ${r.status}`}）\n${r.stderr ?? ""}`.trim());
  }
  return r.stdout;
}
const json = (deps, cwd, args, Fail) => JSON.parse(call(deps, cwd, "gh", args, Fail));

/** マージを試みたあとの「確かめられなかった」（exit 3） */
class PostError extends Error {}

/** catch した値を文字列にする。`null` / `undefined` を throw されても catch の中で TypeError を投げない */
const describe = (e, stack = false) => String((stack ? e?.stack : undefined) ?? e?.message ?? e);

/**
 * ツールの本体。終了コードを返す。`deps` はテストが gh / git / 要約の照合を差し替えるための口
 */
export function run(argv, cwd, deps = realDeps) {
  let opts;
  try {
    opts = parseArgs(argv);
  } catch (e) {
    if (!(e instanceof UsageError)) throw e;
    deps.err(`merge-pr: ${e.message}\n${USAGE}`);
    return 2;
  }

  const dryRun = opts.dryRun || deps.env?.npm_config_dry_run === "true";

  // ---- マージ前: 読みをすべて終えてから書く ----
  let pr, repo, root, body;
  const bodyPath = path.resolve(cwd, opts.bodyFile);
  try {
    root = deps.mainRoot(cwd);
    body = deps.readFile(bodyPath);
    repo = json(deps, root, ["repo", "view", "--json", "nameWithOwner"], EnvError).nameWithOwner;
    pr = json(
      deps,
      root,
      ["pr", "view", opts.pr, "--json", "number,state,mergeStateStatus,title,headRefName,headRefOid,closingIssuesReferences"],
      EnvError,
    );
    if (!SHA.test(String(pr.headRefOid))) throw new EnvError(`PR の head の sha の形が違う（${pr.headRefOid}）`);

    const refusals = [];
    if (pr.state !== "OPEN") refusals.push(`PR が OPEN でない（${pr.state}）`);
    else if (pr.mergeStateStatus !== "CLEAN") {
      refusals.push(`mergeStateStatus が CLEAN でない（${pr.mergeStateStatus}）。必須チェックの未通過・衝突・レビュー待ちを解消してから再実行する`);
    }

    const declared = opts.closes.map((r) => normalizeRef(r, repo));
    const actual = pr.closingIssuesReferences.map((i) => `${i.repository.owner.login}/${i.repository.name}#${i.number}`);
    const { extra, missing } = diffClosing(declared, actual);
    if (extra.length > 0) {
      refusals.push(
        `宣言に無い issue を PR 本文が閉じる: ${extra.join(", ")}。閉じないなら PR 本文から closing keyword を外して（gh pr edit ${opts.pr} --body-file <path>）再実行する。閉じるなら --closes へ足す`,
      );
    }
    if (missing.length > 0) {
      refusals.push(`宣言した issue を PR 本文が閉じない: ${missing.join(", ")}。宣言か PR 本文のどちらかを直す`);
    }

    if (titleClosesIssue(pr.title) || findClosingKeywords(pr.title).length > 0) {
      refusals.push(`PR タイトルが issue を閉じる形をしている（${pr.title}）。番号を末尾へ回すか括弧で包む（gh pr edit ${opts.pr} --title）`);
    }
    const inBody = findClosingKeywords(body);
    if (inBody.length > 0) {
      refusals.push(`--body-file に closing keyword がある（${inBody.join(" / ")}）。squash 本文の close は一覧に現れないまま効く`);
    }

    const stacked = json(deps, root, ["pr", "list", "--state", "open", "--base", pr.headRefName, "--json", "number", "-L", "100"], EnvError);
    if (stacked.length > 0) {
      refusals.push(
        `${pr.headRefName} を base にする open PR がある（${stacked.map((p) => `#${p.number}`).join(", ")}）。マージで head は自動で消え、それらは CLOSED になりうる。先にマージするか retarget する`,
      );
    }

    // head の commit を手元に揃えてから、ツールの要約を照合する（#1304）
    if (deps.exec("git", ["cat-file", "-e", `${pr.headRefOid}^{commit}`], root).status !== 0) {
      call(deps, root, "git", ["fetch", "-q", "origin", `refs/pull/${opts.pr}/head`], EnvError);
      call(deps, root, "git", ["cat-file", "-e", `${pr.headRefOid}^{commit}`], EnvError);
    }
    const verified = deps.verify(root, pr.headRefName, pr.headRefOid);
    if (!verified.ok) {
      refusals.push(
        `ツールの要約の照合に通らない: ${verified.reason}\n  復帰: gh pr checkout ${opts.pr} → npm run verify:tree -- run --sha HEAD --categories <該当カテゴリ> → このツールを再実行`,
      );
    }

    if (refusals.length > 0) {
      deps.err(`merge-pr: マージしていない（#${opts.pr}）。止めた理由 ${refusals.length} 件:\n${refusals.map((r) => `- ${r}`).join("\n")}`);
      return 1;
    }
  } catch (e) {
    // EnvError 以外（verify-tree が本体のルートを導けない・JSON の破損など）も 2 にする。この try はマージより前だけを囲む
    deps.err(`merge-pr: マージしていない（#${opts.pr}）。${describe(e)}`);
    return 2;
  }

  if (dryRun) {
    const closes = opts.closes.length > 0 ? opts.closes.map((r) => normalizeRef(r, repo)).join(", ") : "なし";
    const lines = body.replace(/\r\n/g, "\n").trimEnd().split("\n");
    const preview = lines.slice(0, BODY_PREVIEW_LINES).map((l) => `    | ${l}`).join("\n");
    const rest = lines.length > BODY_PREVIEW_LINES ? `\n    （ほか ${lines.length - BODY_PREVIEW_LINES} 行）` : "";
    deps.out(
      `merge-pr: dry-run——マージしていない（#${opts.pr}）。マージ前の検査はすべて通った。本番で打てばマージする内容:\n` +
        `- PR: #${opts.pr}（件名は PR タイトルから組まれる: ${pr.title}）\n` +
        `- head: ${pr.headRefOid}（${pr.headRefName}。本番はこの sha にだけ当てる）\n` +
        `- 閉じる issue: ${closes}（宣言と PR 本文の一覧が一致）\n` +
        `- squash 本文の先頭（${opts.bodyFile}）:\n${preview}${rest}`,
    );
    return 0;
  }

  // ---- マージ: 確認した head にだけ当てる。ここから先で 2 は返さない ----
  let mergedAt;
  let mergeCommit = null;
  let mergeStatus;
  let ghNote = "";
  try {
    const merged = deps.exec(
      "gh",
      ["pr", "merge", opts.pr, "--squash", "--body-file", bodyPath, "--match-head-commit", pr.headRefOid],
      root,
    );
    mergeStatus = merged.status;
    // gh の失敗は判定の行のあとへ添える——出力の最初の行は常に判定（マージ済み／していない／確かめられない）にする
    if (merged.status !== 0) ghNote = `\n（gh pr merge は exit ${merged.status} を返した）\n${merged.stderr ?? ""}`.trimEnd();

    let state = json(deps, root, ["pr", "view", opts.pr, "--json", "state,mergedAt,mergeCommit"], PostError);
    if (state.state !== "MERGED") {
      // gh が成功を返したのに MERGED でないのは反映の遅れでありうる——「マージしていない」と断定しない
      if (mergeStatus === 0) throw new PostError(`gh pr merge は成功を返したが、#${opts.pr} は ${state.state} である`);
      // gh が失敗を返しても反映が遅れているだけかもしれないので、1 度だけ待って引き直してから断定する
      deps.sleep(SEARCH_RETRIES[0]);
      state = json(deps, root, ["pr", "view", opts.pr, "--json", "state,mergedAt,mergeCommit"], PostError);
    }
    if (state.state !== "MERGED") {
      deps.err(`merge-pr: マージしていない（#${opts.pr} は ${state.state}）。gh の出力を読み、原因を直して再実行する${ghNote}`);
      return 1;
    }
    mergedAt = state.mergedAt;
    mergeCommit = state.mergeCommit?.oid ?? null;
  } catch (e) {
    deps.err(
      `merge-pr: マージの成否を確かめられない（#${opts.pr}）。gh pr view ${opts.pr} --json state,mergedAt で状態を見る（/merge-pr の「マージ後に要確認が出たとき（exit 3）」）\n${describe(e)}${ghNote}`,
    );
    return 3;
  }

  const findings = [];
  let observations = 0;
  let waited = 0;
  try {
    if (!isIsoTimestamp(mergedAt)) throw new PostError(`mergedAt が ISO 8601 でない（${JSON.stringify(mergedAt)}）。検索に渡せない`);

    const declared = opts.closes.map((r) => normalizeRef(r, repo));
    const local = declared.filter((r) => r.startsWith(`${repo}#`)).map((r) => r.split("#")[1]);

    // 1 回分の観測。3 段（宣言した issue の状態・検算・closed の検索）をすべて引く。マージと close の反映、
    // 検索の索引は秒単位で遅れるので、どの段も「揃うまで待つ」の対象にする（#1321）
    const observe = () => {
      // 宣言した issue は番号で引く（検索の索引に依らない）
      const notClosed = [];
      const openLocal = new Set();
      for (const ref of declared) {
        const [owner_repo, n] = ref.split("#");
        const s = json(deps, root, ["issue", "view", n, "-R", owner_repo, "--json", "state"], PostError);
        if (s.state !== "CLOSED") {
          notClosed.push(`${ref} が閉じていない（${s.state}）`);
          if (owner_repo === repo) openLocal.add(n);
        }
      }
      // 検算: 同じ時刻の形で、当の PR が merged の検索に現れること。通らなければ closed の検索を信用しない
      const prs = json(deps, root, ["pr", "list", "--state", "merged", "--search", `merged:>=${mergedAt}`, "--json", "number", "-L", String(SEARCH_LIMIT)], PostError);
      const verified = prs.some((p) => String(p.number) === opts.pr);
      const closed = json(deps, root, ["issue", "list", "--state", "closed", "--search", `closed:>=${mergedAt}`, "--json", "number", "-L", String(SEARCH_LIMIT)], PostError);
      // 上限への到達は待っても解消しないので、その場で打ち切る
      if (closed.length >= SEARCH_LIMIT) throw new PostError(`closed:>=${mergedAt} の検索が上限 ${SEARCH_LIMIT} 件に達した。打ち切りと区別できない`);
      const numbers = closed.map((i) => String(i.number));
      // 閉じていないと番号で確かめた issue は、検索に現れなくて当然である。索引の未反映には数えない
      const unseen = local.filter((n) => !numbers.includes(n) && !openLocal.has(n));
      return { notClosed, verified, unseen, numbers };
    };

    // 揃うまで観測し直す。宣言外の close は待っても消えない所見なので、揃ったかの判定には入れない
    let obs;
    for (let attempt = 0; ; attempt++) {
      observations = attempt + 1;
      obs = observe();
      if (obs.notClosed.length === 0 && obs.verified && obs.unseen.length === 0) break;
      if (attempt >= SEARCH_RETRIES.length) break;
      deps.sleep(SEARCH_RETRIES[attempt]);
      waited += SEARCH_RETRIES[attempt];
    }

    for (const r of obs.notClosed) findings.push(`宣言した ${r}`);
    // 依存の順を保つ: 検算が通らない観測の closed 一覧からは、何も結論しない
    if (!obs.verified) {
      findings.push(`検算が通らない: merged:>=${mergedAt} の検索に #${opts.pr} が現れない。検索が効いていないので、閉じた issue の一覧を信用できない`);
    } else if (obs.unseen.length > 0) {
      findings.push(`宣言した issue（${obs.unseen.map((n) => `#${n}`).join(", ")}）が closed の検索に現れない。索引が未反映で、検索を信用できない`);
    } else {
      const undeclared = obs.numbers.filter((n) => !local.includes(n));
      if (undeclared.length > 0) {
        findings.push(
          `宣言していない issue がマージ以降に閉じた: ${undeclared.map((n) => `#${n}`).join(", ")}。並行する作業が閉じた可能性もあるので、各 issue の timeline で閉じた主体を確かめ、このマージが閉じたなら gh issue reopen <N>`,
        );
      }
    }
  } catch (e) {
    // 想定外の例外も 3 にする——未捕捉で落とすと node は exit 1（「マージしていない」）を返す
    findings.push(`事後検証を確かめられない: ${e instanceof PostError ? e.message : describe(e, true)}`);
  }

  // 観測回数と待ちの合計は、索引の反映の遅れを次の実マージから溜めるために常に出す（#1321）
  const tally = `\n（事後検証: 観測 ${observations} 回・待ち計 ${waited / 1000} 秒）`;
  if (findings.length > 0) {
    // 本体の main は進めない（人が調べる場面で本体を動かさない）。ただしマージは済んでいるので、旧版が動く害は残る（#1528）
    const syncHint = `\n本体の main は進めていない。本体が main に居て追跡ファイルに変更が無いことを確かめてから、${manualSyncCommand(root)} を打つ`;
    deps.err(`merge-pr: マージ済み（#${opts.pr}・${mergedAt}）。ただし要確認 ${findings.length} 件:\n${findings.map((f) => `- ${f}`).join("\n")}${tally}${ghNote}${syncHint}`);
    return 3;
  }
  deps.out(`merge-pr: マージ済み（#${opts.pr}・${mergedAt}）。閉じた issue: ${opts.closes.length > 0 ? opts.closes.map((r) => normalizeRef(r, repo)).join(", ") : "なし"}（宣言どおり）${tally}${ghNote}`);

  // マージ済みのブランチのツールの worktree と要約は、もう誰も読まない（#1359）。片づけるのは exit 0 のときだけである——
  // exit 3 では人が調べる材料として残す。片づけの失敗は終了コードを変えない（マージは済んでいる）
  try {
    const cleaned = deps.cleanup(root, pr.headRefName);
    if (cleaned.ok) deps.out(`ツールの worktree と要約の片づけ:\n${cleaned.notes.map((n) => `- ${n}`).join("\n")}`);
    else deps.err(`ツールの worktree と要約を片づけられなかった（マージは済んでいる）:\n${cleaned.notes.map((n) => `- ${n}`).join("\n")}`);
  } catch (e) {
    deps.err(`ツールの worktree と要約を片づけられなかった（マージは済んでいる）: ${describe(e)}`);
  }

  // スラッシュコマンド・Skill ツール・hook は本体の版を読むので、本体のローカル main を進める（#1528）。
  // 進められなくても終了コードを変えない（マージは済んでいる）
  try {
    const synced = deps.syncMain(root, mergeCommit);
    (synced.ok ? deps.out : deps.err)(synced.notes.join("\n"));
  } catch (e) {
    deps.err(`本体の main を進めなかった（マージは済んでいる）: ${describe(e)}`);
  }
  return 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = run(process.argv.slice(2), process.cwd());
}
