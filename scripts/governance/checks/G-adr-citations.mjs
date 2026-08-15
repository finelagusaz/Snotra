//! G-adr-citations — `ADR-<slug>` の短縮引用が実在の ADR を指すか（#812 の A）。
import { finding, linesOutsideFences } from "../lib.mjs";

export const id = "G-adr-citations";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（docs・record を使う） */
export function run(snapshot, ctx) {
  return ctx.record("adrCitations", scanAdrCitations(snapshot, adrCitationDocs(snapshot, ctx.docs)));
}

// ---------------------------------------------------------------------------
// G-adr-citations — `ADR-<slug>` の短縮引用が実在の ADR を指すか（#812 の A）。
//
// **連番だった頃、この検査は書けなかった。** `ADR-0007` はファイル名の一部でしかなく、
// 引用文字列とファイル名 stem が別物だったためである（`0007-results-presentation-two-stage.md`）。
// `ADR-<slug>.md` へ移して stem = 引用文字列にしたことで、初めて機械照合できるようになった
// ——`docs/adr/ADR-canonical-heading-references.md` が見出し参照に正準形を与えて照合可能にしたのと同じ手。
//
// **母集団はコードコメントを含む。** 製品コードの 5 箇所（`view.rs` ほか）が ADR を短縮名で呼んでおり、
// そこは今日まで検出器を 1 つも持っていなかった。
// **テストファイル（`*.test.mjs`）は母集団外である**——フィクスチャは赤経路を測るために
// 意図的に実在しない名前を持つ（実測: 本検査の初回実行で 5 件すべてが自分のフィクスチャだった）。
// md のコードフェンスを見ないのと同じ理由で、構造的に外す。
// **受容する残余**: `docs/superpowers/` は歴史資料（#589 で非規範化）ゆえ母集団外である。
// 旧番号のパスが残るが、その時点の事実の記録であり、書き換えると当時を偽ることになる。
// ---------------------------------------------------------------------------

/** 短縮引用の形。`ADR-` + kebab slug */
const ADR_CITATION = /\bADR-([a-z][a-z0-9]*(?:-[a-z0-9]+)*)\b/g;

/** G-adr-citations の母集団: ガバナンス文書 + skills + 製品ソース（コメントに引用が在る） */
export function adrCitationDocs(snapshot, docs) {
  return [
    ...docs,
    // 凍結された歴史も**実在の辺だけ**は守る——ADR → ADR の短縮引用は、指す側が凍結でも
    // 指される側の削除で壊れる。`docs`（governanceDocs）は docs/adr/ を含まないため明示的に足す。
    // この 1 行が落ちると ADR→ADR の実在検査が沈黙で消える（母集団カナリアがテストで膜を張る）
    ...snapshot.files.filter((f) => /^docs\/adr\/[^/]+\.md$/.test(f)),
    ...snapshot.files.filter((f) => /^\.claude\/skills\/.*\.md$/.test(f)),
    // 非 docs のソース。**見るのは直下の正規表現が挙げる拡張子だけである**——`.ts` / `.tsx` /
    // `.ps1` / `.psm1` に書いた ADR の短縮引用は実在照合を素通りする（2026-08-09 実測・#1008）。
    ...snapshot.files.filter((f) => /\.(rs|mjs)$/.test(f) && !f.startsWith("docs/") && !f.endsWith(".test.mjs")),
  ];
}

export function scanAdrCitations(snapshot, docs) {
  const findings = [];
  let checked = 0;
  const exists = (slug) => snapshot.files.includes(`docs/adr/ADR-${slug}.md`);
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) continue;
    const isMd = doc.endsWith(".md");
    const lines = isMd ? linesOutsideFences(text) : text.split("\n").map((l, i) => [i + 1, l]);
    for (const [lineNo, line] of lines) {
      for (const m of line.matchAll(ADR_CITATION)) {
        checked += 1;
        if (!exists(m[1])) {
          findings.push(finding(doc, lineNo, `ADR の短縮引用が実在しない: \`${m[0]}\`（docs/adr/ADR-${m[1]}.md が無い）`));
        }
      }
    }
  }
  return { findings, checked };
}

export function checkAdrCitations(snapshot, docs) {
  return scanAdrCitations(snapshot, docs).findings;
}
