//! G-hook-fires — PostToolUse hook の発火割り当て ↔ docs/hooks.md「PostToolUse（post-edit.mjs）の発火一覧」の照合（#863）。
import { finding } from "../lib.mjs";
// G-hook-fires は判定を再実装せず hook の純関数そのものを呼ぶ（理由は同検査のコメント）。
// post-edit.mjs は import しただけでは main() を走らせない（I13 のガード）。
import { selectChecks } from "../../../.claude/hooks/post-edit.mjs";

export const id = "G-hook-fires";
export const domains = "unmigrated";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return checkHookFires(snapshot);
}

// ---------------------------------------------------------------------------
// G-hook-fires — PostToolUse hook の発火割り当て ↔ docs/hooks.md「PostToolUse（post-edit.mjs）の
// 発火一覧」の照合（#863）。
//
// 表は selectChecks の写しであり、同型のドリフトは一度起きている——ルート CLAUDE.md の同じ表が腐り、
// その退去先が docs/hooks.md である（#474〜#497）。#858 で fmt を足したとき、この行は実際に計画の
// 変更ファイル一覧から漏れた。
//
// **G-hook-commands と形が違うのは、可視性が違うからである。** あちらが hook のソーステキストから
// `cargoSpec([...])` を抽出するのは cargoSpec が**非 export** だからであって、import が危険だから
// ではない（post-edit.mjs は I13 のガードで import 安全であり、pre-bash.test.mjs が既に import している）。
// selectChecks は export 済みなので、**判定を再実装せず関数そのものを呼ぶ** — 抽出で近似すると、
// 閉じたい写しを一段下で作り直すことになる。テストのため既定値付き引数で注入する
// （checkModuleIndex / checkConfigFieldReachability と同形）。
//
// 照合は 2 方向:
//   (1) 行ごと — select(代表パス) と「走る検査 id」列が**順序込みで**一致する。順序は表自身の主張
//       （fmt を先頭に置く理由が本文に書いてある）ゆえ、集合ではなく配列で比較する。
//   (2) 母集団 — ソースの `checks.push("<id>")` リテラル全件が表のどこかに現れる。逆向き（表にあって
//       発行されない id）は (1) から導かれるので、こちらは順方向だけでよい。
//
// 母集団を BUDGETS から取らない: BUDGETS ⊇ 発行されうる id であり、どのパスも発火させない id が
// BUDGETS に入ったとき、表に「決して走らない検査」を書かせることになる。
//
// **代表パスの実在は自前で見る。** G-references に委ねる案は却下した——あちらの述語は「バッククォート内で
// `/` を含み REF_EXTENSIONS に当たる」であり、実測すると現行 10 行のうち `Cargo.toml`（`/` 無し）と
// `.githooks/pre-commit`（拡張子無し）の 2 行が述語の外にある。代償として実在しない入力（4 crate 外の
// `.rs`・`config.toml`）は行にできない＝受容する残余は ADR-hook-fires-table-check が名指しする。
// ---------------------------------------------------------------------------
const HOOK_FIRES_HEADER = /^\|\s*編集したファイル（代表パス）\s*\|\s*走る検査 id\s*\|/;

