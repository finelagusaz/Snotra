//! G-rules-globs — .claude/rules/*.md の paths glob が実在ファイルに 1 件以上マッチするかの照合。
//! **逆向き（実在ファイルが glob に覆われているか）は `G-rules-script-coverage` が見る**——
//! こちらが緑でも向こうは赤くなりうる（#1143: facade 1 本に当たる glob は、部分木ごと外れても 0 件にならない）。
import { finding, globToRegex, rulePathPatterns } from "../lib.mjs";

export const id = "G-rules-globs";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return checkRulesGlobs(snapshot);
}

// ---------------------------------------------------------------------------
// G-rules-globs — .claude/rules/*.md の paths glob が実在ファイルに 1 件以上マッチ（旧 Check 8）。
// **glob の変換（`globToRegex`）と paths の取り出し（`rulePathPatterns`）は `lib.mjs` が持つ**——
// 2 検査が共有するため（#1143）。近似であることの宣言も、そちらの doc が正本である。
// ---------------------------------------------------------------------------
export function checkRulesGlobs(snapshot) {
  const findings = [];
  const rules = snapshot.files.filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f));
  if (rules.length === 0) return [finding(".claude/rules", 1, "rules ファイルが 0 件（G-rules-globs 母集団の欠落）")];
  for (const rule of rules) {
    const text = snapshot.read(rule) ?? "";
    const patterns = rulePathPatterns(text);
    if (patterns.length === 0) {
      findings.push(finding(rule, 1, "frontmatter に paths パターンが 1 件も無い"));
      continue;
    }
    for (const pat of patterns) {
      const re = globToRegex(pat);
      if (!snapshot.files.some((f) => re.test(f))) {
        findings.push(finding(rule, 1, `paths glob が実在ファイルに 1 件もマッチしない: ${pat}`));
      }
    }
  }
  return findings;
}
