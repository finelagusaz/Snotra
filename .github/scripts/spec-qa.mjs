#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

const outputPath = process.argv[2] || ".github/tmp/spec-qa-result.json";
const specInputPath = process.argv[3] || process.env.SPEC_INPUT_PATH || "";

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

let specBody = "";
if (specInputPath) {
  if (!fs.existsSync(specInputPath)) {
    console.error(`SPEC_INPUT_PATH file not found: ${specInputPath}`);
    process.exit(1);
  }
  specBody = fs.readFileSync(specInputPath, "utf8");
} else {
  specBody = issue.body || "";
}

function extractSection(body, sectionNames) {
  const lines = body.split(/\r?\n/);
  let start = -1;

  for (let i = 0; i < lines.length; i += 1) {
    const m = lines[i].match(/^#{1,6}\s*(.+?)\s*$/);
    if (!m) {
      continue;
    }
    const heading = m[1].trim().toLowerCase();
    if (sectionNames.some((name) => heading.includes(name.toLowerCase()))) {
      start = i + 1;
      break;
    }
  }

  if (start === -1) {
    return "";
  }

  const collected = [];
  for (let i = start; i < lines.length; i += 1) {
    if (/^#{1,6}\s+/.test(lines[i])) {
      break;
    }
    collected.push(lines[i]);
  }

  return collected.join("\n").trim();
}

function extractBullets(text) {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => /^[-*]\s+/.test(line))
    .map((line) =>
      line
        .replace(/^[-*]\s+\[[ xX]\]\s*/, "")
        .replace(/^[-*]\s+/, "")
        .trim(),
    )
    .filter(Boolean);
}

function normalize(text) {
  return (text || "")
    .toLowerCase()
    .replace(/[`*_~[\](){}:;,.!?/\\|'"-]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

const sectionNames = {
  background: ["背景/目的", "背景", "目的", "background", "purpose"],
  acceptance: [
    "受入条件",
    "受け入れ条件",
    "acceptance criteria",
    "acceptance",
  ],
  outOfScope: ["非対象", "対象外", "out of scope", "non-goals", "not in scope"],
};

const background = extractSection(specBody, sectionNames.background);
const acceptance = extractSection(specBody, sectionNames.acceptance);
const outOfScope = extractSection(specBody, sectionNames.outOfScope);

const blockingReasons = [];
const questions = [];
const warnings = [];

if (!background) {
  blockingReasons.push("背景/目的 セクションが未記入です。");
  questions.push(
    "Q1. この変更で解決したい課題と、期待する最終状態を1-3行で明記してください。",
  );
}

const acceptanceBullets = extractBullets(acceptance);
if (!acceptance || acceptanceBullets.length === 0) {
  blockingReasons.push("受入条件 セクションが未記入、または箇条書き条件がありません。");
  questions.push(
    "Q2. 検証可能な受入条件を箇条書きで最低1つ以上記載してください。",
  );
}

const ambiguousWords = [
  "いい感じ",
  "適切",
  "なるべく",
  "できれば",
  "必要に応じて",
  "十分",
  "使いやすく",
  "可能な限り",
  "as appropriate",
  "good",
  "better",
];

if (acceptanceBullets.length > 0) {
  const ambiguousHits = acceptanceBullets.filter((item) =>
    ambiguousWords.some((word) => normalize(item).includes(normalize(word))),
  );
  if (ambiguousHits.length > 0) {
    blockingReasons.push("受入条件 に曖昧語を含む項目があります。");
    questions.push(
      "Q3. 受入条件の曖昧語を、確認可能な条件（具体的な挙動・表示・保持条件）に書き換えてください。",
    );
  }
}

if (!outOfScope) {
  blockingReasons.push("非対象 セクションが未記入です。");
  questions.push("Q4. 今回やらないこと（非対象）を箇条書きで明記してください。");
}

const outOfScopeBullets = extractBullets(outOfScope);
if (acceptanceBullets.length > 0 && outOfScopeBullets.length > 0) {
  const conflicts = [];
  for (const acc of acceptanceBullets) {
    const nAcc = normalize(acc);
    for (const non of outOfScopeBullets) {
      const nNon = normalize(non);
      if (!nAcc || !nNon) {
        continue;
      }
      if (nAcc === nNon || nAcc.includes(nNon) || nNon.includes(nAcc)) {
        conflicts.push({ acc, non });
      }
    }
  }
  if (conflicts.length > 0) {
    blockingReasons.push("受入条件 と 非対象 に矛盾する記述があります。");
    questions.push(
      "Q5. 矛盾している項目の優先順位を指定し、受入条件または非対象を修正してください。",
    );
  }
}

if (!/テスト|test/i.test(specBody)) {
  warnings.push("テスト観点が未記載です（任意）。");
}
if (!/#\d+|https?:\/\/github\.com\//i.test(specBody)) {
  warnings.push("関連Issue/PRの参照が未記載です（任意）。");
}

const status = blockingReasons.length > 0 ? "BLOCK" : "PASS";

const result = {
  status,
  issue_number: issue.number,
  issue_title: issue.title,
  blocking_reasons: blockingReasons,
  questions,
  warnings,
  sections: {
    background,
    acceptance,
    out_of_scope: outOfScope,
  },
  source: specInputPath ? "spec_input_path" : "issue_body",
};

fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(outputPath, JSON.stringify(result, null, 2), "utf8");

function appendOutput(name, value) {
  const ghOutput = process.env.GITHUB_OUTPUT;
  if (!ghOutput) {
    return;
  }
  const delim = `EOF_${name}_${Date.now()}`;
  fs.appendFileSync(ghOutput, `${name}<<${delim}\n${value}\n${delim}\n`, "utf8");
}

appendOutput("status", status);
appendOutput("result_path", outputPath);
appendOutput("blocking_reasons_json", JSON.stringify(blockingReasons));
appendOutput("questions_json", JSON.stringify(questions));
appendOutput("warnings_json", JSON.stringify(warnings));

console.log(`Spec QA: ${status}`);
if (blockingReasons.length > 0) {
  console.log(`Blocking reasons: ${blockingReasons.length}`);
}
