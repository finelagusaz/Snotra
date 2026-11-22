import { afterAll, describe, it, expect } from "vitest";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync, appendFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import {
  fastForwardMain,
  parseArgs,
  UsageError,
  findClosingKeywords,
  titleClosesIssue,
  normalizeRef,
  diffClosing,
  isIsoTimestamp,
  run,
  realDeps,
  SEARCH_LIMIT,
} from "./merge-pr.mjs";

const HEAD = "a".repeat(40);
const MERGE = "m".repeat(40);
const ARGS = ["7", "--closes", "3", "--body-file", "body.md"];

/**
 * 偽の gh / git。`over` で既定の応答を差し替える。応答は `{ status, stdout, stderr }` か、
 * 呼び出しの回数ごとに変えるなら関数。呼ばれた gh のサブコマンドを `calls` に積む
 */
function fake(over = {}) {
  const calls = [];
  const ok = (v) => ({ status: 0, stdout: JSON.stringify(v), stderr: "" });
  const table = {
    "repo view": ok({ nameWithOwner: "o/r" }),
    "pr view:pre": ok({
      number: 7,
      state: "OPEN",
      mergeStateStatus: "CLEAN",
      title: "chore: 何かを直す",
      headRefName: "chore/x",
      headRefOid: HEAD,
      closingIssuesReferences: [{ number: 3, repository: { name: "r", owner: { login: "o" } } }],
    }),
    "pr list:open": ok([]),
    "cat-file": { status: 0, stdout: "", stderr: "" },
    fetch: { status: 0, stdout: "", stderr: "" },
    "pr merge": { status: 0, stdout: "", stderr: "" },
    "pr view:post": ok({ state: "MERGED", mergedAt: "2026-10-31T01:02:03Z", mergeCommit: { oid: MERGE } }),
    "issue view": ok({ state: "CLOSED" }),
    "pr list:merged": ok([{ number: 7 }]),
    "issue list": ok([{ number: 3 }]),
    ...over,
  };
  const keyOf = (cmd, args) => {
    if (cmd === "git") return args[0];
    const k = `${args[0]} ${args[1]}`;
    if (k === "pr view") return args.some((a) => a.startsWith("state,mergedAt")) ? "pr view:post" : "pr view:pre";
    if (k === "pr list") return args.includes("merged") ? "pr list:merged" : "pr list:open";
    return k;
  };
  const counts = {};
  const deps = {
    exec: (cmd, args) => {
      const key = keyOf(cmd, args);
      calls.push(key);
      counts[key] = (counts[key] ?? 0) + 1;
      const r = table[key];
      if (r === undefined) throw new Error(`想定外の呼び出し: ${cmd} ${args.join(" ")}`);
      return typeof r === "function" ? r(counts[key], args) : r;
    },
    readFile: () => "本文\n",
    mainRoot: () => "/repo",
    verify: over.verify ?? (() => ({ ok: true })),
    cleanup: over.cleanup ?? ((root, branch) => (cleanups.push({ root, branch, after: [...calls] }), { ok: true, notes: ["削除: worktree"] })),
    syncMain:
      over.syncMain ??
      ((root, mergeCommit) => (syncs.push({ root, mergeCommit, afterCleanup: cleanups.length }), { ok: true, notes: ["本体の main を進めた: x..y"] })),
    sleep: (ms) => sleeps.push(ms),
    out: (s) => out.push(s),
    err: (s) => out.push(s),
  };
  const out = [];
  const sleeps = [];
  const cleanups = [];
  const syncs = [];
  return { deps, calls, out, sleeps, cleanups, syncs, text: () => out.join("\n") };
}
const ok = (v) => ({ status: 0, stdout: JSON.stringify(v), stderr: "" });
const preview = (patch) =>
  ok({
    number: 7,
    state: "OPEN",
    mergeStateStatus: "CLEAN",
    title: "chore: 何かを直す",
    headRefName: "chore/x",
    headRefOid: HEAD,
    closingIssuesReferences: [{ number: 3, repository: { name: "r", owner: { login: "o" } } }],
    ...patch,
  });

describe("run — 正常経路", () => {
  it("すべて通ればマージし、宣言どおりに閉じたことを確かめて 0", () => {
    const f = fake();
    expect(run(ARGS, "/repo", f.deps)).toBe(0);
    expect(f.calls).toContain("pr merge");
    expect(f.text()).toContain("マージ済み");
  });
  it("マージは --squash・--body-file・--match-head-commit だけで打つ（件名・auto・ブランチ削除を渡さない）", () => {
    let mergeArgs;
    const f = fake({ "pr merge": (_, args) => ((mergeArgs = args), { status: 0, stdout: "", stderr: "" }) });
    run(ARGS, "/repo", f.deps);
    expect(mergeArgs).toEqual(["pr", "merge", "7", "--squash", "--body-file", expect.stringMatching(/body\.md$/), "--match-head-commit", HEAD]);
  });
  it("head の commit が手元に無ければ PR の ref を取ってから照合する", () => {
    let probes = 0;
    const f = fake({ "cat-file": () => ({ status: probes++ === 0 ? 1 : 0, stdout: "", stderr: "" }) });
    expect(run(ARGS, "/repo", f.deps)).toBe(0);
    expect(f.calls).toContain("fetch"); // 無いと indexOf が -1 を返し、下の比較だけでは検出されない
    expect(f.calls.indexOf("fetch")).toBeLessThan(f.calls.indexOf("pr merge"));
  });
});

