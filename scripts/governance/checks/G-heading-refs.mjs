//! G-heading-refs — 見出し参照の実在（正準形 `<対象>`「<見出し>」）。
import { finding, linesOutsideFences, collectAnchors, resolveRefTarget, normAnchor } from "../lib.mjs";

export const id = "G-heading-refs";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（allRefDocs・record を使う） */
export function run(snapshot, ctx) {
  return ctx.record("headingRefs", scanHeadingRefs(snapshot, ctx.allRefDocs));
}

// ---------------------------------------------------------------------------
// G-heading-refs — 見出し参照の実在（正準形 `<対象>`「<見出し>」）。
// 参照に構文を与えて機械照合可能にする。これが `.claude/rules/governance-docs.md` の
// 「改変前に参照側を名前と序数で数え上げる」手作業を置き換える機構である。
// アンカーは ATX 見出し・番号付きリスト項目・太字リードの 3 種（この repo の参照実態に合わせた。
// 節ではなく箇条書きのリード文を指す参照が実在する: `src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」）。
// 照合は正規化（`**`・バッククォート・「」・空白の除去）後の**前方一致**——見出しが後置の
// 括弧注記（「…の不変条件（#532 SU5）」「条件別チェック（トリガー → 参照先）」）を持つため。
// 受容する偽陰性: 「ルート `CLAUDE.md` のフック節」のような散文形の参照は見ない。
// 検査されるのは正準形に書かれたものだけであり、**この検査は規範の完全な代替ではない**。
// ---------------------------------------------------------------------------

/** 見出し参照の正準形。対象は `<path>.md` か `/skill-name`。
 *  `§` には節番号を伴ってよい（`SPEC.md` §11「見た目の規範」）——番号を許さないと、
 *  節番号つきの参照は正準形へ直しても照合されず、G-near-heading-refs が「直せない指摘」を出し続ける（#727 で実測）。 */
const HEADING_REF = /`([^`\n]+)`\s*(?:§\s*[\d.]*\s*)?「([^「」\n]+)」/g;

/** findings に加えて照合件数を返す（「差分ゼロ」と「照合していない」を区別する証跡・#497） */
export function scanHeadingRefs(snapshot, docs) {
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
    if (text == null) {
      findings.push(finding(doc, 1, "対象文書が読めない（G-heading-refs 母集団の欠落）"));
      continue;
    }
    for (const [lineNo, line] of linesOutsideFences(text, doc, findings)) {
      for (const m of line.matchAll(HEADING_REF)) {
        const [, target, label] = m;
        if (!target.endsWith(".md") && !/^\/[a-z0-9-]+$/.test(target)) continue;
        checked += 1;
        const p = resolveRefTarget(snapshot, doc, target);
        if (p == null) {
          findings.push(finding(doc, lineNo, `見出し参照の対象が解決できない: \`${target}\`「${label}」`));
          continue;
        }
        const anchors = anchorsOf(p);
        if (anchors == null) {
          findings.push(finding(doc, lineNo, `見出し参照の対象が読めない: ${p}`));
          continue;
        }
        if (!anchors.some((a) => a.startsWith(normAnchor(label)))) {
          findings.push(
            finding(doc, lineNo, `見出し参照が着地しない: \`${target}\`「${label}」（${p} に該当する見出し・リード文が無い）`),
          );
        }
      }
    }
  }
  return { findings, checked };
}

export function checkHeadingRefs(snapshot, docs) {
  return scanHeadingRefs(snapshot, docs).findings;
}
