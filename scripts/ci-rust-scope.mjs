// ci-rust-scope — rust-check の後続 step を走らせるかを決め、`$GITHUB_OUTPUT` へ `rust=true|false` を書く（#1508）。
// 読み手は `.github/workflows/ci.yml` の rust-check である。
// shebang を置かない（CRLF 化された shebang 行は vitest の transform を落とす。`merge-pr.mjs` と同じ理由）。
//
// 省くのは、PR の変更がすべて `.md` のときだけである。判定できない入力は走らせる。
// 走らせるべきファイル（`Cargo.toml`・`build.rs`・workflow など）を列挙しないのは、列挙から漏れたものが黙って省かれるからである。
import { spawnSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

/** 後続の step を走らせるか。0 件は走らせる（何も見ていないので、省く根拠が無い） */
export function needsRustCheck(paths) {
  if (paths.length === 0) return true;
  return !paths.every((p) => p.endsWith(".md"));
}

export function parseNameOnlyZ(stdout) {
  return stdout.split("\0").filter((p) => p !== "");
}

/**
 * @param {{ eventName: string | undefined, diff: () => ({ ok: true, stdout: string } | { ok: false, error: string }) }} args
 * @returns {{ rust: boolean, reason: string, paths: string[] }}
 */
export function decide({ eventName, diff }) {
  // main への push は判定しない。rust-check のキャッシュを main へ保存する run であり、マージ後の検査でもある
  if (eventName !== "pull_request") {
    return { rust: true, reason: `イベントが pull_request でない（${eventName ?? "未設定"}）`, paths: [] };
  }
  const r = diff();
  if (!r.ok) return { rust: true, reason: `差分を取れない（${r.error}）`, paths: [] };
  const paths = parseNameOnlyZ(r.stdout);
  if (needsRustCheck(paths)) {
    return { rust: true, reason: paths.length === 0 ? "差分が 0 件" : ".md 以外の変更を含む", paths };
  }
  return { rust: false, reason: "変更がすべて .md", paths };
}

/**
 * PR 全体の変更を返す。PR の checkout は merge commit（`refs/pull/N/merge`）なので、第 1 親（base の tip）と比べる
 * （`fetch-depth: 2` が要る）。直近の push 分だけを見ると、Rust を変えた PR に文書のコミットを足した run で省いてしまう。
 */
function gitDiff(cwd) {
  // HEAD が merge commit でなければ、`HEAD^1..HEAD` は最後の 1 コミットに縮む。差分は取れてしまうので、ここで止める
  const merge = spawnSync("git", ["rev-parse", "--verify", "-q", "HEAD^2"], { cwd, encoding: "utf8" });
  if (merge.error) return { ok: false, error: merge.error.message };
  if (merge.status !== 0) return { ok: false, error: "HEAD が merge commit でない（第 2 親が無い）" };
  // `--no-renames`: 既定の rename 検出は改名を新しいパス 1 行にまとめ、`.rs` → `.md` の改名元を落とす。
  // `-z`: 既定の `core.quotePath` は非 ASCII のパスを引用符で囲み、末尾が `.md` でなくなる
  const r = spawnSync("git", ["diff", "--name-only", "--no-renames", "-z", "HEAD^1", "HEAD"], { cwd, encoding: "utf8" });
  if (r.error) return { ok: false, error: r.error.message };
  if (r.status !== 0) return { ok: false, error: (r.stderr || `exit ${r.status}`).trim() };
  return { ok: true, stdout: r.stdout };
}

export function run(env, cwd) {
  if (!env.GITHUB_OUTPUT) {
    console.error("ci-rust-scope: GITHUB_OUTPUT が無い。結果を書く先が無いので止める");
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