describe("run — マージ済みのブランチのツールの worktree と要約を片づける（#1359）", () => {
  it("exit 0 のときだけ、本体のルートと PR の head のブランチを渡して片づけ、結果を出す", () => {
    const f = fake();
    expect(run(ARGS, "/repo", f.deps)).toBe(0);
    expect(f.cleanups).toHaveLength(1);
    expect(f.cleanups[0].root).toBe("/repo");
    expect(f.cleanups[0].branch).toBe("chore/x");
    expect(f.cleanups[0].after).toContain("issue list"); // 事後検証を終えてから
    expect(f.text()).toContain("削除: worktree");
  });
  it("片づけられなくても exit 0 のまま（マージは済んでいる）。理由を出す", () => {
    const f = fake({ cleanup: () => ({ ok: false, notes: ["消せなかった: worktree"] }) });
    expect(run(ARGS, "/repo", f.deps)).toBe(0);
    expect(f.text()).toContain("片づけられなかった");
    expect(f.text()).toContain("消せなかった: worktree");
  });
  it("片づけが投げても exit 0 のまま。理由を出す", () => {
    const f = fake({
      cleanup: () => {
        throw new Error("想定外");
      },
    });
    expect(run(ARGS, "/repo", f.deps)).toBe(0);
    expect(f.text()).toContain("片づけられなかった");
    expect(f.text()).toContain("想定外");
  });
  it("事後検証が赤（exit 3）なら片づけない——人が調べる材料を残す", () => {
    const f = fake({ "issue view": ok({ state: "OPEN" }) });
    expect(run(ARGS, "/repo", f.deps)).toBe(3);
    expect(f.cleanups).toHaveLength(0);
  });
  it("マージ前に止めた（exit 1）なら片づけない", () => {
    const f = fake({ "pr view:pre": preview({ state: "CLOSED" }) });
    expect(run(ARGS, "/repo", f.deps)).toBe(1);
    expect(f.cleanups).toHaveLength(0);
  });
  it("--dry-run では片づけない", () => {
    const f = fake();
    expect(run([...ARGS, "--dry-run"], "/repo", f.deps)).toBe(0);
    expect(f.cleanups).toHaveLength(0);
  });
});

describe("run — マージ後に本体のローカル main を進める（#1528）", () => {
  it("exit 0 のときだけ、片づけの後に、本体のルートとマージ commit を渡して進め、結果を出す", () => {
    const f = fake();
    expect(run(ARGS, "/repo", f.deps)).toBe(0);
    expect(f.syncs).toEqual([{ root: "/repo", mergeCommit: MERGE, afterCleanup: 1 }]);
    expect(f.text()).toContain("本体の main を進めた: x..y");
  });
  it("マージ commit が取れなければ null を渡す（判定は fastForwardMain が持つ）", () => {
    const f = fake({ "pr view:post": ok({ state: "MERGED", mergedAt: "2026-10-31T01:02:03Z", mergeCommit: null }) });
    expect(run(ARGS, "/repo", f.deps)).toBe(0);
    expect(f.syncs[0].mergeCommit).toBeNull();
  });
  it("進められなくても exit 0 のまま。理由を出す", () => {
    const f = fake({ syncMain: () => ({ ok: false, notes: ["本体の main を進めなかった: 本体が main に居ない（feat/y）"] }) });
    expect(run(ARGS, "/repo", f.deps)).toBe(0);
    expect(f.text()).toContain("本体が main に居ない（feat/y）");
  });
  it("進める処理が投げても exit 0 のまま。理由を出す", () => {
    const f = fake({
      syncMain: () => {
        throw new Error("想定外の失敗");
      },
    });
    expect(run(ARGS, "/repo", f.deps)).toBe(0);
    expect(f.text()).toContain("本体の main を進めなかった");
    expect(f.text()).toContain("想定外の失敗");
  });
  it("事後検証が赤（マージ済みの exit 3）なら進めず、確かめてから打つコマンドを案内する", () => {
    const f = fake({ "issue view": ok({ state: "OPEN" }) });
    expect(run(ARGS, "/repo", f.deps)).toBe(3);
    expect(f.syncs).toHaveLength(0);
    expect(f.text()).toContain("本体の main は進めていない");
    expect(f.text()).toContain('git -C "/repo" merge --ff-only origin/main');
  });
  it("マージの成否を確かめられない（exit 3）なら、進めず案内もしない", () => {
    const f = fake({ "pr view:post": { status: 1, stdout: "", stderr: "boom" } });
    expect(run(ARGS, "/repo", f.deps)).toBe(3);
    expect(f.syncs).toHaveLength(0);
    expect(f.text()).not.toContain("本体の main は進めていない");
  });
  it("マージ前に止めた（exit 1）・走らなかった（exit 2）・--dry-run では進めない", () => {
    const f1 = fake({ "pr view:pre": preview({ state: "CLOSED" }) });
    expect(run(ARGS, "/repo", f1.deps)).toBe(1);
    const f2 = fake({ "repo view": { status: 1, stdout: "", stderr: "no gh" } });
    expect(run(ARGS, "/repo", f2.deps)).toBe(2);
    const f3 = fake();
    expect(run([...ARGS, "--dry-run"], "/repo", f3.deps)).toBe(0);
    expect([f1.syncs, f2.syncs, f3.syncs]).toEqual([[], [], []]);
  });
  it("実際の依存は fastForwardMain へ直結している", () => {
    expect(realDeps.syncMain).toBe(fastForwardMain);
  });
});

