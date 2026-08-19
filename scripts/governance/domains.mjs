//! ドメイン — 名前つきの母集団と、その**錨**（ここには必ずこれが居る、という構造的事実）。
//!
//! **錨は構造を名指す。単一ファイルを名指さない。件数も錨にしない。**
//! 単一ファイルを錨にすると、そのファイルの移設だけで赤くなり、メッセージが原因から目を逸らさせる
//! （#1143 で実測）。件数を錨にすると、文書が 1 枚増えるたびに赤くなり、無視されるゲートに化ける
//! （`ADR-retire-area-budget` が面積 ratchet について通った道と同じ）。
//! **例外**: `governanceDocs` の第 1 錨はルート `AGENTS.md` / `CLAUDE.md` の 2 ファイルを名指す
//! ——これは構造的な定点（`instrument.mjs` の `ALWAYS_LOADED_FILES` が同じ固定名を持つ常時ロード面）
//! を指しているため、移設ではなく「その定点が動いた」ことそのものを言い当てる。
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
  commentFamilyOf,
  ruleDocs,
  crateSourceFiles,
} from "./lib.mjs";
import { adrFiles } from "./checks/G-adr-file-names.mjs";
import { judgingScripts } from "./checks/G-rules-script-coverage.mjs";
import { skillFiles } from "./checks/G-skill-table.mjs";
import { moduleIndexSources, MODULE_INDEX_CRATES } from "./checks/G-module-index.mjs";

/** そのディレクトリ**直下**に 1 件以上（前方一致にしない——配下が在れば真になり、
 *  中間層が消えても沈黙する。#1143 で実測した形）。 */
const hasDirectChild = (members, dir) => members.some((f) => f.slice(0, f.lastIndexOf("/")) === dir);

/** `CLAUDE.md` を持つ workspace member（#701 のカナリアと同じ導出。正本は `Cargo.toml`）。 */
const cratesWithClaudeMd = (snapshot) =>
  workspaceMembers(snapshot).members.filter((c) => snapshot.read(`${c}/CLAUDE.md`) !== null);

/** `.claude/skills/` 直下のディレクトリ名。**メンバーの導出（`SKILL.md` の glob）とは独立に、
 *  走査結果のディレクトリ構造から取る**——同じ述語から導くと錨が母集団の写しになり、
 *  述語を狭める変異に対して両辺が同時に動いて沈黙する。 */
