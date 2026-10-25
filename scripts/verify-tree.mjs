// scripts/verify-tree.mjs
//
// `/implement` Step 3 の権威ある検証（`docs/build-commands.md` のカテゴリ A〜F）を、sha に固定した
// worktree で回す道具（issue #1298）。HEAD 合わせと「対象が無いから緑」の拒否を、指示文ではなく
// この道具の構造に置く。
//
//   npm run verify:tree -- run --sha <sha> [--categories A,C,D] [--also <npm-script>]... [--out <path>]
//   node <本体>/scripts/verify-tree.mjs align --sha <sha>      # 委譲先が自分の木の cwd で打つ
//   npm run verify:tree -- remove [--delegate-tree <path>]
//
// 終了コード: 0 = すべて緑 / 1 = 赤（run）・消せなかった（remove） / 2 = 環境の失敗・使用法の誤り。
// **2 を 1 と混ぜない**——「検査が赤かった」と「検査が走らなかった」は直し方が違う。
//
// - コマンドの正本は `docs/build-commands.md` である。各カテゴリの節の、最初の bash フェンスを読む
//   （写しを持たない）。読めたコマンドが 0 件なら exit 2（文書の改題・フェンスの書き換えで黙って空にしない）
// - E と F は要求に依らず常に回す（数秒で終わり、該当の判定に意味判断が混じるため）。D は回さず
//   `handoff` と書く（`smoke:manual` は人間専用）。B はコマンドを持たないので受け付けない
// - 道具の木は `.claude/worktrees/agent-verify-<branch>`。`agent-` 接頭辞は `clean:worktrees` が
//   取りこぼしを拾うため。node_modules の junction は張らない（`.claude/rules/safety-nets.md`）
// - 委譲先は harness の木（`isolation: "worktree"`）に居る。出力を木の外へ出すと木が「変更なし」になり、
//   停止時に harness が片づけ、再開した委譲先は本体で動く（#1298 で実測）。ゆえに報告は木の中の
//   `.delegate/`（どのコミットにも現れず、ignore もされないパス）に置かせ、`align` がその前提を確かめる
// - 出力するパスは `/` 区切り（pre-bash hook の `usesBackslashPath` を踏ませない）

