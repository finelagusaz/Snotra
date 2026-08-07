import { describe, it, expect, vi } from "vitest";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
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
  countUnchecked,
  readPlanState,
  segmentEnd,
  gitSegments,
  usesHeredoc,
  usesBackslashPath,
  needsPyEncoding,
  usesNoVerify,
  pullWithoutFfOnly,
  judgeCommandShape,
} from "./pre-bash.mjs";
import { buildCommand, BUDGETS } from "./post-edit.mjs";

// CI は ubuntu(frontend-check) と windows(rust-check) の両方で走る（#509）。どちらでも
// 落ちないよう、OS 依存のリテラルパスを書かず path.join / tmpdir から組み立てる。

/** upstream 設定済み・未 push なし＝安全な状態 */
const CLEAN = () => ({ ok: true, upstream: "origin/feat", unpushed: false });
/** upstream 未設定 */
const NO_UPSTREAM = () => ({ ok: true, upstream: null, unpushed: false });
/** 未 push コミットあり */
const UNPUSHED = () => ({ ok: true, upstream: "origin/feat", unpushed: true });
/** git の状態を確認できない */
const UNKNOWN = () => ({ ok: false, reason: "git が見つかりません" });

/** plan.md が無い（計画なしタスク・他リポジトリ） */
const NO_PLAN = () => ({ ok: true, exists: false, unchecked: 0 });
/** 全項目チェック済み */
const PLAN_DONE = () => ({ ok: true, exists: true, unchecked: 0 });
/** 未チェック n 件 */
const PLAN_OPEN = (n = 1) => () => ({ ok: true, exists: true, unchecked: n });
/** 存在するのに読めない */
const PLAN_UNREADABLE = () => ({ ok: false, reason: "EACCES" });

const bash = (command, extra = {}) => ({ tool_name: "Bash", tool_input: { command, ...extra } });
const pwsh = (command) => ({ tool_name: "PowerShell", tool_input: { command } });

// テストソース自身がコマンド位置に該当語を書くと、旧 hook（payload 全体 grep）が
// 誤爆する。本 issue が根治する当のバグなので、組み立てて記述する。
const GH = "gh";
const PR_CREATE = "pr create";
const ghPrCreate = (rest = "") => `${GH} ${PR_CREATE}${rest}`;

