// merge:pr — PR を squash マージし、閉じる issue をマージの前後で照合する道具（#1312）。
// 手順の読み方（終了コードごとの次の一手）は `/merge-pr` が持ち、なぜこの形かは
// `docs/adr/ADR-squash-merge-issue-autoclose.md` が持つ。
// shebang を置かない — CI の Windows checkout（autocrlf=true）で CRLF 化された shebang 行は
// vitest の transform を SyntaxError で落とす（PR #592 で実測。起動は常に `node scripts/...` 経由）。
//
//   npm run merge:pr -- <PR> --closes <N[,N...]|owner/repo#N|none> --body-file <path>
//
// 終了コード: 0 = マージ済みで事後検証もすべて緑 / 1 = マージ前に止めた（マージしていない）
//   / 2 = 走らなかった（使用法・環境・マージ前の gh/git の失敗。マージしていない）
//   / 3 = マージを試みたあと、事後検証が赤か、確かめられなかった（人の判断が要る）。
// **3 を 0〜2 と混ぜない**——「マージしたか」を終了コードだけで読めるようにするため。
//
// 構造で消している落とし穴（手順で守っていた頃の経緯は ADR の追記）:
// - 件名（`--subject`）・`--auto`・`--delete-branch` を渡す口を持たない
// - マージ前に `closingIssuesReferences` を宣言（`--closes`）と完全一致で照合する。本文は直さない（人へ返す）
// - マージ後の検索は ISO 8601 の `mergedAt` だけを渡し、0 件を「該当なし」として受け取る経路を持たない
// - 道具の要約の照合は hook と共有する（`.claude/hooks/pre-bash.mjs` の `verifiedSummaryState`）
// - gh / git は `shell: false` で呼ぶ（`closed:>=` の `>` を shell に渡さない）
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { mainRoot, summaryPathFor } from "./verify-tree.mjs";
import { verifiedSummaryState } from "../.claude/hooks/pre-bash.mjs";

export const USAGE = `使い方: npm run merge:pr -- <PR> --closes <N[,N...]|owner/repo#N|none> --body-file <path>
  --closes     このマージで閉じる issue の宣言。閉じないなら none（必須）
  --body-file  squash commit の本文（必須）。closing keyword を書かない
  件名は PR タイトルから組まれる（変えるなら gh pr edit <PR> --title）。--subject / --auto / --delete-branch は受け付けない
  PowerShell からは npm run を経由せず node scripts/merge-pr.mjs ... と打つ（npm.ps1 が最初の -- を食う）`;

export class UsageError extends Error {}

/** 事後の検索の上限。これに達したら打ち切りと区別できないので、検算不能として止める */
export const SEARCH_LIMIT = 200;

const REF = /^(?:[\w.-]+\/[\w.-]+)?#\d+$/;

export function parseArgs(argv) {
  const out = { pr: null, closes: null, bodyFile: null };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--closes" || a === "--body-file") {
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

// 9 形（close / fix / resolve とその -s / -d 形）。コロンの有無・他リポジトリ・issue URL を含め、
// 大文字小文字を問わない。過剰検出の側へ倒す（拾い損ねると一覧に現れないまま閉じる）
const KEYWORD = String.raw`\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\b`;
const TARGET = String.raw`(?:[\w.-]+\/[\w.-]+#\d+|#\d+|https?:\/\/github\.com\/[\w.-]+\/[\w.-]+\/issues\/\d+)`;
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
/** 宣言した issue が事後の検索に現れるまでの再試行（索引の遅延）。間隔はミリ秒 */
const SEARCH_RETRIES = [5_000, 10_000];

const realDeps = {
  exec: (cmd, args, cwd) => spawnSync(cmd, args, { cwd, encoding: "utf8", shell: false }),
  readFile: (p) => readFileSync(p, "utf8"),
  mainRoot,
  verify: (root, branch, target) => verifiedSummaryState({ root, branch, target, summaryPathFor }),
  sleep: (ms) => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms),
  out: (s) => console.log(s),
  err: (s) => console.error(s),
};

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

