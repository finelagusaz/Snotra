//! テスト専用の最小スナップショット。**`.test.mjs` でないため registry には拾われない**
//! （registry は `checks/` 直下だけを走査するので、そもそもこの位置は対象外である）。

/** 最小スナップショット: files はリポジトリ相対（"/" 区切り）、contents は path → 本文 */
export function snap(contents, extraFiles = []) {
  const files = [...Object.keys(contents), ...extraFiles];
  return { files, read: (p) => contents[p] ?? null };
}
