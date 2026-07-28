// PostToolUse (Edit|Write) ディスパッチャ。
//
// 設計: payload 全体を grep するのではなく tool_input.file_path を読み、
// そのファイルが属するツリー（最近接の .git）で、そのファイルに対応する
// 検査だけを走らせ、結果を JSON エンベロープでエージェントへ届ける。
//
// 契約: **検査が走ったなら、沈黙は合格を意味する。** 検出は exit code で行い、
// 成功した検査は何も出力しない。ゆえに「沈黙しうる経路」はすべて塞がねばならない
// （タイムアウト・起動失敗・出力溢れは、いずれも必ず報告する）。
//
// この契約は「検査が割り当てられたファイル」についてのみ成り立つ。割り当てが
// 無いファイル（*.md 等）の沈黙は「何も走らなかった」であり、合格ではない。
// 割り当ての SSOT は selectChecks である（#497）。
//
// 詳細と実測の根拠は issue #471。

import { existsSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import path from "node:path";

// 現行のパイプ（`cargo clippy ... | head -20`）には出力量の上限が無い。
// spawnSync の既定 1MB のままだと ENOBUFS で診断が失われる（I11）。
const MAX_BUFFER = 32 * 1024 * 1024;

// 検査 1 本あたりの上限。settings.json の hook timeout (900s) より内側に置き、
// 打ち切られてもスクリプト自身が「タイムアウトした」と報告できるようにする。
// hook timeout に丸投げするとプロセスごと kill され、沈黙が「合格」と読まれる。
const PER_CHECK_TIMEOUT_MS = 300_000;

// 検査ごとの出力予算。現行 hook の head/tail と行数を一字一句保存する（I10）。
// 予算は検査が**失敗したときだけ**読まれる。エントリ漏れは全検査が緑の間は沈黙し、
// 最初の失敗で hook 自体を TypeError で落とす（診断が届かなくなる）。
// selectChecks が発行しうる id との完全性は post-edit.test.mjs のカナリアが固定する。
export const BUDGETS = {
  clippy: { lines: 20, from: "head" },
  "core-test": { lines: 5, from: "tail" },
  "egui-runtime-test": { lines: 8, from: "tail" },
  "settings-test": { lines: 8, from: "tail" },
  "tauri-test": { lines: 8, from: "tail" },
  "cargo-check": { lines: 20, from: "head" },
  "hook-selftest": { lines: 30, from: "head" },
  "githooks-selftest": { lines: 30, from: "head" },
};

// cargo が stderr に流す進捗行。worktree は target/ を持たないため cold build になり、
// これを落とさないと head 20 が進捗行だけで埋まる（I14）。
const PROGRESS_LINE =
  /^\s*(Compiling|Checking|Finished|Blocking|Updating|Downloading|Downloaded|Locking|Adding|Removing|Fresh|Installing|Running)\b/;

// TS 型検査は #532 SU7 のフロント撤去で消滅した（tsconfig ごと削除）。TS_LIKE は
// 「.ts を編集したのに何も走らない」ことを明示する情報行（I16）のためだけに残る。
const TS_LIKE = /\.(m|c)?tsx?$/;

// cargo のワークスペース定義。basename でアンカーする — 過小検出は沈黙（false green）
// になるが、過剰検出は cargo check が走るだけで無害（fail-closed 方向）。
const CARGO_MANIFEST = /(^|\/)Cargo\.toml$/;

// 「検査の定義を変えるファイル」。編集すると hook-selftest が走り、その中の
// カナリアが「定義と selectChecks の前提が一致しているか」を検証する。
// カナリアの無いファイルをここに足してはならない — 何も検証しない緑になる。
//
// Cargo.toml は**ルートのみ**。crate 追加は必ず members を編集するのでそこで捕まる。
// メンバーの Cargo.toml（パッケージ名の SSOT）は要らない — 改名すれば
// `cargo test -p snotra` が "package did not match" で落ちる。沈黙する経路だけを守る。
const CHECK_DEFINITION = new Set([
  ".claude/settings.json",
  "package.json",
  "vitest.config.ts",
  "Cargo.toml",
]);

/** 祖先を遡り relTarget を含むディレクトリを返す。見つからなければ null。 */
export function findUp(startDir, relTarget) {
  let dir = path.resolve(startDir);
  for (;;) {
    if (existsSync(path.join(dir, relTarget))) return dir;
    const parent = path.dirname(dir);
    if (parent === dir) return null;
    dir = parent;
  }
}

/**
 * ファイルが属するツリーの root。
 * .git はメインツリーではディレクトリ、worktree ではファイルとして存在するため
 * 「存在するか」だけを見る。CLAUDE_PROJECT_DIR にも cwd にも依存しない（I4）。
 */
export function resolveRoot(filePath) {
  return findUp(path.dirname(path.resolve(filePath)), ".git");
}

/**
 * パス区切りを常に `/` へ正規化する。
 *
 * `replaceAll("\\", "/")` は使わない — POSIX では `\` はファイル名に使える普通の文字なので、
 * 正当な名前を潰す。`path.sep` は POSIX で `"/"` ゆえ、下の split/join はそこでは no-op になる。
 */
export function toPosixPath(p) {
  return p.split(path.sep).join("/");
}

/** root からの相対パス。区切りは常に / に正規化する。 */
export function toRelative(root, filePath) {
  return toPosixPath(path.relative(root, filePath));
}

/** payload から file_path を取り出す。無ければ null（I3）。 */
export function extractFilePath(payload) {
  const p = payload?.tool_input?.file_path;
  return typeof p === "string" && p.length > 0 ? p : null;
}

/**
 * 相対パスから走らせるべき検査を決める純関数。
 * （TS typecheck / csp-test は #532 SU7 のフロント撤去で消滅——`.ts` は情報行のみ・I16）
 */
export function selectChecks(rel) {
  const checks = [];
  const isRust = rel.endsWith(".rs");

  if (isRust) checks.push("clippy");
  if (isRust && rel.startsWith("snotra-core/")) checks.push("core-test");
  if (isRust && rel.startsWith("snotra-egui-runtime/")) checks.push("egui-runtime-test");
  if (isRust && rel.startsWith("snotra-settings/")) checks.push("settings-test");
  if (isRust && rel.startsWith("src-tauri/")) checks.push("tauri-test");

  if (CARGO_MANIFEST.test(rel)) checks.push("cargo-check");

  if (/(^|\/)(tauri\.conf\.json|config\.toml)$/.test(rel)) checks.push("config-warn");

  // セーフティネットそのものと、検査の定義を変えるファイルを編集したときは、セーフティネットが生きているか
  // 確かめる。これが無いと、全検査の発火を決めるファイルだけが誰にも検査されない（#497）。
  if (CHECK_DEFINITION.has(rel) || rel.startsWith(".claude/hooks/")) {
    checks.push("hook-selftest");
  }

  // .githooks/ は #480 以降 main 保護の Layer 1 であり、.claude/hooks/ と同じ理由で
  // 「セーフティネットそのものを編集したらセーフティネットを検査する」が適用される（#484）。
  if (rel.startsWith(".githooks/")) checks.push("githooks-selftest");

  return checks;
}

/**
 * 新規ソースファイル（.rs）を Write したかの述語。真なら main() が
 * 「モジュール索引の更新忘れ」を促す WARN を出す。
 *
 * 索引整合の判定そのもの（どのファイルが索引に在るべきか）は governance:check が SSOT。
 * ここで再実装すると drift する（DRY）。ゆえに hook は**索引を検査せず**、「Write された
 * ソースファイル」という低頻度シグナルで reminder を出すだけに留める（gate ではない）。
 * Write に絞るのは、既存ファイルの Edit まで拾うと沈黙=合格を壊す頻度になるため——
 * 新規ファイルは必ず Write で作られる（削除は Edit|Write matcher に届かず、CI の
 * governance-check が索引の orphan を捕捉する残余）。#629/#630 で src-tauri/CLAUDE.md の
 * モジュール索引更新漏れが同型再発したことへの、実装中の気づきを増やす手当て。
 */
export function isSourceFileWrite(rel, toolName) {
  if (toolName !== "Write") return false;
  return rel.endsWith(".rs");
}

/**
 * payload から「どのツリーの・どのファイルを・どう検査するか」を一度に決める。
 * main() もテストもここを通ることで、「抽出は正しいが選択へ渡す配線を間違えた」
 * という失敗をユニットテストが捕まえられる（§1 の回帰）。
 */
export function resolveTarget(payload, rootResolver = resolveRoot) {
  const file = extractFilePath(payload);
  if (!file) return null; // I3
  const abs = path.resolve(file);
  const root = rootResolver(abs);
  if (!root) return null; // I6: リポジトリ外
  const rel = toRelative(root, abs);
  return { root, rel, ids: selectChecks(rel) };
}

export function checksForPayload(payload, rootResolver = resolveRoot) {
  return resolveTarget(payload, rootResolver)?.ids ?? [];
}

/** cargo の進捗行を落とす。tsc の診断（`x.ts(1,1): error TS...`）には当たらない。 */
export function stripProgressLines(text) {
  if (!text) return "";
  return text
    .split(/\r?\n/)
    .filter((line) => !PROGRESS_LINE.test(line))
    .join("\n");
}

/**
 * 2 つのストリームを 1 本にする。単純な連結は `2>&1` と等価ではない。
 *
 * `cargo test` は `test result:` を stdout に、`Running unittests` を stderr に出す。
 * tail 予算の検査で単純連結すると、末尾に来た stderr が枠を食い、stdout 側の
 * 失敗テスト名が枠外へ落ちる。tail 予算では stderr を先に置く。
 */
export function combineStreams(stdout, stderr, from) {
  const parts = [stdout, stderr]
    .filter((s) => s && s.length > 0)
    .map((s) => s.replace(/\r?\n$/, ""));
  if (parts.length === 0) return "";
  return (from === "tail" ? parts.reverse() : parts).join("\n");
}

/**
 * 証拠テキストを予算内に収める。通知は付けない。
 *
 * 検出は exit code が担うため（I9）、ここで失われるのは証拠だけであり、
 * 失敗の事実と再現コマンドは buildSection が必ず全文出す（I21）。
 * head / tail の別は「証拠がどこにあるか」を表す:
 *   clippy / tsc は先頭に、cargo test は末尾（failures: と test result:）に出る。
 */
export function takeLines(text, { lines, from }) {
  if (!text) return "";
  const all = text.split(/\r?\n/);
  while (all.length > 0 && all[all.length - 1] === "") all.pop();
  if (all.length <= lines) return all.join("\n");
  return (from === "tail" ? all.slice(-lines) : all.slice(0, lines)).join("\n");
}

/**
 * 失敗した検査の報告を組み立てる。
 * evidence が空文字列でも、id・status・再現コマンドは必ず含まれる（I21）。
 */
export function buildSection({ id, status, repro, evidence }) {
  const head = `--- ${id}: 失敗 (${status}) ---`;
  const how = `再現: ${repro}`;
  return [head, how, evidence].filter((s) => s && s.length > 0).join("\n");
}

/**
 * root から上方へ探索して実行ファイルの絶対パスを返す。
 *
 * 必ずフルパスで probe する。`node_modules` の存在で判定すると、Phase 3 以降
 * hook 自身が <worktree>/node_modules を作るため 2 回目から探索がそこで止まり、
 * ENOENT ではなく「候補なし」になって HOOK ERROR にも捕捉されず沈黙する（I17）。
 */
function resolveBin(root, relPath) {
  const dir = findUp(root, relPath);
  return dir ? path.join(dir, relPath) : null;
}

/**
 * PostToolUse の出力エンベロープ。
 * プレーン stdout は debug log 行きでエージェントに届かない（issue #471 実測 15）。
 * additionalContext がエージェント向け、systemMessage が人間向け。
 */
export function buildEnvelope({ context, systemMessage } = {}) {
  if (!context && !systemMessage) return null;
  const envelope = {};
  if (context) {
    envelope.hookSpecificOutput = {
      hookEventName: "PostToolUse",
      additionalContext: context,
    };
  }
  if (systemMessage) envelope.systemMessage = systemMessage;
  return JSON.stringify(envelope);
}

/**
 * 検査 id → 実行する spec（`cmd`/`args`）と、エージェントへ見せる再現コマンド（`repro`）。
 *
 * **`repro` の区切りは `/` に正規化する（#768）。** Windows では `resolveBin` が `path.join` で
 * `\` 区切りの絶対パスを作るため、素で出すと `node C:\...\vitest.mjs run .claude/hooks` になり、
 * **PreToolUse の `\` パス判定が拒む形**になる。片方の hook が指示するコマンドをもう片方が拒む
 * 状態は、規範を機構へ移した設計の信頼を直に壊す（`pre-bash.test.mjs` の相互契約カナリアが固定）。
 * 実行に使う `cmd`/`args` は正規化しない — spawnSync は `\` をそのまま受けるうえ、
 * `governance:check` の G-hook-commands が `cargoSpec` の引数リテラルをソースから読むためである
 * （G-hook-commands の抽出は散文中の呼び出し形にも一致するので、ここでその形を書かない）。
 */
export function buildCommand(id, root) {
  const nodeSpec = (args) => ({
    cmd: process.execPath,
    args,
    repro: ["node", ...args.map(toPosixPath)].join(" "),
  });
  const cargoSpec = (args) => ({ cmd: "cargo", args, repro: ["cargo", ...args].join(" ") });

  /** vitest を走らせる検査の共通形。見つからなければ null（runCheck が HOOK ERROR にする）。 */
  const vitestSpec = (target) => {
    const vitest = resolveBin(root, path.join("node_modules", "vitest", "vitest.mjs"));
    return vitest ? nodeSpec([vitest, "run", target]) : null;
  };

  switch (id) {
    // check / clippy の --workspace は cargo に Cargo.toml の members を読ませる。
    // crate 名を列挙すると members の写しになり、新しい crate で気づかれないまま漏れる（#500）。
    case "clippy":
      return cargoSpec([
        "clippy", "--workspace",
        "--all-targets", "--message-format", "short", "--", "-D", "warnings",
      ]);
    // test の -p は写しではなく「編集した crate → そのテスト」の写像。--workspace に
    // すると編集していない crate のテストまで走り、hook の即時性が失われる。
    case "core-test":
      return cargoSpec(["test", "-p", "snotra-core"]);
    case "egui-runtime-test":
      return cargoSpec(["test", "-p", "snotra-egui-runtime"]);
    case "settings-test":
      return cargoSpec(["test", "-p", "snotra-settings"]);
    case "tauri-test":
      return cargoSpec(["test", "-p", "snotra"]);
    case "cargo-check":
      return cargoSpec(["check", "--workspace"]);
    case "hook-selftest":
      return vitestSpec(".claude/hooks");
    case "githooks-selftest":
      return vitestSpec(".githooks");
    default:
      return null;
  }
}

/**
 * `.claude/settings.json` が壊れると、PostToolUse だけでなく PR 前 push チェックを
 * 含む全 hook が停止する。パースは実質 0ms なので、hook 系を編集したら必ず見る。
 */
function validateSettings(root) {
  const p = path.join(root, ".claude", "settings.json");
  if (!existsSync(p)) return null;
  try {
    JSON.parse(readFileSync(p, "utf8"));
    return null;
  } catch (e) {
    return `.claude/settings.json が妥当な JSON ではありません（全 hook が停止します）: ${e.message}`;
  }
}

function runCheck(id, root, sections, errors) {
  const spec = buildCommand(id, root);
  if (!spec) {
    errors.push(`HOOK ERROR: ${id} を実行できません（必要なツールが見つかりません）`);
    return;
  }

  const res = spawnSync(spec.cmd, spec.args, {
    cwd: root,
    encoding: "utf8",
    maxBuffer: MAX_BUFFER,
    shell: false,
    timeout: PER_CHECK_TIMEOUT_MS,
  });

  // ENOBUFS / ETIMEDOUT はいずれも「検査が完走しなかった」状態であり、起動失敗とは違う。
  // どちらも子は kill されるが、それまでの出力は残っている。捨ててはならない。
  const overflowed = res.error?.code === "ENOBUFS";
  const timedOut = res.error?.code === "ETIMEDOUT";
  if (res.error && !overflowed && !timedOut) {
    errors.push(`HOOK ERROR: ${id} を起動できませんでした: ${res.error.message}`);
    return;
  }

  // 検出は exit code。出力テキストは証拠であって信号ではない（I9）。
  // 成功した検査は何も出力しない（I20）。完走しなかったものは fail-closed に倒す。
  const failed = overflowed || timedOut || res.status !== 0;
  if (!failed) return;

  const budget = BUDGETS[id];
  const combined = combineStreams(res.stdout, res.stderr, budget.from);
  const evidence = takeLines(stripProgressLines(combined), budget).trim();

  const status = timedOut
    ? `検査を完走できませんでした（${PER_CHECK_TIMEOUT_MS / 1000}s でタイムアウト）`
    : overflowed
      ? `検査を完走できませんでした（出力が maxBuffer ${MAX_BUFFER} bytes を超過）`
      : `exit ${res.status ?? `signal ${res.signal}`}`;

  sections.push(
    buildSection({ id, status, repro: `${spec.repro}  (cwd: ${toPosixPath(root)})`, evidence }),
  );
}

function main() {
  let payload;
  try {
    payload = JSON.parse(readFileSync(0, "utf8"));
  } catch (e) {
    return emitError(`payload の JSON 解析に失敗しました: ${e.message}`);
  }

  const target = resolveTarget(payload);
  if (!target) return; // I3 / I6
  const { root, rel, ids } = target;

  const sections = [];
  const warnings = [];
  const errors = [];

  // 新規ソースファイル Write は索引更新の reminder を出す（config-warn と同じインライン WARN・
  // subprocess は走らせない）。判定は isSourceFileWrite（索引整合の SSOT は governance:check）。
  if (isSourceFileWrite(rel, payload?.tool_name)) {
    warnings.push(
      `WARN: ソースファイル ${rel} を Write しました。新規追加なら該当 CLAUDE.md の` +
        " モジュール構成節の索引にファイル名を足し、`npm run governance:check` で確認してください" +
        "（#629/#630 で索引更新漏れが再発）。",
    );
  }

  for (const id of ids) {
    if (id === "config-warn") {
      warnings.push(`WARN: ${rel} を変更しました。Windows 互換性を確認してください。`);
      continue;
    }
    if (id === "hook-selftest") {
      const invalid = validateSettings(root);
      if (invalid) errors.push(`HOOK ERROR: ${invalid}`);
    }
    runCheck(id, root, sections, errors);
  }

  // 検査が 0 件でも、TypeScript 系なら「型検査は存在しない」と言う。
  // 沈黙は「検査が通った」と読まれる（I16）。TS 型検査は #532 SU7 のフロント撤去で消滅した。
  if (ids.length === 0 && TS_LIKE.test(rel)) {
    sections.push(`[post-edit] ${rel} に検査はありません（TS 型検査は #532 SU7 で撤去済み）。`);
  }

  const context = [...errors, ...sections].join("\n\n");
  const systemMessage = [...warnings, ...errors].join("\n");
  write(buildEnvelope({ context, systemMessage }));
}

/** 内部エラーはエージェントにも人間にも見せる。沈黙は最悪の失敗様式（I8）。 */
function emitError(message) {
  const text = `HOOK ERROR: ${message}`;
  write(buildEnvelope({ context: text, systemMessage: text }));
}

function write(json) {
  if (json) process.stdout.write(`${json}\n`);
}

// テストが import しただけで stdin 読み取りが走らないようにする（I13）。
const invokedDirectly =
  Boolean(process.argv[1]) && import.meta.url === pathToFileURL(process.argv[1]).href;

if (invokedDirectly) {
  try {
    main();
  } catch (e) {
    emitError(e?.stack ?? String(e));
  }
  // process.exit(0) は使わない。stdout がパイプのとき未 flush 出力を切り捨てる（I1）。
  process.exitCode = 0;
}
