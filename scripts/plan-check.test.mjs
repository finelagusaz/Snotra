// plan-check.mjs の判定を、理由ごとに「その理由だけに当たる計画」で確かめる（.claude/rules/safety-nets.md
// 「検出器のカバー範囲は、欠落のパターンごとに検算する」）。fixture の形は、git の履歴にある実際の plan.md の形に倣う
// （#1552 の敵対的調査が 831 版を測った）。
import { describe, it, expect } from "vitest";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { judgePlan, parseArgs, UsageError, findRoot } from "./plan-check.mjs";

const FENCE = "```";

const HUMAN = '- [x] 承認済み — 2026-11-28 / 問い: "計画を承認しますか？" / 回答: "承認"';
const SELF = "- [x] 自己承認 — 2026-11-28 / 仕様変更: なし — SPEC.md を変えない / 戻せない変更: なし — 永続形式に触れない / 人の裁定: なし";

/** 既定で成立する自己承認の計画。部品を差し替えて、理由ごとの計画を作る */
function plan({
  files = "- `scripts/x.mjs` — 新規",
  unresolved = "- [x] 何かを測る — 測った",
  approval = SELF,
  selfReview = "- リスク: 通常\n- check スキル: なし",
  extra = "",
  omit = [],
} = {}) {
  const parts = ["# 計画 — issue #1", "", "## 目的と受け入れ条件", "", "- 何か"];
  if (!omit.includes("files")) parts.push("", "## 変更するファイル", "", files);
  parts.push("", "## フェーズ 1", "", "- [ ] 着手前の作業項目（未確定欄の外なので数えない）");
  if (extra) parts.push("", extra);
  if (!omit.includes("unresolved")) parts.push("", "## 未確定（実装前に潰す）", "", unresolved);
  if (!omit.includes("selfReview")) parts.push("", "## セルフレビュー", "", selfReview);
  if (!omit.includes("approval")) parts.push("", "## 承認", "", approval);
  return parts.join("\n") + "\n";
}

const fails = (text, needle) => {
  const r = judgePlan(text);
  expect(r.ok, JSON.stringify(r)).toBe(false);
  expect(r.reasons.join("\n")).toContain(needle);
  return r;
};

describe("judgePlan — 成立する計画", () => {
  it("自己承認", () => {
    expect(judgePlan(plan())).toEqual({ ok: true, kind: "self", reasons: [] });
  });
  it("人間承認（自己承認の追加条件は見ない）", () => {
    const r = judgePlan(plan({ approval: HUMAN, files: "- `SPEC.md` — 直す", unresolved: "- [x] （裁定）範囲 — 回答を得た" }));
    expect(r).toEqual({ ok: true, kind: "human", reasons: [] });
  });
  it("未確定欄の見出しは前方一致（`— ラウンド N` が付く形が実在する）", () => {
    const text = plan().replace("## 未確定（実装前に潰す）", "## 未確定（実装前に潰す）— ラウンド 2");
    expect(judgePlan(text).ok).toBe(true);
  });
  it("フェーズ配下の `- [ ]` は数えない（節の中に限る）", () => {
    expect(judgePlan(plan()).ok).toBe(true);
  });
  it("行の途中の「- [ ]」という言及は数えない", () => {
    expect(judgePlan(plan({ unresolved: "- [x] `- [ ]` の数え方を決める — 決めた" })).ok).toBe(true);
  });
  it("変更するファイルの節の散文の `SPEC.md`（変えないと書いた行）は数えない", () => {
    expect(judgePlan(plan({ files: "- `scripts/x.mjs` — 新規\n\n`SPEC.md` は変えない。" })).ok).toBe(true);
  });
  it("末尾に空白のある承認の見出しも承認欄として読む", () => {
    expect(judgePlan(plan().replace("## 承認\n", "## 承認  \n")).ok).toBe(true);
  });
});