import { execFileSync, spawnSync } from "node:child_process";
import { closeSync, existsSync, openSync, readFileSync, rmSync, writeFileSync, writeSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { sectionOf } from "./governance/lib.mjs";

const DOC = "docs/build-commands.md";
const RUNNABLE = ["A", "C", "E", "F"];
const ALWAYS = ["E", "F"];
const DELEGATE_DIR = ".delegate";

class EnvError extends Error {}

function git(cwd, args) {
  return execFileSync("git", args, { cwd, encoding: "utf8", stdio: "pipe" }).trim();
}

const posix = (p) => p.split(path.sep).join("/").replaceAll("\\", "/");

/**
 * カテゴリ `<X>` の節（`### <X>.` 見出しから次の同レベル見出しまで）の、最初の ` ```bash ` フェンスの
 * 各行から、行末コメントを落としたコマンドを返す。節が切り出せなければ `sectionOf` の findings を返す。
 * @returns {{ commands: string[], findings: object[] }}
 */
export function parseCategoryCommands(markdown, category) {
  const { body, findings } = sectionOf(markdown, new RegExp(`^### ${category}\\. `), {
    file: DOC,
    ending: "heading",
    by: "verify-tree",
  });
  if (body == null) return { commands: [], findings };
  const lines = body.split(/\r?\n/);
  const open = lines.findIndex((l) => /^```bash\s*$/.test(l));
  if (open < 0) return { commands: [], findings: [] };
  const commands = [];
  for (const line of lines.slice(open + 1)) {
    if (/^```\s*$/.test(line)) break;
    const command = line.replace(/\s+#.*$/, "").trim();
    if (command !== "" && !command.startsWith("#")) commands.push(command);
  }
  return { commands, findings: [] };
}

function parseArgs(argv) {
  const [sub, ...rest] = argv;
  const opts = { sub, also: [] };
  for (let i = 0; i < rest.length; i++) {
    const key = rest[i];
    const value = rest[i + 1];
    if (value === undefined) throw new EnvError(`${key} に値が無い`);
    i++;
    if (key === "--sha") opts.sha = value;
    else if (key === "--categories") opts.categories = value;
    else if (key === "--also") opts.also.push(value);
    else if (key === "--out") opts.out = value;
    else if (key === "--delegate-tree") opts.delegateTree = value;
    else throw new EnvError(`知らない引数: ${key}`);
  }
  return opts;
}

/** 本体の作業ツリーのルートと、現在のブランチから導いた道具の木のパス */
function mainContext() {
  const root = git(process.cwd(), ["rev-parse", "--show-toplevel"]);
  if (git(root, ["rev-parse", "--git-dir"]) !== git(root, ["rev-parse", "--git-common-dir"])) {
    throw new EnvError(`本体の作業ツリーから打つこと（ここは linked worktree: ${root}）`);
  }
  const branch = git(root, ["branch", "--show-current"]);
  if (branch === "") throw new EnvError("本体が detached HEAD——道具の木の名前をブランチから決められない");
  const tree = path.join(root, ".claude", "worktrees", `agent-verify-${branch.replace(/[^A-Za-z0-9._-]/g, "-")}`);
  return { root, branch, tree };
}

/** `.delegate/` の外の変更（追跡ファイルの変更と未追跡ファイル）。`-z` で読む——既定の porcelain は
 *  空白や非 ASCII を含むパスを引用符で囲むので、`?? .delegate/` の接頭辞に当たらなくなる */
function changesOutsideDelegate(cwd) {
  return execFileSync("git", ["status", "--porcelain", "-z", "--untracked-files=all"], { cwd, encoding: "utf8", stdio: "pipe" })
    .split("\0")
    .filter((e) => e !== "" && !e.startsWith(`?? ${DELEGATE_DIR}/`));
}

/** 登録された worktree（`git worktree list --porcelain` の各項目）。path は `path.resolve` 済み */
function worktrees(root) {
  return git(root, ["worktree", "list", "--porcelain"])
    .split(/\n\s*\n/)
    .filter((e) => e.startsWith("worktree "))
    .map((e) => ({
      path: path.resolve(e.split("\n")[0].slice("worktree ".length)),
      branch: /^branch refs\/heads\/(.+)$/m.exec(e)?.[1] ?? null,
      locked: /^locked\b/m.test(e),
    }));
}

function resolveSha(cwd, sha) {
  if (!sha) throw new EnvError("--sha が要る");
  try {
    return git(cwd, ["rev-parse", "--verify", `${sha}^{commit}`]);
  } catch {
    throw new EnvError(`sha が解決できない: ${sha}`);
  }
}

/** sha の lock と、本体に実際に入っている依存（npm が書く `node_modules/.package-lock.json`）の食い違い */
function dependencyWarnings(root, sha) {
  let lock;
  try {
    lock = JSON.parse(git(root, ["show", `${sha}:package-lock.json`])).packages ?? {};
  } catch {
    return []; // lock を持たない sha
  }
  const installedPath = path.join(root, "node_modules", ".package-lock.json");
  let installed;
  try {
    installed = JSON.parse(readFileSync(installedPath, "utf8")).packages ?? {};
  } catch {
    return [`${posix(installedPath)} が読めない——木は本体の node_modules を解決するので、依存の版を確かめられない`];
  }
  const drift = Object.entries(lock)
    .filter(([name, meta]) => name !== "" && !meta.optional && installed[name]?.version !== meta.version)
    .map(([name, meta]) => `${name}（lock ${meta.version} / 入っている ${installed[name]?.version ?? "無し"}）`);
  if (drift.length === 0) return [];
  return [`sha の package-lock.json と本体の node_modules が食い違う——木は本体の node_modules を解決するので、別の版を測りうる: ${drift.join(", ")}`];
}

function cmdRun(opts) {
  const { root, branch, tree } = mainContext();
  const sha = resolveSha(root, opts.sha);

  // ---- 木を作る前に、回す物と前提をすべて確かめる（回す物が無いのに cold の木を残さない）
  const requested = (opts.categories ?? "").split(",").map((c) => c.trim()).filter(Boolean);
  for (const c of requested) {
    if (c === "B") throw new EnvError("B はコマンドを持たない（docs/build-commands.md の B 節）。E が常に回る");
    if (![...RUNNABLE, "D"].includes(c)) throw new EnvError(`知らないカテゴリ: ${c}`);
  }
  const run = RUNNABLE.filter((c) => requested.includes(c) || ALWAYS.includes(c));
  const doc = git(root, ["show", `${sha}:${DOC}`]);
  const plan = [];
  const seen = new Set();
  for (const category of run) {
    const { commands, findings } = parseCategoryCommands(doc, category);
    if (commands.length === 0) {
      const why = findings.map((f) => f.message).join(" / ") || "節に bash フェンスが無い";
      throw new EnvError(`カテゴリ ${category} のコマンドが 0 件: ${why}`);
    }
    for (const command of commands) {
      if (seen.has(command)) continue;
      seen.add(command);
      plan.push({ category, command });
    }
  }
  const scripts = JSON.parse(git(root, ["show", `${sha}:package.json`])).scripts ?? {};
  for (const name of opts.also) {
    if (!(name in scripts)) throw new EnvError(`--also の npm script が package.json に無い: ${name}`);
    plan.push({ category: "also", command: `npm run ${name}` });
  }

  // ---- 本体の前提（コミットし忘れた変更を、対象ゼロのまま緑で返さない）
  const dirty = git(root, ["status", "--porcelain"]);
  if (dirty !== "") throw new EnvError(`本体の作業ツリーに未コミットの変更がある——sha に入っていない:\n${dirty}`);
  const head = git(root, ["rev-parse", "HEAD"]);
  if (head !== sha) throw new EnvError(`sha が本体の HEAD でない（HEAD ${head} / sha ${sha}）`);

  // ---- 木を作るか、再利用する
  git(root, ["worktree", "prune"]); // ディレクトリだけ消えた登録が add を阻まないように
  if (worktrees(root).some((w) => w.path === path.resolve(tree))) {
    const changed = git(tree, ["status", "--porcelain", "--untracked-files=no"]);
    if (changed !== "") throw new EnvError(`再利用する木に追跡ファイルの変更がある: ${posix(tree)}\n${changed}`);
    git(tree, ["checkout", "-q", "--detach", sha]);
  } else {
    if (existsSync(tree)) throw new EnvError(`木のパスに登録されていないディレクトリがある: ${posix(tree)}`);
    git(root, ["worktree", "add", "-q", "--detach", tree, sha]);
  }
  const treeHead = git(tree, ["rev-parse", "HEAD"]);
  if (treeHead !== sha) throw new EnvError(`木の HEAD が sha でない（HEAD ${treeHead} / sha ${sha}）`);

  // ---- 回す（警告は先に出しておく——回した後に throw して結果を失わないため）
  const warnings = dependencyWarnings(root, sha);
  const out = path.resolve(opts.out ?? `${tree}.summary.json`);
  const log = out.replace(/\.json$/, "") + ".log";
  const fd = openSync(log, "w");
  const results = [];
  try {
    for (const { category, command } of plan) {
      const started_at = new Date().toISOString();
      writeSync(fd, `\n===== [${category}] ${command}\n`);
      const r = spawnSync(command, { cwd: tree, shell: true, stdio: ["ignore", fd, fd] });
      const exit = r.status ?? 1;
      results.push({ category, command, exit, started_at, ended_at: new Date().toISOString() });
    }
  } finally {
    closeSync(fd);
  }
  const red = results.some((r) => r.exit !== 0);
  const summary = {
    sha,
    branch,
    worktree: posix(tree),
    categories_requested: requested,
    categories_run: run,
    also: opts.also,
    handoff: requested.includes("D") ? ["D"] : [],
    status: red ? "red" : "green",
    results,
    warnings,
    log: posix(log),
  };
  writeFileSync(out, JSON.stringify(summary, null, 2) + "\n");
  console.log(JSON.stringify({ sha, status: summary.status, summary: posix(out), log: posix(log) }));
  return red ? 1 : 0;
}

function cmdAlign(opts) {
  const tree = git(process.cwd(), ["rev-parse", "--show-toplevel"]);
  if (git(tree, ["rev-parse", "--git-dir"]) === git(tree, ["rev-parse", "--git-common-dir"])) {
    throw new EnvError(`linked worktree の中で打つこと（ここは本体の作業ツリー: ${posix(tree)}）`);
  }
  const sha = resolveSha(tree, opts.sha);
  // 追跡ファイルの変更と、`.delegate/` の外の未追跡ファイル（変異で足したまま残ったファイル——
  // vitest の include に当たれば次の巡のテストに混ざる）の両方を、巻き戻されていない変異として止める
  const leftovers = changesOutsideDelegate(tree);
  if (leftovers.length > 0) throw new EnvError(`変更が残っている（巻き戻されていない変異か）:\n${leftovers.join("\n")}`);
  if (git(tree, ["ls-tree", "--name-only", sha, DELEGATE_DIR]) !== "") {
    throw new EnvError(`${DELEGATE_DIR}/ が sha に追跡されている——報告の置き場が checkout と衝突する`);
  }
  try {
    git(tree, ["checkout", "-q", "--detach", sha]);
  } catch (e) {
    throw new EnvError(`checkout --detach ${sha} が失敗した: ${e.stderr ?? e.message}`);
  }
  const head = git(tree, ["rev-parse", "HEAD"]);
  if (head !== sha) throw new EnvError(`木の HEAD が sha でない（HEAD ${head} / sha ${sha}）`);
  // ignore されたパスは木を「変更あり」に保たない。checkout 後の sha の .gitignore で判定する
  const ignored = spawnSync("git", ["check-ignore", "-q", `${DELEGATE_DIR}/report.txt`], { cwd: tree });
  if (ignored.status === 0) throw new EnvError(`${DELEGATE_DIR}/ が ignore されている——報告を置いても木が片づけられる`);
  if (ignored.status !== 1) throw new EnvError(`git check-ignore が判定できなかった（status ${ignored.status}）——ignore されていないとは言えない`);
  console.log(JSON.stringify({ sha, worktree: posix(tree), delegate_dir: posix(path.join(tree, DELEGATE_DIR)) }));
  return 0;
}

function removeTree(root, tree) {
  try {
    git(root, ["worktree", "remove", tree]);
  } catch (e) {
    console.error(`消せなかった（dirty なら git が拒む）: ${posix(tree)}\n${e.stderr ?? e.message}`);
    return false;
  }
  if (existsSync(tree)) {
    console.error(`登録は消えたがディレクトリが残った（cwd がこの木の中にあるプロセスが握っている）: ${posix(tree)}`);
    return false;
  }
  console.log(`削除: ${posix(tree)}`);
  return true;
}

function cmdRemove(opts) {
  const { root, tree } = mainContext();
  let ok = true;
  if (existsSync(tree)) ok = removeTree(root, tree) && ok;
  else console.log(`道具の木は無い: ${posix(tree)}`);
  // 既定の置き場の要約とログ（gitignore 済み）。`--out` で外へ出したものは呼び出し側が持つ
  if (ok) for (const f of [`${tree}.summary.json`, `${tree}.summary.log`]) rmSync(f, { force: true });

  if (opts.delegateTree) {
    const target = path.resolve(opts.delegateTree);
    const entry = worktrees(root).find((w) => w.path === target);
    if (!entry) throw new EnvError(`登録された worktree でない: ${posix(target)}`);
    if (target === path.resolve(root) || target === path.resolve(tree)) throw new EnvError(`委譲先の木でない: ${posix(target)}`);
    const others = changesOutsideDelegate(target);
    if (others.length > 0) {
      console.error(`${DELEGATE_DIR}/ 以外の変更が残っている——消さない: ${posix(target)}\n${others.join("\n")}`);
      return 1;
    }
    // ロックされた木は `--force` なしの remove が拒む。先に `.delegate/` を消すと、報告だけ失って木が残る
    if (entry.locked) {
      console.error(`ロックされている（委譲先がまだ動いているか）——何も消さない: ${posix(target)}`);
      return 1;
    }
    // align が detach するので、porcelain にはブランチが載らない。harness の命名（`worktree-<木の名前>`）から導く
    const harnessBranch = `worktree-${path.basename(target)}`;
    const branch = entry.branch ?? (git(root, ["branch", "--list", harnessBranch]) !== "" ? harnessBranch : null);
    rmSync(path.join(target, DELEGATE_DIR), { recursive: true, force: true });
    if (removeTree(root, target)) {
      if (branch) {
        try {
          git(root, ["branch", "-d", branch]);
          console.log(`ブランチ削除: ${branch}`);
        } catch (e) {
          console.error(`ブランチを消せなかった: ${branch}\n${e.stderr ?? e.message}`);
          ok = false;
        }
      }
    } else ok = false;
  }
  return ok ? 0 : 1;
}

function main(argv) {
  try {
    const opts = parseArgs(argv);
    if (opts.sub === "run") return cmdRun(opts);
    if (opts.sub === "align") return cmdAlign(opts);
    if (opts.sub === "remove") return cmdRemove(opts);
    throw new EnvError(`サブコマンドは run / align / remove のいずれか（受け取った値: ${opts.sub}）`);
  } catch (e) {
    // EnvError 以外（git の想定外の失敗など）も 2 にする——未捕捉で落とすと node は exit 1 を返し、
    // 「検査が赤」と区別できなくなる。exit 1 は cmdRun / cmdRemove の明示的な経路からしか返さない
    console.error(`verify-tree: ${e instanceof EnvError ? e.message : (e.stderr ?? e.stack ?? e)}`);
    return 2;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = main(process.argv.slice(2));
}
