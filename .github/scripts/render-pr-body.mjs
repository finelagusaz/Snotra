#!/usr/bin/env node

import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const outputPath = process.argv[2];
if (!outputPath) {
  console.error("Usage: node .github/scripts/render-pr-body.mjs <output-md>");
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

let qaStatus = process.env.QA_STATUS || "N/A";
const qaPath = process.env.QA_RESULT_PATH || "";
if (qaStatus === "N/A" && qaPath && fs.existsSync(qaPath)) {
  try {
    const qa = JSON.parse(fs.readFileSync(qaPath, "utf8"));
    qaStatus = qa.status || "N/A";
  } catch {
    qaStatus = "N/A";
  }
}

function readIfExists(p) {
  if (!fs.existsSync(p)) {
    return "";
  }
  return fs.readFileSync(p, "utf8").trim();
}

const review1Path =
  process.env.REVIEW_ROUND1_PATH || ".github/tmp/review-round-1.md";
const review2Path =
  process.env.REVIEW_ROUND2_PATH || ".github/tmp/review-round-2.md";

const review1 = readIfExists(review1Path);
const review2 = readIfExists(review2Path);

let commitList = "";
try {
  execSync(`git rev-parse --verify --quiet ${baseBranch}`, {
    stdio: "ignore",
  });
  execSync(`git rev-parse --verify --quiet ${workBranch}`, {
    stdio: "ignore",
  });
  commitList = execSync(`git log --oneline ${baseBranch}..${workBranch}`, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  }).trim();
} catch {
  commitList = "";
}

const lines = [];
lines.push(`## 対応Issue`);
lines.push(`- Closes #${issue.number}`);
lines.push("");
lines.push("## ブランチ");
lines.push(`- Base: \`${baseBranch}\``);
lines.push(`- Head: \`${workBranch}\``);
lines.push("");
lines.push("## 変更内容");
lines.push("- Codex により実装・自己レビュー・必要な修正を実施");
lines.push("");
lines.push("## 受入条件チェック");
lines.push("- [ ] Issue の受入条件を満たすことを確認");
lines.push("- [ ] 非対象に違反していないことを確認");
lines.push("");
lines.push("## Spec QA");
lines.push(`- 判定: ${qaStatus}`);
lines.push("");
lines.push("## Codexレビュー結果");
if (review1) {
  lines.push("<details><summary>Round 1</summary>");
  lines.push("");
  lines.push("```md");
  lines.push(review1);
  lines.push("```");
  lines.push("");
  lines.push("</details>");
}
if (review2) {
  lines.push("<details><summary>Round 2</summary>");
  lines.push("");
  lines.push("```md");
  lines.push(review2);
  lines.push("```");
  lines.push("");
  lines.push("</details>");
}
if (!review1 && !review2) {
  lines.push("- レビュー記録なし");
}
lines.push("");
lines.push("## コミット");
if (commitList) {
  lines.push("```text");
  lines.push(commitList);
  lines.push("```");
} else {
  lines.push("- 取得できませんでした");
}
lines.push("");
lines.push("## テスト結果");
lines.push("- [ ] `cargo test -p snotra-core`");
lines.push("- [ ] `cargo check -p snotra`");
lines.push("- [ ] `npm run build`");

fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(outputPath, `${lines.join("\n")}\n`, "utf8");