describe("judgePlan — 未確定ゼロ", () => {
  it("未確定欄の見出しが無ければ不成立（#821 の fail-open の再現）", () => {
    fails(plan({ omit: ["unresolved"] }), "未確定欄の見出し");
  });
  it("レベル 3 の `### 未確定（実装前に潰す）` は未確定欄でない", () => {
    fails(plan({ omit: ["unresolved"], extra: "### 未確定（実装前に潰す）\n\n- [x] a" }), "未確定欄の見出し");
  });
  it("未確定欄に未チェックが残れば不成立（件数を出す）", () => {
    fails(plan({ unresolved: "- [ ] a — b\n- [ ] c — d\n- [x] e — f" }), "未確定欄に未チェックの項目が 2 件");
  });
  it.each([
    ["`*` の箇条", "* [ ] a"],
    ["字下げ", "  - [ ] a"],
  ])("PR 前ゲートと同じく %s も未チェックとして数える", (_label, line) => {
    fails(plan({ unresolved: line }), "未確定欄に未チェック");
  });
  it("フェンスの中の未チェックも数える（不成立の証拠はフェンスの中でも数える）", () => {
    fails(plan({ unresolved: `${FENCE}markdown\n- [ ] 雛形\n${FENCE}` }), "未確定欄に未チェック");
  });
  it("フェンスの中の見出しは、未確定欄の見出しとして数えない", () => {
    fails(plan({ omit: ["unresolved"], extra: `${FENCE}markdown\n## 未確定（実装前に潰す）\n\n- [x] a\n${FENCE}` }), "未確定欄の見出し");
  });
});

describe("judgePlan — 承認", () => {
  it("承認欄の見出しが無ければ不成立", () => {
    fails(plan({ omit: ["approval"] }), "承認欄の見出し");
  });
  it("`## 承認時の前提` は承認欄でない（完全一致）", () => {
    fails(plan({ omit: ["approval"], extra: `## 承認時の前提（明示する）\n\n${SELF}` }), "承認欄の見出し");
  });
  it("承認欄に承認の行が無ければ不成立", () => {
    fails(plan({ approval: "- [ ] 承認待ち" }), "承認の行が無い");
  });
  it("フェンスの中の承認の行は数えない", () => {
    fails(plan({ approval: `${FENCE}markdown\n${SELF}\n${FENCE}` }), "承認の行が無い");
  });
  it("大文字の `[X]` は承認の行として読まない（成立の証拠は狭く取る）", () => {
    fails(plan({ approval: SELF.replace("[x]", "[X]") }), "承認の行が無い");
  });
  it("ダッシュが U+2014 でない行は承認の行として読まない", () => {
    fails(plan({ approval: SELF.replace("自己承認 —", "自己承認 -") }), "承認の行が無い");
  });
  it.each([
    ["問いが空", HUMAN.replace('"計画を承認しますか？"', '""')],
    ["回答が空", HUMAN.replace('"承認"', '""')],
  ])("人間承認の%sなら不成立", (_label, line) => {
    fails(plan({ approval: line }), "承認の行が無い");
  });
});

describe("judgePlan — 自己承認の追加条件", () => {
  it.each([
    ["仕様変更", SELF.replace("仕様変更: なし", "仕様変更: あり")],
    ["戻せない変更", SELF.replace("戻せない変更: なし", "戻せない変更: あり")],
    ["人の裁定", SELF.replace("人の裁定: なし", "人の裁定: あり")],
  ])("%s が「なし」でなければ不成立", (_label, line) => {
    fails(plan({ approval: line }), "承認の行が無い");
  });
  it("未確定欄に（裁定）の印があれば不成立", () => {
    fails(plan({ unresolved: "- [x] （裁定）範囲 — 回答を得た" }), "（裁定）");
  });
  it("半角の (裁定) も印として数える（不成立の証拠は広く取る）", () => {
    fails(plan({ unresolved: "- [x] (裁定) 範囲 — 回答を得た" }), "（裁定）");
  });
  it("項目の途中の「（裁定）」という言及は印として数えない", () => {
    expect(judgePlan(plan({ unresolved: "- [x] 「（裁定）」の数え方 — 決めた" })).ok).toBe(true);
  });
  it("変更するファイルの節が無ければ不成立", () => {
    fails(plan({ omit: ["files"] }), "「変更するファイル」");
  });
  it.each([
    ["- の箇条", "- `SPEC.md` — §7 を直す"],
    ["番号つきの箇条", "1. `SPEC.md` — §7 を直す"],
  ])("変更するファイルに SPEC.md の項目（%s）があれば不成立", (_label, line) => {
    fails(plan({ files: `- \`scripts/x.mjs\` — 新規\n${line}` }), "SPEC.md");
  });
  it("`.claude/rules/spec.md` は SPEC.md でない（大文字小文字を区別する）", () => {
    expect(judgePlan(plan({ files: "- `.claude/rules/spec.md` — 直す" })).ok).toBe(true);
  });
  it("セルフレビューの節が無ければ不成立", () => {
    fails(plan({ omit: ["selfReview"] }), "check スキル");
  });
  it("セルフレビューに check スキルの行が無ければ不成立", () => {
    fails(plan({ selfReview: "- リスク: 通常" }), "check スキル");
  });
  it("check スキルの行に /persistence-check があれば不成立", () => {
    fails(plan({ selfReview: "- check スキル: /persistence-check" }), "/persistence-check");
  });
  it("check スキルを子の箇条で並べ、その中に /persistence-check があっても不成立", () => {
    fails(plan({ selfReview: "- check スキル:\n  - /symmetric-check\n  - /persistence-check" }), "/persistence-check");
  });
  it("フェンスの中の check スキルの行は数えない", () => {
    fails(plan({ selfReview: `${FENCE}markdown\n- check スキル: なし\n${FENCE}` }), "check スキル");
  });
});

