// ci-rust-scope — CI の rust-check で、後続の step を走らせるかを決める（#1508）。
// 結果は `$GITHUB_OUTPUT` へ `rust=true|false` の 1 行で書き、`.github/workflows/ci.yml` の rust-check が
// 後続の各 step の `if: steps.scope.outputs.rust != 'false'` で読む。
// shebang を置かない — CI の Windows checkout（autocrlf=true）で CRLF 化された shebang 行は
// vitest の transform を SyntaxError で落とす（`merge-pr.mjs` の冒頭と同じ理由。起動は常に `node scripts/...` 経由）。
//
// **省くのは「pull_request で、変更が 1 件以上あり、すべて `.md`」のときだけである**（許可リスト）。
// `Cargo.toml`・`build.rs`・`rust-toolchain.toml`・workflow・`scripts/**` などを列挙しないのは、列挙から漏れたものが
// 黙って省かれる形を作らないためである。push（main）・差分 0 件・差分を取れないときは、すべて走らせる側へ倒す。
// main の rust-check は rust-cache を保存する唯一の経路でもある（`ci.yml` の "Rust cache" step）。
//
// 母集団は PR 全体の変更である。`actions/checkout` が PR で取る merge commit（`refs/pull/N/merge`）と、
// その第 1 親（base の tip）を比べる（`fetch-depth: 2` が要る）。直近の push 分だけを見ると、Rust を変えた PR に
// 文書のコミットを足した run で省いてしまう。HEAD が merge commit でなければ（第 2 親が無ければ）走らせる。
// - `--no-renames`: 既定の rename 検出は改名を新しいパス 1 行にまとめ、`.rs` → `.md` の改名元を落とす
// - `-z`: 既定の `core.quotePath` は非 ASCII のパスを引用符で囲み、末尾が `.md` でなくなる
import { spawnSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

/** 後続の step を走らせるか。省く（false）のは、変更が 1 件以上あり、すべて小文字の `.md` で終わるときだけ */
export function needsRustCheck(paths) {
  if (paths.length === 0) return true;
  return !paths.every((p) => p.endsWith(".md"));
}

/** `git diff --name-only -z` の出力をパスの配列にする */
export function parseNameOnlyZ(stdout) {
  return stdout.split("\0").filter((p) => p !== "");
}

/**
 * @param {{ eventName: string | undefined, diff: () => ({ ok: true, stdout: string } | { ok: false, error: string }) }} args
 * @returns {{ rust: boolean, reason: string, paths: string[] }}
 */
export function decide({ eventName, diff }) {
  if (eventName !== "pull_request") {
    return { rust: true, reason: `イベントが pull_request でない（${eventName ?? "未設定"}）。push では常に全部走らせる`, paths: [] };
  }
  const r = diff();
  if (!r.ok) return { rust: true, reason: `差分を取れない（${r.error}）。走らせる側へ倒す`, paths: [] };
  const paths = parseNameOnlyZ(r.stdout);
  if (paths.length === 0) return { rust: true, reason: "差分が 0 件。走らせる側へ倒す", paths };
  if (needsRustCheck(paths)) return { rust: true, reason: ".md 以外の変更を含む", paths };
  return { rust: false, reason: "変更がすべて .md", paths };
}

function gitDiff(cwd) {
  // HEAD が merge commit であることを先に確かめる。checkout が PR の head（`ref: …head.sha`）を取る形に
  // 変わると、`HEAD^1..HEAD` は最後の 1 コミットだけになり、差分は取れたまま黙って母集団が縮む
  const merge = spawnSync("git", ["rev-parse", "--verify", "-q", "HEAD^2"], { cwd, encoding: "utf8" });
  if (merge.error) return { ok: false, error: merge.error.message };
  if (merge.status !== 0) return { ok: false, error: "HEAD が merge commit でない（第 2 親が無い）" };
  const r = spawnSync("git", ["diff", "--name-only", "--no-renames", "-z", "HEAD^1", "HEAD"], { cwd, encoding: "utf8" });
  if (r.error) return { ok: false, error: r.error.message };
  if (r.status !== 0) return { ok: false, error: (r.stderr || `exit ${r.status}`).trim() };
  return { ok: true, stdout: r.stdout };
}

export function run(env, cwd) {
  if (!env.GITHUB_OUTPUT) {
    console.error("ci-rust-scope: GITHUB_OUTPUT が無い。結果を書く先が無いので止める（黙って省かない）");
    return 2;
  }
  const d = decide({ eventName: env.GITHUB_EVENT_NAME, diff: () => gitDiff(cwd) });
  console.log(`ci-rust-scope: rust=${d.rust} — ${d.reason}`);
  if (d.paths.length > 0) console.log(`変更 ${d.paths.length} 件:\n${d.paths.map((p) => `- ${p}`).join("\n")}`);
  appendFileSync(env.GITHUB_OUTPUT, `rust=${d.rust}\n`);
  return 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = run(process.env, process.cwd());
}
