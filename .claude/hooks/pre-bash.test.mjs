import { describe, it, expect, vi } from "vitest";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import {
  TARGET_TOOLS,
  tokenStart,
  GH_PR_CREATE,
  GIT_PUSH,
  hasSafeChain,
  decide,
  readGitState,
} from "./pre-bash.mjs";

// CI は ubuntu-latest。Windows リテラルパスを書くと path の挙動差で落ちる。

/** upstream 設定済み・未 push なし＝安全な状態 */
const CLEAN = () => ({ ok: true, upstream: "origin/feat", unpushed: false });
/** upstream 未設定 */
const NO_UPSTREAM = () => ({ ok: true, upstream: null, unpushed: false });
/** 未 push コミットあり */
const UNPUSHED = () => ({ ok: true, upstream: "origin/feat", unpushed: true });
/** git の状態を確認できない */
const UNKNOWN = () => ({ ok: false, reason: "git が見つかりません" });

const bash = (command, extra = {}) => ({ tool_name: "Bash", tool_input: { command, ...extra } });
const pwsh = (command) => ({ tool_name: "PowerShell", tool_input: { command } });

// テストソース自身がコマンド位置に該当語を書くと、旧 hook（payload 全体 grep）が
// 誤爆する。本 issue が根治する当のバグなので、組み立てて記述する。
const GH = "gh";
const PR_CREATE = "pr create";
const ghPrCreate = (rest = "") => `${GH} ${PR_CREATE}${rest}`;

describe("tokenStart — 区切り文字を消費せずコマンド本体の位置を返す", () => {
  // §2.2: 先頭の区切り文字を食ったまま match.index を使うと `&&` の片方しか
  // between に入らず、hasSafeChain が正準 allow ケースを block する。
  it("先頭のコマンドは 0 を返す", () => {
    expect(tokenStart(GH_PR_CREATE, ghPrCreate())).toBe(0);
  });

  it("`&&` の後ろのコマンドは `gh` の位置を返す（`&` の位置ではない）", () => {
    const cmd = `git push -u origin HEAD && ${ghPrCreate()}`;
    expect(tokenStart(GH_PR_CREATE, cmd)).toBe(cmd.indexOf(GH));
  });

  it("不一致は -1", () => {
    expect(tokenStart(GH_PR_CREATE, "echo hi")).toBe(-1);
  });
});

describe("GH_PR_CREATE — コマンド位置検出（過剰検出は許容・過小検出は許容しない）", () => {
  // U4 / D2 の回帰: 検索パターン文字列として現れるだけでは発火しない
  it("引数文字列の中は検出しない（誤爆の根治）", () => {
    expect(GH_PR_CREATE.test(`grep -n "${ghPrCreate()}" CLAUDE.md`)).toBe(false);
  });

  it("素の呼び出しを検出する", () => {
    expect(GH_PR_CREATE.test(ghPrCreate(" --title x"))).toBe(true);
  });

  // 原案の `(?:--?\S+\s+)*` はスペース区切りのフラグ値を読み飛ばせず過小検出した（fail-open）
  it("サブコマンド前のフラグ（スペース区切りの値）を読み飛ばす", () => {
    expect(GH_PR_CREATE.test(`${GH} --repo o/r ${PR_CREATE}`)).toBe(true);
  });

  it("サブコマンド前のフラグ（= 区切りの値）を読み飛ばす", () => {
    expect(GH_PR_CREATE.test(`${GH} --repo=o/r ${PR_CREATE}`)).toBe(true);
  });

  it("pr と create の間のフラグを読み飛ばす", () => {
    expect(GH_PR_CREATE.test(`${GH} pr --repo x create`)).toBe(true);
  });

  it("`&&` の直後を検出する", () => {
    expect(GH_PR_CREATE.test(`git push && ${ghPrCreate()}`)).toBe(true);
  });

  // U14: 過剰検出。fail-closed 方向なので許容し、意図をここで固定する
  it("引用内の `&&` の後ろは検出する（過剰検出を意図として固定）", () => {
    expect(GH_PR_CREATE.test(`echo "&& ${ghPrCreate()}"`)).toBe(true);
  });

  it.each([
    [`${GH} pr list`],
    [`${GH} pr view 1`],
    [`${GH} issue create`],
    [`${GH} pr list --search create`],
  ])("別サブコマンドは検出しない: %s", (cmd) => {
    expect(GH_PR_CREATE.test(cmd)).toBe(false);
  });

  it("環境変数の前置もコマンド位置と見なす", () => {
    expect(GH_PR_CREATE.test(`GH_TOKEN=x ${ghPrCreate()}`)).toBe(true);
    expect(GH_PR_CREATE.test(`FOO=bar BAZ=1 ${ghPrCreate(" --title x")}`)).toBe(true);
  });

  // 受容する穴（意図的迂回であり事故モードではない）。ここで意図を固定し、
  // 「過小検出ゼロ」と誤読されないようにする。
  it.each([
    [`timeout 5 ${ghPrCreate()}`],
    [`nohup ${ghPrCreate()}`],
    [`echo x | xargs ${GH} ${PR_CREATE}`],
    [`sh -c '${ghPrCreate()}'`],
  ])("ラッパ経由は検出しない（受容する穴・人間専用の迂回）: %s", (cmd) => {
    expect(GH_PR_CREATE.test(cmd)).toBe(false);
  });
});