describe("tokenStart — 区切り文字を消費せずコマンド本体の位置を返す", () => {
  // §2.2: 先頭の区切り文字を食ったまま match.index を使うと `&&` の片方しか
  // between に入らず、hasSafeChain が正規 allow ケースを block する。
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

  // 受容する未対応リスク（意図的迂回であり事故モードではない）。ここで意図を固定し、
  // 「過小検出ゼロ」と誤読されないようにする。
  it.each([
    [`timeout 5 ${ghPrCreate()}`],
    [`nohup ${ghPrCreate()}`],
    [`echo x | xargs ${GH} ${PR_CREATE}`],
    [`sh -c '${ghPrCreate()}'`],
  ])("ラッパ経由は検出しない（受容する未対応リスク・人間専用の迂回）: %s", (cmd) => {
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
    expect(decide({ tool_name: "Edit", tool_input: { file_path: "a.ts" } }, UNKNOWN, NO_PLAN).action).toBe("allow");
  });

  // matcher が部分一致した場合の二重防御。BashOutput の tool_input に command は無い
  it("BashOutput は allow（command が無くてもブロックしない）", () => {
    expect(decide({ tool_name: "BashOutput", tool_input: { bash_id: "x" } }, UNKNOWN, NO_PLAN).action).toBe("allow");
  });

  it("対象ツールで command が文字列でなければ block（判定不能）", () => {
    expect(decide({ tool_name: "Bash", tool_input: {} }, UNKNOWN, NO_PLAN).action).toBe("block");
    expect(decide({ tool_name: "Bash", tool_input: null }, UNKNOWN, NO_PLAN).action).toBe("block");
  });

  // I2: description は判定に使わない
  it("description に該当語があっても command が無害なら allow", () => {
    const payload = bash("ls -la", { description: `${ghPrCreate()} を実行する` });
    expect(decide(payload, UNKNOWN, NO_PLAN).action).toBe("allow");
  });

  it("検出しなければ git 状態を読まない", () => {
    const spy = vi.fn(CLEAN);
    expect(decide(bash("echo hi"), spy, NO_PLAN).action).toBe("allow");
    expect(spy).not.toHaveBeenCalled();
  });

  it("upstream 未設定なら block", () => {
    expect(decide(bash(ghPrCreate()), NO_UPSTREAM, NO_PLAN).action).toBe("block");
  });

  it("未 push コミットがあれば block", () => {
    expect(decide(bash(ghPrCreate()), UNPUSHED, NO_PLAN).action).toBe("block");
  });

  it("upstream 設定済み・未 push なしなら allow", () => {
    expect(decide(bash(ghPrCreate()), CLEAN, NO_PLAN).action).toBe("allow");
  });

  it("git の状態を確認できなければ block（fail-closed）", () => {
    expect(decide(bash(ghPrCreate()), UNKNOWN, NO_PLAN).action).toBe("block");
  });

  // D3: upstream 未設定でも、鎖が push を先に走らせるなら allow
  it("`git push … && gh pr create` は upstream 未設定でも allow", () => {
    const spy = vi.fn(NO_UPSTREAM);
    expect(decide(bash(`git push -u origin HEAD && ${ghPrCreate()}`), spy, NO_PLAN).action).toBe("allow");
    expect(spy).not.toHaveBeenCalled();
  });

  // D1: PowerShell tool も同じ判定を受ける
  it("PowerShell tool でも block する", () => {
    expect(decide(pwsh(ghPrCreate()), NO_UPSTREAM, NO_PLAN).action).toBe("block");
  });

  it("PowerShell tool でも安全な鎖は allow", () => {
    expect(decide(pwsh(`git push -u origin HEAD && ${ghPrCreate()}`), NO_UPSTREAM, NO_PLAN).action).toBe("allow");
  });

  it("cwd を変える鎖は block（どのリポジトリか判定不能）", () => {
    expect(decide(bash(`cd ../other && ${ghPrCreate()}`), CLEAN, NO_PLAN).action).toBe("block");
    expect(decide(pwsh(`Set-Location ../other && ${ghPrCreate()}`), CLEAN, NO_PLAN).action).toBe("block");
  });

  it("cwd 変更が後ろにあるだけなら判定に影響しない", () => {
    expect(decide(bash(`${ghPrCreate()} && cd ..`), CLEAN, NO_PLAN).action).toBe("allow");
  });

  it("TARGET_TOOLS は Bash と PowerShell", () => {
    expect([...TARGET_TOOLS].sort()).toEqual(["Bash", "PowerShell"]);
  });
});

describe("countUnchecked — 未チェック行の数え上げ", () => {
  it("`- [ ]` / `* [ ]`（インデント許容）を数える", () => {
    expect(countUnchecked("- [ ] a\n  - [ ] b\n* [ ] c")).toBe(3);
  });
  it("チェック済み・チェックボックス以外の行は数えない", () => {
    expect(countUnchecked("- [x] done\n- [X] done\n- plain\ntext [ ] inline")).toBe(0);
  });
  it("空文字列は 0", () => {
    expect(countUnchecked("")).toBe(0);
  });
});

describe("decide — plan.md ゲート", () => {
  it("未チェック項目が残っていれば block（push の鎖が安全でも）", () => {
    const cmd = `git push -u origin HEAD && ${ghPrCreate()}`;
    const r = decide(bash(cmd), CLEAN, PLAN_OPEN(3));
    expect(r.action).toBe("block");
    expect(r.reason).toContain("3 件");
  });
  it("全項目チェック済みなら push 判定へ進み allow", () => {
    expect(decide(bash(ghPrCreate()), CLEAN, PLAN_DONE).action).toBe("allow");
  });
  it("plan.md が無ければこの検査は管轄外（push 判定のみで allow）", () => {
    expect(decide(bash(ghPrCreate()), CLEAN, NO_PLAN).action).toBe("allow");
  });
  it("plan.md が存在するのに読めなければ block（fail-closed）", () => {
    expect(decide(bash(ghPrCreate()), CLEAN, PLAN_UNREADABLE).action).toBe("block");
  });
  it("gh pr create を検出しなければ plan.md を読まない", () => {
    const spy = vi.fn(NO_PLAN);
    expect(decide(bash("echo hi"), CLEAN, spy).action).toBe("allow");
    expect(spy).not.toHaveBeenCalled();
  });
  it("PowerShell tool でも未チェック項目で block する", () => {
    expect(decide(pwsh(ghPrCreate()), CLEAN, PLAN_OPEN()).action).toBe("block");
  });
});

