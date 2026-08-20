//! G-folded-heading-refs — 正準形の参照が物理改行で折れた形（#1154）。
import { finding, refScanLines, isRefTargetSpelling, REF_HEAD } from "../lib.mjs";

export const id = "G-folded-heading-refs";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（allRefDocs・record を使う） */
export function run(snapshot, ctx) {
  return ctx.record("foldedRefs", scanFoldedHeadingRefs(snapshot, ctx.allRefDocs));
}

// ---------------------------------------------------------------------------
// G-folded-heading-refs — 正準形の参照が物理改行で折れた形（#1154）。
//
// `HEADING_REF` も `G-near-heading-refs` の `NEAR_REF` も**ラベルの閉じを同一行に要求する**ので、
// 折れた参照は一致そのものが生成されない——`checked` にも `findings` にも現れず、
// **「照合していない」と「差分ゼロ」を分ける証跡すら残らない**。#1128 の変異注入が
// 照合件数 285 → 284 の静かな減少として実測した形であり、#1154 の時点で **33 件実在**した
// （うち 1 件は繋いでも着地しない＝折れが本物の腐りを隠していた）。
//
// **見る形は 2 つある。片方だけでは足りない**（形 A だけを実装すると 13 件の既知の見逃しを持つ）:
//   形 A: 対象綴りで行が終わり、ラベルが次行から始まる
//   形 B: `「` まで同一行に開き、ラベル本文だけが次行へ流れる
// **形 B は次行を見ない**——対象綴りの直後に開いた `「` がその行で閉じていない時点で折れであり、
// 次行が何であっても一致は生成されない。この形ならラベルが 3 行以上に割れても同じ述語で捕まる。
//
// **射程はファイル種別に依らない**——正準形の指しはどの言語のコメントでも機構の入力だからである。
// これは宣言として要る: 折れの 9 件は `.rs` 以外に在る一方、`docs/comment-guidelines.md` の配送は
// 4 crate の `**/*.rs` のみで、**`.ps1` / `.psm1` にはコメント作法の生きた規範が一つも無い**。
// 規範が一言も無い面で機構だけが赤を出すので、**この宣言が赤を受け取った者の唯一の拠り所になる**。
//
// **宣言する死角——ここが見ないもの:**
//   - **バッククォート自体が行を跨ぐ形**（対象の綴りが 2 行に割れる）。今日 0 件で、0 である理由は
//     `docs/comment-guidelines.md`「日本語の折返し」の「コードスパンを行またぎさせない」が先に効いているため
//   - **助詞を挟んで折れた近傍形**（`G-near-heading-refs` の対象が折れたもの）
//   - **`refScanLines` が次行を供給しない形**（フェンス内・非コメント行に落ちた次行）。形 A のみ・fail-open
// 決定と却下した代替案は ADR-folded-canonical-reference-detector が持つ。
//
// **例示に実在の対象を置かない**——`checks/` はこの検査群の母集団（コメントの腕）である。
// プレースホルダは対象の形（`.md` か `/skill`）に当たらないので、この検査自身の finding にならない。
// ---------------------------------------------------------------------------

// **正準形の頭は `lib.mjs` の `REF_HEAD` が正本である**——ここで再定義すると、`HEADING_REF` を
// 直したときに折れの検知だけが古い形のまま残る（`G-heading-refs` のヘッダが同じ理由で禁じている）。
// 違うのは末尾だけで、それが形 A と形 B の差そのものである。
// CRLF は特別扱いしない: `refScanLines` が `split("\n")` で残す `\r` は、形 A では末尾の `\s*` が食い、
// 形 B ではラベルの文字クラス `[^「」\n]` が含む。

/** 形 A: 行末が `` `<対象>` ``（+ 任意の `§ <番号>`）で終わる */
const TAIL_TARGET = new RegExp(`${REF_HEAD}$`);

/** 形 B: 対象綴りの直後に `「` が開き、その行に `」` が無い */
const OPEN_UNCLOSED = new RegExp(`${REF_HEAD}「[^「」\\n]*$`);

/** 継続行の先頭から、散文でない記号（コメント標識・blockquote・箇条書き）を落とす。
 *  **判定の中核であって装飾ではない**——恒等写像へ変異させると形 A が 17 件全滅した
 *  （2026-08-20 の複製での実測。形 B は次行を見ないので影響を受けない）。
 *  **記号のあいだの空白も食う**（`> - 「…` のように 2 記号が空白で隔たる形が実在しうる。
 *  今日 0 件だが、見逃しは沈黙側の誤りなので塞ぐ側へ倒した）。 */
const stripLead = (line) => line.replace(/^\s*(?:(?:\/\/[!/]?|#|\*|>|-)\s*)*/, "");

/** findings に加えて照合件数を返す（「差分ゼロ」と「照合していない」を区別する証跡・#497） */
export function scanFoldedHeadingRefs(snapshot, docs) {
  const findings = [];
  let checked = 0;
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) {
      findings.push(finding(doc, 1, "対象文書が読めない（G-folded-heading-refs 母集団の欠落）"));
      continue;
    }
    const lines = [...refScanLines(text, doc, findings)];
    const byNo = new Map(lines.map(([n, t]) => [n, t]));
    for (const [lineNo, line] of lines) {
      const open = OPEN_UNCLOSED.exec(line);
      if (open && isRefTargetSpelling(open[1])) {
        checked += 1;
        findings.push(fold(doc, lineNo, open[1], "ラベルが次行へ流れている"));
        // 同じ行を形 A でも数えない。**「`「` が在れば行末は対象綴りで終わらない」からではない**
        // ——1 行に 2 つの参照が在り、後ろが開いたまま前が行末で終わる形は構築できる（実測）。
        // 二重計上を避ける選択であり、そのとき形 A 側は見送る（1 行 1 件で報告すれば直せる）。
        continue;
      }
      const tail = TAIL_TARGET.exec(line);
      if (!tail || !isRefTargetSpelling(tail[1])) continue;
      checked += 1;
      const next = byNo.get(lineNo + 1);
      if (next !== undefined && stripLead(next).startsWith("「")) {
        findings.push(fold(doc, lineNo, tail[1], "ラベルが次行から始まっている"));
      }
    }
  }
  return { findings, checked };
}

const fold = (doc, lineNo, target, how) =>
  finding(
    doc,
    lineNo,
    `正準形の参照が物理改行で折れている（${how}）: \`${target}\` — G-heading-refs の母集団から落ちる。1 物理行に収めること`,
  );

export function checkFoldedHeadingRefs(snapshot, docs) {
  return scanFoldedHeadingRefs(snapshot, docs).findings;
}
