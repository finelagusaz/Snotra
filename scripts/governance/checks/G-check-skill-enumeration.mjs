//! G-check-skill-enumeration — `/implement`「4a. check スキルの実行」の列挙 ↔ `AGENTS.md`「条件別チェック」表（#778）。
import { finding } from "../lib.mjs";

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
// grep 指示を含む混成表で、`/plan-review` のような非 check スキルも現れるため）:
// - 表の `/…-check` = {cache, dry, persistence, race, state, symmetric}（6 件）
// - 4a の `/…-check` = 同じ 6 件
// `/plan-review` は `-check` で終わらないため**構造的に外れる**。`/health-check` は
// 表に現れない（ルート `CLAUDE.md` のスキル表に在り、そちらは G-skill-table が見る）。
//
// これで #778 の (a)（表側へ同期義務を 1 行置く）が不要になった——`AGENTS.md` は G-area-instrument の常時ロード面で
// 余裕が小さいため、機構で吸収できるならそちらが安い。
// ---------------------------------------------------------------------------

const CHECK_SKILL_REF = /\/[a-z][a-z0-9-]*-check\b/g;
/** 節を見出しで切り出す（次の同レベル以上の見出しまで）。見つからなければ null */
function sectionOf(text, headingRe) {
  const lines = text.split("\n");
  const start = lines.findIndex((l) => headingRe.test(l));
  if (start < 0) return null;
  const level = (lines[start].match(/^#+/) ?? ["#"])[0].length;
  const rest = lines.slice(start + 1);
  const end = rest.findIndex((l) => /^#+\s/.test(l) && (l.match(/^#+/) ?? [""])[0].length <= level);
  return rest.slice(0, end < 0 ? rest.length : end).join("\n");
}

export function checkCheckSkillEnumeration(snapshot) {
  const findings = [];
  const agents = snapshot.read("AGENTS.md");
  const impl = snapshot.read(".claude/skills/implement/SKILL.md");
  if (agents == null || impl == null) {
    return [finding("AGENTS.md", 1, "G-check-skill-enumeration の母集団が読めない（AGENTS.md か /implement の SKILL.md）")];
  }
  const table = sectionOf(agents, /^##\s+条件別チェック/);
  const step4a = sectionOf(impl, /^###\s+4a\./);
  if (table == null) findings.push(finding("AGENTS.md", 1, "G-check-skill-enumeration: 「条件別チェック」節が見つからない（見出しが変わった）"));
  if (step4a == null) findings.push(finding(".claude/skills/implement/SKILL.md", 1, "G-check-skill-enumeration: 「4a.」節が見つからない（見出しが変わった）"));
  if (findings.length > 0) return findings;

  const setOf = (t) => new Set((t.match(CHECK_SKILL_REF) ?? []).map((s) => s.trim()));
  const inTable = setOf(table);
  const in4a = setOf(step4a);
  // 空母集団は明示 fail（沈黙経路の閉塞）
  if (inTable.size === 0) findings.push(finding("AGENTS.md", 1, "G-check-skill-enumeration: 表に check スキルが 0 件（母集団の欠落）"));
  if (in4a.size === 0) findings.push(finding(".claude/skills/implement/SKILL.md", 1, "G-check-skill-enumeration: 4a に check スキルが 0 件（母集団の欠落）"));

  for (const s of inTable) {
    if (!in4a.has(s)) {
      findings.push(
        finding(".claude/skills/implement/SKILL.md", 1, `G-check-skill-enumeration: \`${s}\` が AGENTS.md の表に在るが 4a の列挙に無い（報告母集団から沈黙して落ちる）`),
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
