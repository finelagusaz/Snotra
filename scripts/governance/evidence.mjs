//! evidence 行の組み立てと、その入力の読み取りガード（#1098）。
//! **合否を持つ検査ではない**——`checks/` の外にあるので `registry.mjs` の登録走査には拾われない。
//! ただし `instrument.mjs`（合否を持たない計器）とも違い、ここは findings を出しうる。
//! 出すのは 1 種類だけで、「evidence が読もうとしたキーが未記録だった」である。
import { finding } from "./lib.mjs";

/** view の brand。**export しない**——`assembleEvidence` が「view 越しか」を判定する唯一の手掛かりで、
 *  外へ出せば呼び出し側が偽造できてしまう。実体のプロパティではなく `get` トラップが合成するので、
 *  `{...view}` のスプレッド（own な文字列キーだけを写す）でも複製されない。 */
const VIEW = Symbol("evidenceView");

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
 * **ガードが覆うのは view からの 1 段目の読みである。** 覆われない形は 2 つ在り、どちらも
 * 「ここへ渡していない」だけが理由ではない:
 *   - **渡していない値**——evidence 行へ view の外から差し込む形。`assembleEvidence` が
 *     brand 無しの引数を throw で拒むので、袋ごと外す形は表現できない
 *   - **渡してあるのに覆われない読み**——view が返した値の**中**をさらに読む形。2 段目は
 *     生のオブジェクトからの読みなので、欠けても finding にならない。**入れ子のキーを
 *     テンプレートへ書かないこと**（`ev.area.always` を平坦な `ev.areaAlways` へ倒した理由が
 *     これである。倒す前は、供給側が `always` を落とすと findings 0 件のまま
 *     「常時ロード undefined 字」を印字して exit 0 だった・2026-08-17 実測）
 *
 * **`.length` の読みは残余である。** トップレベルの配列が欠ければ finding は出る（＝赤にはなり、
 * 沈黙ではない）が、ガードが返す `"?"` の `.length` は 1 なので、evidence 行には
 * 「1 件」というもっともらしい数字が並ぶ。行の数字は findings が空のときだけ意味を持つ。
 *
 * @param {object} source 記録済みの値の袋（`buildChecks` の sink + facade が導出した値）
 * @param {object[]} findings 未記録の読みを積む先（`runAll` の findings と同一の配列）
 */
export function evidenceView(source, findings) {
  return new Proxy(source, {
    get(target, key) {
      // brand は target に実体を持たない——**Symbol の素通しより先に返す**（後ろに置くと
      // 生の袋へ抜けて undefined になり、判定が常に throw 側へ倒れる）
      if (key === VIEW) return true;
      // 残る Symbol はテンプレート展開の内部読み（`Symbol.toPrimitive` 等）ゆえ素通しする
      if (typeof key !== "string") return target[key];
      if (target[key] === undefined) {
        findings.push(
          finding(
            "scripts/governance/evidence.mjs",
            1,
            `evidence が読む \`${key}\` が未記録である（供給側——検査の \`ctx.record\` か facade の導出——が消えた疑い）——undefined を印字して exit 0 になる経路`,
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

/** evidence 行を組む口。**brand の無い引数は throw で拒む**——brand は上の `VIEW` で、
 *  `get` トラップが合成するので実体を持たず、export もしていない。ゆえに「view を外して
 *  生の袋を渡す」形はここで落ちる。
 *
 *  **かつてこれは散文の規約だった**（「引数を view だけにしてあるので構造で保たれる」）。
 *  それは偽で、view を外しても throw せず、供給が揃った状態では `governance-check.test.mjs` の
 *  カナリアも `npm test` も緑のままだった（2026-08-17 実測: `governance:check` exit 0 /
 *  745 件全緑）。今この 1 行が落とすので、同じ変異は `runAll` を呼ぶ検査すべてを赤にする。 */
export function assembleEvidence(ev) {
  if (!ev?.[VIEW]) {
    throw new Error("assembleEvidence: 引数は `evidenceView` が返した view でなければならない（生の袋を渡すと未記録の読みが finding にならない）");
  }
  return (
    `検査 ${ev.checkCount} 件 / 対象文書 ${ev.docs.length} 件 / rules ${ev.rules} 件 / skills ${ev.skills} 件` +
    ` / 恒久規範 常時ロード ${ev.areaAlways} 字・rules ${ev.areaRules} 字` +
    ` / 見出し参照 ${ev.headingRefs} 件を md ${ev.refDocs.length} 件 + .rs ${ev.refSourceDocs.length} 件から照合` +
    ` / workspace member ${ev.workspaceMembers} 件の lints opt-in / clippy 禁止 ${ev.clippyDisallowed} 件` +
    ` / 散文の識別子 ${ev.stale} 件を ${ev.staleTargets.length} 文書から照合 / 近傍の見出し参照 ${ev.nearRefs} 件` +
    ` / ADR ${ev.adrFiles} 本の名前 / ADR の短縮引用 ${ev.adrCitations} 件`
  );
}