describe("run — マージ前に止める（gh pr merge を呼ばない・exit 1）", () => {
  it.each([
    ["PR が OPEN でない", { "pr view:pre": preview({ state: "CLOSED" }) }, "OPEN でない"],
    ["必須チェック未通過など CLEAN でない", { "pr view:pre": preview({ mergeStateStatus: "BLOCKED" }) }, "CLEAN でない"],
    [
      "宣言に無い issue を本文が閉じる",
      { "pr view:pre": preview({ closingIssuesReferences: [{ number: 3, repository: { name: "r", owner: { login: "o" } } }, { number: 9, repository: { name: "r", owner: { login: "o" } } }] }) },
      "o/r#9",
    ],
    [
      "他リポジトリの issue を本文が閉じる",
      { "pr view:pre": preview({ closingIssuesReferences: [{ number: 3, repository: { name: "r", owner: { login: "o" } } }, { number: 3, repository: { name: "y", owner: { login: "x" } } }] }) },
      "x/y#3",
    ],
    ["宣言した issue を本文が閉じない", { "pr view:pre": preview({ closingIssuesReferences: [] }) }, "o/r#3"],
    // 2 つの検査の片方だけに当たるタイトルをそれぞれ置く（両方に当たる 1 種類では片方を消しても緑のまま）
    ["タイトルの型名（scope つき）の直後に番号", { "pr view:pre": preview({ title: "fix(core): #985 直す" }) }, "タイトル"],
    ["タイトルの散文の closing keyword", { "pr view:pre": preview({ title: "chore: closes #3" }) }, "タイトル"],
    ["base にする open PR がある", { "pr list:open": ok([{ number: 8 }]) }, "#8"],
    ["要約の照合に通らない", { verify: () => ({ ok: false, reason: "要約が無い" }) }, "gh pr checkout 7"],
  ])("%s", (_, over, expected) => {
    const f = fake(over);
    expect(run(ARGS, "/repo", f.deps)).toBe(1);
    expect(f.calls).not.toContain("pr merge");
    expect(f.text()).toContain("マージしていない");
    expect(f.text()).toContain(expected);
  });
  it("--body-file に closing keyword があれば止める", () => {
    const f = fake();
    f.deps.readFile = () => "まとめ\n\nfixes #3\n";
    expect(run(ARGS, "/repo", f.deps)).toBe(1);
    expect(f.calls).not.toContain("pr merge");
  });
  it("要約の照合は PR の head ブランチ名と head の sha で行う（今のブランチと HEAD ではない）", () => {
    let seen;
    const f = fake({ verify: (root, branch, target) => ((seen = { root, branch, target }), { ok: true }) });
    run(ARGS, "/repo", f.deps);
    expect(seen).toEqual({ root: "/repo", branch: "chore/x", target: HEAD });
  });
  it("止める理由はすべて並べる（1 件ずつ直して再実行させない）", () => {
    const f = fake({ "pr view:pre": preview({ mergeStateStatus: "DIRTY", title: "fix: #3 x" }), "pr list:open": ok([{ number: 8 }]) });
    expect(run(ARGS, "/repo", f.deps)).toBe(1);
    expect(f.text()).toContain("止めた理由 3 件");
  });
});

describe("run — --dry-run（検査をすべて行い、gh pr merge を呼ばない）", () => {
  const DRY = [...ARGS, "--dry-run"];
  it("検査がすべて通れば、マージする内容を出して 0。gh pr merge も事後の照会も呼ばない", () => {
    const f = fake();
    f.deps.readFile = () => "まとめの 1 行目\n2 行目\n";
    expect(run(DRY, "/repo", f.deps)).toBe(0);
    expect(f.calls).not.toContain("pr merge");
    expect(f.calls).not.toContain("pr view:post");
    expect(f.text()).toContain("マージしていない");
    expect(f.text()).toContain(HEAD);
    expect(f.text()).toContain("o/r#3");
    expect(f.text()).toContain("まとめの 1 行目");
  });
  it("マージ前の検査は本番と同じ呼び出しを同じ順で行う（照合を省かない）", () => {
    const real = fake();
    run(ARGS, "/repo", real.deps);
    const dry = fake();
    run(DRY, "/repo", dry.deps);
    expect(dry.calls).toEqual(real.calls.slice(0, real.calls.indexOf("pr merge")));
  });
  it.each([
    ["CLEAN でない", { "pr view:pre": preview({ mergeStateStatus: "BLOCKED" }) }, "CLEAN でない"],
    ["宣言した issue を本文が閉じない", { "pr view:pre": preview({ closingIssuesReferences: [] }) }, "宣言した issue を PR 本文が閉じない"],
    ["要約の照合に通らない", { verify: () => ({ ok: false, reason: "要約が無い" }) }, "要約が無い"],
  ])("止める理由があれば dry-run でも 1: %s", (_, over, expected) => {
    const f = fake(over);
    expect(run(DRY, "/repo", f.deps)).toBe(1);
    expect(f.calls).not.toContain("pr merge");
    expect(f.text()).toContain("止めた理由 1 件");
    expect(f.text()).toContain(expected);
    expect(f.text()).not.toContain("dry-run——");
  });
  it("npm が --dry-run を自分の設定として食った形（npm_config_dry_run=true）でも dry-run として扱う", () => {
    const f = fake();
    f.deps.env = { npm_config_dry_run: "true" };
    expect(run(ARGS, "/repo", f.deps)).toBe(0);
    expect(f.calls).not.toContain("pr merge");
    expect(f.text()).toContain("dry-run——マージしていない");
  });
  it("npm_config_dry_run が true 以外なら本番（dry-run の拾い方を広げすぎない）", () => {
    const f = fake();
    f.deps.env = { npm_config_dry_run: "" };
    expect(run(ARGS, "/repo", f.deps)).toBe(0);
    expect(f.calls).toContain("pr merge");
  });
});

