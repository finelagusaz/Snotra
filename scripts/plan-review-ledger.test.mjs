// plan-review-ledger.mjs の判定関数を、フォールトインジェクションフィクスチャ（赤）と
// 正常フィクスチャ（緑）の両方向で検証する（`.claude/rules/safety-nets.md`
// 「効いていることは、フォールトインジェクションで一度は実測する」）。
//
// 赤フィクスチャの出所は `/plan-review`「Step 3 — 結果の統合と報告」が名指しする落ち方である——
// 未生成・空・スタブ・命名逸脱の 4 つは、いずれも「N 件中 N 件実在のまま照合を通る」形として
// SKILL 本文が実測付きで記録していたものを、そのまま入力へ写した。
import { describe, it, expect } from "vitest";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  SLUG_RE,
  MIN_CHARS,
  PRESENT,
  MISSING,
  REASON,
  LEDGER_DIR,
  LEDGER_FILE,
  parseArgs,
  validateSlugs,
  classifyEntries,
  formatReport,
  readLedgerDir,
  readLedger,
  writeLedger,
} from "./plan-review-ledger.mjs";

/** 実在を名乗れる長さの成果物（`MIN_CHARS` を確実に超える）。 */
const ok = (name) => ({ name, chars: MIN_CHARS + 40 });

describe("parseArgs", () => {
  it("init はサブコマンドと --slug の繰り返しを受ける", () => {
    expect(parseArgs(["init", "--slug", "rust", "--slug", "docs"])).toEqual({
      command: "init",
      slugs: ["rust", "docs"],
    });
  });

  it("【赤】verify は --slug を拒否する — 無視すると穴が『引数が効かないだけ』の形で残る", () => {
    // #831 の本体。verify の母集団は init が書いた台帳であって argv ではない。
    expect(parseArgs(["verify", "--slug", "rust"]).error).toBeTruthy();
    expect(parseArgs(["verify", "--slug", "rust", "--slug", "docs"]).error).toBeTruthy();
    // サブコマンドが --slug より後に来た形も拾う。
    expect(parseArgs(["--slug", "rust", "verify"]).error).toBeTruthy();
  });

  it("verify は引数なしで通る", () => {
    expect(parseArgs(["verify"])).toEqual({ command: "verify", slugs: [] });
  });

  it("サブコマンドが無ければエラー", () => {
    expect(parseArgs(["--slug", "rust"]).error).toBeTruthy();
  });

  it("内容を受け取る口を持たない（契約: 識別子は書くが、呼び出し側の内容は書かない）", () => {
    // --write / --report-to のような書き込み口は「未知の引数」として拒否されること。
    // これが通ると /plan-review のオーケストレーターが自作自演で照合を通せるようになる。
    expect(parseArgs(["init", "--write", "x"]).error).toBeTruthy();
    expect(parseArgs(["init", "--report-to", "x"]).error).toBeTruthy();
  });
});

