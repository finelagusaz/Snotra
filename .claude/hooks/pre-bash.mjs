// PreToolUse (Bash|PowerShell) ガード。`gh pr create` が空 PR を作るのを防ぐ。
//
// 設計: payload 全体を grep するのではなく tool_input.command **だけ**を読み、
// 「コマンド位置」に現れる `gh pr create` を検出して、それが走る瞬間にコミットが
// remote に存在するかを判定する。真になる経路は 2 つある:
//   1. 静的 — 鎖の中で `git push` が `&&` で先行する（`&&` が前段の成功を保証する）
//   2. 動的 — upstream が設定済みで、未 push コミットが無い
//
// 契約: **exit 2 だけがツール呼び出しをブロックする。** exit 0 は許可であり、
// それ以外の非ゼロ（Node が未捕捉例外で返す 1 を含む）は「非ブロッキングエラー」で
// コマンドはそのまま実行される。ゆえに fail-closed とは
// 「既定の exitCode を 2 に置き、許可が確定した経路だけが 0 を書く」ことである。
//
// この領域は hook にしか見えない。`gh pr create` はリポジトリを触らないため git hook は
// 鳴らず、push もしないので pre-push も鳴らない。GitHub ruleset も空 PR を防げない。
//
// 受容する未対応リスク（いずれも「gh がコマンド位置に現れない」形。意図的迂回であり事故モードではない。
// `--no-verify` と同格に人間専用として扱う）:
//   - `sh -c 'gh pr create'` / `eval` / バッククォート / `$(...)` の内側
//   - ラッパ経由（`timeout 5 gh pr create` / `nohup` / `xargs gh pr create`）
// 検出を shell パーサ相当まで広げると、payload 全体 grep の誤爆（#482 の D2）を作り直すことになる。
//
// 詳細と実測の根拠は issue #482。

// 実測（#482 Phase 2 のフォールトインジェクション V2）: Windows の PowerShell tool の tool_name は
// 文字列 "PowerShell" である。matcher が取りこぼすと hook は気づかれないまま素通りするため
// （それがこの issue の D1 そのもの）、ここは推測ではなく実測値で固定している。

import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

/** この hook が管轄するツール。matcher が部分一致しても、ここが最終的な境界になる。 */
export const TARGET_TOOLS = Object.freeze(["Bash", "PowerShell"]);

/** git 1 コマンドあたりの上限。hook 全体の timeout に丸投げすると kill された沈黙が
 *  exit≠2 になり fail-open へ倒れる。自分で打ち切って block する。 */
const GIT_TIMEOUT_MS = 10_000;

// コマンド位置 = 文字列先頭、または区切り文字の直後。`&&` の 2 文字目も区切り文字なので
// `&& gh pr create` は一致する。引用の内側までは解釈しない（過剰検出は許容、過小検出は不可）。
const AT_CMD_POS = String.raw`(?:^|[;&|\n\r(){}])\s*`;

// フラグはサブコマンドの前後どちらにも来うる。値がスペース区切り（`--repo o/r`）でも読み飛ばす。
const FLAG = String.raw`(?:-{1,2}\S+(?:\s+[^-\s]\S*)?\s+)*`;

// `GH_TOKEN=x gh pr create` のような環境変数の前置もコマンド位置と見なす。
const ENV_PREFIX = String.raw`(?:[A-Za-z_]\w*=\S*\s+)*`;

// AT_CMD_POS は先頭の区切り文字を**消費する**。ゆえにコマンド本体をキャプチャし、
// tokenStart() で位置を復元する。match.index をそのまま使うと `&&` の片方の `&` しか
// 「push と gh の間」に入らず、`git push … && gh pr create` を誤って block する。
export const GH_PR_CREATE = new RegExp(
  `${AT_CMD_POS}${ENV_PREFIX}(gh\\s+${FLAG}pr\\s+${FLAG}create\\b)`,
);

// `git -C <tree> push` は意図的に一致させない。別ツリーへ push しても
// このツリーのコミットは remote に載らないため、安全な鎖ではない。
export const GIT_PUSH = new RegExp(`${AT_CMD_POS}(git\\s+(?:-\\S+\\s+)*push\\b)`);

// `git push --dry-run` は何も送信しない。動詞の名前ではなく意味を見る。
const DRY_RUN = /(?:^|\s)(?:--dry-run|-n)(?:\s|$)/;

// 鎖の途中で cwd が変わると、どのリポジトリに PR が作られるか判定できない。
export const CWD_CHANGE = new RegExp(`${AT_CMD_POS}((?:cd|pushd|popd|chdir|Set-Location)\\b)`, "i");

const REMEDY = "`git push -u origin HEAD` してから PR を作るか、`git push … && gh pr create` と `&&` で繋いでください。";

/** マッチしたコマンド本体（キャプチャ 1）の開始位置。区切り文字を含まない。不一致は -1。 */
export function tokenStart(re, command) {
  const m = re.exec(command);
  return m ? m.index + m[0].length - m[1].length : -1;
}

/**
 * `gh pr create` より前で `git push` が走り、両者の間の区切りが**すべて `&&`**か。
 *
 * 区切りが 1 つでも `;` / `||` / `|` / 改行なら、push が失敗しても `gh pr create` が
 * 走りうる。そのときコミットは remote に無く、空 PR になる。判定不能として block する。
 */
