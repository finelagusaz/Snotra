//! G-near-heading-refs — 正準形に見えて隣接していない見出し参照（#727）。
import { finding, refScanLines, collectAnchors, resolveRefTarget, normAnchor, isRefTargetSpelling } from "../lib.mjs";

export const id = "G-near-heading-refs";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（allRefDocs・record を使う） */
export function run(snapshot, ctx) {
  return ctx.record("nearRefs", scanNearHeadingRefs(snapshot, ctx.allRefDocs));
}

// ---------------------------------------------------------------------------
// G-near-heading-refs — 正準形に見えて隣接していない見出し参照（#727）。
//
// G-heading-refs が見るのはバッククォートを閉じた直後（`§` と空白のみを挟む）に `「` が続く形だけで、
// **助詞が 1 つ挟まると検査対象から外れる**。人の目には同じ参照に見える:
//   `<対象>`「<見出し>」      ← G-heading-refs が見る
//   `<対象>` は「<見出し>」   ← 見ない
// **例示に実在の対象を置かない**——スクリプトのコメントもこの検査群の母集団だからである。
// 実在させれば下の行が自分の指摘対象になり、実在しない名前を書けば G-heading-refs が
// 着地しないと言う。プレースホルダは対象の形（`.md` か `/skill`）に当たらないので両方が見送る。
// #725 では Claude 自身が書いた 3 件がこの形で、しかも `/implement` の入口判定の中核推論を
// 支えていた（`/start-issue` が改番されれば黙って壊れる）。
//
// **判定の要は窓幅ではなく「引用が実際に着地するか」である。** 近傍に `「…」` があるだけでは
// 参照と散文の引用（「`SPEC.md`（…）は「何を実現すべきか」を記す」）を分けられない。実測:
//
// | 窓幅 | 着地する（＝正準形へ直せる真の参照） | 着地しない（＝散文の引用） |
// |---|---|---|
// | 2 | 5 | 8 |
// | 4 | 7 | 12 |
// | 8 | **8** | 28 |
// | 12 | 8 | 34 |
//
// 着地条件を課すと、窓を広げても真の参照は 8 件で頭打ちになり、増えるのは無視する側だけである。
// ゆえに**窓幅 8・着地必須**とした。この形なら誤爆の代償は「散文の引用がたまたま見出しと同名」
// に限られ、そのときは正準形へ直すのが正しい（G-heading-refs の保護下へ入る）。
//
// **受容する残余**: 着地しない非隣接参照は見ない。腐った参照（消滅した節を指す散文形）は
// この検査では捕まらない——歴史記述と区別できないためである（`.claude/rules/governance-docs.md`
// 「既に消滅した節の名前を正準形で書かない」が規範として担う）。
// ---------------------------------------------------------------------------

/** 閉じバッククォートから `「` までに挟まってよい最大文字数（実測で頭打ちになる値） */
const NEAR_REF_GAP = 8;
/** 非隣接の近傍参照。gap は最短一致で取る */
const NEAR_REF = new RegExp("`([^`\\n]+)`([^`\\n]{1," + NEAR_REF_GAP + "}?)「([^「」\\n]+)」", "g");
/** G-heading-refs が既に見ている隣接形（`§` + 節番号と空白のみを挟む）。HEADING_REF と同じ前提を持つ */
const ADJACENT_REF = /`[^`\n]+`\s*(?:§\s*[\d.]*\s*)?「/;

export function scanNearHeadingRefs(snapshot, docs) {
  const findings = [];
  let checked = 0;
  const anchorCache = new Map();
  const anchorsOf = (p) => {
    if (!anchorCache.has(p)) {
      const t = snapshot.read(p);
      anchorCache.set(p, t == null ? null : collectAnchors(t).map(normAnchor));
    }
    return anchorCache.get(p);
  };
  for (const doc of docs) {
    const text = snapshot.read(doc);
    if (text == null) continue; // 読めない文書は G-heading-refs が母集団の欠落として報告済み
    for (const [lineNo, line] of refScanLines(text, doc, findings)) {
      for (const m of line.matchAll(NEAR_REF)) {
        const [, target, gap, label] = m;
        if (!isRefTargetSpelling(target)) continue;
        if (ADJACENT_REF.test(m[0])) continue;
        const p = resolveRefTarget(snapshot, doc, target);
        if (p == null) continue;
        const anchors = anchorsOf(p);
        if (anchors == null) continue;
        checked += 1;
        if (anchors.some((a) => a.startsWith(normAnchor(label)))) {
          // 節番号は正準形が許すので、直し方の提示から落とさない
          const section = (gap.match(/§\s*[\d.]+/) ?? [""])[0];
          const canonical = `\`${target}\`${section ? ` ${section}` : ""}「${label}」`;
          findings.push(
            finding(doc, lineNo, `見出し参照が正準形でない（G-heading-refs の視界外）: \`${target}\`【${gap}】「${label}」— ${canonical}と書く`),
          );
        }
      }
    }
  }
  return { findings, checked };
}

export function checkNearHeadingRefs(snapshot, docs) {
  return scanNearHeadingRefs(snapshot, docs).findings;
}
