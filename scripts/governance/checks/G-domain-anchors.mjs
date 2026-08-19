//! G-domain-anchors — ドメインのメンバーに錨が居ることの照合。
//! **これは「検査を検査する層」である**——各検査が見る母集団そのものが縮む向きを、ここ 1 か所で赤くする。
//! 言えるのは「錨が居る」までであり、母集団が正しい・十分であることは言わない（受容する残余）。
import { finding } from "../lib.mjs";

export const id = "G-domain-anchors";
export const domains = ["*"]; // このメタ検査は全ドメインを見る（Task 3 の宣言要求を満たす）

const SELF = "scripts/governance/domains.mjs";

/** @param {object} snapshot  @param {object} ctx `ctx.domains` を使う */
export function run(snapshot, ctx) {
  return checkDomainAnchors(ctx.domains, snapshot);
}

export function checkDomainAnchors(domains, snapshot) {
  if (!domains || domains.size === 0) return [finding(SELF, 1, "ドメインが 0 件（G-domain-anchors 母集団の欠落）")];
  const findings = [];
  for (const d of domains.values()) {
    if (d.members.length === 0) {
      findings.push(finding(SELF, 1, `ドメイン ${d.name} のメンバーが 0 件（走査の欠落）`));
    }
    // 錨が消えたドメインは、無言で保護の外へ落ちる——`DOMAIN_SPECS` の編集で `anchors: []` になっても
    // 下の holds ループは何もせず findings 0 件のまま緑になる。ここで独立に赤くする。
    if (d.anchors.length === 0) {
      findings.push(finding(SELF, 1, `ドメイン ${d.name} の錨が 0 件（保護の欠落——このドメインは何も守られていない）`));
    }
    // どちらかが 0 件なら、上の 2 件で原因を言い切っている。holds() をそれでも呼ぶと、
    // 「メンバーが 0 件」と同じ原因が錨の数だけ重複して積まれる（メンバー 0 件のとき、
    // 大半の holds は member を探せず false を返すため）。
    if (d.members.length === 0 || d.anchors.length === 0) continue;
    for (const a of d.anchors) {
      if (!a.holds(d.members, snapshot)) {
        findings.push(finding(SELF, 1, `ドメイン ${d.name} の錨が母集団に居ない: ${a.label}`));
      }
    }
  }
  return findings;
}