export function hasSafeChain(command) {
  const ghAt = tokenStart(GH_PR_CREATE, command);
  if (ghAt < 0) return false;

  const pushAt = tokenStart(GIT_PUSH, command);
  if (pushAt < 0) return false;

  // push セグメントの終端 = 次の区切り文字。フラグは区切りを跨がないのでここで閉じる。
  const rest = command.slice(pushAt).search(/[;&|\n\r]/);
  const pushEnd = rest < 0 ? command.length : pushAt + rest;
  if (pushEnd > ghAt) return false; // push が後ろにある

  // `git push --dry-run` は送信しない。安全な鎖ではない。
  if (DRY_RUN.test(command.slice(pushAt, pushEnd))) return false;

  const separators = command.slice(pushEnd, ghAt).match(/[;&|\n\r]+/g) ?? [];
  return separators.length >= 1 && separators.every((s) => s === "&&");
}

const ALLOW = Object.freeze({ action: "allow" });
const block = (reason) => ({ action: "block", reason });

/**
 * 判定の SSOT。`readGitState` を注入することで、分岐表全体が git 無しでテストできる。
 *
 * 「管轄外」と「判定不能」を混同しない。前者は allow（対象ツール以外・`gh pr create` 以外）、
 * 後者は block（見えないものは通さない）。
 */
export function decide(payload, readGitState) {
  const tool = payload?.tool_name;
  if (!TARGET_TOOLS.includes(tool)) return ALLOW; // 管轄外

  const command = payload?.tool_input?.command;
  if (typeof command !== "string") {
    return block(`${tool} の tool_input.command を読めませんでした。何が実行されるか判定できません。`);
  }

  const ghAt = tokenStart(GH_PR_CREATE, command);
  if (ghAt < 0) return ALLOW; // 管轄外

  const cwdAt = tokenStart(CWD_CHANGE, command);
  if (cwdAt >= 0 && cwdAt < ghAt) {
    return block("PR 作成の前に作業ディレクトリを変更しています。どのリポジトリに PR が作られるか判定できません。");
  }

  if (hasSafeChain(command)) return ALLOW; // push が `&&` で先行する

  const state = readGitState();
  if (!state.ok) return block(`git の状態を確認できませんでした（${state.reason}）。${REMEDY}`);
  if (!state.upstream) return block(`upstream が未設定です（またはリポジトリ外です）。${REMEDY}`);
  if (state.unpushed) return block(`未 push のコミットがあります。空 PR / \`Closes\` 誤 close を防ぐため止めました。${REMEDY}`);

  return ALLOW;
}

/** upstream の有無と未 push コミットの有無。判定できないときは `{ ok: false }`。 */
export function readGitState(cwd) {
  const git = (args) =>
    spawnSync("git", args, { cwd, encoding: "utf8", shell: false, timeout: GIT_TIMEOUT_MS });

  const upstream = git(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]);
  if (upstream.error) {
    return { ok: false, reason: `git rev-parse を実行できません: ${upstream.error.code ?? upstream.error.message}` };
  }
  // 非ゼロは「upstream 未設定」または「リポジトリ外」。どちらも block へ倒すため区別しない。
  if (upstream.status !== 0) return { ok: true, upstream: null, unpushed: false };

  const log = git(["log", "@{u}..HEAD", "--oneline"]);
  if (log.error) {
    return { ok: false, reason: `git log を実行できません: ${log.error.code ?? log.error.message}` };
  }
  if (log.status !== 0) return { ok: false, reason: `git log が exit ${log.status} で失敗しました` };

  return { ok: true, upstream: upstream.stdout.trim(), unpushed: log.stdout.trim().length > 0 };
}

/** stderr は exit 2 のときだけ Claude に届く。allow は無出力（沈黙 = 許可）。 */
function emitBlock(reason, toolName) {
  const who = toolName ? ` (tool_name=${toolName})` : "";
  process.stderr.write(`BLOCKED [pre-bash]${who}: ${reason}\n`);
  process.exitCode = 2;
}

function main() {
  let payload;
  try {
    payload = JSON.parse(readFileSync(0, "utf8"));
  } catch (e) {
    return emitBlock(`payload の JSON 解析に失敗しました: ${e.message}`);
  }

  // hook プロセスの cwd ではなく、セッションの作業ディレクトリで git を評価する。
  const cwd = typeof payload?.cwd === "string" && payload.cwd.length > 0 ? payload.cwd : process.cwd();

  const result = decide(payload, () => readGitState(cwd));
  if (result.action === "allow") {
    process.exitCode = 0;
    return;
  }
  emitBlock(result.reason, payload?.tool_name);
}

// テストが import しただけで stdin 読み取りが走らないようにする。
const invokedDirectly =
  Boolean(process.argv[1]) && import.meta.url === pathToFileURL(process.argv[1]).href;

if (invokedDirectly) {
  // 既定は block。allow が確定した経路だけが 0 を書く。
  // Node は未捕捉例外時に process.exitCode を無視して 1 で終了するため、catch は省略できない
  // （exit 1 = 非ブロッキング = コマンドが実行される = fail-open）。
  process.exitCode = 2;
  try {
    main();
  } catch (e) {
    emitBlock(`HOOK ERROR: ${e?.stack ?? String(e)}`);
  }
  // process.exit() は使わない。stdout/stderr がパイプのとき未 flush 出力を切り捨てる。
}