const skillDirs = (snapshot) =>
  new Set(snapshot.files.filter((f) => f.startsWith(".claude/skills/")).map((f) => f.split("/")[2]));

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
      // 5 腕のうち残り 3 本——足さないと docs/・.claude/rules/・.claude/skills/ が丸ごと消えても
      // 上の 2 錨（固定名・crate CLAUDE.md）は無傷のまま緑になる（#1143 と同じ形。レビューで実測）。
      { label: "docs/ の腕", holds: (m) => m.some((f) => f.startsWith("docs/")) },
      { label: ".claude/rules/ の腕", holds: (m) => m.some((f) => f.startsWith(".claude/rules/")) },
      { label: ".claude/skills/ の腕", holds: (m) => m.some((f) => f.startsWith(".claude/skills/")) },
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
      // 上の 3 本は .mjs だけを見ている——ps 族（.ps1/.psm1/.psd1）の腕が丸ごと消えても
      // どれも鳴らない（I2 と同じクラス。レビューで実測）。族の述語は `commentFamilyOf` を使う
      // ——拡張子を列挙し直すと `headingRefCommentDocs` 自身の母集団の述語からずれる。
      { label: "ps 族（.ps1/.psm1/.psd1）の腕", holds: (m) => m.some((f) => commentFamilyOf(f) === "ps") },
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
  {
    name: "adrFiles",
    members: adrFiles,
    anchors: [{ label: "docs/adr/ 直下", holds: (m) => hasDirectChild(m, "docs/adr") }],
  },
  {
    name: "ruleDocs",
    members: ruleDocs,
    // 単一ディレクトリの母集団なので、構造が差し出す錨は「直下に居る」1 本だけである。
    // それでも `|P| > 0` より強い——前方一致ではないので、rules が下位ディレクトリへ移された
    // （＝harness の配送が届かなくなる）形で倒れる。この母集団の**中身**の下界は
    // `G-rules-script-coverage` の `COVERAGE` が名指しで持つ（そちらが正本）。
    anchors: [{ label: ".claude/rules/ 直下", holds: (m) => hasDirectChild(m, ".claude/rules") }],
  },
  {
    name: "workspaceMemberDirs",
    members: (s) => workspaceMembers(s).members,
    // 錨は両向きに置く。宣言 → 実在（メンバーに Cargo.toml が在る）だけでは、メンバーが
    // 宣言から**落ちた**形が沈黙する——残ったメンバーについては every が成立し続けるためである。
    anchors: [
      {
        label: "全メンバーのディレクトリに Cargo.toml が実在する",
        holds: (m, s) => m.length > 0 && m.every((d) => s.read(`${d}/Cargo.toml`) !== null),
      },
      {
        label: "Cargo.toml を持つ直下ディレクトリがすべてメンバーに居る",
        holds: (m, s) => {
          const dirs = s.files.filter((f) => /^[^/]+\/Cargo\.toml$/.test(f)).map((f) => f.split("/")[0]);
          return dirs.length > 0 && dirs.every((d) => m.includes(d));
        },
      },
    ],
  },
  {
    name: "crateSources",
    members: crateSourceFiles,
    // crate ごとが腕である。`<crate>/src/` **直下**で見る——前方一致だと、直下が消えても配下の
    // モジュールディレクトリが同じ接頭辞に当たって沈黙する（#1143 の実形）。
    anchors: [
      {
        label: "全 workspace member の src/ 直下に .rs が居る",
        holds: (m, s) => {
          const crates = workspaceMembers(s).members;
          return crates.length > 0 && crates.every((c) => m.some((f) => f.slice(0, f.lastIndexOf("/")) === `${c}/src`));
        },
      },
    ],
  },
  {
    // **`crateSources` と同じ集合を返すが、畳んではならない。** crate の一覧の出所が違う
    // （こちらは `MODULE_INDEX_CRATES`、あちらはルート `Cargo.toml`）。この 2 本目の導出は意図で、
    // 食い違いは `governance-check.test.mjs` の母集団カナリア（#701）が捕まえる。
    // `instrument.mjs` の `duplicateDomains` が「同一メンバー」として報告するのは想定どおりで、
    // **合否は持たない**——判断は人に残す（`ADR-retire-area-budget` と同じ向き）。
    name: "moduleIndexSources",
    members: moduleIndexSources,
    anchors: [
      {
        label: "MODULE_INDEX_CRATES の全 crate の src/ 直下に索引対象が居る",
        holds: (m) => {
          const crates = Object.values(MODULE_INDEX_CRATES);
          return crates.length > 0 && crates.every((cfg) => m.some((f) => f.slice(0, f.lastIndexOf("/") + 1) === cfg.src));
        },
      },
    ],
  },
  {
    name: "skillDocs",
    members: skillFiles,
    // 錨は他の SSOT（走査結果のディレクトリ構造）から導く——`governanceDocs` の
    // 「`CLAUDE.md` を持つ workspace member のすべて」と同じ形である。
    // **受け入れるトレードオフ**: `.claude/skills/` 直下へ skill でないディレクトリを置くと、
    // 正当な変更でも赤くなる。そのときは錨の側を直す（起きたら loud で、沈黙はしない向き）。
    anchors: [
      {
        label: ".claude/skills/ 直下の全ディレクトリが SKILL.md を持つ",
        holds: (m, s) => {
          const dirs = [...skillDirs(s)];
          return dirs.length > 0 && dirs.every((d) => m.includes(`.claude/skills/${d}/SKILL.md`));
        },
      },
    ],
  },
  {
    name: "judgingScripts",
    members: judgingScripts,
    // #1143 の当の母集団。腕（ディレクトリ）ごとに 1 本ずつ置く——束ねた長さは他の腕の消滅を隠し、
    // 前方一致は中間層の消滅を隠す（`scripts/governance/` 直下が消えても配下の `checks/` が
    // 同じ接頭辞に当たって沈黙した・実測）。**`G-rules-script-coverage.test.mjs` の実ツリー canary と
    // 同じ下界を、テストではなく `governance:check` の実行時に見る層である。**
    //
    // **`.githooks/` は錨にしない**——母集団へ 1 件しか出さないため、そのファイルの移設だけで
    // 赤くなる「単一ファイルの錨」に化ける（#1143 の canary が通った道）。ここは宣言する死角である。
    anchors: [
      { label: "scripts/ 直下", holds: (m) => hasDirectChild(m, "scripts") },
      { label: "scripts/governance/ 直下", holds: (m) => hasDirectChild(m, "scripts/governance") },
      { label: "scripts/governance/checks/ 直下", holds: (m) => hasDirectChild(m, "scripts/governance/checks") },
      { label: "scripts/lib/ 直下", holds: (m) => hasDirectChild(m, "scripts/lib") },
      { label: ".claude/hooks/ 直下", holds: (m) => hasDirectChild(m, ".claude/hooks") },
      // 拡張子の腕。`SCRIPT_EXT` を `.mjs` へ狭める変異は、実ツリーが全件被覆である限り
      // 検査の判定を変えないので、母集団の側で捕まえるほかない。
      { label: "ps 族（.ps1/.psm1）の腕", holds: (m) => m.some((f) => /\.(ps1|psm1)$/i.test(f)) },
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
