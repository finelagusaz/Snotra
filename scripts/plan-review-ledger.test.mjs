// plan-review-ledger.mjs の判定関数を、フォールトインジェクションフィクスチャ（赤）と
// 正常フィクスチャ（緑）の両方向で検証する（`.claude/rules/safety-nets.md`
// 「効いていることは、フォールトインジェクションで一度は実測する」）。
//
// 赤フィクスチャの出所は `/plan-review`「Step 3 — 結果の統合と報告」が名指しする落ち方である——
// 未生成・空・スタブ・命名逸脱の 4 つは、いずれも「N 件中 N 件実在のまま照合を通る」形として
// SKILL 本文が実測付きで記録していたものを、そのまま入力へ写した。
import { describe, it, expect } from "vitest";
import {
  SLUG_RE,
  MIN_CHARS,
  PRESENT,
  MISSING,
  REASON,
  LEDGER_DIR,
  parseArgs,
  validateSlugs,
  classifyEntries,
  formatReport,
  readLedgerDir,
} from "./plan-review-ledger.mjs";

/** 実在を名乗れる長さの成果物（`MIN_CHARS` を確実に超える）。 */
const ok = (name) => ({ name, chars: MIN_CHARS + 40 });

describe("parseArgs", () => {
  it("サブコマンドと --slug の繰り返しを受ける", () => {
    expect(parseArgs(["verify", "--slug", "rust", "--slug", "docs"])).toEqual({
      command: "verify",
      slugs: ["rust", "docs"],
    });
  });

  it("サブコマンドが無ければエラー", () => {
    expect(parseArgs(["--slug", "rust"]).error).toBeTruthy();
  });

  it("内容を受け取る口を持たない（契約: 呼び出し側の内容を書かない）", () => {
    // --write / --report-to のような書き込み口は「未知の引数」として拒否されること。
    // これが通ると /plan-review のオーケストレーターが自作自演で照合を通せるようになる。
    expect(parseArgs(["verify", "--write", "x"]).error).toBeTruthy();
    expect(parseArgs(["verify", "--report-to", "x"]).error).toBeTruthy();
  });
});

describe("validateSlugs（母集団の健全性）", () => {
  it("正常なスラグは通る", () => {
    expect(validateSlugs(["rust-core", "docs", "scripts2"])).toEqual([]);
  });

  it("【赤】空集合を通さない — 0 件中 0 件実在で自動成立する経路", () => {
    expect(validateSlugs([]).length).toBe(1);
    expect(validateSlugs([])[0]).toMatch(/0 件/);
  });

  it("【赤】重複スラグを通さない — 後勝ちで 1 体分の成果物が消える", () => {
    const errs = validateSlugs(["rust", "docs", "rust"]);
    expect(errs.some((e) => e.includes("重複"))).toBe(true);
  });

  it("【赤】形の不正を通さない", () => {
    for (const bad of ["Rust", "rust_core", "rust/core", "-rust", "rust-"]) {
      expect(SLUG_RE.test(bad)).toBe(false);
      expect(validateSlugs([bad]).some((e) => e.includes("形が不正"))).toBe(true);
    }
  });
});

describe("classifyEntries（双方向照合）", () => {
  it("【緑】全件そろえば全て実在", () => {
    const rows = classifyEntries(["a", "b"], [ok("a.md"), ok("b.md")]);
    expect(rows.map((r) => r.status)).toEqual([PRESENT, PRESENT]);
  });

  it("【赤】未生成 — 起動したが届かなかった体", () => {
    const rows = classifyEntries(["a", "b"], [ok("a.md")]);
    expect(rows[1]).toMatchObject({ slug: "b", status: MISSING, reason: REASON.absent });
  });

  it("【赤】空ファイル — 実在確認だけなら通り抜ける形", () => {
    const rows = classifyEntries(["a"], [{ name: "a.md", chars: 0 }]);
    expect(rows[0]).toMatchObject({ status: MISSING, reason: REASON.empty });
  });

  it("【赤】スタブ — 途中終了。MIN_CHARS 未満は不着へ倒す", () => {
    const rows = classifyEntries(["a"], [{ name: "a.md", chars: MIN_CHARS - 1 }]);
    expect(rows[0]).toMatchObject({ status: MISSING, reason: REASON.stub });
  });

  it("境界: ちょうど MIN_CHARS は実在", () => {
    const rows = classifyEntries(["a"], [{ name: "a.md", chars: MIN_CHARS }]);
    expect(rows[0].status).toBe(PRESENT);
  });

  it("【赤】命名逸脱 — ディスクにあって台帳に無いものも不着として数える", () => {
    // 台帳が 1 件・ディスクが 1 件で「1 件中 1 件実在」に見えるが、中身は別物である。
    const rows = classifyEntries(["a"], [ok("typo-a.md")]);
    expect(rows).toHaveLength(2);
    expect(rows[0]).toMatchObject({ slug: "a", status: MISSING, reason: REASON.absent });
    expect(rows[1]).toMatchObject({ slug: "typo-a", status: MISSING, reason: REASON.unexpected });
  });

  it("【赤】件数だけの照合では通ってしまう組み合わせを、双方向で捕まえる", () => {
    // 台帳 2 件 / ディスク 2 件。件数は一致するが b が落ち、別名が 1 件ある。
    const rows = classifyEntries(["a", "b"], [ok("a.md"), ok("stray.md")]);
    expect(rows.filter((r) => r.status === PRESENT)).toHaveLength(1);
    expect(rows.filter((r) => r.status === MISSING)).toHaveLength(2);
  });
});

describe("formatReport", () => {
  it("【緑】全件実在なら不成立の警告を出さない", () => {
    const out = formatReport(classifyEntries(["a"], [ok("a.md")]));
    expect(out).toContain("台帳 1 件中 1 件が実在");
    expect(out).not.toContain("不成立");
  });

  it("【赤】不着があれば独立レビュー不成立と completeness の制約を明記する", () => {
    const out = formatReport(classifyEntries(["a", "b"], [ok("a.md")]));
    expect(out).toContain("台帳 2 件中 1 件が実在");
    expect(out).toContain("独立レビュー不成立");
    expect(out).toContain("completeness");
    expect(out).toContain(`${LEDGER_DIR}/b.md`);
  });
});

describe("readLedgerDir", () => {
  const io = (files) => ({
    existsSync: () => true,
    readdirSync: () => Object.keys(files),
    readFileSync: (p) => {
      const name = p.split(/[\\/]/).pop();
      if (files[name] == null) throw new Error("読めない");
      return files[name];
    },
  });

  it("*.md だけを拾い、空白・改行を除いて数える", () => {
    const got = readLedgerDir("d", io({ "a.md": "  あい\nうえ  ", "b.txt": "xxxx" }));
    expect(got).toEqual([{ name: "a.md", chars: 4 }]);
  });

  it("【赤】読めないファイルは 0 字＝不着へ倒す（沈黙経路の閉塞）", () => {
    const got = readLedgerDir("d", io({ "a.md": null }));
    expect(got).toEqual([{ name: "a.md", chars: 0 }]);
    expect(classifyEntries(["a"], got)[0].status).toBe(MISSING);
  });

  it("ディレクトリが無ければ空（全件が未生成として報告される）", () => {
    const got = readLedgerDir("d", { existsSync: () => false });
    expect(got).toEqual([]);
    expect(classifyEntries(["a"], got)[0].reason).toBe(REASON.absent);
  });
});