/**
 * 道具の本体。終了コードを返す。`deps` はテストが gh / git / 要約の照合を差し替えるための口
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

  // ---- マージ前: 読みをすべて終えてから書く ----
  let pr, repo, root;
  try {
    root = deps.mainRoot(cwd);
    const bodyPath = path.resolve(cwd, opts.bodyFile);
    const body = deps.readFile(bodyPath);
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

    // head の commit を手元に揃えてから、道具の要約を照合する（#1304）
    if (deps.exec("git", ["cat-file", "-e", `${pr.headRefOid}^{commit}`], root).status !== 0) {
      call(deps, root, "git", ["fetch", "-q", "origin", `refs/pull/${opts.pr}/head`], EnvError);
      call(deps, root, "git", ["cat-file", "-e", `${pr.headRefOid}^{commit}`], EnvError);
    }
    const verified = deps.verify(root, pr.headRefName, pr.headRefOid);
    if (!verified.ok) {
      refusals.push(
        `道具の要約の照合に通らない: ${verified.reason}\n  復帰: gh pr checkout ${opts.pr} → npm run verify:tree -- run --sha HEAD --categories <該当カテゴリ> → この道具を再実行`,
      );
    }

    if (refusals.length > 0) {
      deps.err(`merge-pr: マージしていない（#${opts.pr}）。止めた理由 ${refusals.length} 件:\n${refusals.map((r) => `- ${r}`).join("\n")}`);
      return 1;
    }

    // ---- マージ: 確認した head にだけ当てる ----
    const merged = deps.exec(
      "gh",
      ["pr", "merge", opts.pr, "--squash", "--body-file", bodyPath, "--match-head-commit", pr.headRefOid],
      root,
    );
    if (merged.status !== 0) deps.err(`merge-pr: gh pr merge が exit ${merged.status} を返した。成否は PR の状態で確かめる\n${merged.stderr ?? ""}`);
  } catch (e) {
    // EnvError 以外（verify-tree の linked worktree の拒否・JSON の破損など）も 2 にする。ここはすべてマージより前
    deps.err(`merge-pr: マージしていない（#${opts.pr}）。${e instanceof EnvError ? e.message : (e.message ?? e)}`);
    return 2;
  }

  // ---- マージ後: ここから先の失敗はすべて 3 ----
  let mergedAt;
  try {
    const state = json(deps, root, ["pr", "view", opts.pr, "--json", "state,mergedAt"], PostError);
    if (state.state !== "MERGED") {
      deps.err(`merge-pr: マージされていない（#${opts.pr} は ${state.state}）。上の gh の出力を読み、原因を直して再実行する`);
      return 1;
    }
    mergedAt = state.mergedAt;
  } catch (e) {
    deps.err(`merge-pr: マージの成否を確かめられない（#${opts.pr}）。gh pr view ${opts.pr} で状態を見る\n${e.message}`);
    return 3;
  }

  const findings = [];
  try {
    if (!isIsoTimestamp(mergedAt)) throw new PostError(`mergedAt が ISO 8601 でない（${JSON.stringify(mergedAt)}）。検索に渡せない`);

    const declared = opts.closes.map((r) => normalizeRef(r, repo));
    // (1) 宣言した issue は番号で引く（検索の索引に依らない）
    for (const ref of declared) {
      const [owner_repo, n] = ref.split("#");
      const s = json(deps, root, ["issue", "view", n, "-R", owner_repo, "--json", "state"], PostError);
      if (s.state !== "CLOSED") findings.push(`宣言した ${ref} が閉じていない（${s.state}）`);
    }

    // 検算: 同じ時刻の形で、当の PR が merged の検索に現れること
    const prs = json(deps, root, ["pr", "list", "--state", "merged", "--search", `merged:>=${mergedAt}`, "--json", "number", "-L", String(SEARCH_LIMIT)], PostError);
    if (!prs.some((p) => String(p.number) === opts.pr)) {
      throw new PostError(`merged:>=${mergedAt} の検索に #${opts.pr} が現れない。検索が効いていないので、閉じた issue の一覧を信用できない`);
    }

    // (3) マージ以降に閉じた issue。宣言した同リポジトリの issue が現れるまで待つ（索引の遅延）
    const local = declared.filter((r) => r.startsWith(`${repo}#`)).map((r) => r.split("#")[1]);
    let closed;
    for (let attempt = 0; ; attempt++) {
      closed = json(deps, root, ["issue", "list", "--state", "closed", "--search", `closed:>=${mergedAt}`, "--json", "number", "-L", String(SEARCH_LIMIT)], PostError);
      if (closed.length >= SEARCH_LIMIT) throw new PostError(`closed:>=${mergedAt} の検索が上限 ${SEARCH_LIMIT} 件に達した。打ち切りと区別できない`);
      const seen = new Set(closed.map((i) => String(i.number)));
      const unseen = local.filter((n) => !seen.has(n));
      if (unseen.length === 0) break;
      if (attempt >= SEARCH_RETRIES.length) {
        throw new PostError(`宣言した issue（${unseen.map((n) => `#${n}`).join(", ")}）が closed の検索に現れない。索引が未反映で、検索を信用できない`);
      }
      deps.sleep(SEARCH_RETRIES[attempt]);
    }
    const undeclared = closed.map((i) => String(i.number)).filter((n) => !local.includes(n));
    if (undeclared.length > 0) {
      findings.push(
        `宣言していない issue がマージ以降に閉じた: ${undeclared.map((n) => `#${n}`).join(", ")}。並行する作業が閉じた可能性もあるので、各 issue の timeline で閉じた主体を確かめ、このマージが閉じたなら gh issue reopen <N>`,
      );
    }
  } catch (e) {
    // 想定外の例外も 3 にする——未捕捉で落とすと node は exit 1（「マージしていない」）を返す
    findings.push(`事後検証を確かめられない: ${e instanceof PostError ? e.message : (e.stack ?? e)}`);
  }

  if (findings.length > 0) {
    deps.err(`merge-pr: マージ済み（#${opts.pr}・${mergedAt}）。ただし要確認 ${findings.length} 件:\n${findings.map((f) => `- ${f}`).join("\n")}`);
    return 3;
  }
  deps.out(`merge-pr: マージ済み（#${opts.pr}・${mergedAt}）。閉じた issue: ${opts.closes.length > 0 ? opts.closes.map((r) => normalizeRef(r, repo)).join(", ") : "なし"}（宣言どおり）`);
  return 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = run(process.argv.slice(2), process.cwd());
}