describe("readPlanState — 実ファイル読み取り", () => {
  it("workspace/plan.md が無いディレクトリは exists:false", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "pre-bash-plan-"));
    expect(readPlanState(dir)).toEqual({ ok: true, exists: false, unchecked: 0 });
  });
  it("未チェック 2 件・チェック済み 1 件の plan.md を正しく数える", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "pre-bash-plan-"));
    mkdirSync(path.join(dir, "workspace"));
    writeFileSync(path.join(dir, "workspace", "plan.md"), "- [x] a\n- [ ] b\n- [ ] c\n", "utf8");
    expect(readPlanState(dir)).toEqual({ ok: true, exists: true, unchecked: 2 });
  });

  // I2: session cwd がリポジトリのサブディレクトリ（例: src-tauri/）だと、素朴な
  // path.join(cwd, "workspace/plan.md") は永遠に見つからずゲートだけが沈黙で外れる。
  // post-edit.mjs の resolveRoot と同じ「最近接の .git へ遡る」で揃える。
  it("cwd がリポジトリのサブディレクトリでも、最近接の .git を遡ってリポジトリルートの plan.md を見る", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "pre-bash-plan-root-"));
    mkdirSync(path.join(dir, ".git"));
    mkdirSync(path.join(dir, "workspace"));
    writeFileSync(path.join(dir, "workspace", "plan.md"), "- [ ] 残タスク\n", "utf8");
    const sub = path.join(dir, "sub", "inner");
    mkdirSync(sub, { recursive: true });
    expect(readPlanState(sub)).toEqual({ ok: true, exists: true, unchecked: 1 });
  });

  it(".git が無い temp dir のサブディレクトリでは従来どおりそのサブディレクトリ基準（exists:false）", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "pre-bash-plan-noroot-"));
    const sub = path.join(dir, "sub", "inner");
    mkdirSync(sub, { recursive: true });
    expect(readPlanState(sub)).toEqual({ ok: true, exists: false, unchecked: 0 });
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

  it("未チェックの plan.md がある repo での gh pr create は exit 2（安全な鎖でも）", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "pre-bash-e2e-plan-"));
    mkdirSync(path.join(dir, "workspace"));
    writeFileSync(path.join(dir, "workspace", "plan.md"), "- [ ] 残タスク\n", "utf8");
    const res = runHook(
      { tool_name: "Bash", tool_input: { command: `git push -u origin HEAD && ${ghPrCreate()}` }, cwd: dir },
      dir,
    );
    expect(res.status).toBe(2);
    expect(res.stderr).toContain("plan.md");
  });

  it("全項目チェック済みの plan.md なら安全な鎖は exit 0", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "pre-bash-e2e-plan-"));
    mkdirSync(path.join(dir, "workspace"));
    writeFileSync(path.join(dir, "workspace", "plan.md"), "- [x] 済\n", "utf8");
    const res = runHook(
      { tool_name: "Bash", tool_input: { command: `git push -u origin HEAD && ${ghPrCreate()}` }, cwd: dir },
      dir,
    );
    expect(res.status).toBe(0);
  });

  // #768: 配線（main() が process.platform を渡すこと）を**実行で**固定する。
  // `npm test` は ubuntu と windows の両方で走るため（ci.yml）、この 1 本で両分岐が
  // それぞれの OS で一度は実行される——注入テストだけでは通らない経路が残る。
  it("heredoc は win32 で exit 2・他 OS で exit 0（platform 分岐のライブ実測）", () => {
    const res = runHook({ tool_name: "Bash", tool_input: { command: "cat <<EOF" } });
    if (process.platform === "win32") {
      expect(res.status).toBe(2);
      expect(res.stderr).toContain("HEREDOC");
    } else {
      expect(res.status).toBe(0);
      expect(res.stderr).toBe("");
    }
  });

  it("`--no-verify` はどの OS でも exit 2", () => {
    const res = runHook({ tool_name: "Bash", tool_input: { command: "git commit --no-verify -m x" } });
    expect(res.status).toBe(2);
    expect(res.stderr).toContain("人間専用");
  });

  it("`--ff-only` の無い pull はどの OS でも exit 2", () => {
    const res = runHook({ tool_name: "PowerShell", tool_input: { command: "git pull origin main" } });
    expect(res.status).toBe(2);
    expect(res.stderr).toContain("--ff-only");
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

  // #768 I5: platform 省略時は Windows 専用判定が発火しない。その倒し方が安全なのは
  // main() が必ず process.platform を渡すからである。この 1 行が抜けると Windows で
  // 3 判定が沈黙で外れる——振る舞いテストからは「非 Windows での正常動作」と見分けがつかない。
  it("main() は decide へ process.platform を渡す", () => {
    expect(code).toMatch(/decide\(\s*payload,[\s\S]*?process\.platform\s*\)/);
  });
});