describe("GIT_PUSH", () => {
  it("素の push を検出する", () => {
    expect(GIT_PUSH.test("git push -u origin HEAD")).toBe(true);
  });

  // `git -C <tree> push` は別ツリーへ push する。安全な鎖と見なしてはならない
  it("`git -C <tree> push` は検出しない（fail-closed）", () => {
    expect(GIT_PUSH.test("git -C /x push")).toBe(false);
  });
});

describe("hasSafeChain — `gh pr create` の前に `git push` が `&&` で走るか", () => {
  // D3 の回帰: これが false だと「`gh pr create` をチェーンしない」という運用ルールが復活する
  it("`git push … && gh pr create` は安全", () => {
    expect(hasSafeChain(`git push -u origin HEAD && ${ghPrCreate()}`)).toBe(true);
  });

  it("区切りがすべて `&&` なら中間コマンドがあっても安全", () => {
    expect(hasSafeChain(`git push -u origin HEAD && echo x && ${ghPrCreate()}`)).toBe(true);
  });

  // 原案が fail-open だった箇所: `;` は push 失敗時も gh を走らせる
  it("`&&` と `;` が混ざる鎖は安全でない（fail-open の回帰）", () => {
    expect(hasSafeChain(`git push -u origin HEAD && npm test; ${ghPrCreate()}`)).toBe(false);
    expect(hasSafeChain(`git push origin x && echo hi; ${ghPrCreate()}`)).toBe(false);
  });

  it("`;` 区切りは安全でない", () => {
    expect(hasSafeChain(`git push -u origin HEAD; ${ghPrCreate()}`)).toBe(false);
  });

  it("改行区切りは安全でない", () => {
    expect(hasSafeChain(`git push -u origin HEAD\n${ghPrCreate()}`)).toBe(false);
  });

  it("`||` 区切りは安全でない", () => {
    expect(hasSafeChain(`git push -u origin HEAD || ${ghPrCreate()}`)).toBe(false);
  });

  it("push が後ろにあるのは安全でない", () => {
    expect(hasSafeChain(`${ghPrCreate()} && git push`)).toBe(false);
  });

  it("`git -C <tree> push` の鎖は安全でない", () => {
    expect(hasSafeChain(`git -C /x push && ${ghPrCreate()}`)).toBe(false);
  });

  // `&&` が gh pr create より後ろにしか無い場合、誤って安全と判定してはならない
  it("引数中の `&&` を鎖と誤認しない", () => {
    expect(hasSafeChain(`${ghPrCreate(' --body "a && b"')}`)).toBe(false);
  });

  it("`gh pr create` が無ければ安全でない（安全の定義は「検出時のみ」）", () => {
    expect(hasSafeChain("git push -u origin HEAD")).toBe(false);
  });

  // 動詞の名前ではなく意味を見る。--dry-run は何も送信しないので鎖は安全でない
  it.each([["--dry-run"], ["-n"]])("`git push %s` の鎖は安全でない", (flag) => {
    expect(hasSafeChain(`git push ${flag} && ${ghPrCreate()}`)).toBe(false);
    expect(hasSafeChain(`git push ${flag} origin HEAD && ${ghPrCreate()}`)).toBe(false);
  });

  it("`-n` を含む別フラグは dry-run と誤認しない", () => {
    expect(hasSafeChain(`git push --no-thin origin HEAD && ${ghPrCreate()}`)).toBe(true);
  });
});