export function checkHookFires(snapshot, select = selectChecks) {
  const findings = [];
  const docsPath = "docs/hooks.md";
  const hookPath = ".claude/hooks/post-edit.mjs";
  const text = snapshot.read(docsPath);
  if (text == null) return [finding(docsPath, 1, "docs/hooks.md が読めない（G-hook-fires 母集団の欠落）")];
  const hookSrc = snapshot.read(hookPath);
  if (hookSrc == null) return [finding(hookPath, 1, "post-edit.mjs が読めない（G-hook-fires 母集団の欠落）")];
  // 行分割は \r?\n — CRLF checkout では列末に \r が残り、id の一致がすべて外れる（#587/#589 で二度踏んでいる）
  const lines = text.split(/\r?\n/);
  const headers = lines.map((l, i) => (HOOK_FIRES_HEADER.test(l) ? i : -1)).filter((i) => i >= 0);
  if (headers.length === 0) {
    return [
      finding(docsPath, 1, "発火一覧のヘッダ行（| 編集したファイル（代表パス） | 走る検査 id | …）が見つからない（G-hook-fires 母集団の欠落）"),
    ];
  }
  // 例示（コードフェンス内の書き方の見本など）でヘッダが 2 本になると、findIndex は先に現れた方を
  // 掴み、本物の表が照合されないまま緑になる。母集団の曖昧化は明示的な赤にする
  if (headers.length > 1) {
    return [finding(docsPath, headers[1] + 1, `発火一覧のヘッダ行が ${headers.length} 本ある（どれが本物か決まらない・G-hook-fires 母集団の曖昧化）`)];
  }
  const start = headers[0];
  const fileSet = new Set(snapshot.files);
  const mentioned = new Set();
  let rows = 0;
  let sawEmptyRow = false;
  // 終了条件は「最初の空行」である（checkCiTable と同じ理由）——`startsWith("|")` で打ち切ると、
  // 表の途中に非表の行が 1 行紛れただけで**以降の行が照合されないまま緑になる**（#863 の code-review が実測）
  for (let i = start + 2; i < lines.length && lines[i].trim() !== ""; i++) {
    if (!lines[i].startsWith("|")) {
      findings.push(finding(docsPath, i + 1, `発火一覧の途中に表でない行がある（以降が照合されない経路）: ${lines[i]}`));
      continue;
    }
    // 必要なのは先頭 2 列 = split で 4 要素以上（実表は補足列を持つ 3 列 = 5 要素）
    const cols = lines[i].split("|").map((c) => c.trim());
    if (cols.length < 4) {
      findings.push(finding(docsPath, i + 1, `発火一覧の行に代表パス列と検査 id 列がそろっていない: ${lines[i]}`));
      continue;
    }
    // 代表パス列は「バッククォート括りの単一パス」だけを許す。glob や複数併記を通すと
    // select() へ何を食わせたかが曖昧になり、緑の意味が薄まる
    const rel = cols[1].match(/^`([^`]+)`$/)?.[1];
    if (!rel) {
      findings.push(finding(docsPath, i + 1, `代表パス列がバッククォート括りの単一パスでない: ${cols[1]}`));
      continue;
    }
    rows += 1;
    // 実在は自前で見る。G-references に委ねると、その述語（`/` を含み REF_EXTENSIONS に当たる）から
    // 外れる代表パス（`Cargo.toml`・`.githooks/pre-commit`）が素通りする——実測で 10 行中 2 行。
    // 死んだパスは `selectChecks` の接頭辞判定を通り続けるので、表だけが静かに嘘になる
    if (!fileSet.has(rel)) {
      findings.push(finding(docsPath, i + 1, `代表パスが実在しない（死んだ行は判定を通り続ける）: ${rel}`));
    }
    // 検査 id 列のバッククォートは検査 id だけ（散文は補足列へ置く、が書式規約）
    const declared = [...cols[2].matchAll(/`([^`]+)`/g)].map((m) => m[1]);
    if (declared.length === 0) {
      // 空集合は「（なし）」と綴らせる。散文・空白・空バッククォートを空集合と読むと、
      // 書き崩しが「検査は走らない」という主張に化ける
      if (cols[2] === "（なし）") sawEmptyRow = true;
      else findings.push(finding(docsPath, i + 1, `検査 id 列にバッククォートが無い。空集合は「（なし）」と書く: ${cols[2]}`));
    }
    for (const id of declared) mentioned.add(id);
    const actual = select(rel);
    if (declared.join(" ") !== actual.join(" ")) {
      findings.push(
        finding(
          docsPath,
          i + 1,
          `発火一覧が selectChecks と一致しない: \`${rel}\` は [${actual.join(", ")}] を発火するが表は [${declared.join(", ")}]`,
        ),
      );
    }
  }
  if (rows === 0) {
    findings.push(finding(docsPath, start + 1, "発火一覧の行が 0 件（G-hook-fires 母集団の欠落）"));
    return findings;
  }
  // 行の削除は、id を持つ行なら下の母集団照合が拾う。**唯一拾えないのが空集合の行**であり、
  // それは「割り当ての無いファイルの沈黙は合格ではない」という最上位の契約を運ぶ行である（#471/#497）。
  // 消しても緑になる経路をここで閉じる
  if (!sawEmptyRow) {
    findings.push(finding(docsPath, start + 1, "検査が 1 つも走らないパスの行が無い——「沈黙は合格ではない」という主張が表から消えている"));
  }
  const emitted = new Set([...hookSrc.matchAll(/checks\.push\("([^"]+)"\)/g)].map((m) => m[1]));
  if (emitted.size === 0) {
    findings.push(
      finding(hookPath, 1, 'checks.push("<id>") が 1 件も抽出できない（G-hook-fires 母集団の欠落。抽出アンカーの腐敗か selectChecks のリファクタ）'),
    );
    return findings;
  }
  for (const id of emitted) {
    if (!mentioned.has(id)) {
      findings.push(finding(docsPath, start + 1, `selectChecks が発行しうる検査 id が発火一覧のどの行にも現れない: ${id}`));
    }
  }
  return findings;
}