// ---------------------------------------------------------------------------
// #768 コマンドの形で判定する規範 5 件。
// ---------------------------------------------------------------------------

describe("segmentEnd / gitSegments — 区切りまでの切り出し", () => {
  it("区切りが無ければ文末", () => {
    expect(segmentEnd("git status", 0)).toBe("git status".length);
  });

  it("次の区切り文字で閉じる", () => {
    const cmd = "git status && npm test";
    expect(segmentEnd(cmd, 0)).toBe(cmd.indexOf(" &&") + 1);
  });

  it("コマンド位置の git ごとにセグメントを返す", () => {
    expect(gitSegments("git switch main && git pull --ff-only")).toEqual([
      "git switch main ",
      "git pull --ff-only",
    ]);
  });

  // `git -C <tree> push` を外す GIT_PUSH とは違い、こちらは -C 付きも拾う
  it("`git -C <tree>` も拾う（--no-verify はツリーを問わず人間専用）", () => {
    expect(gitSegments("git -C /x commit")).toEqual(["git -C /x commit"]);
  });

  it("引数文字列の中の git は拾わない（言及と実行の区別）", () => {
    expect(gitSegments('grep -n "git pull" AGENTS.md')).toEqual([]);
  });
});

describe("usesHeredoc", () => {
  it.each([
    ["cat > f.txt <<'EOF'\nhello\nEOF"],
    ["git commit -F- <<EOF\nmsg body\nEOF"],
    ["cat <<-EOF > out\n\tbody\n\tEOF"],
    ["python - <<PY\nprint(1)\nPY"],
    ["cat <<EOF > out.txt\nbody\nEOF"],
    ["cat <<EOF"],
    ["cat <<EOF | tee f"],
    ["cat <<EOF > out 2>&1"],
    ["cat <<'EOF' && echo done"],
  ])("heredoc を検出する: %s", (cmd) => {
    expect(usesHeredoc(cmd)).toBe(true);
  });

  // 最初の候補だけを見る実装では取り落とした形（fail-open の回帰）。
  // 囮のシフト演算子が先行しても本物の heredoc を見つけねばならない。
  it.each([
    ['grep -rn "x << y" src && cat <<EOF\nbody\nEOF'],
    ['echo "1 << 3" && git commit -F- <<MSG\nm\nMSG'],
  ])("囮の `<<` が先行しても検出する: %s", (cmd) => {
    expect(usesHeredoc(cmd)).toBe(true);
  });

  it.each([
    ['grep -rn "1 << 3" src/'],
    ['grep -rn "x << y" src | head'],
    ['echo "shift: a << b" && ls'],
    ["echo x <<< y"],
    ["sort < file.txt"],
    ["git log --oneline -5"],
    ["npm test 2>&1 | tail -30"],
  ])("shift 演算子・herestring・単純リダイレクトは検出しない: %s", (cmd) => {
    expect(usesHeredoc(cmd)).toBe(false);
  });

  // code-reviewer M1 の回帰: 追従集合に `)` を入れるとシフト演算子を誤爆した
  it.each([['node -e "console.log(1<<n)"'], ['node -e "x = (a << b)"']])(
    "閉じ括弧が続くシフト演算子は検出しない: %s",
    (cmd) => {
      expect(usesHeredoc(cmd)).toBe(false);
    },
  );

  // 候補ごとに終端行を全文走査する実装は 20000 候補で 1800ms 超（実測）。
  // 単独行を 1 パスで索引する形は数 ms で終わる。桁が違うので閾値は緩くてよい。
  it("候補が多数同居しても線形で終わる（no-hang）", () => {
    const adversarial = "cat <<AAA ".repeat(20_000) + "x".repeat(10_000);
    const t0 = process.hrtime.bigint();
    expect(usesHeredoc(adversarial)).toBe(false);
    expect(Number(process.hrtime.bigint() - t0) / 1e6).toBeLessThan(500);
  });
});