describe("run — 走らなかった（exit 2・マージしていない）", () => {
  it.each([
    ["使用法の誤り", ["7", "--subject", "x"], {}, "受け付けない引数: --subject"],
    ["gh pr view の失敗", ARGS, { "pr view:pre": { status: 1, stdout: "", stderr: "boom" } }, "boom"],
    ["head の sha の形が違う", ARGS, { "pr view:pre": preview({ headRefOid: "--output=x" }) }, "head の sha の形が違う"],
    ["fetch しても head が無い", ARGS, { "cat-file": { status: 1, stdout: "", stderr: "" }, fetch: { status: 1, stdout: "", stderr: "x" } }, "git fetch -q origin refs/pull/7/head"],
  ])("%s", (_, argv, over, reason) => {
    const f = fake(over);
    expect(run(argv, "/repo", f.deps)).toBe(2);
    expect(f.calls).not.toContain("pr merge");
    // 理由まで見る: マージ前の try の catch-all は、どの例外も 2 にする（#1319）
    expect(f.text()).toContain(reason);
  });
  it("本体のルートを導けなければ止める", () => {
    const f = fake();
    f.deps.mainRoot = () => {
      throw new Error("git の共通ディレクトリから本体の作業ツリーを導けない");
    };
    expect(run(ARGS, "/repo", f.deps)).toBe(2);
    expect(f.calls).not.toContain("pr merge");
    expect(f.text()).toContain("git の共通ディレクトリから本体の作業ツリーを導けない");
  });
});

describe("run — マージの成否は PR の状態で確定する", () => {
  it("gh pr merge が非 0 でも MERGED なら事後検証へ進む", () => {
    const f = fake({ "pr merge": { status: 1, stdout: "", stderr: "warn" } });
    expect(run(ARGS, "/repo", f.deps)).toBe(0);
    expect(f.calls).toContain("issue list");
  });
  it("--match-head-commit の不一致などで MERGED にならなければ 1（マージしていない）", () => {
    const f = fake({
      "pr merge": { status: 1, stdout: "", stderr: "Head branch was modified" },
      "pr view:post": ok({ state: "OPEN", mergedAt: null }),
    });
    expect(run(ARGS, "/repo", f.deps)).toBe(1);
    expect(f.calls).not.toContain("issue list");
  });
  it("gh pr merge が成功を返したのに MERGED でなければ、反映の遅れでありうるので 3（1 と断定しない）", () => {
    const f = fake({ "pr view:post": ok({ state: "OPEN", mergedAt: null }) });
    expect(run(ARGS, "/repo", f.deps)).toBe(3);
    expect(f.text()).toContain("gh pr merge は成功を返したが");
  });
  it("gh pr merge の呼び出しが例外を投げても 2 ではなく 3（マージの後かもしれない）", () => {
    const f = fake({
      "pr merge": () => {
        throw new Error("boom");
      },
    });
    expect(run(ARGS, "/repo", f.deps)).toBe(3);
    expect(f.text()).toContain("boom");
  });
  it("gh pr merge が失敗を返し、直後は OPEN でも、引き直して MERGED なら事後検証へ進む（反映の遅れ）", () => {
    const f = fake({
      "pr merge": { status: 1, stdout: "", stderr: "timeout" },
      "pr view:post": (n) => ok(n === 1 ? { state: "OPEN", mergedAt: null } : { state: "MERGED", mergedAt: "2026-10-31T01:02:03Z" }),
    });
    expect(run(ARGS, "/repo", f.deps)).toBe(0);
    expect(f.calls.filter((c) => c === "pr view:post")).toHaveLength(2);
  });
  it.each([
    ["gh 非 0 → MERGED → 宣言外 close", { "pr merge": { status: 1, stdout: "", stderr: "w" }, "issue list": ok([{ number: 3 }, { number: 5 }]) }, 3, "merge-pr: マージ済み"],
    ["gh 非 0 → 状態を取れない", { "pr merge": { status: 1, stdout: "", stderr: "w" }, "pr view:post": { status: 1, stdout: "", stderr: "x" } }, 3, "merge-pr: マージの成否を確かめられない"],
    ["gh 非 0 → OPEN のまま", { "pr merge": { status: 1, stdout: "", stderr: "w" }, "pr view:post": ok({ state: "OPEN", mergedAt: null }) }, 1, "merge-pr: マージしていない"],
    ["gh 非 0 → MERGED → 緑", { "pr merge": { status: 1, stdout: "", stderr: "w" } }, 0, "merge-pr: マージ済み"],
  ])("出力の最初の行は常に判定である（gh の失敗の通知はそのあと）: %s", (_, over, code, head) => {
    const f = fake(over);
    expect(run(ARGS, "/repo", f.deps)).toBe(code);
    expect(f.text().split("\n")[0]).toMatch(new RegExp(`^${head}`));
    expect(f.text()).toContain("gh pr merge は exit 1");
  });
  // catch は 3 か所（マージ前・マージ区間・事後）にあり、それぞれへ null を届ける
  it.each([
    ["マージ前（2）", "repo view", 2],
    ["マージ区間（3）", "pr merge", 3],
    ["事後（3）", "issue view", 3],
  ])("null を throw されても catch の中で落ちない（node の exit 1 にならない）: %s", (_, key, code) => {
    const f = fake({
      [key]: () => {
        throw null;
      },
    });
    expect(run(ARGS, "/repo", f.deps)).toBe(code);
  });
  it("状態を確かめられなければ 3（マージしたかもしれない）", () => {
    const f = fake({ "pr view:post": { status: 1, stdout: "", stderr: "x" } });
    expect(run(ARGS, "/repo", f.deps)).toBe(3);
    expect(f.text()).toContain("マージの成否を確かめられない");
  });
});

