import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import {
  findUp,
  resolveRoot,
  toRelative,
  extractFilePath,
  selectChecks,
  resolveTarget,
  checksForPayload,
  stripProgressLines,
  combineStreams,
  takeLines,
  buildSection,
  resolveTscBin,
  buildEnvelope,
} from "./post-edit.mjs";

// CI は ubuntu-latest。Windows リテラルパスを書くと path.relative の挙動差で落ちる。
// パスは必ず path.join / tmpdir から組み立てる。

describe("selectChecks", () => {
  // §5: 発火条件は tsconfig の include - exclude と一致しなければならない
  it("ui/src の非テスト .ts は typecheck", () => {
    expect(selectChecks("ui/src/api.ts")).toEqual(["typecheck"]);
  });

  it("ui/src の非テスト .tsx は typecheck", () => {
    expect(selectChecks("ui/src/components/SearchWindow.tsx")).toEqual(["typecheck"]);
  });

  it("ui/src の .d.ts は typecheck（program に含まれる）", () => {
    expect(selectChecks("ui/src/vite-env.d.ts")).toEqual(["typecheck"]);
  });

  it("テストファイルも typecheck（#474 で exclude を撤廃）", () => {
    expect(selectChecks("ui/src/lib/i18n.test.ts")).toEqual(["typecheck"]);
  });

  // I19: 深さ 0 の実在ファイル。「1 段以上のディレクトリ」を要求する正規表現で書くと
  // ここだけ発火しなくなり、§5（発火するが検査されない）が再現する。
  it("深さ 0 のテストファイルも typecheck（ui/src/MainApp.test.tsx）", () => {
    expect(selectChecks("ui/src/MainApp.test.tsx")).toEqual(["typecheck"]);
  });

  it("e2e の .ts も typecheck（#474 で include に追加）", () => {
    expect(selectChecks("e2e/tauri.slash.e2e.ts")).toEqual(["typecheck"]);
  });

  it("ルートの設定 .ts は typecheck（include が名指しする 3 ファイル）", () => {
    expect(selectChecks("vite.config.ts")).toEqual(["typecheck"]);
    expect(selectChecks("playwright.tauri.config.ts")).toEqual(["typecheck"]);
  });

  // #497: 「検査の定義を変えるファイル」も検査集合に載せる。hook-selftest が走ることで、
  // その定義と selectChecks の前提が一致しているかをカナリアが検証する。
  it("tsconfig.json は typecheck + hook-selftest（定義とカナリアの両方）", () => {
    expect(selectChecks("tsconfig.json")).toEqual(["typecheck", "hook-selftest"]);
  });

  it("vitest.config.ts は typecheck + hook-selftest（検査の実行範囲を定義する）", () => {
    expect(selectChecks("vitest.config.ts")).toEqual(["typecheck", "hook-selftest"]);
  });

  it("package.json は hook-selftest（prepare が Layer 1 を bootstrap する）", () => {
    expect(selectChecks("package.json")).toEqual(["hook-selftest"]);
  });

  // ルートの Cargo.toml だけが hook-selftest を撃つ。crate 追加は必ず members を
  // 編集するのでそこで捕まる（#500）。メンバーの Cargo.toml は cargo-check のみ。
  it("ルートの Cargo.toml は cargo-check + hook-selftest", () => {
    expect(selectChecks("Cargo.toml")).toEqual(["cargo-check", "hook-selftest"]);
  });

  it("メンバーの Cargo.toml は cargo-check のみ", () => {
    expect(selectChecks("snotra-core/Cargo.toml")).toEqual(["cargo-check"]);
    expect(selectChecks("src-tauri/Cargo.toml")).toEqual(["cargo-check"]);
  });

  // basename アンカーゆえ深い階層も発火する。過小検出は沈黙（false green）だが、
  // 過剰検出は cargo check が走るだけで無害（fail-closed 方向）。
  it("深い階層の Cargo.toml も cargo-check（過剰検出は許容する）", () => {
    expect(selectChecks("a/b/Cargo.toml")).toEqual(["cargo-check"]);
  });

  it("Cargo.lock / 小文字 cargo.toml は発火しない（負例）", () => {
    expect(selectChecks("Cargo.lock")).toEqual([]);
    expect(selectChecks("cargo.toml")).toEqual([]);
  });

  // #484: .githooks/ は #480 以降 main 保護の Layer 1。.claude/hooks/ と同じ理由。
  it(".githooks/ は githooks-selftest（安全網そのものの自己検査）", () => {
    expect(selectChecks(".githooks/pre-commit")).toEqual(["githooks-selftest"]);
    expect(selectChecks(".githooks/githooks.test.mjs")).toEqual(["githooks-selftest"]);
  });

  it("ルート以外の package.json / 規範ファイルは発火しない（負例）", () => {
    expect(selectChecks("ui/package.json")).toEqual([]);
    expect(selectChecks(".github/workflows/ci.yml")).toEqual([]);
  });

  // ルート config は完全一致で判定する。部分一致（endsWith）で書くと sub/vite.config.ts が
  // 誤発火し、program に無いファイルを「検査が通った」と沈黙させる（入力集合の両方向検算）。
  it("include 外の .ts は発火しない（負例）", () => {
    expect(selectChecks("scripts/foo.ts")).toEqual([]);
    expect(selectChecks("ui/vite.config.ts")).toEqual([]);
  });

  it("e2e/ でも TS_LIKE 外は発火しない（負例）", () => {
    expect(selectChecks("e2e/README.md")).toEqual([]);
  });

  it("snotra-core の .rs は clippy + core-test", () => {
    expect(selectChecks("snotra-core/src/lib.rs")).toEqual(["clippy", "core-test"]);
  });

  // §1: 現行は content 中の "config.toml" に反応して config-warn が誤発火する
  it("snotra-core/src/config.rs は config-warn を誤発火しない", () => {
    expect(selectChecks("snotra-core/src/config.rs")).toEqual(["clippy", "core-test"]);
  });

  it("snotra-settings の .rs は clippy + settings-test", () => {
    expect(selectChecks("snotra-settings/src/main.rs")).toEqual(["clippy", "settings-test"]);
  });

  it("src-tauri の .rs は clippy + tauri-test", () => {
    expect(selectChecks("src-tauri/src/lib.rs")).toEqual(["clippy", "tauri-test"]);
    expect(selectChecks("src-tauri/build.rs")).toEqual(["clippy", "tauri-test"]);
  });

  it("src-tauri/tauri.conf.json は config-warn + csp-test", () => {
    expect(selectChecks("src-tauri/tauri.conf.json")).toEqual(["config-warn", "csp-test"]);
  });

  // csp-test は完全一致のみ。CSP 契約テストが読まないファイルの編集で
  // 「検査が通った」と沈黙させない（入力集合の両方向検算）。
  it("深い階層の tauri.conf.json は config-warn のみ（負例）", () => {
    expect(selectChecks("foo/tauri.conf.json")).toEqual(["config-warn"]);
  });

  // 安全網そのものを編集したときは、安全網が生きているか確かめる。
  // これが無いと、全検査の発火を決めるファイルだけが誰にも検査されない。
  // （settings.json を parse するだけの検査に自己参照ループは生じない。
  //   hook は settings.json を読んで発火判断をしていないため）
  it("hook 自身の設定・スクリプトは hook-selftest を発火する", () => {
    expect(selectChecks(".claude/settings.json")).toEqual(["hook-selftest"]);
    expect(selectChecks(".claude/hooks/post-edit.mjs")).toEqual(["hook-selftest"]);
    expect(selectChecks(".claude/hooks/post-edit.test.mjs")).toEqual(["hook-selftest"]);
  });

  it(".claude 配下でも hooks 以外は発火しない", () => {
    expect(selectChecks(".claude/skills/implement/SKILL.md")).toEqual([]);
    expect(selectChecks(".claude/settings.local.json")).toEqual([]);
  });

  // tsconfig の include はディレクトリ展開で .mts / .cts も program に入れる。
  // 「沈黙 = 合格」の下では、拾い漏らしは安全側ではなく false green になる。
  it("ui/src の .mts / .cts も typecheck 対象", () => {
    expect(selectChecks("ui/src/lib/x.mts")).toEqual(["typecheck"]);
    expect(selectChecks("ui/src/lib/x.cts")).toEqual(["typecheck"]);
  });

  it("ドキュメントは何も発火しない", () => {
    expect(selectChecks("docs/notes.md")).toEqual([]);
    expect(selectChecks("AGENTS.md")).toEqual([]);
  });
});

