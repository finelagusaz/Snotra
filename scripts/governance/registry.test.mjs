import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, it, expect } from "vitest";
import { CHECK_MODULES, checkModulesFrom } from "./registry.mjs";

describe("registry（checks/ の走査から導出する・#1088）", () => {
  it("検査モジュールが 1 本以上あり、すべて id と run を持つ", () => {
    expect(CHECK_MODULES.length).toBeGreaterThan(0);
    for (const m of CHECK_MODULES) {
      expect(typeof m.id, `${m.id} の id が string でない`).toBe("string");
      expect(typeof m.run, `${m.id} の run が function でない`).toBe("function");
    }
  });
  it("id は昇順（readdir 順の揺れが出力順に出ない）", () => {
    const ids = CHECK_MODULES.map((m) => m.id);
    expect(ids).toEqual([...ids].sort());
  });
  it("id が重複しない", () => {
    const ids = CHECK_MODULES.map((m) => m.id);
    expect(new Set(ids).size, `重複: ${ids.join(",")}`).toBe(ids.length);
  });
});

describe("registry の形の検証（複製に変異を当てる）", () => {
  const withDir = async (files, fn) => {
    const dir = mkdtempSync(path.join(tmpdir(), "gov-registry-"));
    try {
      for (const [name, body] of Object.entries(files)) writeFileSync(path.join(dir, name), body);
      return await fn(dir);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  };
  it("id を持たないモジュールがあれば throw する（沈黙して落とさない）", async () => {
    await withDir({ "G-a.mjs": 'export const id = "G-a";\nexport const domains = "unmigrated";\nexport function run() { return []; }\n', "G-bad.mjs": "export function run() { return []; }\n" }, async (dir) => {
      await expect(checkModulesFrom(dir)).rejects.toThrow(/G-bad\.mjs/);
    });
  });
  it("run を持たないモジュールがあれば throw する", async () => {
    await withDir({ "G-bad.mjs": 'export const id = "G-bad";\n' }, async (dir) => {
      await expect(checkModulesFrom(dir)).rejects.toThrow(/G-bad\.mjs/);
    });
  });
  it("ファイル名の stem と id が食い違えば throw する（一覧が ID の一覧であることの担保）", async () => {
    await withDir({ "G-a.mjs": 'export const id = "G-different";\nexport function run() { return []; }\n' }, async (dir) => {
      await expect(checkModulesFrom(dir)).rejects.toThrow(/G-a\.mjs/);
    });
  });
  it("`.test.mjs` は検査として読まない", async () => {
    await withDir({ "G-a.mjs": 'export const id = "G-a";\nexport const domains = "unmigrated";\nexport function run() { return []; }\n', "G-a.test.mjs": "export const nothing = 1;\n" }, async (dir) => {
      expect((await checkModulesFrom(dir)).map((m) => m.id)).toEqual(["G-a"]);
    });
  });
});

describe("domains の宣言要求（Task 3）", () => {
  it("domains を宣言していない検査モジュールはファイル名を名指して throw する", async () => {
    const dir = mkdtempSync(path.join(tmpdir(), "checks-"));
    writeFileSync(path.join(dir, "G-x.mjs"), 'export const id = "G-x";\nexport function run() { return []; }\n');
    await expect(checkModulesFrom(dir)).rejects.toThrow(/G-x\.mjs/);
    rmSync(dir, { recursive: true, force: true });
  });
});

describe("domains の中身の検証（I1 — 空配列・綴り違いはラチェットを無償で減らす）", () => {
  const withOne = async (body, fn) => {
    const dir = mkdtempSync(path.join(tmpdir(), "checks-i1-"));
    try {
      writeFileSync(path.join(dir, "G-x.mjs"), body);
      return await fn(dir);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  };
  it("domains = [] （空配列）は throw する", async () => {
    await withOne('export const id = "G-x";\nexport const domains = [];\nexport function run() { return []; }\n', async (dir) => {
      await expect(checkModulesFrom(dir)).rejects.toThrow(/G-x\.mjs/);
    });
  });
  it("DOMAIN_SPECS に無い名前（綴り違い）は throw する", async () => {
    await withOne(
      'export const id = "G-x";\nexport const domains = ["governanceDocsTYPO"];\nexport function run() { return []; }\n',
      async (dir) => {
        await expect(checkModulesFrom(dir)).rejects.toThrow(/G-x\.mjs/);
      },
    );
  });
  it('["*"]（メタ検査と同じ形）は登録される', async () => {
    await withOne('export const id = "G-x";\nexport const domains = ["*"];\nexport function run() { return []; }\n', async (dir) => {
      expect((await checkModulesFrom(dir)).map((m) => m.id)).toEqual(["G-x"]);
    });
  });
  it("実在するドメイン名の配列は登録される", async () => {
    await withOne(
      'export const id = "G-x";\nexport const domains = ["governanceDocs", "adrFiles"];\nexport function run() { return []; }\n',
      async (dir) => {
        expect((await checkModulesFrom(dir)).map((m) => m.id)).toEqual(["G-x"]);
      },
    );
  });
});

// I4 — 未移行マーカーのラチェット。`"unmigrated"` は Phase 4 で API から消えるまでの逃げ道であり、
// **減る方向にしか動かない**。evidence は残数を印字するだけで、増えても何も落ちない
// （Phase 1 の全体レビューが実測した欠落）。ここで現在の id を凍結し、実際の未移行集合と
// **一致**することを検める。一致（片側の包含ではなく）にするのは 2 方向の理由がある。
//   - 増える向き: 新しい検査が `"unmigrated"` を選ぶ／移行済みが逆流する → 凍結に無い id が出て赤
//   - 減る向き: 移行が進んだら**同じコミットで刈る**ことを強制する。刈らずに残すと、移行済みの
//     検査が後から `"unmigrated"` へ戻る後退が凍結集合の内側に収まって沈黙する
// **射程の限界**: この列自身を書き換えれば通る。機構が保証するのは「その 1 行が diff に現れる」
// ことであって、レビュアが見落とさないことではない。
//
// 残っている 5 本は**同じクラス**である——母集団が走査（`snapshot.files`）ではなく、名指しされた
// ファイルの中身から出る。実測（2026-08-20）: 4 本は `snapshot.files` を一度も読まず、`G-hook-fires` の
// 使用も表の代表パスの実在照合であって母集団の filter ではない。
// **このクラスへ錨を足すと、検査自身の判定の言い直しになる**（clippy の禁止メソッドなら、縮み方の
// 全モード——ファイル消失・配列消失・行消失——を検査が既に fail-closed で赤にしており、ドメインは
// 新しい被覆を持たず同じ欠陥に finding を 2 件出すだけである）。Phase 1 の全体レビューが 5 回捕まえた
// 「常に真になる錨」を、こちらの手で再生産することになる。
// **ゆえに完了判定を書き換えるまで着手しない**（Phase 3 の申し送りが要求しているのと同じ書き換え）。
const FROZEN_UNMIGRATED = [
  "G-architecture-table",
  "G-ci-table",
  "G-clippy-disallowed",
  "G-hook-commands",
  "G-hook-fires",
];

describe("未移行マーカーのラチェット（I4）", () => {
  it("未移行の検査は凍結した集合と一致する", () => {
    const actual = CHECK_MODULES.filter((m) => m.domains === "unmigrated")
      .map((m) => m.id)
      .sort();
    const frozen = [...FROZEN_UNMIGRATED].sort();
    const added = actual.filter((id) => !frozen.includes(id));
    const migrated = frozen.filter((id) => !actual.includes(id));
    expect(added, `未移行が増えた: ${added.join(", ")}（新しい検査はドメインを宣言する）`).toEqual([]);
    expect(migrated, `移行済み: ${migrated.join(", ")} — FROZEN_UNMIGRATED から同じコミットで消す`).toEqual([]);
  });
});

describe("走査が母集団である — ファイルの増減がそのまま検査の増減になる（#1088）", () => {
  it("使い捨てディレクトリからファイルを 1 本消すと、その id が registry から消える", async () => {
    const dir = mkdtempSync(path.join(tmpdir(), "gov-registry-"));
    try {
      const mod = (id) => `export const id = "${id}";\nexport const domains = "unmigrated";\nexport function run() { return []; }\n`;
      writeFileSync(path.join(dir, "G-a.mjs"), mod("G-a"));
      writeFileSync(path.join(dir, "G-b.mjs"), mod("G-b"));
      expect((await checkModulesFrom(dir)).map((m) => m.id)).toEqual(["G-a", "G-b"]);
      rmSync(path.join(dir, "G-b.mjs"));
      // 対照との差が証拠である——緑になったこと自体は証拠ではない
      expect((await checkModulesFrom(dir)).map((m) => m.id)).toEqual(["G-a"]);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