describe("usesBackslashPath", () => {
  it.each([
    ["node C:\\workspace\\Snotra\\x.mjs"],
    ["git commit -F $env:TEMP\\msg.txt"],
    ["type %TEMP%\\msg.txt"],
  ])("`\\` パス区切りを検出する: %s", (cmd) => {
    expect(usesBackslashPath(cmd)).toBe(true);
  });

  // 広く `\` を見ると正規表現エスケープや find -exec を巻き込む（意図的に narrow）
  it.each([
    ['grep -E "\\d+" f.txt'],
    ['rg "\\s+$" src'],
    ["find . -exec rm {} \\;"],
    ['git commit -F "$env:TEMP/msg.txt"'],
    ["node C:/workspace/Snotra/x.mjs"],
  ])("エスケープ・POSIX 区切りは検出しない: %s", (cmd) => {
    expect(usesBackslashPath(cmd)).toBe(false);
  });

  // 受容する過剰検出。fail-closed 方向で回復は自明（`/` にする）ゆえ意図として固定する
  it("散文中の `C:\\path` も検出する（過剰検出を意図として固定）", () => {
    expect(usesBackslashPath('git commit -m "fix: C:\\path handling"')).toBe(true);
  });

  // code-reviewer H2 の回帰: 素の `[A-Za-z]:\\` は「語末 1 文字 + `:` + 正規表現エスケープ」と
  // 区別できず、これらを block していた。拒否文言は「`/` で統一せよ」なので**復帰手順が
  // 適用できない拒否**になる——誤爆の中でも最も害が大きい形である。
  it.each([
    ['rg "version:\\s+" src'],
    ['grep -E "error:\\s" log.txt'],
    ['rg "name:\\w+" src'],
  ])("`:\\` を含む正規表現は検出しない: %s", (cmd) => {
    expect(usesBackslashPath(cmd)).toBe(false);
  });

  // 上の誤爆を落とすために付けた `{2,}` の代償（過小検出・意図として固定）
  it.each([["cd C:\\"], ["node C:\\a"]])("ドライブルート・1 文字続きは検出しない: %s", (cmd) => {
    expect(usesBackslashPath(cmd)).toBe(false);
  });
});

describe("needsPyEncoding", () => {
  it.each([['python -c "print(\'日本語\')"'], ["python3 scripts/x.py --名前 a"]])(
    "非 ASCII を含む python 起動を検出する: %s",
    (cmd) => {
      expect(needsPyEncoding(cmd)).toBe(true);
    },
  );

  it.each([
    ['PYTHONIOENCODING=utf-8 python -c "print(\'日本語\')"'],
    ['PYTHONUTF8=1 python -c "print(\'日本語\')"'],
    ['python -X utf8 -c "print(\'日本語\')"'],
    ['python -c "print(1)"'],
    ["echo 日本語"],
    ['grep -n "python" 日本語.md'],
  ])("設定済み・非 ASCII 無し・python でない形は検出しない: %s", (cmd) => {
    expect(needsPyEncoding(cmd)).toBe(false);
  });
});

describe("usesNoVerify", () => {
  it.each([
    ["git commit --no-verify -m x"],
    ["git push --no-verify"],
    ["npm test && git commit -n -m x"],
    ["git commit -nm x"],
    // `(?:-\S+\s+)*` ではスペース区切りのフラグ値を飛ばせず取りこぼした（fail-open の回帰）
    ["git -C /x commit -n"],
    ["git --git-dir=/x commit -nm y"],
  ])("迂回を検出する: %s", (cmd) => {
    expect(usesNoVerify(cmd)).toBe(true);
  });

  it.each([
    // 「言及」で発火しない（#482 の根治を新判定でも保つ）
    ['grep -n "--no-verify" CLAUDE.md'],
    ["git commit -m x"],
    ["git commit --amend --no-edit"],
    ["git commit -F- x"],
    // push の `-n` は --dry-run。短縮形の判定は commit セグメント限定である
    ["git push -n origin HEAD"],
    ["git log -n 5"],
    ["git -C /x log -n 5"],
  ])("迂回でない形は検出しない: %s", (cmd) => {
    expect(usesNoVerify(cmd)).toBe(false);
  });

  // 引用内の区切りでセグメントが切れる過小検出。`sh -c` と同格の意図的迂回として受容する
  it("引用内の `;` でセグメントが切れる形は検出しない（受容する過小検出）", () => {
    expect(usesNoVerify('git commit -m "a;b" --no-verify')).toBe(false);
  });
});