describe("checksForPayload — §1 誤爆の回帰", () => {
  // selectChecks 単体では §1 を証明できない。§1 は「抽出」のバグであって
  // 「選択」のバグではないため、payload から check id までを一気に固定する。
  const root = path.join(path.sep, "repo");
  const fakeResolver = () => root;

  it("content 末尾の ui/src/api.ts に反応しない", () => {
    const payload = {
      tool_name: "Write",
      tool_input: {
        file_path: path.join(root, "docs", "notes.md"),
        content: "参照すべきは ui/src/api.ts",
      },
    };
    expect(checksForPayload(payload, fakeResolver)).toEqual([]);
  });

  it("content 末尾の snotra-core/src/lib.rs に反応しない", () => {
    const payload = {
      tool_name: "Write",
      tool_input: {
        file_path: path.join(root, "AGENTS.md"),
        content: "実装は snotra-core/src/lib.rs",
      },
    };
    expect(checksForPayload(payload, fakeResolver)).toEqual([]);
  });

  // 現行の 'snotra-core.*\.rs"' は payload が 1 行 JSON のため
  // 「snotra-core」と「.rs"」の共起だけで発火していた
  it("old_string に snotra-core の語があっても core-test を発火しない", () => {
    const payload = {
      tool_name: "Edit",
      tool_input: {
        file_path: path.join(root, "src-tauri", "src", "main.rs"),
        old_string: "snotra-core",
        new_string: "x",
      },
    };
    expect(checksForPayload(payload, fakeResolver)).toEqual(["clippy", "tauri-test"]);
  });

  it("file_path が無ければ何もしない（I3）", () => {
    expect(checksForPayload({ tool_name: "Edit", tool_input: {} }, fakeResolver)).toEqual([]);
    expect(checksForPayload({}, fakeResolver)).toEqual([]);
  });

  it("どの .git にも属さないファイルは何もしない（I6）", () => {
    const payload = { tool_input: { file_path: path.join(path.sep, "elsewhere", "x.rs") } };
    expect(checksForPayload(payload, () => null)).toEqual([]);
  });
});