describe("run — マージ後の検証（落ちたら exit 3・最初に「マージ済み」を示す）", () => {
  const post = (over) => {
    const f = fake(over);
    const code = run(ARGS, "/repo", f.deps);
    return { code, text: f.text(), calls: f.calls };
  };
  it("mergedAt が ISO 8601 でなければ検索に渡さない", () => {
    const r = post({ "pr view:post": ok({ state: "MERGED", mergedAt: "08/10/2026 10:26:23" }) });
    expect(r.code).toBe(3);
    expect(r.calls).not.toContain("issue list");
    expect(r.text).toContain("ISO 8601 でない");
  });
  it("宣言した issue が閉じていなければ 3", () => {
    const r = post({ "issue view": ok({ state: "OPEN" }) });
    expect(r.code).toBe(3);
    expect(r.text).toContain("宣言した o/r#3");
  });
  it("宣言していない issue が閉じていれば 3（reopen はしない）", () => {
    const r = post({ "issue list": ok([{ number: 3 }, { number: 5 }]) });
    expect(r.code).toBe(3);
    expect(r.text).toContain("#5");
    expect(r.calls).not.toContain("issue reopen");
  });
  const NONE = ["7", "--closes", "none", "--body-file", "b.md"];
  const noRefs = { "pr view:pre": preview({ closingIssuesReferences: [] }), "issue list": ok([]) };
  it("検算: 当の PR が merged の検索に現れなければ、0 件を信用せず 3", () => {
    const f = fake({ ...noRefs, "pr list:merged": ok([]) });
    expect(run(NONE, "/repo", f.deps)).toBe(3);
    expect(f.text()).toContain("検算が通らない");
  });
  it("宣言なし・検算あり・0 件なら 0（0 件を受け取ってよいのは検算が通ったときだけ）", () => {
    const f = fake(noRefs);
    expect(run(NONE, "/repo", f.deps)).toBe(0);
    expect(f.calls).toEqual(expect.arrayContaining(["pr merge", "pr list:merged", "issue list"]));
  });
  it("索引の遅延: 宣言した issue が後から現れれば 0", () => {
    const r = post({ "issue list": (n) => ok(n === 1 ? [] : [{ number: 3 }]) });
    expect(r.code).toBe(0);
    expect(r.calls.filter((c) => c === "issue list")).toHaveLength(2);
  });
  it("索引の遅延: 再試行しても現れなければ 3", () => {
    const r = post({ "issue list": ok([]) });
    expect(r.code).toBe(3);
    expect(r.text).toContain("closed の検索に現れない");
  });
  it(`検索が上限（${SEARCH_LIMIT} 件）に達したら打ち切りと区別できないので 3`, () => {
    const many = Array.from({ length: SEARCH_LIMIT }, (_, i) => ({ number: i === 0 ? 3 : 1000 + i }));
    const r = post({ "issue list": ok(many) });
    expect(r.code).toBe(3);
    // 宣言外 close の検出でも 3 になるので、止めた理由まで見る（終了コードだけでは上限判定を消しても緑）
    expect(r.text).toContain("上限");
  });
  // ---- 観測のループ（#1321）。待ちは 3 段（状態・検算・closed 検索）すべてに掛かる ----
  const TWO = ["7", "--closes", "3,4", "--body-file", "b.md"];
  const twoRefs = preview({
    closingIssuesReferences: [3, 4].map((number) => ({ number, repository: { name: "r", owner: { login: "o" } } })),
  });
  // 観測の回数は、`issue view` の 1 件目（#3）が呼ばれた回数で数える（呼び出し回数はサブコマンドごとに数えられるため）
  const byRound = (fn) => {
    let round = 0;
    return (_, args) => {
      if (args[2] === "3") round++;
      return fn(round, args[2]);
    };
  };
  it("2 件のうち 1 件だけが遅れて閉じる（PR #1313 の実形）→ 待って観測し直し、0", () => {
    const f = fake({
      "pr view:pre": twoRefs,
      "issue view": byRound((round, n) => ok({ state: n === "4" && round === 1 ? "OPEN" : "CLOSED" })),
      "issue list": ok([{ number: 3 }, { number: 4 }]),
    });
    expect(run(TWO, "/repo", f.deps)).toBe(0);
    expect(f.sleeps).toEqual([5_000]);
    expect(f.text()).toContain("観測 2 回・待ち計 5 秒");
  });
  it("検算の検索に PR が後から現れる → 待って観測し直し、0", () => {
    const f = fake({ "pr list:merged": (n) => ok(n === 1 ? [] : [{ number: 7 }]) });
    expect(run(ARGS, "/repo", f.deps)).toBe(0);
    expect(f.sleeps).toEqual([5_000]);
  });
  it("最後まで揃わなければ、待ちを使い切って 3（観測 4 回・待ち計 30 秒）", () => {
    // 本当に OPEN のままの issue は、closed の検索にも現れない（実際の GitHub と同じ形）
    const f = fake({ "issue view": ok({ state: "OPEN" }), "issue list": ok([]) });
    expect(run(ARGS, "/repo", f.deps)).toBe(3);
    expect(f.sleeps).toEqual([5_000, 10_000, 15_000]);
    expect(f.text()).toContain("観測 4 回・待ち計 30 秒");
    expect(f.text()).toContain("o/r#3 が閉じていない");
    // 本当に OPEN の issue は closed の検索に現れなくて当然なので、索引の未反映とは言わない
    expect(f.text()).not.toContain("索引が未反映");
  });
  it("検算が最後まで通らなければ、closed の検索を信用しない——宣言外の close も未反映も出さない（依存の順）", () => {
    const f = fake({ "pr list:merged": ok([]), "issue list": ok([{ number: 5 }]) });
    expect(run(ARGS, "/repo", f.deps)).toBe(3);
    expect(f.text()).toContain("検算");
    expect(f.text()).not.toContain("#5");
    expect(f.text()).not.toContain("索引が未反映");
  });
  it("他リポジトリの同じ番号が OPEN でも、このリポジトリの issue の索引の未反映は出す（OPEN の除外は同リポジトリに限る）", () => {
    const f = fake({
      "pr view:pre": preview({
        closingIssuesReferences: [
          { number: 3, repository: { name: "r", owner: { login: "o" } } },
          { number: 3, repository: { name: "y", owner: { login: "x" } } },
        ],
      }),
      "issue view": (_, args) => ok({ state: args[4] === "x/y" ? "OPEN" : "CLOSED" }),
      "issue list": ok([]),
    });
    expect(run(["7", "--closes", "3,x/y#3", "--body-file", "b.md"], "/repo", f.deps)).toBe(3);
    expect(f.text()).toContain("x/y#3 が閉じていない");
    expect(f.text()).toContain("索引が未反映");
  });
  it("揃っていて宣言外の close があれば、待たずに 3（宣言外は待ちの判定に入れない）", () => {
    const f = fake({ "issue list": ok([{ number: 3 }, { number: 5 }]) });
    expect(run(ARGS, "/repo", f.deps)).toBe(3);
    expect(f.sleeps).toEqual([]);
    expect(f.text()).toContain("#5");
  });
  it("検索が上限に達したら、待たずに打ち切る", () => {
    const many = Array.from({ length: SEARCH_LIMIT }, (_, i) => ({ number: i === 0 ? 3 : 1000 + i }));
    const f = fake({ "issue view": ok({ state: "OPEN" }), "issue list": ok(many) });
    expect(run(ARGS, "/repo", f.deps)).toBe(3);
    expect(f.sleeps).toEqual([]);
    expect(f.text()).toContain("上限");
  });
  it("揃えば観測 1 回で 0（正常なマージに待ちの費用は掛からない）", () => {
    const f = fake();
    expect(run(ARGS, "/repo", f.deps)).toBe(0);
    expect(f.sleeps).toEqual([]);
    expect(f.text()).toContain("観測 1 回・待ち計 0 秒");
  });
  it("事後の gh が失敗しても 3（2 ではない——マージは済んでいる）", () => {
    const f = fake({ "issue view": { status: 1, stdout: "", stderr: "x" } });
    expect(run(ARGS, "/repo", f.deps)).toBe(3);
    expect(f.text()).toContain("マージ済み");
    // gh の失敗は反映の遅れではないので待たない（待つ変異は終了コードだけでは検出されない）
    expect(f.sleeps).toEqual([]);
  });
});