describe("decide — 分岐表", () => {
  it("管轄外ツールは allow（判定不能ではない）", () => {
    expect(decide({ tool_name: "Edit", tool_input: { file_path: "a.ts" } }, UNKNOWN).action).toBe("allow");
  });

  // matcher が部分一致した場合の二重防御。BashOutput の tool_input に command は無い
  it("BashOutput は allow（command が無くてもブロックしない）", () => {
    expect(decide({ tool_name: "BashOutput", tool_input: { bash_id: "x" } }, UNKNOWN).action).toBe("allow");
  });

  it("対象ツールで command が文字列でなければ block（判定不能）", () => {
    expect(decide({ tool_name: "Bash", tool_input: {} }, UNKNOWN).action).toBe("block");
    expect(decide({ tool_name: "Bash", tool_input: null }, UNKNOWN).action).toBe("block");
  });

  // I2: description は判定に使わない
  it("description に該当語があっても command が無害なら allow", () => {
    const payload = bash("ls -la", { description: `${ghPrCreate()} を実行する` });
    expect(decide(payload, UNKNOWN).action).toBe("allow");
  });

  it("検出しなければ git 状態を読まない", () => {
    const spy = vi.fn(CLEAN);
    expect(decide(bash("echo hi"), spy).action).toBe("allow");
    expect(spy).not.toHaveBeenCalled();
  });

  it("upstream 未設定なら block", () => {
    expect(decide(bash(ghPrCreate()), NO_UPSTREAM).action).toBe("block");
  });

  it("未 push コミットがあれば block", () => {
    expect(decide(bash(ghPrCreate()), UNPUSHED).action).toBe("block");
  });

  it("upstream 設定済み・未 push なしなら allow", () => {
    expect(decide(bash(ghPrCreate()), CLEAN).action).toBe("allow");
  });

  it("git の状態を確認できなければ block（fail-closed）", () => {
    expect(decide(bash(ghPrCreate()), UNKNOWN).action).toBe("block");
  });

  // D3: upstream 未設定でも、鎖が push を先に走らせるなら allow
  it("`git push … && gh pr create` は upstream 未設定でも allow", () => {
    const spy = vi.fn(NO_UPSTREAM);
    expect(decide(bash(`git push -u origin HEAD && ${ghPrCreate()}`), spy).action).toBe("allow");
    expect(spy).not.toHaveBeenCalled();
  });

  // D1: PowerShell tool も同じ判定を受ける
  it("PowerShell tool でも block する", () => {
    expect(decide(pwsh(ghPrCreate()), NO_UPSTREAM).action).toBe("block");
  });

  it("PowerShell tool でも安全な鎖は allow", () => {
    expect(decide(pwsh(`git push -u origin HEAD && ${ghPrCreate()}`), NO_UPSTREAM).action).toBe("allow");
  });

  it("cwd を変える鎖は block（どのリポジトリか判定不能）", () => {
    expect(decide(bash(`cd ../other && ${ghPrCreate()}`), CLEAN).action).toBe("block");
    expect(decide(pwsh(`Set-Location ../other && ${ghPrCreate()}`), CLEAN).action).toBe("block");
  });

  it("cwd 変更が後ろにあるだけなら判定に影響しない", () => {
    expect(decide(bash(`${ghPrCreate()} && cd ..`), CLEAN).action).toBe("allow");
  });

  it("TARGET_TOOLS は Bash と PowerShell", () => {
    expect([...TARGET_TOOLS].sort()).toEqual(["Bash", "PowerShell"]);
  });
});

describe("readGitState", () => {
  it("git リポジトリでないディレクトリは upstream=null（block へ倒す）", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "pre-bash-"));
    const state = readGitState(dir);
    expect(state.ok).toBe(true);
    expect(state.upstream).toBeNull();
  });
});

const SCRIPT = fileURLToPath(new URL("./pre-bash.mjs", import.meta.url));

function runHook(payload, cwd) {
  return spawnSync(process.execPath, [SCRIPT], {
    input: typeof payload === "string" ? payload : JSON.stringify(payload),
    encoding: "utf8",
    timeout: 20_000,
    cwd,
  });
}