describe("extractFilePath", () => {
  it("tool_input.file_path を取り出す", () => {
    expect(extractFilePath({ tool_input: { file_path: "a.ts" } })).toBe("a.ts");
  });

  it("無い・空・非文字列なら null", () => {
    expect(extractFilePath({ tool_input: {} })).toBeNull();
    expect(extractFilePath({ tool_input: { file_path: "" } })).toBeNull();
    expect(extractFilePath({ tool_input: { file_path: 42 } })).toBeNull();
    expect(extractFilePath(null)).toBeNull();
  });
});

describe("toRelative", () => {
  it("区切りを / に正規化する", () => {
    const root = path.join(path.sep, "repo");
    const file = path.join(root, "ui", "src", "api.ts");
    expect(toRelative(root, file)).toBe("ui/src/api.ts");
  });
});

describe("resolveRoot / findUp", () => {
  let tmp;

  beforeAll(() => {
    tmp = mkdtempSync(path.join(tmpdir(), "post-edit-root-"));
    // main ツリー: .git はディレクトリ
    mkdirSync(path.join(tmp, "main", ".git"), { recursive: true });
    mkdirSync(path.join(tmp, "main", "ui", "src"), { recursive: true });
    // worktree: .git はファイル（gitdir: ...）。main ツリーの内側に置く
    mkdirSync(path.join(tmp, "main", "wt", "ui", "src"), { recursive: true });
    writeFileSync(path.join(tmp, "main", "wt", ".git"), "gitdir: /somewhere\n");
    // どの .git にも属さないツリー
    mkdirSync(path.join(tmp, "orphan"), { recursive: true });
  });

  afterAll(() => rmSync(tmp, { recursive: true, force: true }));

  it("最近接の .git ディレクトリを返す", () => {
    const file = path.join(tmp, "main", "ui", "src", "api.ts");
    expect(resolveRoot(file)).toBe(path.join(tmp, "main"));
  });

  it("worktree の .git は「ファイル」でも root として認識する", () => {
    const file = path.join(tmp, "main", "wt", "ui", "src", "api.ts");
    expect(resolveRoot(file)).toBe(path.join(tmp, "main", "wt"));
  });

  it("ネストしたとき内側が勝つ", () => {
    const inner = resolveRoot(path.join(tmp, "main", "wt", "ui", "src", "api.ts"));
    const outer = resolveRoot(path.join(tmp, "main", "ui", "src", "api.ts"));
    expect(inner).not.toBe(outer);
    expect(inner.startsWith(outer)).toBe(true);
  });

  // tmpdir の祖先に .git を持つ環境（C:\Users\<name>\.git 等）でも落ちないよう、
  // 「このツリーの内側では見つからない」ことだけを主張する。
  it("どの .git にも属さなければ、少なくともこのツリー内では root を見つけない（I6）", () => {
    const found = resolveRoot(path.join(tmp, "orphan", "x.rs"));
    expect(found === null || !found.startsWith(tmp)).toBe(true);
  });

  it("findUp は見つからなければ null", () => {
    expect(findUp(path.join(tmp, "orphan"), "no-such-marker")).toBeNull();
  });
});