const REPO = "o/r";

describe("parseArgs — 口は 1 通りだけ", () => {
  it("PR 番号・宣言・本文ファイルを読む", () => {
    expect(parseArgs(["12", "--closes", "3,o/x#4", "--body-file", "b.md"])).toEqual({
      pr: "12",
      closes: ["#3", "o/x#4"],
      bodyFile: "b.md",
      dryRun: false,
    });
  });
  it("--dry-run は位置を問わず読む", () => {
    expect(parseArgs(["--dry-run", "12", "--closes", "none", "--body-file", "b.md"]).dryRun).toBe(true);
    expect(parseArgs(["12", "--closes", "none", "--body-file", "b.md", "--dry-run"]).dryRun).toBe(true);
  });
  it("--closes none は空の宣言", () => {
    expect(parseArgs(["12", "--closes", "none", "--body-file", "b.md"]).closes).toEqual([]);
  });
  it.each([
    [["--closes", "none", "--body-file", "b.md"]], // PR 番号が無い
    [["12", "--body-file", "b.md"]], // 宣言が無い（閉じない意図も明示させる）
    [["12", "--closes", "none"]], // 本文が無い（省くと PR 本文がそのまま squash 本文になる）
    [["12", "--closes", "", "--body-file", "b.md"]],
    [["12", "--closes", "3,x", "--body-file", "b.md"]],
    [["abc", "--closes", "none", "--body-file", "b.md"]],
    [["12", "13", "--closes", "none", "--body-file", "b.md"]],
  ])("足りない・形が違う引数は使用法の誤り: %j", (argv) => {
    expect(() => parseArgs(argv)).toThrow(UsageError);
  });
  it.each(["--subject", "-t", "--auto", "--delete-branch", "-d", "--body", "-b", "--admin"])(
    "%s は受け付けない（件名の逐語使用・確認とマージの引き離し・ブランチ削除の口を持たない）",
    (flag) => {
      expect(() => parseArgs(["12", "--closes", "none", "--body-file", "b.md", flag, "x"])).toThrow(UsageError);
    },
  );
});

describe("findClosingKeywords — GitHub が close に使う 9 形", () => {
  const forms = ["close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves", "resolved"];
  it.each(forms)("%s #N を拾う（大文字小文字を問わない）", (kw) => {
    expect(findClosingKeywords(`本文\n${kw} #12`)).toHaveLength(1);
    expect(findClosingKeywords(`${kw.toUpperCase()} #12`)).toHaveLength(1);
  });
  it.each([
    "Closes: #10",
    "fixes o/r#3",
    "Resolved https://github.com/o/r/issues/7",
    "| 1 | fixes #3 |",
    "- [x] closes #4",
    "partially fixes #5",
    "closes GH-6",
  ])("コロン・他リポジトリ・URL・表・チェックリスト・散文の中も拾う: %s", (text) => {
    expect(findClosingKeywords(text)).toHaveLength(1);
  });
  it("1 行に同居する複数の参照を別々に拾う", () => {
    expect(findClosingKeywords("fixes #1, closes #2")).toHaveLength(2);
  });
  it.each(["関連: #1312", "prefix #3", "refs #4", "fixture #5", "(#1311)"])("keyword でない語は拾わない: %s", (text) => {
    expect(findClosingKeywords(text)).toEqual([]);
  });
});

describe("titleClosesIssue — conventional commit の型名が keyword になる形", () => {
  it.each(["fix: #985 直す", "fix(core): #985 直す", "Close: #3", "resolve!: o/r#4"])("止める: %s", (t) => {
    expect(titleClosesIssue(t)).toBe(true);
  });
  it.each([
    "fix(core): フォルダ絞り込みで前後空白を正規化する (#1284)",
    "chore: #985 を進める",
    "refactor: #985",
    "fix: 直す",
  ])("通す: %s", (t) => {
    expect(titleClosesIssue(t)).toBe(false);
  });
});

