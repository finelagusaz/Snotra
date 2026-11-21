// scripts/delete-merged-branch.mjs
//
// 委譲先などの使い捨てブランチを、中身が失われないときだけ消す（issue #1507）。
// `verify-tree.mjs` の `remove` と `clean-worktrees.mjs` が共有する。
//
// `git branch -d` の判定の基準は、打った作業ツリーの HEAD か upstream であり、`origin/main` は見ない。
// ローカルの `main` が古いと、`origin/main` へ入ったブランチも「not fully merged」で拒まれる。そこで、
// `-d` が拒んだら `refs/remotes/origin/main` の祖先かを確かめ、祖先なら `-D` で消す。fetch はしない
// （`origin/main` が古ければ「祖先でない」側、つまり残す側へ倒れる）。

import { execFileSync, spawnSync } from "node:child_process";

export const REASON_NOT_IN_ORIGIN = "origin/main に無いコミットがある";
export const REASON_UNDECIDABLE = "origin/main の祖先か判定できない";

// 短縮名の `origin/main` は、同じ名前のローカルブランチがあるとそちらへ解決される
const ORIGIN_MAIN = "refs/remotes/origin/main";

/**
 * @param {string} cwd git を打つ作業ツリー
 * @param {string} branch 消すブランチの短い名前
 * @returns {{ deleted: boolean, basis?: "-d" | "origin/main", reason?: string, detail?: string }}
 *   消さなかったときの `detail` は `git branch -d` の stderr
 */
export function deleteMergedBranch(cwd, branch) {
  const soft = spawnSync("git", ["branch", "-d", branch], { cwd, encoding: "utf8" });
  if (soft.status === 0) return { deleted: true, basis: "-d" };
  const detail = (soft.stderr || soft.error?.message || "").trim();

  // 終了コード: 0 = 祖先 / 1 = 祖先でない / それ以外（128: 参照を解決できない など）= 判定できない。
  // 1 とそれ以外を混ぜない——「origin/main が無い」を「固有のコミットがある」と報告しない
  const anc = spawnSync("git", ["merge-base", "--is-ancestor", `refs/heads/${branch}`, ORIGIN_MAIN], { cwd, encoding: "utf8" });
  if (anc.status === 0) {
    execFileSync("git", ["branch", "-D", branch], { cwd, encoding: "utf8", stdio: "pipe" });
    return { deleted: true, basis: "origin/main" };
  }
  return { deleted: false, reason: anc.status === 1 ? REASON_NOT_IN_ORIGIN : REASON_UNDECIDABLE, detail };
}
