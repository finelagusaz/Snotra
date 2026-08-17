//! evidence 行の組み立てと、その入力の読み取りガード（#1098）。
//! **合否を持つ検査ではない**——`checks/` の外にあるので `registry.mjs` の登録走査には拾われない。
//! ただし `instrument.mjs`（合否を持たない計器）とも違い、ここは findings を出しうる。
//! 出すのは 1 種類だけで、「evidence が読もうとしたキーが未記録だった」である。
import { finding } from "./lib.mjs";

/**
 * evidence を組むための**読み取り専用ビュー**。`undefined` の読み取り自体を finding にして
 * `"?"` を返す（#1098）。
 *
 * **`REQUIRED_RECORDS` のような必須キー一覧を持たない。** 一覧は腐る写しになる——
 * evidence テンプレートが読むキーの集合こそが定義なので、**消費点を SSOT にする**。
 * 新しい `${ev.foo}` は自動で母集団に入り、供給側（検査の `ctx.record` 呼び出し）が消えれば
 * 必ず赤になる。
 *
 * 塞ぐ経路の実測（使い捨て worktree・`G-heading-refs.mjs` の `run()` から `ctx.record` の
 * 呼び出しだけを外した）: `見出し参照 undefined 件を…照合` と印字しながら **exit 0**、
 * `npm test` も全緑だった。
 *
 * **ガードが覆うのは `source` 越しの読みだけである**——ここへ渡されずに evidence 行へ
 * 差し込まれる値は対象外になる。ゆえに `assembleEvidence` は view 以外の引数を取らない
 * （残余: 「evidence は view 越しでのみ組む」という 1 行の規約が残る）。
 *
 * @param {object} source 記録済みの値の袋（`buildChecks` の sink + facade が導出した値）
 * @param {object[]} findings 未記録の読みを積む先（`runAll` の findings と同一の配列）
 */
export function evidenceView(source, findings) {
  return new Proxy(source, {
    get(target, key) {
      // Symbol はテンプレート展開の内部読み（`Symbol.toPrimitive` 等）ゆえ素通しする
      if (typeof key !== "string") return target[key];
      if (target[key] === undefined) {
        findings.push(
          finding(
            "scripts/governance/evidence.mjs",
            1,
            `evidence が読む \`${key}\` が未記録である（供給側の \`ctx.record("${key}", …)\` が消えた疑い）——undefined を印字して exit 0 になる経路`,
          ),
        );
        return "?";
      }
      return target[key];
    },
    set() {
      throw new Error("evidence の view は読み取り専用である（組み立ての途中で入力を書き換えない）");
    },
  });
}

/** evidence 行を組む唯一の口。**引数は `evidenceView` が返した view だけである**——
 *  生の袋を受け取らないことで、「evidence は view 越しでのみ組む」が構造で保たれる。 */
export function assembleEvidence(ev) {
  return (
    `検査 ${ev.checkCount} 件 / 対象文書 ${ev.docs.length} 件 / rules ${ev.rules} 件 / skills ${ev.skills} 件` +
    ` / 恒久規範 常時ロード ${ev.area.always} 字・rules ${ev.area.rules} 字` +
    ` / 見出し参照 ${ev.headingRefs} 件を md ${ev.refDocs.length} 件 + .rs ${ev.refSourceDocs.length} 件から照合` +
    ` / workspace member ${ev.workspaceMembers} 件の lints opt-in / clippy 禁止 ${ev.clippyDisallowed} 件` +
    ` / 散文の識別子 ${ev.stale} 件を ${ev.staleTargets.length} 文書から照合 / 近傍の見出し参照 ${ev.nearRefs} 件` +
    ` / ADR ${ev.adrFiles} 本の名前 / ADR の短縮引用 ${ev.adrCitations} 件`
  );
}