describe("プロセス起動 — exit code の契約", () => {
  // exit 2 だけがブロックする。exit 1 は「非ブロッキング」でコマンドが実行される
  it("壊れた payload は exit 2（exit 1 では素通りする）", () => {
    const res = runHook("{ not json");
    expect(res.status).toBe(2);
    expect(res.stderr).toContain("BLOCKED");
  });

  it("管轄外ツールは exit 0 で無出力", () => {
    const res = runHook({ tool_name: "Edit", tool_input: { file_path: "a.ts" } });
    expect(res.status).toBe(0);
    expect(res.stdout).toBe("");
    expect(res.stderr).toBe("");
  });

  it("tool_input が無い Bash payload は exit 2", () => {
    expect(runHook({ tool_name: "Bash" }).status).toBe(2);
  });

  it("無害なコマンドは exit 0 で無出力", () => {
    const res = runHook({ tool_name: "Bash", tool_input: { command: "echo hi" } });
    expect(res.status).toBe(0);
    expect(res.stderr).toBe("");
  });

  it("git リポジトリ外での gh pr create は exit 2 で tool_name を報告する", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "pre-bash-e2e-"));
    const res = runHook({ tool_name: "PowerShell", tool_input: { command: ghPrCreate() }, cwd: dir }, dir);
    expect(res.status).toBe(2);
    expect(res.stderr).toContain("tool_name=PowerShell");
  });

  it("安全な鎖は git を読まずに exit 0", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "pre-bash-e2e-"));
    const res = runHook(
      {
        tool_name: "Bash",
        tool_input: { command: `git push -u origin HEAD && ${ghPrCreate()}` },
        cwd: dir,
      },
      dir,
    );
    expect(res.status).toBe(0);
  });

  // I13 と同型: import しただけで stdin 読み取りが走ると npm test が停止する
  it("import しただけでは main() が走らない", () => {
    const res = spawnSync(
      process.execPath,
      ["-e", `import(${JSON.stringify(pathToFileURL(SCRIPT).href)}).then(() => console.log("imported"))`],
      { encoding: "utf8", timeout: 10_000 },
    );
    expect(res.status).toBe(0);
    expect(res.stdout).toContain("imported");
  });
});

describe("fail-closed の骨格カナリア", () => {
  // I1: 既定を 2 に置き、許可が確定した経路だけが 0 を書く。
  // 現時点では main() の全経路が exit code を明示するため、この既定は冗長な belt である。
  // だが将来 exit code を書かない return が 1 本入れば、これだけが fail-open を止める。
  // ミューテーション（既定を 0 へ反転）が振る舞いテストでは赤くならないので、ここで縛る。
  const source = readFileSync(SCRIPT, "utf8");
  // コメント中の言及は「呼び出し」ではない。検出器は文脈を問わねば誤爆する（この hook 自身の教訓）。
  const code = source.replace(/^\s*\/\/.*$/gm, "");

  it("invokedDirectly ブロックは最初に process.exitCode = 2 を置く", () => {
    const block = code.slice(code.indexOf("if (invokedDirectly)"));
    expect(block).toMatch(/^if \(invokedDirectly\) \{\s*process\.exitCode = 2;/);
  });

  it("既定の block は try より前にある", () => {
    const block = code.slice(code.indexOf("if (invokedDirectly)"));
    expect(block.indexOf("process.exitCode = 2;")).toBeLessThan(block.indexOf("try {"));
  });

  it("process.exit() を使わない（未 flush 出力の切り捨て防止）", () => {
    expect(code).not.toMatch(/process\.exit\(/);
  });
});

describe("settings.json ドリフト検出カナリア", () => {
  // PreToolUse の発火（matcher）は settings.json、判定は pre-bash.mjs が SSOT。
  // matcher が後退すると hook は静かに素通りする（本 issue の D1 そのもの）。
  const settings = JSON.parse(readFileSync(fileURLToPath(new URL("../settings.json", import.meta.url)), "utf8"));
  const preToolUse = settings.hooks.PreToolUse;

  it("PreToolUse は 1 本だけ", () => {
    expect(preToolUse).toHaveLength(1);
  });

  it("matcher が TARGET_TOOLS をすべて含む", () => {
    const matcher = preToolUse[0].matcher;
    for (const tool of TARGET_TOOLS) expect(matcher).toContain(tool);
  });

  it("pre-bash.mjs を呼んでいる", () => {
    expect(preToolUse[0].hooks[0].command).toContain("pre-bash.mjs");
  });

  it("インライン grep が残っていない（payload 全体 grep の回帰）", () => {
    expect(JSON.stringify(preToolUse)).not.toContain("grep");
  });
});
