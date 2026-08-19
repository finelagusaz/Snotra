//! 合否を持たない計器。**`checks/` の外に在ることが「検査ではない」の担保である**——
//! registry は `checks/` だけを走査するので、ここに在る限り「検査 N 件」には数えられない
//! （`ADR-retire-area-budget`）。母集団が欠ければ evidence が嘘になるため、
//! 入力の健全性だけは findings に残す。

import { skillFiles, modelHiddenSkills } from "./checks/G-skill-table.mjs";
import { finding } from "./lib.mjs";

// ---------------------------------------------------------------------------
// G-area-instrument — 恒久規範の面積の計器（合否を持たない）。ADR-retire-area-budget。
// **上限判定は廃止した。** 一次規範は「書く約束」（`.claude/rules/governance-docs.md`: かぶりなく・
// 必要なことだけ・古い情報を残さない）であり、数字はその代役になれない。ratchet 期の 3 発火は
// すべて正当な追記に対するもので（#894 の実績調査）、続く火災報知器期は 8 日で両面が +30% 育つ間
// 一度も鳴らなかった——通算 4 観測点で欠陥検出はゼロである。残るのは実測値の報告だけで、
// 推移は `governance:check` の成功行と git 履歴が運ぶ。
//
// **合否を持たない道具でも、母集団の欠落だけは判定する。** 読めない入力の上で出した数字は
// 静かに誤り、判定が無いぶん誰も気づかない（`check:colors` がロック画面を撮る形と同型）。
// ゆえにこの検査が残すのは「計器が入力を読めているか」だけである。
//
// 指標は**文字数（コードポイント・CR 除く）**——行数は「改行を消す」で読む量を減らさず数字だけ
// 下がる（ADR-area-metric-characters に実測）。CR を除くのは CRLF checkout 対策（#587/#589）。
// 常時ロード面には skill の description を含める（毎セッション注入される面）。skills 本文・
// モジュール CLAUDE.md・docs・ADR は対象外——「その作業に入った者だけが読む面」への退去は
// #593 が推奨する経路であり、課税すれば登ってほしい階梯を登る側が罰せられる。
// 二面（常時ロード / rules）を分けて報告するのは、面替えによる片面の肥大が合計では見えないため。
// ---------------------------------------------------------------------------

/** 常時ロードされる恒久規範ファイル（ルート直下の 2 文書。ほかに skill description が同じ面に載る）。
 *  **保証は狭い**——常時ロード面にファイルが増えてもここへ足さなければ、その面積は報告に
 *  一度も算入されない（2026-08-09 実測: 5000 字の文書を新設して `CLAUDE.md` から `@` で読み込ませても、
 *  計上が動いたのは `CLAUDE.md` 側の 1 行分だけ・#1008）。足し忘れを知るのはファイルシステムであって
 *  この検査ではない。 */
export const ALWAYS_LOADED_FILES = ["CLAUDE.md", "AGENTS.md"];

/** コードポイント数（CR は除く）。読めなければ null（母集団欠落を上位で検知） */
function countChars(text) {
  return text == null ? null : [...text.replace(/\r/g, "")].length;
}

/** 指定ファイル群の総文字数。読めないファイルは finding に積み、面積へは算入しない */
function sumChars(snapshot, files, gLabel) {
  let total = 0;
  const findings = [];
  for (const f of files) {
    const c = countChars(snapshot.read(f));
    if (c == null) findings.push(finding(f, 1, `${f} が読めない（${gLabel} 母集団の欠落）`));
    else total += c;
  }
  return { total, findings };
}

/**
 * 毎セッション注入される skill の `description` の総文字数。
 * 複数行スカラー（`|` / `>`）と欠落は数えられないので finding に倒す（沈黙経路の閉塞）。
 * **`disable-model-invocation: true` の skill は除く** — ADR-area-metric-characters が description を常時ロード面へ
 * 算入した根拠は「毎セッション注入されるのに ratchet から見えていない」であり、roster に載らない
 * skill はその前提を満たさない（載らないものを数えれば、実際には注入されていない字に課税する）。
 * `count` は母集団の存在確認用なので、除外前の全 skill 数を返す。
 */
export function skillDescriptionArea(snapshot) {
  const all = skillFiles(snapshot);
  const hidden = modelHiddenSkills(snapshot);
  const files = all.filter((f) => !hidden.has(f.split("/")[2]));
  let total = 0;
  const findings = [];
  for (const f of files) {
    const text = snapshot.read(f);
    if (text == null) {
      findings.push(finding(f, 1, `${f} が読めない（G-area-instrument 母集団の欠落）`));
      continue;
    }
    const m = text.match(/^description:[ \t]*(.*)$/m);
    const v = m ? m[1].trim() : "";
    if (!m || v === "" || v.startsWith("|") || v.startsWith(">")) {
      findings.push(finding(f, 1, "description が 1 行スカラーでない（G-area-instrument が面積を数えられない）"));
      continue;
    }
    total += [...v.replace(/^["']/, "").replace(/["']$/, "")].length;
  }
  return { total, findings, count: all.length };
}

/**
 * 計器の母集団だけを見る（面積の大小は判定しない・ADR-retire-area-budget）。
 * 返す finding はすべて「入力が読めない／空」であり、面積が大きいことは finding にならない。
 */
export function checkNormativeAreaInstrument(snapshot) {
  const findings = [];

  const docs = sumChars(snapshot, ALWAYS_LOADED_FILES, "G-area-instrument");
  const desc = skillDescriptionArea(snapshot);
  findings.push(...docs.findings, ...desc.findings);
  if (desc.count === 0) findings.push(finding(".claude/skills", 1, "skills が 0 件（G-area-instrument 母集団の欠落）"));

  const ruleFiles = snapshot.files.filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f));
  if (ruleFiles.length === 0) {
    findings.push(finding(".claude/rules", 1, "rules が 0 件（G-area-instrument 母集団の欠落）"));
  } else {
    findings.push(...sumChars(snapshot, ruleFiles, "G-area-instrument").findings);
  }
  return findings;
}

// ---------------------------------------------------------------------------
// duplicateDomains — 同一メンバーのドメインを報告する計器（合否を持たない）。
// このリポジトリの「重複に見える」対はすべて意図的で、畳むと片方向が沈黙する実測記録を持つ
// （G-heading-refs↔G-near-heading-refs / G-module-index↔G-module-linkage /
//  G-adr-citations↔G-adr-file-names / G-rules-globs↔G-rules-script-coverage）。
// ゆえに gate にせず、報告だけを行い判断は人に残す（`ADR-retire-area-budget` と同じ向き）。
// ---------------------------------------------------------------------------

/** 同一メンバーのドメインの組を返す。**合否を持たない**——判断は人に残す。 */
export function duplicateDomains(domains) {
  const byKey = new Map();
  for (const d of domains.values()) {
    const key = JSON.stringify([...d.members].sort());
    byKey.set(key, [...(byKey.get(key) ?? []), d.name]);
  }
  return [...byKey.values()].filter((names) => names.length > 1).map((names) => names.sort());
}

/** evidence 用の実測（検査と同じ母集団・同じ数え方であることを型で担保するための共有関数） */
export function normativeArea(snapshot) {
  const always =
    (sumChars(snapshot, ALWAYS_LOADED_FILES, "G-area-instrument").total ?? 0) + skillDescriptionArea(snapshot).total;
  const rules = sumChars(
    snapshot,
    snapshot.files.filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f)),
    "G-area-instrument",
  ).total;
  return { always, rules };
}