describe("resolveTscBin — I17 順序依存の沈黙", () => {
  let tmp;
  const TSC_REL = path.join("node_modules", "typescript", "bin", "tsc");

  beforeAll(() => {
    tmp = mkdtempSync(path.join(tmpdir(), "post-edit-tsc-"));
    // 上位ツリーに本物の tsc
    mkdirSync(path.join(tmp, "outer", "node_modules", "typescript", "bin"), { recursive: true });
    writeFileSync(path.join(tmp, "outer", TSC_REL), "// tsc");
    // 内側の worktree。Phase 3 後、tsc 自身が buildinfo を書くときに
    // <worktree>/node_modules/.cache を作るため、空の node_modules が生える。
    mkdirSync(path.join(tmp, "outer", "wt", "node_modules", ".cache"), { recursive: true });
  });

  afterAll(() => rmSync(tmp, { recursive: true, force: true }));

  it("上位ツリーの tsc を見つける", () => {
    expect(resolveTscBin(path.join(tmp, "outer"))).toBe(path.join(tmp, "outer", TSC_REL));
  });

  // node_modules ディレクトリの存在で判定すると、2 回目以降は hook 自身が作った
  // 空ディレクトリで探索が止まる。しかも ENOENT ではなく「候補なし」なので
  // HOOK ERROR にも捕捉されず沈黙する。必ずフルパスで probe すること。
  it("root 直下に空の node_modules があっても上位の tsc を見つける", () => {
    expect(resolveTscBin(path.join(tmp, "outer", "wt"))).toBe(path.join(tmp, "outer", TSC_REL));
  });

  it("どこにも無ければ null", () => {
    expect(resolveTscBin(tmp)).toBeNull();
  });
});

describe("stripProgressLines — I14", () => {
  it("cargo の進捗行を落とし、診断を残す", () => {
    const input = [
      "   Compiling snotra-core v0.1.0 (C:\\workspace\\Snotra\\snotra-core)",
      "    Checking snotra v0.1.0",
      "    Blocking waiting for file lock on build directory",
      "snotra-core/src/lib.rs:10:5: error: unused variable `x`",
      "    Finished `dev` profile [unoptimized] target(s) in 42.13s",
    ].join("\n");
    expect(stripProgressLines(input)).toBe("snotra-core/src/lib.rs:10:5: error: unused variable `x`");
  });

  it("tsc の診断は落とさない", () => {
    const input = "ui/src/api.ts(4,7): error TS2322: Type 'string' is not assignable to type 'number'.";
    expect(stripProgressLines(input)).toBe(input);
  });

  it("空文字列は空文字列", () => {
    expect(stripProgressLines("")).toBe("");
  });
});

describe("combineStreams — 単純連結は 2>&1 と等価ではない", () => {
  // cargo test は `test result:` を stdout、`Running unittests` を stderr に出す（実測）。
  // tail 予算で単純連結すると、末尾に来た stderr が枠を食い、stdout 側の
  // 失敗テスト名が枠外へ落ちる。
  it("tail 予算では stderr を先に置き、証拠のある stdout を末尾に残す", () => {
    expect(combineStreams("test result: FAILED", "Running unittests", "tail")).toBe(
      "Running unittests\ntest result: FAILED",
    );
  });

  it("head 予算では stdout を先に置く", () => {
    expect(combineStreams("out", "err", "head")).toBe("out\nerr");
  });

  // stdout が改行で終わらないと、単純連結は最後の stdout 行と最初の stderr 行を接着する
  it("行を接着しない", () => {
    expect(combineStreams("error TS1005: x", "clippy: y", "head")).toBe("error TS1005: x\nclippy: y");
  });

  it("末尾の改行で空行を作らない", () => {
    expect(combineStreams("a\n", "b\n", "head")).toBe("a\nb");
  });

  it("片方だけ・両方空", () => {
    expect(combineStreams("a", "", "head")).toBe("a");
    expect(combineStreams("", "b", "tail")).toBe("b");
    expect(combineStreams("", "", "head")).toBe("");
    expect(combineStreams(undefined, undefined, "head")).toBe("");
  });
});