describe("pullWithoutFfOnly", () => {
  it.each([
    ["git pull"],
    ["git pull origin main"],
    ["git switch main && git pull"],
    ["git -C /x pull"],
    // --rebase も止める。ブランチを見ないので過剰検出だが fail-closed 方向である
    ["git pull --rebase"],
  ])("`--ff-only` の無い pull を検出する: %s", (cmd) => {
    expect(pullWithoutFfOnly(cmd)).toBe(true);
  });

  it.each([
    ["git pull --ff-only"],
    ["git pull --ff-only origin main"],
    ["git checkout main && git pull --ff-only"],
    ["git --no-pager pull --ff-only origin main"],
    ['grep -n "git pull" AGENTS.md'],
    ["git fetch origin"],
  ])("`--ff-only` 付き・pull でない形は検出しない: %s", (cmd) => {
    expect(pullWithoutFfOnly(cmd)).toBe(false);
  });
});

describe("judgeCommandShape — platform ゲート", () => {
  const WIN_ONLY = [
    ["heredoc", "cat <<EOF"],
    ["`\\` パス", "echo C:\\workspace"],
    ["python 非 ASCII", 'python -c "print(\'日本語\')"'],
  ];

  it.each(WIN_ONLY)("%s は win32 で block", (_label, cmd) => {
    expect(judgeCommandShape(cmd, "win32")?.action).toBe("block");
  });

  it.each(WIN_ONLY)("%s は darwin で allow（規範の射程外）", (_label, cmd) => {
    expect(judgeCommandShape(cmd, "darwin")).toBeNull();
  });

  it.each(WIN_ONLY)("%s は linux で allow", (_label, cmd) => {
    expect(judgeCommandShape(cmd, "linux")).toBeNull();
  });

  // I5: platform 省略は「判定不能」ではなく「射程外」。block へ倒すと非 Windows で false block になる。
  // この状態へ至る経路は「呼び出し側の渡し忘れ」1 本だけで、下のソースカナリアと e2e が配線を固定する。
  it.each(WIN_ONLY)("%s は platform 省略時 allow（I5）", (_label, cmd) => {
    expect(judgeCommandShape(cmd)).toBeNull();
  });

  const PLATFORM_FREE = [
    ["--no-verify", "git commit --no-verify -m x"],
    ["pull", "git pull origin main"],
  ];

  it.each(PLATFORM_FREE)("%s は platform を問わず block（win32）", (_label, cmd) => {
    expect(judgeCommandShape(cmd, "win32")?.action).toBe("block");
  });

  it.each(PLATFORM_FREE)("%s は platform を問わず block（darwin）", (_label, cmd) => {
    expect(judgeCommandShape(cmd, "darwin")?.action).toBe("block");
  });

  it.each(PLATFORM_FREE)("%s は platform を問わず block（省略）", (_label, cmd) => {
    expect(judgeCommandShape(cmd)?.action).toBe("block");
  });

  // I6: 規範を降ろした設計は、この文言がその場で教えることに賭けている
  it("拒否文言は代わりの手段を含む", () => {
    expect(judgeCommandShape("cat <<EOF", "win32").reason).toContain("git commit -F");
    expect(judgeCommandShape("echo C:\\workspace", "win32").reason).toContain("`/` で統一");
    expect(judgeCommandShape('python -c "日本語"', "win32").reason).toContain("PYTHONIOENCODING");
    expect(judgeCommandShape("git commit --no-verify", "darwin").reason).toContain("人間専用");
    expect(judgeCommandShape("git pull", "darwin").reason).toContain("--ff-only");
  });

  it("無害なコマンドは null（管轄外）", () => {
    expect(judgeCommandShape("npm test 2>&1 | tail -30", "win32")).toBeNull();
    expect(judgeCommandShape("git push -u origin HEAD", "win32")).toBeNull();
  });

  // #768 の norm-review 1 巡目が暴いた欠陥の機構化（当該スキルは 2026-08-07 に廃止・欠陥と機構は残る）。
  // heredoc の拒否文言は代替として `$env:TEMP` 配下の一時ファイルを挙げていたが、区切りを
  // 示していなかった。忠実な読者は `$env:TEMP\msg.txt` と書き、**同じ hook の `\` 判定に落ちる** —
  // 「代わりにこうせよ」が別の拒否を招く状態だった。文書で戒めるのではなくここで縛る。
  const ALL_REASONS = [
    "cat <<EOF",
    "echo C:\\workspace",
    'python -c "日本語"',
    "git commit --no-verify",
    "git pull",
  ].map((cmd) => judgeCommandShape(cmd, "win32").reason);

  // `\b` が要る: `\w+` は貪欲でもバックトラックするので、これが無いと `$env:TEM` で
  // 一致してから「次は `/` でない（`P`）」が成立し、正しい文言でも赤になる（この形で一度踏んだ）。
  it("拒否文言が示す `$env:` パスの区切りは必ず `/` である", () => {
    for (const reason of ALL_REASONS) {
      expect(reason, reason).not.toMatch(/\$env:\w+\b(?![/])/);
    }
  });

  it("拒否文言はドライブレター絶対パスを示さない", () => {
    for (const reason of ALL_REASONS) {
      expect(reason, reason).not.toMatch(/[A-Za-z]:\\/);
    }
  });
});