describe("readLedger / writeLedger（母集団の永続化・#831）", () => {
  const io = (files) => ({
    writeFileSync: (p, data) => {
      const name = p.split(/[\\/]/).pop();
      if (files[name] != null) throw Object.assign(new Error("EEXIST"), { code: "EEXIST" });
      files[name] = data;
    },
    readFileSync: (p) => {
      const name = p.split(/[\\/]/).pop();
      if (files[name] == null) throw Object.assign(new Error("ENOENT"), { code: "ENOENT" });
      return files[name];
    },
  });

  it("【緑】ラウンドトリップで集合が保存される", () => {
    const files = {};
    writeLedger("d", ["rust", "docs"], io(files));
    expect(readLedger("d", io(files))).toEqual({ slugs: ["rust", "docs"] });
  });

  it("【赤】台帳が不在 — 空配列へ倒さない（0 件中 0 件実在で緑になる経路を塞ぐ）", () => {
    const r = readLedger("d", io({}));
    expect(r.slugs).toBeUndefined();
    expect(r.error).toMatch(/読めない/);
  });

  it("【赤】JSON として parse 不能", () => {
    const r = readLedger("d", io({ [LEDGER_FILE]: "{ not json" }));
    expect(r.slugs).toBeUndefined();
    expect(r.error).toBeTruthy();
  });

  it("【赤】shape 不正 — slugs が配列でない", () => {
    const r = readLedger("d", io({ [LEDGER_FILE]: '{"slugs":"rust"}' }));
    expect(r.slugs).toBeUndefined();
    expect(r.error).toMatch(/形が不正/);
  });

  it("【赤】shape 不正 — 要素が文字列でない", () => {
    const r = readLedger("d", io({ [LEDGER_FILE]: '{"slugs":["rust",3]}' }));
    expect(r.slugs).toBeUndefined();
    expect(r.error).toMatch(/形が不正/);
  });

  it("【赤】空配列は読めるが、validateSlugs が母集団の欠落として弾く", () => {
    // readLedger の責務は「形」まで。0 件の拒否は validateSlugs が担う（担当の分離）。
    const r = readLedger("d", io({ [LEDGER_FILE]: '{"slugs":[]}' }));
    expect(r).toEqual({ slugs: [] });
    expect(validateSlugs(r.slugs).length).toBe(1);
  });

  it("writeLedger は既存ファイルを上書きしない（意図の表明・flag wx）", () => {
    const files = { [LEDGER_FILE]: "old" };
    expect(() => writeLedger("d", ["rust"], io(files))).toThrow();
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

  it("【回帰固定】台帳ファイルは成果物の母集団に現れない", () => {
    // ledger.json が `.md` フィルタを通ると classifyEntries が毎回「命名逸脱」で不着 1 件を
    // 立て、verify が常に赤になる。台帳を `.md` にしてはならない理由でもある。
    const got = readLedgerDir("d", io({ [LEDGER_FILE]: '{"slugs":["a"]}', "a.md": "x".repeat(MIN_CHARS) }));
    expect(got.map((f) => f.name)).toEqual(["a.md"]);
    expect(classifyEntries(["a"], got).every((r) => r.status === PRESENT)).toBe(true);
  });
});

describe("CLI（プロセス級 e2e・#831）", () => {
  const SCRIPT = fileURLToPath(new URL("./plan-review-ledger.mjs", import.meta.url));

  /**
   * cwd は**必ず一時ディレクトリ**にする。LEDGER_DIR は process.cwd() 基準で、init は
   * rmSync(recursive) を打つため、リポジトリルートで走らせると進行中の /plan-review
   * ラウンドの成果物を消す（`npm test` が誰かの作業を壊す）。
   */
  function run(cwd, args) {
    try {
      const stdout = execFileSync(process.execPath, [SCRIPT, ...args], { cwd, encoding: "utf8", stdio: "pipe" });
      return { code: 0, stdout };
    } catch (e) {
      return { code: e.status, stdout: `${e.stdout ?? ""}${e.stderr ?? ""}` };
    }
  }

  const withTmp = (fn) => {
    const cwd = fs.mkdtempSync(path.join(os.tmpdir(), "plan-ledger-"));
    try {
      return fn(cwd);
    } finally {
      fs.rmSync(cwd, { recursive: true, force: true });
    }
  };

  const artifact = (cwd, slug) =>
    fs.writeFileSync(path.join(cwd, LEDGER_DIR, `${slug}.md`), "x".repeat(MIN_CHARS + 10));

  it("【緑】init → 全件書く → verify（引数なし）が exit 0", () =>
    withTmp((cwd) => {
      expect(run(cwd, ["init", "--slug", "a", "--slug", "b"]).code).toBe(0);
      expect(fs.existsSync(path.join(cwd, LEDGER_DIR, LEDGER_FILE))).toBe(true);
      artifact(cwd, "a");
      artifact(cwd, "b");
      expect(run(cwd, ["verify"]).code).toBe(0);
    }));

  it("【赤・#831 の種】init 2 件・成果物 1 件なら verify は exit 1（打ち直す口が無い）", () =>
    withTmp((cwd) => {
      run(cwd, ["init", "--slug", "a", "--slug", "b"]);
      artifact(cwd, "a");
      const r = run(cwd, ["verify"]);
      expect(r.code).toBe(1);
      expect(r.stdout).toContain("不着");
    }));

  it("【赤・#831 の種】verify --slug は exit 2 で拒否される", () =>
    withTmp((cwd) => {
      run(cwd, ["init", "--slug", "a", "--slug", "b"]);
      artifact(cwd, "a");
      artifact(cwd, "b");
      // 母集団を縮めようとする操作そのもの。無視されて緑になってはならない。
      expect(run(cwd, ["verify", "--slug", "a"]).code).toBe(2);
    }));

  it("【赤】init なしの verify は exit 2（母集団の欠落・不着ではない）", () =>
    withTmp((cwd) => {
      const r = run(cwd, ["verify"]);
      expect(r.code).toBe(2);
      expect(r.stdout).toContain("init");
    }));

  it("【赤】台帳が壊れていれば exit 2", () =>
    withTmp((cwd) => {
      run(cwd, ["init", "--slug", "a"]);
      artifact(cwd, "a");
      fs.writeFileSync(path.join(cwd, LEDGER_DIR, LEDGER_FILE), "{ not json");
      expect(run(cwd, ["verify"]).code).toBe(2);
    }));

  it("init を 2 度走らせると台帳が入れ替わり、前ラウンドの成果物は残らない（I3 の順序を固定）", () =>
    withTmp((cwd) => {
      expect(run(cwd, ["init", "--slug", "a"]).code).toBe(0);
      artifact(cwd, "a");
      expect(run(cwd, ["init", "--slug", "b"]).code).toBe(0);
      const led = JSON.parse(fs.readFileSync(path.join(cwd, LEDGER_DIR, LEDGER_FILE), "utf8"));
      // writeLedger が mkdirSync の後にあることの帰結（前ラウンドの台帳が残らない）。
      expect(led.slugs).toEqual(["b"]);
      // rmSync が効いていることの帰結。
      expect(fs.existsSync(path.join(cwd, LEDGER_DIR, "a.md"))).toBe(false);
    }));

  it("【赤】サブコマンドを 2 つ渡すと exit 2（読み取りのつもりが消去へ落ちる口を塞ぐ）", () =>
    withTmp((cwd) => {
      run(cwd, ["init", "--slug", "a"]);
      artifact(cwd, "a");
      expect(run(cwd, ["verify", "init", "--slug", "a"]).code).toBe(2);
      // 破壊分岐へ入っていない＝成果物が残っている。
      expect(fs.existsSync(path.join(cwd, LEDGER_DIR, "a.md"))).toBe(true);
    }));

  it("【赤】台帳を縮めても、残った成果物が命名逸脱で不着になる（I6）", () =>
    withTmp((cwd) => {
      run(cwd, ["init", "--slug", "a", "--slug", "b"]);
      artifact(cwd, "a");
      artifact(cwd, "b");
      fs.writeFileSync(path.join(cwd, LEDGER_DIR, LEDGER_FILE), JSON.stringify({ slugs: ["a"] }));
      const r = run(cwd, ["verify"]);
      expect(r.code).toBe(1);
      expect(r.stdout).toContain("命名逸脱");
      // 見出しの「台帳 N 件」に命名逸脱の行を混ぜない（母集団の件数を誤って名乗らない）。
      expect(r.stdout).toContain("台帳 1 件中 1 件が実在 / 台帳に無いファイル 1 件");
    }));
});
