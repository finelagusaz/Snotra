//! G-skill-table — ルート CLAUDE.md「利用できるスキル」表 ↔ roster に載らない skill の双方向照合。
import { finding, sectionOf } from "../lib.mjs";

export const id = "G-skill-table";
export const domains = "unmigrated";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return checkSkillTable(snapshot);
}

const SKILL_FILE_RE = /^\.claude\/skills\/[^/]+\/SKILL\.md$/;

/** `.claude/skills/<name>/SKILL.md` の一覧（G-skill-table・G-area-instrument の共通母集団） */
export function skillFiles(snapshot) {
  return snapshot.files.filter((f) => SKILL_FILE_RE.test(f));
}

/**
 * `disable-model-invocation: true` の skill 名の集合 = **harness の roster に注入されない skill**。
 * G-skill-table（表が索引すべき対象）と G-area-instrument（常時ロード面に載る description）の両方がこの集合で決まるため、
 * 導出は 1 箇所に閉じる。判定は frontmatter ブロックの中だけを見る——本文が同じキー名に言及する
 * 実例がある（`/retrospective` が `/health-check` の起動方法を説明している）。
 *
 * **判定不能はすべて「隠しでない」へ倒す。** 同じ状態へ入る経路は 4 本ある——値が `true` でない /
 * キーが無い / frontmatter が壊れている・無い / ファイルが読めない。この向きなら、判定できなかった
 * skill は「roster が覆う側」に残り、表に行が無くても緑になる（静かな方の失敗）。逆へ倒すと、
 * 存在しない表の行を要求して赤になる。
 */
export function modelHiddenSkills(snapshot) {
  const hidden = new Set();
  for (const f of skillFiles(snapshot)) {
    const text = snapshot.read(f);
    if (text == null) continue;
    const fm = text.match(/^---\r?\n([\s\S]*?)\r?\n---/);
    if (!fm) continue;
    if (/^disable-model-invocation:[ \t]*true[ \t]*$/m.test(fm[1])) hidden.add(f.split("/")[2]);
  }
  return hidden;
}

// ---------------------------------------------------------------------------
// G-skill-table — ルート CLAUDE.md「利用できるスキル」表 ↔ roster に載らない skill（旧 Check 9）
// harness は毎セッション skill roster を description 付きで注入するため、注入される skill を
// 表へ書き写すことは同じ面での二重課税である（ADR-area-metric-characters が description を常時ロード面に算入した
// のと同じ理由）。ゆえに表が索引すべき対象は `disable-model-invocation: true` の skill だけであり、
// G-skill-table はその集合と表の**双方向**一致を見る。「表の射程」を規範ではなくこの判定で固定する。
// ---------------------------------------------------------------------------
export function checkSkillTable(snapshot) {
  const findings = [];
  const text = snapshot.read("CLAUDE.md");
  if (text == null) return [finding("CLAUDE.md", 1, "ルート CLAUDE.md が読めない")];
  // この節はルート `CLAUDE.md` の**最終節**であり、終端の見出しを持たない＝`ending: "eof"`。
  // 旧実装の `?? ""` は「節が無い」も「節が空」も空文字へ潰していた（表が丸ごと消えても、
  // 隠しスキルが 0 件なら緑になりうる形）。`sectionOf` は①（アンカー消滅）と
  // ④（節の後ろに `##` が現れて宣言が腐った）の両方を赤にする
  const sec = sectionOf(text, /^## 利用できるスキル$/, { file: "CLAUDE.md", ending: "eof", by: id });
  if (sec.body == null) return sec.findings;
  const inTable = new Set([...sec.body.matchAll(/^\|\s*`\/([a-z0-9-]+)`/gm)].map((m) => m[1]));
  const inDirs = new Set(skillFiles(snapshot).map((f) => f.split("/")[2]));
  const hidden = modelHiddenSkills(snapshot);
  if (inDirs.size === 0) findings.push(finding(".claude/skills", 1, "SKILL.md が 0 件（G-skill-table 母集団の欠落）"));
  for (const s of inTable) {
    if (!inDirs.has(s)) findings.push(finding("CLAUDE.md", 1, `スキル表の /${s} に SKILL.md が無い（.claude/skills/${s}/）`));
    else if (!hidden.has(s)) {
      findings.push(
        finding("CLAUDE.md", 1, `スキル表の /${s} は harness の roster に載る（表の対象は disable-model-invocation: true の skill だけ）`),
      );
    }
  }
  for (const s of hidden) {
    if (!inTable.has(s)) findings.push(finding("CLAUDE.md", 1, `.claude/skills/${s}/ は roster に載らないのにスキル表に無い`));
  }
  return findings;
}