describe("normalizeRef / diffClosing — 宣言と一覧は完全一致で比べる", () => {
  it("#N はこのリポジトリへ、owner/repo#N はそのまま", () => {
    expect(normalizeRef("#3", REPO)).toBe("o/r#3");
    expect(normalizeRef("x/y#3", REPO)).toBe("x/y#3");
  });
  it("一致なら差は無い", () => {
    expect(diffClosing(["o/r#1", "o/r#2"], ["o/r#2", "o/r#1"])).toEqual({ extra: [], missing: [] });
  });
  it("宣言に無いものは extra（閉じたくない issue）、一覧に無いものは missing", () => {
    expect(diffClosing(["o/r#1"], ["o/r#1", "o/r#9", "x/y#2"])).toEqual({ extra: ["o/r#9", "x/y#2"], missing: [] });
    expect(diffClosing(["o/r#1", "o/r#2"], ["o/r#1"])).toEqual({ extra: [], missing: ["o/r#2"] });
  });
});

describe("isIsoTimestamp — 検索に渡せる時刻の形", () => {
  it("gh の --jq が返す生文字列を通す", () => {
    expect(isIsoTimestamp("2026-10-29T13:57:00Z")).toBe(true);
  });
  it.each(["08/10/2026 10:26:23", "", "2026-10-29", "garbage", "2026-10-29T13:57:00Z\n", undefined])(
    "崩れた形を通さない（GitHub はエラーを出さず 0 件を返す）: %j",
    (s) => {
      expect(isIsoTimestamp(s)).toBe(false);
    },
  );
});

// ---- fastForwardMain（#1528）: 使い捨ての bare の origin・seed・本体の clone を実 git で作って確かめる ----

const T = 60_000;
const scratchDirs = [];
afterAll(() => {
  for (const dir of scratchDirs) rmSync(dir, { recursive: true, force: true });
});

const g = (cwd, args) =>
  execFileSync("git", ["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "core.autocrlf=false", ...args], {
    cwd,
    encoding: "utf8",
    stdio: "pipe",
  }).trim();

/**
 * origin（bare）・seed（origin へ push する側）・root（本体）を作る。`advance` なら seed が 1 コミット進めて push し、
 * root を古くする。`files` は最初のコミットの中身。返す `merge` は origin/main の先端（マージ commit の代わり）
 */
function ffRepos({ advance = true, files = { "a.txt": "a\n" }, change = (seed) => writeFileSync(path.join(seed, "b.txt"), "b\n") } = {}) {
  const dir = mkdtempSync(path.join(tmpdir(), "merge-pr-ff-"));
  scratchDirs.push(dir);
  const origin = path.join(dir, "origin.git");
  const seed = path.join(dir, "seed");
  const root = path.join(dir, "root");
  g(dir, ["init", "-q", "--bare", "-b", "main", origin]);
  g(dir, ["init", "-q", "-b", "main", seed]);
  for (const [name, body] of Object.entries(files)) writeFileSync(path.join(seed, name), body);
  g(seed, ["add", "-A"]);
  g(seed, ["commit", "-q", "-m", "init"]);
  g(seed, ["remote", "add", "origin", origin]);
  g(seed, ["push", "-q", "origin", "main"]);
  g(dir, ["clone", "-q", origin, root]);
  if (advance) {
    change(seed);
    g(seed, ["add", "-A"]);
    g(seed, ["commit", "-q", "-m", "next"]);
    g(seed, ["push", "-q", "origin", "main"]);
  }
  return { dir, origin, seed, root, merge: g(seed, ["rev-parse", "HEAD"]), head: () => g(root, ["rev-parse", "HEAD"]) };
}