describe("decide — 第 4 引数 platform の配線", () => {
  it("win32 なら Windows 専用判定で block し、git も plan.md も読まない", () => {
    const git = vi.fn(CLEAN);
    const plan = vi.fn(NO_PLAN);
    expect(decide(bash("cat <<EOF"), git, plan, "win32").action).toBe("block");
    expect(git).not.toHaveBeenCalled();
    expect(plan).not.toHaveBeenCalled();
  });

  it("darwin なら同じコマンドが allow", () => {
    expect(decide(bash("cat <<EOF"), CLEAN, NO_PLAN, "darwin").action).toBe("allow");
  });

  it("既存の 3 引数呼び出しは Windows 専用判定に触れない（I7）", () => {
    expect(decide(bash("cat <<EOF"), CLEAN, NO_PLAN).action).toBe("allow");
  });

  it("PowerShell tool でも同じ判定を受ける", () => {
    expect(decide(pwsh("git pull origin main"), CLEAN, NO_PLAN, "win32").action).toBe("block");
  });

  // #772 の検証で「手を抜く読者」が突いた自己分類の余地: 拒否文言[2]は事故の根拠を
  // 「Bash では黙って食われて」と書くので、「PowerShell ツールを使えば適用外」と読める。
  // **判定は `decide` の入口で両ツールを覆う**——読んだ理由でツールを替えても降りられない。
  // 文言側もその射程を明記したので、機構と文言の両方をここで縛る。
  it("`\\` パスは PowerShell tool でも block し、文言が両ツール発火を明記する", () => {
    const winPath = `Get-Content C:${"\\"}workspace${"\\"}x.md`;
    for (const payload of [bash(winPath), pwsh(winPath)]) {
      const r = decide(payload, CLEAN, NO_PLAN, "win32");
      expect(r.action).toBe("block");
      expect(r.reason).toContain("Bash / PowerShell の両方");
    }
  });

  // 形の判定は plan.md ゲートより前にある（I/O を要さない判定を先に走らせる）
  it("形の判定は plan.md を読む前に決まる", () => {
    const plan = vi.fn(PLAN_OPEN(3));
    expect(decide(bash("git pull"), CLEAN, plan, "win32").action).toBe("block");
    expect(plan).not.toHaveBeenCalled();
  });

  it("description に該当語があっても command が無害なら allow", () => {
    const payload = bash("ls -la", { description: "git pull して cat <<EOF する" });
    expect(decide(payload, CLEAN, NO_PLAN, "win32").action).toBe("allow");
  });
});