describe("takeLines — 証拠の予算適用（通知は持たない）", () => {
  const lines = (n) => Array.from({ length: n }, (_, i) => `line${i + 1}`).join("\n");

  it("予算内ならそのまま返す（head）", () => {
    expect(takeLines(lines(10), { lines: 20, from: "head" })).toBe(lines(10));
  });

  it("予算内ならそのまま返す（tail）", () => {
    expect(takeLines(lines(3), { lines: 5, from: "tail" })).toBe(lines(3));
  });

  // 検出は exit code が担うので、切り捨て通知を足す必要がない（I9）
  it("head は先頭 N 行のみを返し、余計な行を足さない", () => {
    const out = takeLines(lines(50), { lines: 20, from: "head" }).split("\n");
    expect(out).toHaveLength(20);
    expect(out).toEqual(lines(50).split("\n").slice(0, 20));
  });

  it("tail は末尾 N 行のみを返し、余計な行を足さない", () => {
    const out = takeLines(lines(50), { lines: 5, from: "tail" }).split("\n");
    expect(out).toHaveLength(5);
    expect(out).toEqual(lines(50).split("\n").slice(-5));
  });

  it("空文字列は空文字列", () => {
    expect(takeLines("", { lines: 20, from: "head" })).toBe("");
  });

  it("末尾の改行を 1 行と数えない", () => {
    expect(takeLines("a\n", { lines: 20, from: "head" })).toBe("a");
  });
});

describe("buildSection — I21 失敗の事実は決して切り捨てない", () => {
  it("id・exit code・再現コマンド・証拠をこの順で含む", () => {
    const out = buildSection({
      id: "clippy",
      status: "exit 101",
      repro: "cargo clippy --workspace  (cwd: /repo)",
      evidence: "snotra-core/src/lib.rs:10:5: error: unused",
    });
    expect(out).toContain("clippy");
    expect(out).toContain("exit 101");
    expect(out).toContain("cargo clippy --workspace");
    expect(out).toContain("cwd: /repo");
    expect(out).toContain("error: unused");
  });

  // 証拠が予算で全部消えても、失敗の事実と再現手段は残らねばならない
  it("証拠が空でも id・exit code・再現コマンドは必ず残る", () => {
    const out = buildSection({
      id: "typecheck",
      status: "exit 2",
      repro: "node tsc -p tsconfig.json  (cwd: /repo)",
      evidence: "",
    });
    expect(out).toContain("typecheck");
    expect(out).toContain("exit 2");
    expect(out).toContain("node tsc -p tsconfig.json");
    expect(out.split("\n")).toHaveLength(2); // 見出し + 再現。空行を足さない
  });
});

describe("buildEnvelope — I15 JSON の妥当性", () => {
  it("診断に \" と改行と Windows パスが含まれても妥当な JSON", () => {
    const context = 'C:\\workspace\\Snotra\\ui\\src\\api.ts(1,1): error TS1005: "," expected.\n2 行目';
    const json = buildEnvelope({ context });
    const parsed = JSON.parse(json);
    expect(parsed.hookSpecificOutput.hookEventName).toBe("PostToolUse");
    expect(parsed.hookSpecificOutput.additionalContext).toBe(context);
  });

  it("systemMessage を載せられる", () => {
    const parsed = JSON.parse(buildEnvelope({ systemMessage: "WARN: x" }));
    expect(parsed.systemMessage).toBe("WARN: x");
  });

  it("どちらも空なら null（何も出力しない）", () => {
    expect(buildEnvelope({})).toBeNull();
  });
});

