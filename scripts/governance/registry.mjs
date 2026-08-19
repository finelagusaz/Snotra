//! 検査の登録は**ディレクトリの走査から導出する**——`checks/` に置いたファイルがそのまま検査になり、
//! 忘れうる登録行が存在しない（#1088 が問うた欠陥の構造的な解消）。
//! **`checks/` の外にあるものは検査ではない**——合否を持たない計器は `instrument.mjs` に置く
//! （`ADR-retire-area-budget`）。
//! **各検査は `domains` を宣言必須**——非空の配列（要素は `DOMAIN_SPECS` の名前か `"*"`）か
//! `"unmigrated"` のいずれかで、空配列・未知の名前・それ以外の形はここが起動時点で throw で拒む。
import { readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { DOMAIN_SPECS } from "./domains.mjs";

const DOMAIN_NAMES = new Set(DOMAIN_SPECS.map((spec) => spec.name));

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
    if (!Array.isArray(m.domains) && m.domains !== "unmigrated") {
      throw new Error(`検査モジュールが domains を宣言していない（ドメイン名の配列か "unmigrated"）: ${f}`);
    }
    if (Array.isArray(m.domains)) {
      // 空配列は「非空の配列」という契約を通ってしまう——固定パスの検査を新設した作者が
      // 「消費するドメインが無いので空配列」と書く形で素直に到達する（悪意ではない）。
      // 綴り違いのドメイン名も同様に、素朴な `Array.isArray` だけでは通ってしまう。
      // どちらも「宣言はあるが中身が空 or 実在しない」ため、未移行の残数（`"unmigrated"` の件数）を
      // 移行せずに減らせてしまう——ラチェットを無償で消費する経路になる。
      if (m.domains.length === 0) {
        throw new Error(`検査モジュールの domains が空配列（"*" か DOMAIN_SPECS の名前を 1 つ以上宣言する）: ${f}`);
      }
      for (const d of m.domains) {
        if (d !== "*" && !DOMAIN_NAMES.has(d)) {
          throw new Error(`検査モジュールが未知のドメイン名を宣言している: ${f}（domains に "${d}"）`);
        }
      }
    }
    mods.push(m);
  }
  return mods;
}

export const CHECK_MODULES = await checkModulesFrom(CHECKS_DIR);
