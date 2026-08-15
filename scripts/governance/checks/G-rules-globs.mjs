//! G-rules-globs — .claude/rules/*.md の paths glob が実在ファイルに 1 件以上マッチするかの照合。
import { finding } from "../lib.mjs";

export const id = "G-rules-globs";

/** @param {object} snapshot  @param {object} ctx buildChecks が組む共有母集団（この検査は使わない） */
export function run(snapshot, ctx) {
  return checkRulesGlobs(snapshot);
}

// ---------------------------------------------------------------------------
// G-rules-globs — .claude/rules/*.md の paths glob が実在ファイルに 1 件以上マッチ（旧 Check 8）。
// documented 意味論（bare 名 = ルート直下のみ・`**` = 階層横断・{a,b} ブレース）の自前変換。
// harness の配送判定の再現ではなく「マッチ 0 件の検知」に限定した近似。
// ---------------------------------------------------------------------------
export function globToRegex(pattern) {
  let re = "";
  let i = 0;
  while (i < pattern.length) {
    const c = pattern[i];
    if (c === "{" && pattern.indexOf("}", i) === -1) {
      re += "\\{"; // 未閉ブレースは literal 扱い（無限ループ防止・0 件マッチの明示的な赤に倒れる）
      i += 1;
    } else if (c === "*") {
      if (pattern.startsWith("**/", i)) {
        re += "(?:.*/)?";
        i += 3;
        continue;
      }
      if (pattern.startsWith("**", i)) {
        re += ".*";
        i += 2;
        continue;
      }
      re += "[^/]*";
      i += 1;
    } else if (c === "{") {
      const end = pattern.indexOf("}", i);
      re += `(?:${pattern
        .slice(i + 1, end)
        .split(",")
        .map((s) => s.replace(/[.+^$()|[\]]/g, "\\$&"))
        .join("|")})`;
      i = end + 1;
    } else {
      re += /[.+^$()|[\]?\\]/.test(c) ? `\\${c}` : c;
      i += 1;
    }
  }
  return new RegExp(`^${re}$`);
}

export function checkRulesGlobs(snapshot) {
  const findings = [];
  const rules = snapshot.files.filter((f) => /^\.claude\/rules\/[^/]+\.md$/.test(f));
  if (rules.length === 0) return [finding(".claude/rules", 1, "rules ファイルが 0 件（G-rules-globs 母集団の欠落）")];
  for (const rule of rules) {
    const text = snapshot.read(rule) ?? "";
    const fm = text.match(/^---\r?\n([\s\S]*?)\r?\n---/)?.[1] ?? ""; // CRLF checkout 耐性
    const patterns = [...fm.matchAll(/^\s*-\s*"([^"]+)"/gm)].map((m) => m[1]);
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