describe("judgePlan — 理由は打ち切らずに全件並べる", () => {
  it("未確定欄と承認欄の両方が無ければ、両方の理由を出す", () => {
    const r = judgePlan(plan({ omit: ["unresolved", "approval"] }));
    expect(r.ok).toBe(false);
    expect(r.reasons.join("\n")).toContain("未確定欄の見出し");
    expect(r.reasons.join("\n")).toContain("承認欄の見出し");
  });
  it("理由には直し方を添える", () => {
    const r = judgePlan(plan({ files: "- `SPEC.md` — 直す" }));
    expect(r.reasons.join("\n")).toContain("/start-issue");
  });
});

describe("parseArgs", () => {
  it("--plan を読む", () => {
    expect(parseArgs(["--plan", "p.md"])).toEqual({ plan: "p.md" });
  });
  it("未知の引数・値の無い --plan は使用法エラー", () => {
    expect(() => parseArgs(["--nope"])).toThrow(UsageError);
    expect(() => parseArgs(["--plan"])).toThrow(UsageError);
  });
});

describe("findRoot", () => {
  it("cwd から最も近い .git へ遡る", () => {
    const root = mkdtempSync(path.join(tmpdir(), "plan-check-root-"));
    mkdirSync(path.join(root, ".git"));
    mkdirSync(path.join(root, "a", "b"), { recursive: true });
    expect(findRoot(path.join(root, "a", "b"))).toBe(path.resolve(root));
  });
});

describe("CLI の終了コード", () => {
  const SCRIPT = fileURLToPath(new URL("./plan-check.mjs", import.meta.url));
  const run = (args, cwd) => spawnSync(process.execPath, [SCRIPT, ...args], { encoding: "utf8", cwd, timeout: 20_000 });
  const repoWith = (text) => {
    const root = mkdtempSync(path.join(tmpdir(), "plan-check-cli-"));
    mkdirSync(path.join(root, ".git"));
    if (text != null) {
      mkdirSync(path.join(root, "workspace"));
      writeFileSync(path.join(root, "workspace", "plan.md"), text, "utf8");
    }
    return root;
  };
  it("成立なら 0", () => {
    const r = run([], repoWith(plan()));
    expect(r.status, r.stdout + r.stderr).toBe(0);
    expect(r.stdout).toContain("成立");
  });
  it("不成立なら 3 で、理由を出す", () => {
    const r = run([], repoWith(plan({ omit: ["unresolved"] })));
    expect(r.status, r.stdout + r.stderr).toBe(3);
    expect(r.stdout).toContain("未確定欄の見出し");
  });
  it("計画が無ければ 2（判定できない）", () => {
    const r = run([], repoWith(null));
    expect(r.status).toBe(2);
    expect(r.stderr).toContain("計画を読めない");
  });
  it("未知の引数は 2", () => {
    expect(run(["--nope"], repoWith(plan())).status).toBe(2);
  });
  it("--plan で別の計画を指せる", () => {
    const root = repoWith(null);
    writeFileSync(path.join(root, "p.md"), plan(), "utf8");
    expect(run(["--plan", path.join(root, "p.md")], root).status).toBe(0);
  });
});
