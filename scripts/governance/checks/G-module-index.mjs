//! G-module-index — 各サブディレクトリの CLAUDE.md モジュール構成表と実ファイルの双方向対応。
import { finding, sectionOf } from "../lib.mjs";

export const id = "G-module-index";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return checkModuleIndex(snapshot);
}

// ---------------------------------------------------------------------------
// G-module-index — 各サブディレクトリ CLAUDE.md「モジュール構成」↔ 実ファイルの双方向照合。
// basename 包含方式: ディレクトリ集約行（`commands/` のベア名列挙）・`tabs/` プレフィックス省略・
// 1 行複数バッククォートをパースせずに済ませる意図的な弱化（wrong-directory 検出は放棄）。
// ---------------------------------------------------------------------------
// ui は #532 SU7 のフロント撤去で消滅（ui/CLAUDE.md ごと削除）
// snotra-egui-runtime は #701 で追加。「#532 の検証層」として作られたまま母集団から漏れており、
// SU7 で製品の描画層になった後も更新されていなかった（G-references の governanceDocs も同時に是正）
/** G-module-index が照合する crate。**本検査の保証は狭い**——crate を新設してここへ足さなければ、
 *  その `CLAUDE.md` のモジュール構成は順方向も逆方向も一度も照合されず `governance:check` は緑を
 *  返す（2026-08-09 実測: member を 1 つ増やし、その索引へ実在しない `.rs` を書いても緑・#1008）。
 *  真の母集団はルート `Cargo.toml` の `[workspace] members` であり、この表はその写しである。
 *
 *  **写しのずれは 1 つの性質ではなく、性質ごとに守り手が違う**（#1155 で数え直した）:
 *  - **`CLAUDE.md` を持つ member が本表と `governanceDocs()` の両方に載る** — `governance-check.test.mjs`
 *    の母集団カナリア（#701）が実 `Cargo.toml` を読んで `npm test` で強制する。**守り手は在る**
 *    （ただし `skip-ci` ラベルの付いた PR では走らない）
 *  - **本表由来の母集団が member の `src/` の外へ出ない** — 部分集合テストが見ていたが、錨の層と
 *    一緒に #1152 で撤去された。**今日の守り手はゼロである**
 *  - **母集団が黙って縮む（`exts` / `excludeTest` の狭窄）** — 錨が見ることになっていたが同じく撤去された。
 *    **今日の守り手はゼロであり、撤去前も実際には誰も赤にしていなかった**（2026-08-20 実測:
 *    `exts` を狭めて 30 件を落としても、錨・部分集合テスト・#701 のカナリアの 3 つとも緑だった）
 *
 *  `CLAUDE.md` を持たない crate（そのとき照合すべき索引もまだ無い）は元から射程の外である。 */
export const MODULE_INDEX_CRATES = {
  "snotra-core": { src: "snotra-core/src/", exts: /\.rs$/ },
  "snotra-egui-runtime": { src: "snotra-egui-runtime/src/", exts: /\.rs$/ },
  "src-tauri": { src: "src-tauri/src/", exts: /\.rs$/ },
  "snotra-settings": { src: "snotra-settings/src/", exts: /\.rs$/ },
};

/** 索引が覆うべき production ファイル。
 *  **crate の一覧は `MODULE_INDEX_CRATES` から出る**（ルート `Cargo.toml` からではない）。
 *  この 2 本目の導出は意図である。**食い違いを全部捕まえる検知器は無い**——性質ごとの守り手は
 *  上の `MODULE_INDEX_CRATES` の doc が名指す（そこが正本）。`crateSourceFiles` と畳んではならない。 */
export function moduleIndexSources(snapshot, crates = Object.keys(MODULE_INDEX_CRATES)) {
  return snapshot.files.filter((f) =>
    crates.some((c) => {
      const cfg = MODULE_INDEX_CRATES[c];
      return f.startsWith(cfg.src) && cfg.exts.test(f) && !(cfg.excludeTest && cfg.excludeTest.test(f));
    }),
  );
}

export function checkModuleIndex(snapshot, crates = Object.keys(MODULE_INDEX_CRATES)) {
  const findings = [];
  const allBasenames = new Set(snapshot.files.map((f) => f.split("/").pop()));
  for (const crate of crates) {
    const cfg = MODULE_INDEX_CRATES[crate];
    const mdPath = `${crate}/CLAUDE.md`;
    const text = snapshot.read(mdPath);
    if (text == null) {
      findings.push(finding(mdPath, 1, "CLAUDE.md が読めない（G-module-index 母集団の欠落）"));
      continue;
    }
    // **`ending` の宣言は 4 文書で共有される**——どれか 1 つで「モジュール構成」が最終節になれば
    // `sectionOf` が④で赤くする。そのとき直すのは文書か、この宣言を crate ごとに分けるかであり、
    // どちらにせよ気づかれる（受容する残余: 宣言が 1 つゆえ、分ける改修は 4 文書を巻き込む）
    const sec = sectionOf(text, /^## モジュール構成$/, { file: mdPath, ending: "heading", by: id });
    if (sec.body == null) {
      findings.push(...sec.findings);
      continue;
    }
    // 本文が空の節は有効（`""` を「節が無い」と読まない）——逆方向の照合が実ファイルの側を赤にする。
    // **「全件」ではない**——逆方向が見るのは `section` ではなく `text` 全体なので、節を空にしても
    // 本文の他所でバッククォート付きで言及されているファイルは緑のまま残る（2026-08-17 実測）
    const section = sec.body;
    // 順方向: 節内のバッククォート付きソースファイル名 → basename がリポジトリに実在。
    // **見るのは直下の正規表現が挙げる拡張子だけである**——`` `foo.mjs` `` のような他種の
    // バッククォート参照は実在照合されない（2026-08-09 実測・#1008）。どれを対象にするかは
    // 本プロジェクトの編集方針であって、外部仕様の写しではない。
    for (const m of section.matchAll(/`([^`\n]+\.(?:rs|ts|tsx|html))`/g)) {
      const token = m[1];
      if (/[*?{]/.test(token)) continue; // glob・パターン例は対象外
      const base = token.split("/").pop();
      if (!allBasenames.has(base)) {
        findings.push(finding(mdPath, 1, `索引に記載の \`${token}\` に対応する実ファイル（basename: ${base}）が無い`));
      }
    }
    // 逆方向: production ファイルの basename が CLAUDE.md 本文に出現
    const production = moduleIndexSources(snapshot, [crate]);
    for (const f of production) {
      const base = f.split("/").pop();
      if (!text.includes(`\`${base}\``) && !text.includes(`/${base}\``)) {
        findings.push(finding(mdPath, 1, `実ファイル ${f} が索引（本文のバッククォート）に見当たらない`));
      }
    }
  }
  return findings;
}
