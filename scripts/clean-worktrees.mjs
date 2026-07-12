// scripts/clean-worktrees.mjs
//
// Agent 委譲で残った `.claude/worktrees/agent-*` の worktree と
// 対応する `worktree-agent-*` ブランチを掃除する（issue #517）。
//
//   npm run clean:worktrees            # クリーンなものだけ削除
//   npm run clean:worktrees -- --force # 未コミット変更・未マージも削除
//
// 成果保全の最終防衛は git 自身の拒否（`worktree remove` / `branch -d`）に
// 委ねる。自作の dirty 判定はスキップ表示のための事前チェックにすぎない。

import { execFileSync } from "node:child_process";
import path from "node:path";

const force = process.argv.includes("--force");

function git(args, opts = {}) {
  return execFileSync("git", args, { encoding: "utf8", stdio: "pipe", ...opts });
}

// 列挙は git 自身に問う（ファイルシステムの glob を信用しない）
const entries = [];
let current = null;
for (const line of git(["worktree", "list", "--porcelain"]).split("\n")) {
  if (line.startsWith("worktree ")) {
    current = { path: line.slice("worktree ".length), branch: null };
    entries.push(current);
  } else if (line.startsWith("branch refs/heads/") && current) {
    current.branch = line.slice("branch refs/heads/".length);
  }
}

const targets = entries.filter((e) => {
  const p = e.path.replaceAll("\\", "/");
  return p.includes("/.claude/worktrees/") && path.posix.basename(p).startsWith("agent-");
});

const skipped = [];
const failed = [];
let removed = 0;

for (const t of targets) {
  // dirty 判定。status が取れない（壊れた登録など）場合も保守側＝スキップに倒す
  let dirty = true;
  try {
    dirty = git(["-C", t.path, "status", "--porcelain"]).trim() !== "";
  } catch {
    dirty = true;
  }

  if (dirty && !force) {
    console.warn(`スキップ（未コミット変更あり）: ${t.path}`);
    skipped.push(t.path);
    continue;
  }

  try {
    git(["worktree", "remove", ...(force ? ["--force"] : []), t.path]);
    removed++;
    console.log(`削除: ${t.path}`);
  } catch (e) {
    console.warn(`worktree 削除に失敗: ${t.path}\n${e.stderr ?? e.message}`);
    failed.push(t.path);
  }
}

// ディレクトリだけ消えた迷子登録の回収
try {
  git(["worktree", "prune"]);
} catch {
  // prune の失敗は致命的でない
}

// ブランチ削除は worktree 削除後に一括で行う（checkout 中のブランチは消せない）。
// worktree に紐付かない worktree-agent-* をすべて対象にすることで、
// 未マージゆえに -d が拒否された過去実行の残り（squash マージのみの本リポジトリでは
// worktree 削除後にブランチだけ残るのが標準経路）も同じ規則で掃討される
const attached = new Set();
for (const line of git(["worktree", "list", "--porcelain"]).split("\n")) {
  if (line.startsWith("branch refs/heads/")) attached.add(line.slice("branch refs/heads/".length));
}
const orphans = git(["branch", "--list", "worktree-agent-*", "--format=%(refname:short)"])
  .trim()
  .split("\n")
  .filter((b) => b !== "" && !attached.has(b));
let orphanRemoved = 0;
for (const b of orphans) {
  try {
    git(["branch", force ? "-D" : "-d", b]);
    orphanRemoved++;
    console.log(`孤児ブランチ削除: ${b}`);
  } catch (e) {
    console.warn(`孤児ブランチ削除に失敗（未マージ？ --force で消せます）: ${b}\n${e.stderr ?? e.message}`);
    failed.push(b);
  }
}

if (targets.length === 0 && orphans.length === 0) {
  console.log("対象なし: .claude/worktrees/agent-* の worktree・worktree-agent-* ブランチはありません");
  process.exit(0);
}

console.log(
  `完了: worktree 削除 ${removed} / ブランチ掃討 ${orphanRemoved} / スキップ ${skipped.length} / 失敗 ${failed.length}`,
);
if (skipped.length > 0) {
  console.warn("スキップした worktree を消すには --force を付けて再実行してください");
}
process.exit(skipped.length + failed.length > 0 ? 1 : 0);