describe("resolveTarget — main() と同じ配線を通る", () => {
  const root = path.join(path.sep, "repo");
  const fakeResolver = () => root;

  it("root / rel / ids を返す", () => {
    const payload = { tool_input: { file_path: path.join(root, "ui", "src", "api.ts") } };
    expect(resolveTarget(payload, fakeResolver)).toEqual({
      root,
      rel: "ui/src/api.ts",
      ids: ["typecheck"],
    });
  });

  it("file_path が無ければ null（I3）", () => {
    expect(resolveTarget({ tool_input: {} }, fakeResolver)).toBeNull();
  });

  it("どの .git にも属さなければ null（I6）", () => {
    expect(resolveTarget({ tool_input: { file_path: "x.rs" } }, () => null)).toBeNull();
  });
});

// ここまでの検証はすべて純関数。main() / I13 のガード / エンベロープの実出力は
// プロセスを起動しないと守れない。cargo も tsc も起動しない payload だけを使う。
describe("統合: post-edit.mjs をプロセスとして起動する", () => {
  const SCRIPT = fileURLToPath(new URL("./post-edit.mjs", import.meta.url));
  const REPO = fileURLToPath(new URL("../../", import.meta.url));

  const runHook = (input) =>
    spawnSync(process.execPath, [SCRIPT], {
      input: typeof input === "string" ? input : JSON.stringify(input),
      encoding: "utf8",
    });

  it("§1 content 末尾の ui/src/api.ts に反応せず、無音で exit 0", () => {
    const res = runHook({
      tool_name: "Write",
      tool_input: { file_path: path.join(REPO, "docs", "notes.md"), content: "参照は ui/src/api.ts" },
    });
    expect(res.status).toBe(0);
    expect(res.stdout).toBe("");
  });

  it("§5 tsconfig の include 対象外は「対象外」と言う（沈黙させない・I16）", () => {
    // scripts/ 配下は program 外（実在は不要 — 判定はパス文字列のみ）。#474 の include 拡張で
    // 実ソースの .ts はほぼ全て対象になったため、対象外の例はここに残るだけになった。
    // vite.config.ts 等を使うと typecheck が発火し、統合ブロックの契約
    // 「cargo も tsc も起動しない payload だけを使う」を破る。
    const res = runHook({
      tool_name: "Edit",
      tool_input: { file_path: path.join(REPO, "scripts", "example.ts"), old_string: "a", new_string: "b" },
    });
    expect(res.status).toBe(0);
    const parsed = JSON.parse(res.stdout);
    expect(parsed.hookSpecificOutput.additionalContext).toContain("include 対象外");
  });

  it("config-warn は systemMessage に出る", () => {
    // config.toml はリポジトリ非実在だが判定はパス文字列のみ。実在の src-tauri/tauri.conf.json を
    // 使うと csp-test が発火して vitest を再帰 spawn し、このブロックの契約を破る。
    const res = runHook({
      tool_name: "Edit",
      tool_input: { file_path: path.join(REPO, "config.toml"), old_string: "a", new_string: "b" },
    });
    expect(res.status).toBe(0);
    const parsed = JSON.parse(res.stdout);
    expect(parsed.systemMessage).toContain("WARN");
    expect(parsed.hookSpecificOutput).toBeUndefined();
  });

  it("不正な payload は HOOK ERROR を両フィールドへ出し、exit 0（I8）", () => {
    const res = runHook("{ not json");
    expect(res.status).toBe(0);
    const parsed = JSON.parse(res.stdout);
    expect(parsed.hookSpecificOutput.additionalContext).toContain("HOOK ERROR");
    expect(parsed.systemMessage).toContain("HOOK ERROR");
  });

  it("file_path が無い payload は無音で exit 0（I3）", () => {
    const res = runHook({ tool_name: "Edit", tool_input: {} });
    expect(res.status).toBe(0);
    expect(res.stdout).toBe("");
  });

  // I13: import しただけで stdin 読み取りが走ると npm test が停止する
  it("import しただけでは main() が走らない（I13）", () => {
    const res = spawnSync(process.execPath, ["-e", `import(${JSON.stringify(pathToFileURL(SCRIPT).href)}).then(() => console.log("imported"))`], {
      encoding: "utf8",
      timeout: 10_000,
    });
    expect(res.status).toBe(0);
    expect(res.stdout).toContain("imported");
  });
});