describe("5 述語は全域関数である（I2・no-throw）", () => {
  const PREDICATES = [
    usesHeredoc,
    usesBackslashPath,
    needsPyEncoding,
    usesNoVerify,
    pullWithoutFfOnly,
  ];
  const PATHOLOGICAL = [
    "", "<", "<<", "<<<", "<<-", "<<'", '<<"', "$env:", "%", "C:", ":\\", "git", "git ", "python",
    "'", '"', 'unmatched " quote && git commit', "\n\n\n", "\r\n", "\t<<EOF\t",
    "a".repeat(200_000), "<<".repeat(20_000), "git pull; ".repeat(5_000), "日本語".repeat(10_000),
    "$env:TEMP\\".repeat(5_000), `python ${"非ASCII".repeat(5_000)}`,
  ];

  // 全コマンドで走るため、1 つでも throw すると main() の catch が exit 2 を書き、
  // セッションの全コマンドがブロックされる。
  it("病的入力でも throw しない", () => {
    for (const fn of PREDICATES) {
      for (const input of PATHOLOGICAL) {
        expect(() => fn(input), `${fn.name}(${JSON.stringify(input.slice(0, 20))})`).not.toThrow();
      }
    }
  });

  // code-reviewer H1 の回帰。`FLAG` の `-{1,2}\S+` は `--x` の分割が 2 通りあり、
  // 全体が失敗するとき 2^n 通りを探索した（`git --f0=v0 … --f23=v23 x` の 225 字で 747ms・実測）。
  // **これは throw でも hang でもなく「遅い」という形の fail-open** である——hook が timeout すると
  // exit≠2 = 非ブロッキングになり、コマンドはそのまま実行される。時間でしか捕まらないので時間で縛る。
  it("`git` の直後にフラグが並ぶ入力が指数爆発しない", () => {
    const flags = Array.from({ length: 2000 }, (_, i) => `--f${i}=v${i}`).join(" ");
    const cmd = `git ${flags} x`;
    const t0 = process.hrtime.bigint();
    expect(usesNoVerify(cmd)).toBe(false);
    expect(pullWithoutFfOnly(cmd)).toBe(false);
    expect(Number(process.hrtime.bigint() - t0) / 1e6).toBeLessThan(500);
  });

  it("judgeCommandShape も病的入力で throw しない", () => {
    for (const input of PATHOLOGICAL) {
      for (const platform of ["win32", "darwin", undefined]) {
        expect(() => judgeCommandShape(input, platform)).not.toThrow();
      }
    }
  });
});

describe("フック間契約 — post-edit が出す再現コマンドを pre-bash が拒まない（I8・#768）", () => {
  const REPO = fileURLToPath(new URL("../../", import.meta.url));

  // post-edit.mjs の repro が `\` 区切りを出すと、この hook の `\` 判定が拒む。
  // 片方の hook が指示するコマンドをもう片方が拒む状態を、判定そのもので固定する。
  //
  // **母集団は BUDGETS から導出する。手で列挙しない**（#858）。id を手書きで並べていた頃は、
  // 新しい検査を足した人がこのリストを更新し忘れれば緑のまま通った——`buildCommand` の
  // case 欠落を機構で捕まえる唯一の点がここなので、母集団が欠ければ検知器ごと欠ける。
  // BUDGETS 自身は `selectChecks` の発行 id を BUDGETS 完全性カナリア（post-edit.test.mjs）が
  // 固定しているため、これで **selectChecks → BUDGETS → buildCommand の輪が閉じる**。
  // config-warn は runCheck を通らず BUDGETS に無いので、自然に母集団から外れる。
  it.each(Object.keys(BUDGETS).map((id) => [id]))("%s の再現コマンドは win32 でも allow", (id) => {
    const spec = buildCommand(id, REPO);
    expect(spec, `${id} の spec が null（buildCommand の case 欠落）`).not.toBeNull();
    const r = decide(bash(spec.repro), CLEAN, NO_PLAN, "win32");
    expect(r.action, `${id}: ${spec.repro} — ${r.reason ?? ""}`).toBe("allow");
  });
});

describe("settings.json ドリフト検出カナリア", () => {
  // PreToolUse の発火（matcher）は settings.json、判定は pre-bash.mjs が SSOT。
  // matcher が後退すると hook は気づかれないまま素通りする（本 issue の D1 そのもの）。
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
