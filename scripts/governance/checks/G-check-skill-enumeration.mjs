//! G-check-skill-enumeration — `/implement`「4a. check スキルの実行」の列挙 ↔ `AGENTS.md`「条件別チェック」表（#778）。
import { finding, sectionOf } from "../lib.mjs";

export const id = "G-check-skill-enumeration";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return checkCheckSkillEnumeration(snapshot);
}

// ---------------------------------------------------------------------------
// G-check-skill-enumeration — `/implement`「4a. check スキルの実行」の列挙 ↔ `AGENTS.md`「条件別チェック」表（#778）。
//
// `/implement`「出力」のレビュー表は報告の母集団を 4a の列挙で閉じており、それは表の**写し**である。
// 乖離すると、表に増えた check スキルが報告母集団から**沈黙して落ちる**。
//
// **問題は義務が行為者の視界の外にあることだった。** 同期義務は 4a の括弧書きに書いてあるが、
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
  const step4a = sectionOf(impl, /^###\s+4a\./, { file: IMPL_SKILL, ending: "heading", by: id });
  findings.push(...table.findings, ...step4a.findings);
  if (findings.length > 0) return findings;

  const setOf = (t) => new Set((t.match(CHECK_SKILL_REF) ?? []).map((s) => s.trim()));
  const inTable = setOf(table.body);
  const in4a = setOf(step4a.body);
  // 空母集団は明示 fail（沈黙経路の閉塞）
  if (inTable.size === 0) findings.push(finding("AGENTS.md", 1, "G-check-skill-enumeration: 表に check スキルが 0 件（母集団の欠落）"));
  if (in4a.size === 0) findings.push(finding(IMPL_SKILL, 1, "G-check-skill-enumeration: 4a に check スキルが 0 件（母集団の欠落）"));

  for (const s of inTable) {
    if (!in4a.has(s)) {
      findings.push(
        finding(IMPL_SKILL, 1, `G-check-skill-enumeration: \`${s}\` が AGENTS.md の表に在るが 4a の列挙に無い（報告母集団から沈黙して落ちる）`),
      );
    }
  }
  for (const s of in4a) {
    if (!inTable.has(s)) {
      findings.push(finding("AGENTS.md", 1, `G-check-skill-enumeration: \`${s}\` が 4a の列挙に在るが AGENTS.md の表に無い（起動条件を持たない検査）`));
    }
  }
  // 列挙されたスキルが実在するか（誤記の検出）
  for (const s of new Set([...inTable, ...in4a])) {
    const p = `.claude/skills/${s.slice(1)}/SKILL.md`;
    if (!snapshot.files.includes(p)) findings.push(finding("AGENTS.md", 1, `G-check-skill-enumeration: \`${s}\` に対応する ${p} が実在しない`));
  }
  return findings;
}
