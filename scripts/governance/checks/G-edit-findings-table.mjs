//! G-edit-findings-table — 編集時 reminder の判定 ↔ docs/hooks.md「検査ではない reminder（発火一覧に現れない）」の照合（#992）。
import { finding } from "../lib.mjs";
// **判定を再実装せず、実装が持つ配列そのものを読む**（理由は下のヘッダ）。
// edit-findings.mjs は CLI だが `isMain` ガードを持つので import しても main は走らない。
import { SCAN_SCOPED } from "../edit-findings.mjs";

export const id = "G-edit-findings-table";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return ctx.record("editFindingsRows", scanEditFindingsTable(snapshot));
}

// ---------------------------------------------------------------------------
// G-edit-findings-table — 編集時 reminder の判定と、`docs/hooks.md`「検査ではない reminder（発火一覧に現れない）」の照合（#992）。
//
// **表は実装の写しであり、この型は一度腐っている**——`selectChecks` の写しである発火一覧が
// 同じ形で腐り、その手当てが `G-hook-fires` である（#863。さらにその前は ルート `CLAUDE.md` の
// 同じ表が腐って `docs/hooks.md` へ退去した・#474〜#497）。#992 が reminder を 1 本足したとき、
// **表と実装の対応を誰も検算していない**ことが分かった——片方だけ足しても緑のまま推移する。
//
// **判定を再実装せず `SCAN_SCOPED` を import する。** `G-hook-fires` が `selectChecks` を
// 呼ぶのと同じ理由であり、同検査のヘッダが言う「抽出で近似すると、閉じたい写しを一段下で
// 作り直すことになる」がそのまま当たる。ゆえに**ここに判定名の一覧を書かない**。
//
// **見るのは判定名の集合であって、行の意味ではない。** 発火条件の散文（どの母集団を見るか・
// 何が鳴るか）はコードに正本が無く、機械では確かめようがない——**表の値打ちはその散文と
// 「鳴らないケース」の一覧にあり、この検査はそこを守らない**。守るのは「実装に在る判定が
// 表のどこかに現れ、表に在る判定が実装に在る」という対応だけである。
//
// **同じ判定が複数行に現れてよい。** 集合で照合するのはそのためで、`checkModuleIndex` は
// 実際に 2 行（`.rs` 側と `<crate>/CLAUDE.md` 側）を持つ。行数を数える形にすると、
// 発火条件を 1 つ書き分けるたびにこの検査が偽陽性になる。
//
// **宣言する死角——ここが見ないもの:**
//   - **発火条件の散文が実装とずれること**（母集団の取り違え・拡張子ゲートの記述）。上記のとおり
//   - **`dependents.mjs` 側の判定**（`reportFor`）。`SCAN_SCOPED` の外に在るので下の EXTRA が
//     受けている——**その 2 つは手で並べた写しである**（`checkModuleIndex` / `checkReferences` /
//     `checkAdrFileNames` も同じく `SCAN_SCOPED` の外にあり、`edit-findings.mjs` が個別に呼ぶ）。
//     **配列と違い、こちらは実装から導けない**（呼び出しが条件分岐の中に散っているため）ので、
//     この写しが腐る経路は残る。実装側で名前を変えれば import が失敗して鳴るが、
//     **呼び出しごと消えても気づかない**
// ---------------------------------------------------------------------------

/**
 * `SCAN_SCOPED` の外で `edit-findings.mjs` / `dependents.mjs` が呼ぶ判定。
 *
 * **ここだけが手で並べた写しである**（上のヘッダの死角）。import しているのは名前の実在を
 * 静かでない形で保つため——綴りを間違えれば import が失敗し、`governance:check` が起動しない。
 */
import { checkModuleIndex } from "./G-module-index.mjs";
import { checkReferences } from "./G-references.mjs";
import { checkAdrFileNames } from "./G-adr-file-names.mjs";
import { reportFor } from "../dependents.mjs";

const EXTRA = [checkModuleIndex, checkReferences, checkAdrFileNames, reportFor];

/** 実装が持つ判定名の集合。**一覧を書かず `SCAN_SCOPED` から導く**ので、配列へ足せば自動で増える */
export function tableJudgments() {
  return new Set([...SCAN_SCOPED.map((s) => s.check.name), ...EXTRA.map((f) => f.name)]);
}

const HEADING = "## 検査ではない reminder（発火一覧に現れない）";
const TABLE_HEADER = /^\|\s*発火条件\s*\|\s*出るもの\s*\|\s*判定\s*\|/;

