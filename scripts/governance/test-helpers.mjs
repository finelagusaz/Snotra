//! テスト専用の最小スナップショット。**`.test.mjs` でないため registry には拾われない**
//! （registry は `checks/` 直下だけを走査するので、そもそもこの位置は対象外である）。

/**
 * 最小スナップショット: files はリポジトリ相対（"/" 区切り）、contents は path → 本文。
 *
 * **`extraFiles` は「実在するが本文が判定に関係しない」ファイルであり、空文字として読める。**
 * `null`（＝読めない）にしてはならない——実ツリーの `makeSnapshot` は列挙したファイルを
 * ディスクから読むので、**列挙されていて読めない**という状態は編集直後の経路には来ない。
 * 本文を読む検査（参照の書き方・語彙）が `extraFiles` を「母集団の欠落」と報告してしまい、
 * 実運用に無い形のノイズを固定することになる。
 *
 * **「読めない」を試験したいときは `docs` 引数へ contents に無いパスを直接渡す**——
 * `checkFoldedHeadingRefs(snap({}), ["docs/missing.md"])` の形が母集団欠落の経路である。
 */
export function snap(contents, extraFiles = []) {
  const files = [...Object.keys(contents), ...extraFiles];
  return { files, read: (p) => (p in contents ? contents[p] : extraFiles.includes(p) ? "" : null) };
}