describe("fastForwardMain — 本体のローカル main を origin/main へ進める（#1528）", () => {
  it("main に居てきれいなら、マージ後の origin/main まで進める", { timeout: T }, () => {
    const r = ffRepos();
    const before = r.head();
    const res = fastForwardMain(r.root, r.merge);
    expect(res.ok, res.notes.join("\n")).toBe(true);
    expect(r.head()).toBe(r.merge);
    expect(res.notes.join("\n")).toContain(`本体の main を進めた: ${before.slice(0, 8)}..${r.merge.slice(0, 8)}`);
  });

  it("既に最新なら HEAD を変えず「既に最新」と言う", { timeout: T }, () => {
    const r = ffRepos();
    g(r.root, ["pull", "-q", "--ff-only"]);
    const res = fastForwardMain(r.root, r.merge);
    expect(res.ok, res.notes.join("\n")).toBe(true);
    expect(r.head()).toBe(r.merge);
    expect(res.notes.join("\n")).toContain("本体の main は既に最新");
  });

  it("origin/main の祖先である別ブランチに居れば、そのブランチも main も動かさない（--ff-only は成功してしまう）", { timeout: T }, () => {
    const r = ffRepos();
    const before = r.head();
    g(r.root, ["switch", "-q", "-c", "feat/y"]);
    const res = fastForwardMain(r.root, r.merge);
    expect(res.ok).toBe(false);
    expect(res.notes.join("\n")).toContain("本体が main に居ない（feat/y）");
    expect(res.notes.join("\n")).toContain("main へ戻してから");
    expect(g(r.root, ["rev-parse", "feat/y"])).toBe(before);
    expect(g(r.root, ["rev-parse", "main"])).toBe(before);
  });

  it("detached HEAD なら動かさない", { timeout: T }, () => {
    const r = ffRepos();
    const before = r.head();
    g(r.root, ["switch", "-q", "--detach"]);
    const res = fastForwardMain(r.root, r.merge);
    expect(res.ok).toBe(false);
    expect(res.notes.join("\n")).toContain("本体が detached HEAD である");
    expect(r.head()).toBe(before);
  });

  it("追跡ファイルに変更があれば、差分と重ならなくても動かさない", { timeout: T }, () => {
    const r = ffRepos();
    const before = r.head();
    writeFileSync(path.join(r.root, "a.txt"), "作業中\n");
    const res = fastForwardMain(r.root, r.merge);
    expect(res.ok).toBe(false);
    expect(res.notes.join("\n")).toContain("本体の追跡ファイルに変更がある");
    expect(res.notes.join("\n")).toContain("コミットか退避してから");
    expect(r.head()).toBe(before);
    expect(readFileSync(path.join(r.root, "a.txt"), "utf8")).toBe("作業中\n");
  });

  it("ローカルの main が分岐していれば、ff が拒まれて HEAD は変わらない", { timeout: T }, () => {
    const r = ffRepos();
    writeFileSync(path.join(r.root, "c.txt"), "c\n");
    g(r.root, ["add", "c.txt"]);
    g(r.root, ["commit", "-q", "-m", "local"]);
    const before = r.head();
    const res = fastForwardMain(r.root, r.merge);
    expect(res.ok).toBe(false);
    expect(res.notes.join("\n")).toContain("git merge --ff-only が失敗した");
    expect(r.head()).toBe(before);
  });

  it("fetch に失敗したら止める", { timeout: T }, () => {
    const r = ffRepos();
    const before = r.head();
    g(r.root, ["remote", "set-url", "origin", path.join(r.dir, "nope.git")]);
    const res = fastForwardMain(r.root, r.merge);
    expect(res.ok).toBe(false);
    expect(res.notes.join("\n")).toContain("git fetch origin main が失敗した");
    expect(res.notes.join("\n")).toContain("時間を置いて");
    expect(r.head()).toBe(before);
  });

  it("マージ commit が取れなかった（null）なら止める", { timeout: T }, () => {
    const r = ffRepos();
    const before = r.head();
    const res = fastForwardMain(r.root, null);
    expect(res.ok).toBe(false);
    expect(res.notes.join("\n")).toContain("マージ commit が取れなかった");
    expect(r.head()).toBe(before);
  });

  it("マージ commit が origin/main に含まれなければ止める（「既に最新」と誤報しない）", { timeout: T }, () => {
    const r = ffRepos();
    const before = r.head();
    const stray = g(r.root, ["commit-tree", "HEAD^{tree}", "-m", "stray"]);
    const res = fastForwardMain(r.root, stray);
    expect(res.ok).toBe(false);
    expect(res.notes.join("\n")).toContain("origin/main にまだマージが現れていない");
    expect(r.head()).toBe(before);
  });

  it("マージ commit が不正なら、別の理由で止める", { timeout: T }, () => {
    const r = ffRepos();
    const res = fastForwardMain(r.root, "f".repeat(40));
    expect(res.ok).toBe(false);
    expect(res.notes.join("\n")).toContain("マージ commit を確かめられない");
  });

  it("マージで追跡されるパスに無視されたファイルがあれば止め、内容を守る（--ff-only は黙って上書きする）", { timeout: T }, () => {
    const r = ffRepos({ change: (seed) => writeFileSync(path.join(seed, "new.txt"), "n\n") });
    appendFileSync(path.join(r.root, ".git", "info", "exclude"), "new.txt\n");
    writeFileSync(path.join(r.root, "new.txt"), "mine\n");
    const before = r.head();
    const res = fastForwardMain(r.root, r.merge);
    expect(res.ok).toBe(false);
    expect(res.notes.join("\n")).toContain("マージで追跡されるパスに、追跡していないファイルがある");
    expect(res.notes.join("\n")).toContain("new.txt");
    expect(res.notes.join("\n")).toContain("退避してから");
    expect(r.head()).toBe(before);
    expect(readFileSync(path.join(r.root, "new.txt"), "utf8")).toBe("mine\n");
  });

  it("リネームの移動先に無視されたファイルがあっても止める", { timeout: T }, () => {
    const r = ffRepos({
      files: { "a.txt": "a\n", "old.txt": "同じ中身\n" },
      change: (seed) => g(seed, ["mv", "old.txt", "moved.txt"]),
    });
    appendFileSync(path.join(r.root, ".git", "info", "exclude"), "moved.txt\n");
    writeFileSync(path.join(r.root, "moved.txt"), "mine\n");
    const res = fastForwardMain(r.root, r.merge);
    expect(res.ok).toBe(false);
    expect(res.notes.join("\n")).toContain("moved.txt");
    expect(readFileSync(path.join(r.root, "moved.txt"), "utf8")).toBe("mine\n");
  });

  it("マージで追跡されるパスに無視されていない未追跡ファイルがあっても止める", { timeout: T }, () => {
    const r = ffRepos();
    writeFileSync(path.join(r.root, "b.txt"), "mine\n");
    const before = r.head();
    const res = fastForwardMain(r.root, r.merge);
    expect(res.ok).toBe(false);
    expect(res.notes.join("\n")).toContain("b.txt");
    expect(r.head()).toBe(before);
    expect(readFileSync(path.join(r.root, "b.txt"), "utf8")).toBe("mine\n");
  });

  it("ローカルの main が origin/main より先にあれば止め、「既に最新」と言わない（--ff-only は rc=0 を返す）", { timeout: T }, () => {
    const r = ffRepos({ advance: false });
    writeFileSync(path.join(r.root, "c.txt"), "c\n");
    g(r.root, ["add", "c.txt"]);
    g(r.root, ["commit", "-q", "-m", "local"]);
    const before = r.head();
    const res = fastForwardMain(r.root, r.merge);
    expect(res.ok).toBe(false);
    expect(res.notes.join("\n")).toContain("本体の main が origin/main より先にある");
    expect(res.notes.join("\n")).not.toContain("既に最新");
    expect(r.head()).toBe(before);
  });
});
