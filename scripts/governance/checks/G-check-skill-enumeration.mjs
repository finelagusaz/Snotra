//! G-check-skill-enumeration — `/implement`「3a. 委譲の前に」の列挙 ↔ `AGENTS.md`「条件別チェック」表（#778）。
import { finding, sectionOf } from "../lib.mjs";
import { skillFiles } from "./G-skill-table.mjs";

export const id = "G-check-skill-enumeration";
export const domains = ["skillDocs"];

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return checkCheckSkillEnumeration(snapshot);
}

// ---------------------------------------------------------------------------
// G-check-skill-enumeration — `/implement`「3a. 委譲の前に」の列挙 ↔ `AGENTS.md`「条件別チェック」表（#778）。
//
// `/implement`「出力」のレビュー表は報告の母集団を 3a の列挙で閉じており、それは表の**写し**である。
// 乖離すると、表に増えた check スキルが報告母集団から**沈黙して落ちる**。
//
// **問題は義務が行為者の視界の外にあることだった。** 同期義務は 3a の括弧書きに書いてあるが、
// それを実行するのは `AGENTS.md` の表を編集する人であり、その人が `/implement`「出力」を読む
// 必然性は無い。#765 が塞いだ「実施の有無が報告から消える」より一段手前で、報告を読んでも気づけない。
//
// **述語は着手前に現行コーパスへ当てて実測した**（#778 が明示的に要求している。表は rules 参照・
// grep 指示を含む混成表で、`/plan-review` のような非 check スキルも現れるため）。
// **母集団を件数で書かない**——名前も数も足し引きのたびに腐るので、母集団は下の `CHECK_SKILL_REF` が持つ。
// `/plan-review` は `-check` で終わらないため**構造的に外れる**。`/health-check` は
// 表に現れない（ルート `CLAUDE.md` のスキル表に在り、そちらは G-skill-table が見る）。
//
// これで #778 の (a)（表側へ同期義務を 1 行置く）が不要になった——`AGENTS.md` は G-area-instrument の常時ロード面で
// 余裕が小さいため、機構で吸収できるならそちらが安い。
// ---------------------------------------------------------------------------

const CHECK_SKILL_REF = /\/[a-z][a-z0-9-]*-check\b/g;
const IMPL_SKILL = ".claude/skills/implement/SKILL.md";

export function checkCheckSkillEnumeration(snapshot) {
  const findings = [];
  const agents = snapshot.read("AGENTS.md");
  const impl = snapshot.read(IMPL_SKILL);
  if (agents == null || impl == null) {
    return [finding("AGENTS.md", 1, "G-check-skill-enumeration の母集団が読めない（AGENTS.md か /implement の SKILL.md）")];
  }
  // どちらの節も文書の途中にある（後方に同レベル以上の見出しが在る）——`ending` の宣言と
  // 実際の文書構造の食い違いは `sectionOf` が双方向に赤くする
  const table = sectionOf(agents, /^##\s+条件別チェック/, { file: "AGENTS.md", ending: "heading", by: id });
  const step3a = sectionOf(impl, /^###\s+3a\./, { file: IMPL_SKILL, ending: "heading", by: id });
  findings.push(...table.findings, ...step3a.findings);
  if (findings.length > 0) return findings;

  const setOf = (t) => new Set((t.match(CHECK_SKILL_REF) ?? []).map((s) => s.trim()));
  const inTable = setOf(table.body);
  const in3a = setOf(step3a.body);
  // 空母集団は明示 fail（沈黙経路の閉塞）
  if (inTable.size === 0) findings.push(finding("AGENTS.md", 1, "G-check-skill-enumeration: 表に check スキルが 0 件（母集団の欠落）"));
  if (in3a.size === 0) findings.push(finding(IMPL_SKILL, 1, "G-check-skill-enumeration: 3a に check スキルが 0 件（母集団の欠落）"));

  for (const s of inTable) {
    if (!in3a.has(s)) {
      findings.push(
        finding(IMPL_SKILL, 1, `G-check-skill-enumeration: \`${s}\` が AGENTS.md の表に在るが 3a の列挙に無い（報告母集団から沈黙して落ちる）`),
      );
    }
  }
  for (const s of in3a) {
    if (!inTable.has(s)) {
      findings.push(finding("AGENTS.md", 1, `G-check-skill-enumeration: \`${s}\` が 3a の列挙に在るが AGENTS.md の表に無い（起動条件を持たない検査）`));
    }
  }
  // 列挙されたスキルが実在するか（誤記の検出）。**照合先は `skillDocs` ドメインである**
  // ——`snapshot.files` 全体に問うと、母集団が走査側で消えたときに「誤記が 6 件」という
  // 原因から遠い形で赤くなる。ドメインを見ていれば、同じ走査の欠落を錨が名指しで鳴らす。
  //
  // **ただし「ドメインに無い」を「実在しない」と言ってはならない。** 母集団の述語が狭まった場合、
  // ファイルは在るのにドメインから落ちる——そこで「実在しない」と断言すると、検査が偽の主張を出す
  // （レビューが変異注入で実測）。2 つの状態は区別できるので、区別したまま報告する。
  // **前者の枝は今日のフィクスチャからは到達できない**——`skillFiles` の述語が
  // `.claude/skills/<name>/SKILL.md` そのものであり、かつ `CHECK_SKILL_REF` は `/` を含む名前を
  // 生まないので、組み上がる `p` は必ずその述語に当たる。ファイルが在れば必ず母集団にも居る。
  // 到達するのは述語が狭まったときだけであり、この枝はその日のためにある（宣言する死角）。
  const skills = new Set(skillFiles(snapshot));
  const files = new Set(snapshot.files);
  for (const s of new Set([...inTable, ...in3a])) {
    const p = `.claude/skills/${s.slice(1)}/SKILL.md`;
    if (skills.has(p)) continue;
    findings.push(
      files.has(p)
        ? finding(p, 1, `G-check-skill-enumeration: \`${s}\` の ${p} は在るが skillDocs の母集団に無い（走査か述語が狭まっている）`)
        : finding("AGENTS.md", 1, `G-check-skill-enumeration: \`${s}\` に対応する ${p} が実在しない`),
    );
  }
  return findings;
}
