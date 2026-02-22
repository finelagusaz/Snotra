#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

const inputPath = process.argv[2];
const outputPath = process.argv[3];

if (!inputPath || !outputPath) {
  console.error(
    "Usage: node .github/scripts/render-spec-qa-comment.mjs <input-json> <output-md>",
  );
  process.exit(1);
}

const data = JSON.parse(fs.readFileSync(inputPath, "utf8"));

const reasons = data.blocking_reasons || [];
const questions = data.questions || [];
const warnings = data.warnings || [];

const lines = [];
lines.push(`Spec QA: 実装前確認が必要です (#${data.issue_number})`);
lines.push("");
lines.push(`判定: **${data.status}**`);
lines.push("");

if (reasons.length > 0) {
  lines.push("理由サマリ:");
  for (const r of reasons) {
    lines.push(`- ${r}`);
  }
  lines.push("");
}

if (questions.length > 0) {
  lines.push("不足/矛盾ポイント:");
  for (const q of questions.slice(0, 5)) {
    lines.push(`- ${q}`);
  }
  lines.push("");
}

if (warnings.length > 0) {
  lines.push("補足（WARN・任意）:");
  for (const w of warnings) {
    lines.push(`- ${w}`);
  }
  lines.push("");
}

lines.push("回答方法:");
lines.push("- allowlist登録者が `/codex spec` コメントで仕様を更新してください。");
lines.push("- 仕様更新後に `/codex run` コメントで再実行してください。");
lines.push("- 未解消項目が残る場合は再実行時も BLOCK になります。");

fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(outputPath, `${lines.join("\n")}\n`, "utf8");
