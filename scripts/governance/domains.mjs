//! ドメイン — 名前つきの母集団と、その**錨**（ここには必ずこれが居る、という構造的事実）。
//!
//! **錨は構造を名指す。単一ファイルを名指さない。件数も錨にしない。**
//! 単一ファイルを錨にすると、そのファイルの移設だけで赤くなり、メッセージが原因から目を逸らさせる
//! （#1143 で実測）。件数を錨にすると、文書が 1 枚増えるたびに赤くなり、無視されるゲートに化ける
//! （`ADR-retire-area-budget` が面積 ratchet について通った道と同じ）。
//!
//! **`|P| > 0` では足りない**——#1143 のとき母集団は空ではなく、facade が 1 件マッチし続けたために
//! 「マッチ 0 件」を見る検査が緑のままだった。錨は「空でないこと」ではなく
//! 「守りたい対象が実際に入っていること」を言う。
import {
  governanceDocs,
  headingRefDocs,
  headingRefSourceDocs,
  headingRefCommentDocs,
  allHeadingRefDocs,
  staleIdentifierDocs,
  staleIdentifierGuideDocs,
  staleIdentifierTargets,
  workspaceMembers,
} from "./lib.mjs";

/** そのディレクトリ**直下**に 1 件以上（前方一致にしない——配下が在れば真になり、
 *  中間層が消えても沈黙する。#1143 で実測した形）。 */
const hasDirectChild = (members, dir) => members.some((f) => f.slice(0, f.lastIndexOf("/")) === dir);

/** `CLAUDE.md` を持つ workspace member（#701 のカナリアと同じ導出。正本は `Cargo.toml`）。 */
const cratesWithClaudeMd = (snapshot) =>
  workspaceMembers(snapshot).members.filter((c) => snapshot.read(`${c}/CLAUDE.md`) !== null);

export const DOMAIN_SPECS = [
  {
    name: "governanceDocs",
    members: governanceDocs,
    anchors: [
      { label: "ルートの AGENTS.md と CLAUDE.md", holds: (m) => m.includes("AGENTS.md") && m.includes("CLAUDE.md") },
      {
        label: "CLAUDE.md を持つ workspace member のすべて",
        holds: (m, s) => {
          const crates = cratesWithClaudeMd(s);
          return crates.length > 0 && crates.every((c) => m.includes(`${c}/CLAUDE.md`));
        },
      },
    ],
  },
  {
    name: "headingRefDocs",
    members: headingRefDocs,
    anchors: [{ label: "docs/ 配下の md", holds: (m) => m.some((f) => f.startsWith("docs/")) }],
  },
  {
    name: "headingRefSourceDocs",
    members: headingRefSourceDocs,
    anchors: [
      {
        label: "CLAUDE.md を持つ crate の src 配下の .rs",
        holds: (m, s) => cratesWithClaudeMd(s).some((c) => m.some((f) => f.startsWith(`${c}/src/`))),
      },
    ],
  },
  {
    name: "headingRefCommentDocs",
    members: headingRefCommentDocs,
    anchors: [
      // #1143 の教訓——判定を持つスクリプトがこの母集団から落ちると、コメントの参照が沈黙で腐る
      { label: "scripts/governance/checks/ 直下", holds: (m) => hasDirectChild(m, "scripts/governance/checks") },
      { label: "scripts/governance/ 直下", holds: (m) => hasDirectChild(m, "scripts/governance") },
      { label: ".claude/hooks/ 直下", holds: (m) => hasDirectChild(m, ".claude/hooks") },
    ],
  },
  {
    name: "allHeadingRefDocs",
    members: allHeadingRefDocs,
    anchors: [
      // 和は 3 腕から成る。腕ごとに 1 つ錨を置く——束ねた長さは他の腕の消滅を隠す
      { label: "md の腕", holds: (m) => m.some((f) => f.endsWith(".md")) },
      { label: ".rs の腕", holds: (m) => m.some((f) => f.endsWith(".rs")) },
      { label: "スクリプトの腕", holds: (m) => m.some((f) => f.endsWith(".mjs") || f.endsWith(".ps1") || f.endsWith(".psm1")) },
    ],
  },
  {
    name: "staleIdentifierDocs",
    members: staleIdentifierDocs,
    anchors: [{ label: ".claude/ 配下の md", holds: (m) => m.some((f) => f.startsWith(".claude/")) }],
  },
  {
    name: "staleIdentifierGuideDocs",
    members: staleIdentifierGuideDocs,
    anchors: [{ label: "docs/ 配下の開発ガイド", holds: (m) => m.some((f) => f.startsWith("docs/")) }],
  },
  {
    name: "staleIdentifierTargets",
    members: staleIdentifierTargets,
    // `m.length > 0` は錨にならない——`STALE_EXTRA_DOCS`（lib.mjs）が実在を問わず 4 件を無条件に足すため、
    // 実導出では長さが決して 0 にならない（空虚な錨だった。レビューで指摘）。
    // 代わりに、可変な 2 腕（`staleIdentifierDocs` / `staleIdentifierGuideDocs`）をそれぞれ名指す。
    anchors: [
      { label: ".claude/ の腕（staleIdentifierDocs）", holds: (m) => m.some((f) => f.startsWith(".claude/")) },
      { label: "docs/ の腕（staleIdentifierGuideDocs）", holds: (m) => m.some((f) => f.startsWith("docs/")) },
    ],
  },
];

/** 名前 → { name, members, anchors } の Map。`members` はここで 1 度だけ導出する。 */
export function buildDomains(snapshot) {
  const out = new Map();
  for (const spec of DOMAIN_SPECS) {
    out.set(spec.name, { name: spec.name, members: spec.members(snapshot), anchors: spec.anchors });
  }
  return out;
}