describe("tsconfig ドリフト検出カナリア — C2", () => {
  // selectChecks の typecheck 条件は tsconfig の include - exclude と一致していなければならない。
  // tsconfig を触った人がここで気づく。真実源は `tsc --listFilesOnly` の program 内容であり、
  // glob の見た目ではない（git の '**/' と TypeScript の '**/' は意味が違う）。
  it("tsconfig の include / exclude が selectChecks の前提と一致する", () => {
    const tsconfigPath = fileURLToPath(new URL("../../tsconfig.json", import.meta.url));
    const tsconfig = JSON.parse(readFileSync(tsconfigPath, "utf8"));
    expect(tsconfig.include).toEqual([
      "ui/src",
      "e2e",
      "vite.config.ts",
      "vitest.config.ts",
      "playwright.tauri.config.ts",
    ]);
    expect(tsconfig.exclude).toEqual([]);
  });
});

// #497: 発火の追加とカナリアの追加は対にする。カナリアが無いファイルに検査を撃っても、
// そのファイルについては何も検証しない（vitest が起動することしか証明しない）。
// 以下 2 本は package.json / vitest.config.ts を hook-selftest に載せる根拠である。

describe("vitest.config.ts ドリフト検出カナリア — #497", () => {
  // include が縮むと、hook-selftest / githooks-selftest / npm test が静かにテストを
  // 走らせなくなる。`vitest run .claude/hooks` はパス直指定で動くが、`.githooks` の
  // 収集と CI の npm test はこの include に依存する。
  it("vitest の include が 3 つの検査対象をすべて含む", () => {
    const p = fileURLToPath(new URL("../../vitest.config.ts", import.meta.url));
    const src = readFileSync(p, "utf8");
    expect(src).toContain('"ui/src/**/*.test.{ts,tsx}"');
    expect(src).toContain('".claude/hooks/**/*.test.mjs"');
    expect(src).toContain('".githooks/**/*.test.mjs"');
  });
});

describe("package.json ドリフト検出カナリア — #497", () => {
  // prepare は Layer 1（.githooks/）の唯一の bootstrap である。消えると
  // core.hooksPath が設定されず、main 保護のローカル層が沈黙して失われる。
  // test / typecheck は docs/build-commands.md（SSOT）が名指しするコマンド。
  it("scripts が Layer 1 の bootstrap と SSOT のコマンドを保持する", () => {
    const p = fileURLToPath(new URL("../../package.json", import.meta.url));
    const pkg = JSON.parse(readFileSync(p, "utf8"));
    expect(pkg.scripts.prepare).toContain("core.hooksPath .githooks");
    expect(pkg.scripts.test).toBe("vitest run");
    expect(pkg.scripts.typecheck).toBe("tsc");
  });
});

describe("Cargo.toml members ドリフト検出カナリア — #500", () => {
  // check / clippy は --workspace なので members を cargo が読む（写しは消えた）。
  // しかし `cargo test -p <crate>` は「編集した crate → そのテスト」の写像であり、
  // selectChecks のディレクトリ接頭辞と buildCommand の case に手書きされている。
  // 4 つ目の crate が生えると、その .rs を編集してもテストが**静かに走らない**。
  // ここで落とし、selectChecks / buildCommand / ci.yml / SSOT の更新を強制する。
  it("workspace members が test 系のルーティングと一致する", () => {
    const p = fileURLToPath(new URL("../../Cargo.toml", import.meta.url));
    const src = readFileSync(p, "utf8");

    // [workspace] セクションにスコープする。他セクション（[workspace.metadata.*] 等）の
    // members を誤読しないため。非マッチを握り潰さない — 書式が変わったら
    // 「読めなかった」と落ちる（fail-closed）。
    const section = src.match(/\[workspace\]\r?\n([\s\S]*?)(?=\r?\n\[|$)/);
    expect(section, "Cargo.toml の [workspace] セクションを読めなかった").not.toBeNull();

    const m = section[1].match(/^members\s*=\s*\[([^\]]*)\]/m);
    expect(m, "[workspace] の members 行を読めなかった（書式が変わった）").not.toBeNull();

    const members = m[1]
      .split(",")
      .map((s) => s.trim().replace(/^"|"$/g, ""))
      .filter((s) => s.length > 0);
    expect(members).toEqual(["snotra-core", "src-tauri", "snotra-settings"]);
  });
});
