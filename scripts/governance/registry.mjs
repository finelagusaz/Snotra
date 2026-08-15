//! 検査の登録は**ディレクトリの走査から導出する**——`checks/` に置いたファイルがそのまま検査になり、
//! 忘れうる登録行が存在しない（#1088 が問うた欠陥の構造的な解消）。
//! **`checks/` の外にあるものは検査ではない**——合否を持たない計器は `instrument.mjs` に置く
//! （`ADR-retire-area-budget`）。
import { readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// 走査元は `import.meta.url` 起点である。**`process.cwd()` 起点にしてはならない**——CI の
// manifest 差分は base 側のコードを別ツリーを cwd にして走らせるため、cwd 起点だと
// 「読むコードと読む木がずれる」（#1092 の H1 と同じ型）。
const CHECKS_DIR = path.join(path.dirname(fileURLToPath(import.meta.url)), "checks");

/** `dir` 直下の検査モジュールを id 昇順で返す。形が不正なら**そのファイル名を名指しして throw する**
 *  ——沈黙して 1 本落とすと #1088 の欠陥がそのまま戻る。テストが使い捨てディレクトリを渡せるよう
 *  引数に取る（稼働中の `checks/` へ変異を当てないため・`.claude/rules/safety-nets.md`）。 */
export async function checkModulesFrom(dir) {
  const files = readdirSync(dir)
    .filter((f) => f.endsWith(".mjs") && !f.endsWith(".test.mjs"))
    .sort();
  const mods = [];
  for (const f of files) {
    // `import()` へ素の Windows パスを渡すと ERR_UNSUPPORTED_ESM_URL_SCHEME になる。
    // 自前で組み立てず `pathToFileURL` を使う——区切り・ドライブレター・percent-encode の
    // 扱いを自作すると、動く機体と動かない機体が生まれる
    const m = await import(pathToFileURL(path.join(dir, f)).href);
    if (typeof m.id !== "string") throw new Error(`検査モジュールが id を export していない: ${f}`);
    if (typeof m.run !== "function") throw new Error(`検査モジュールが run を export していない: ${f}`);
    if (m.id !== path.basename(f, ".mjs")) throw new Error(`ファイル名と id が食い違う: ${f}（id=${m.id}）`);
    mods.push(m);
  }
  return mods;
}

export const CHECK_MODULES = await checkModulesFrom(CHECKS_DIR);