/** findings に加えて照合件数を返す（「差分ゼロ」と「照合していない」を区別する証跡・#497） */
export function scanEditFindingsTable(snapshot) {
  const findings = [];
  const docsPath = "docs/hooks.md";
  const text = snapshot.read(docsPath);
  if (text == null) return { findings: [finding(docsPath, 1, "docs/hooks.md が読めない（G-edit-findings-table 母集団の欠落）")], checked: 0 };
  // 行分割は `\r?\n` — CRLF checkout では列末に `\r` が残り、名前の一致がすべて外れる（#587/#589 で二度踏んでいる）
  const lines = text.split(/\r?\n/);
  if (!lines.some((l) => l.trim() === HEADING)) {
    return { findings: [finding(docsPath, 1, `「${HEADING.replace(/^#+\s*/, "")}」の節が無い（G-edit-findings-table 母集団の欠落）`)], checked: 0 };
  }
  // **見出しより後だけを探す。** 文書全体から拾うと、無関係な節に同じ列名の表があるとき
  // **本物の表が丸ごと消えていても「母集団の欠落」を報告せず**、decoy を唯一の候補として採る
  // （2026-08-22 実測: checked=1 のまま、実在する 8 判定が「表に無い」という別の偽陽性へすり替わった）。
  const from = lines.findIndex((l) => l.trim() === HEADING);
  const headers = lines.map((l, i) => (i > from && TABLE_HEADER.test(l) ? i : -1)).filter((i) => i >= 0);
  if (headers.length === 0) {
    return { findings: [finding(docsPath, 1, "reminder 表のヘッダ行（| 発火条件 | 出るもの | 判定 |）が見つからない（G-edit-findings-table 母集団の欠落）")], checked: 0 };
  }
  // 例示でヘッダが 2 本になると、先に現れた方を掴んで本物の表が照合されないまま緑になる
  if (headers.length > 1) {
    return {
      findings: [finding(docsPath, headers[1] + 1, `reminder 表のヘッダ行が ${headers.length} 本ある（どれが本物か決まらない・G-edit-findings-table 母集団の曖昧化）`)],
      checked: 0,
    };
  }
  const start = headers[0];
  const declared = new Set();
  let checked = 0;
  // 終了条件は「最初の空行」である（`G-hook-fires` と同じ理由）——`startsWith("|")` で打ち切ると、
  // 表の途中に非表の行が 1 行紛れただけで**以降の行が照合されないまま緑になる**
  for (let i = start + 2; i < lines.length && lines[i].trim() !== ""; i++) {
    if (!lines[i].startsWith("|")) {
      findings.push(finding(docsPath, i + 1, `reminder 表の途中に表でない行がある（以降が照合されない経路）: ${lines[i]}`));
      continue;
    }
    const cols = lines[i].split("|").map((c) => c.trim());
    if (cols.length < 4) {
      findings.push(finding(docsPath, i + 1, `reminder 表の行に判定列がそろっていない: ${lines[i]}`));
      continue;
    }
    // **判定列の span をすべて見る**（注釈が付いてよい: `reportFor`（`dependents.mjs`））。
    // かつては先頭 1 つだけを判定名として取っていたが、**注釈を前に置くだけで 2 件の偽陽性になった**
    // ——`` `dependents.mjs`（`reportFor`） `` は人が読めば等価なのに「実装に無い」＋「表に無い」を
    // 同時に出す（2026-08-22 実測）。全部を候補にして実装が持つ名前と交差させれば、順序に依存しない。
    // 散文へ崩れると照合が静かに緩むので、span が 1 つも無い行は赤にする。
    const spans = [...cols[3].matchAll(/`([^`]+)`/g)].map((s) => s[1]);
    if (spans.length === 0) {
      findings.push(finding(docsPath, i + 1, `判定列にバッククォート括りの判定名が無い（散文へ崩れると照合が緩む）: ${cols[3]}`));
      continue;
    }
    const known = spans.filter((s) => tableJudgments().has(s));
    // **実装に在る名前が 1 つも無い行は、その先頭を「実装に無い判定」として報告させる**
    // （綴り間違いを沈黙させないため。注釈だけの行という形は今日存在しない）
    checked += 1;
    for (const s of known.length > 0 ? known : [spans[0]]) declared.add(s);
  }
  if (checked === 0) {
    findings.push(finding(docsPath, start + 1, "reminder 表の行が 0 件（G-edit-findings-table 母集団の欠落）"));
    return { findings, checked };
  }
  const actual = tableJudgments();
  for (const name of declared) {
    if (!actual.has(name)) {
      findings.push(finding(docsPath, start + 1, `reminder 表の判定が実装に無い: \`${name}\`（edit-findings.mjs / dependents.mjs のどちらも持たない）`));
    }
  }
  for (const name of actual) {
    if (!declared.has(name)) {
      findings.push(finding(docsPath, start + 1, `編集時に走る判定が表に無い: \`${name}\`（鳴るのに docs/hooks.md が告げていない）`));
    }
  }
  return { findings, checked };
}

export function checkEditFindingsTable(snapshot) {
  return scanEditFindingsTable(snapshot).findings;
}
