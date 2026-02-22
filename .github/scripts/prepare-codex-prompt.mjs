#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

const mode = process.argv[2];
const outputPath = process.argv[3];

if (!mode || !outputPath) {
  console.error(
    "Usage: node .github/scripts/prepare-codex-prompt.mjs <implement|review|fix> <output-md>",
  );
  process.exit(1);
}

const eventPath = process.env.GITHUB_EVENT_PATH;
if (!eventPath) {
  console.error("GITHUB_EVENT_PATH is not set.");
  process.exit(1);
}

const payload = JSON.parse(fs.readFileSync(eventPath, "utf8"));
const issue = payload.issue;
if (!issue) {
  console.error("Issue payload not found.");
  process.exit(1);
}

const baseBranch = process.env.BASE_BRANCH || "";
const workBranch = process.env.WORK_BRANCH || "";
const reviewRound = process.env.REVIEW_ROUND || "1";
const reviewNotesPath =
  process.env.REVIEW_NOTES_PATH || `.github/tmp/review-round-${reviewRound}.md`;

const common = `# Context
- Repository: ${process.env.GITHUB_REPOSITORY || ""}
- Base branch: ${baseBranch}
- Work branch: ${workBranch}
- Issue: #${issue.number} ${issue.title}
- Issue URL: ${issue.html_url || ""}

## Issue body
${issue.body || "(empty)"}

## Global constraints
- "受入条件" と "非対象" を最優先すること
- 非対象は実装しないこと
- 変更は最小限にすること
- 無関係ファイルは変更しないこと
`;

let modePrompt = "";

if (mode === "implement") {
  modePrompt = `
# Task: Implement
1. Issueの受入条件を満たす実装を行う。
2. 影響範囲が必要最小限になるように変更する。
3. 変更後に最低限の検証コマンドを実行する:
   - cargo check -p snotra
   - npm run build
4. 失敗したコマンドがある場合は原因を記録し、修正可能な範囲で修正する。
`;
} else if (mode === "review") {
  modePrompt = `
# Task: Review (No code changes)
1. 現在ブランチの差分をレビューする。
2. 重大度順（High/Medium/Low）で指摘を整理する。
3. コードは変更しないこと。
4. レビュー結果を次のファイルに保存する:
   - ${reviewNotesPath}

## Output format (Markdown)
- 判定: HAS_FINDINGS または NO_FINDINGS
- Summary
- Findings (High/Medium/Low)
`;
} else if (mode === "fix") {
  modePrompt = `
# Task: Fix findings
1. レビュー結果ファイルを読み込む:
   - ${reviewNotesPath}
2. 判定が NO_FINDINGS の場合はコード変更しないこと。
3. 判定が HAS_FINDINGS の場合のみ、指摘を修正する。
4. 受入条件を満たす範囲で最小変更に留める。
`;
} else {
  console.error(`Unknown mode: ${mode}`);
  process.exit(1);
}

fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(outputPath, `${common}\n${modePrompt}\n`, "utf8");
console.log(`Prompt generated: ${outputPath}`);
