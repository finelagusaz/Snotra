import { describe, it, expect } from "vitest";
import {
  parseArgs,
  UsageError,
  findClosingKeywords,
  titleClosesIssue,
  normalizeRef,
  diffClosing,
  isIsoTimestamp,
  run,
  SEARCH_LIMIT,
} from "./merge-pr.mjs";

const HEAD = "a".repeat(40);
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
    "pr view:post": ok({ state: "MERGED", mergedAt: "2026-10-31T01:02:03Z" }),
    "issue view": ok({ state: "CLOSED" }),
    "pr list:merged": ok([{ number: 7 }]),
    "issue list": ok([{ number: 3 }]),
    ...over,
  };
  const keyOf = (cmd, args) => {
    if (cmd === "git") return args[0];
    const k = `${args[0]} ${args[1]}`;
    if (k === "pr view") return args.includes("state,mergedAt") ? "pr view:post" : "pr view:pre";
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
    sleep: () => {},
    out: (s) => out.push(s),
    err: (s) => out.push(s),
  };
  const out = [];
  return { deps, calls, out, text: () => out.join("\n") };
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
    expect(f.calls).toContain("fetch"); // 無いと indexOf が -1 を返し、下の比較だけでは素通りする
    expect(f.calls.indexOf("fetch")).toBeLessThan(f.calls.indexOf("pr merge"));
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

describe("run — 走らなかった（exit 2・マージしていない）", () => {
  it.each([
    ["使用法の誤り", ["7", "--subject", "x"], {}],
    ["gh pr view の失敗", ARGS, { "pr view:pre": { status: 1, stdout: "", stderr: "boom" } }],
    ["head の sha の形が違う", ARGS, { "pr view:pre": preview({ headRefOid: "--output=x" }) }],
    ["fetch しても head が無い", ARGS, { "cat-file": { status: 1, stdout: "", stderr: "" }, fetch: { status: 1, stdout: "", stderr: "x" } }],
  ])("%s", (_, argv, over) => {
    const f = fake(over);
    expect(run(argv, "/repo", f.deps)).toBe(2);
    expect(f.calls).not.toContain("pr merge");
  });
  it("linked worktree から打たれたら（root を導けなければ）止める", () => {
    const f = fake();
    f.deps.mainRoot = () => {
      throw new Error("本体の作業ツリーから打つこと");
    };
    expect(run(ARGS, "/repo", f.deps)).toBe(2);
    expect(f.calls).not.toContain("pr merge");
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
  });
  it("gh pr merge の呼び出しが例外を投げても 2 ではなく 3（マージの後かもしれない）", () => {
    const f = fake({
      "pr merge": () => {
        throw new Error("boom");
      },
    });
    expect(run(ARGS, "/repo", f.deps)).toBe(3);
  });
  it("状態を確かめられなければ 3（マージしたかもしれない）", () => {
    const f = fake({ "pr view:post": { status: 1, stdout: "", stderr: "x" } });
    expect(run(ARGS, "/repo", f.deps)).toBe(3);
  });
});

describe("run — マージ後の検証（落ちたら exit 3・最初に「マージ済み」を名乗る）", () => {
  const post = (over) => {
    const f = fake(over);
    const code = run(ARGS, "/repo", f.deps);
    return { code, text: f.text(), calls: f.calls };
  };
  it("mergedAt が ISO 8601 でなければ検索に渡さない", () => {
    const r = post({ "pr view:post": ok({ state: "MERGED", mergedAt: "08/10/2026 10:26:23" }) });
    expect(r.code).toBe(3);
    expect(r.calls).not.toContain("issue list");
  });
  it("宣言した issue が閉じていなければ 3", () => {
    expect(post({ "issue view": ok({ state: "OPEN" }) }).code).toBe(3);
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
    expect(post({ "issue list": ok([]) }).code).toBe(3);
  });
  it(`検索が上限（${SEARCH_LIMIT} 件）に達したら打ち切りと区別できないので 3`, () => {
    const many = Array.from({ length: SEARCH_LIMIT }, (_, i) => ({ number: i === 0 ? 3 : 1000 + i }));
    const r = post({ "issue list": ok(many) });
    expect(r.code).toBe(3);
    // 宣言外 close の検出でも 3 になるので、止めた理由まで見る（終了コードだけでは上限判定を消しても緑）
    expect(r.text).toContain("上限");
  });
  it("事後の gh が失敗しても 3（2 ではない——マージは済んでいる）", () => {
    const r = post({ "issue view": { status: 1, stdout: "", stderr: "x" } });
    expect(r.code).toBe(3);
    expect(r.text).toContain("マージ済み");
  });
});

const REPO = "o/r";

describe("parseArgs — 口は 1 通りだけ", () => {
  it("PR 番号・宣言・本文ファイルを読む", () => {
    expect(parseArgs(["12", "--closes", "3,o/x#4", "--body-file", "b.md"])).toEqual({
      pr: "12",
      closes: ["#3", "o/x#4"],
      bodyFile: "b.md",
    });
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
