#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

const outputPath = process.argv[2] || ".github/tmp/latest-codex-spec.md";

const eventPath = process.env.GITHUB_EVENT_PATH;
if (!eventPath) {
  console.error("GITHUB_EVENT_PATH is not set.");
  process.exit(1);
}

const token = process.env.GITHUB_TOKEN;
if (!token) {
  console.error("GITHUB_TOKEN is not set.");
  process.exit(1);
}

const allowedActors = (process.env.CODEX_ALLOWED_ACTORS || "")
  .split(",")
  .map((name) => name.trim())
  .filter(Boolean);

if (allowedActors.length === 0) {
  console.error("CODEX_ALLOWED_ACTORS is empty.");
  process.exit(1);
}

const payload = JSON.parse(fs.readFileSync(eventPath, "utf8"));
const issue = payload.issue;
const repository = payload.repository;

if (!issue || !repository) {
  console.error("Issue or repository payload not found.");
  process.exit(1);
}

const owner = repository.owner?.login;
const repo = repository.name;
const issueNumber = issue.number;

if (!owner || !repo || !issueNumber) {
  console.error("Failed to resolve owner/repo/issue number.");
  process.exit(1);
}

function appendOutput(name, value) {
  const ghOutput = process.env.GITHUB_OUTPUT;
  if (!ghOutput) {
    return;
  }
  const delim = `EOF_${name}_${Date.now()}`;
  fs.appendFileSync(ghOutput, `${name}<<${delim}\n${value}\n${delim}\n`, "utf8");
}

async function fetchJson(url) {
  const res = await fetch(url, {
    headers: {
      Accept: "application/vnd.github+json",
      Authorization: `Bearer ${token}`,
      "X-GitHub-Api-Version": "2022-11-28",
      "User-Agent": "codex-issue-automation",
    },
  });

  if (!res.ok) {
    const text = await res.text();
    throw new Error(`GitHub API error ${res.status}: ${text}`);
  }

  return res.json();
}

async function listIssueComments() {
  const comments = [];
  let page = 1;
  while (true) {
    const url = `https://api.github.com/repos/${owner}/${repo}/issues/${issueNumber}/comments?per_page=100&page=${page}`;
    const items = await fetchJson(url);
    comments.push(...items);
    if (items.length < 100) {
      break;
    }
    page += 1;
  }
  return comments;
}

function parseSpecComment(body) {
  const text = (body || "").replace(/\r\n/g, "\n").trim();
  if (!text) {
    return null;
  }

  const lines = text.split("\n");
  const firstLine = (lines[0] || "").trim();
  if (firstLine !== "/codex spec") {
    return null;
  }

  const content = lines.slice(1).join("\n").trim();
  if (!content) {
    return { valid: false, reason: "spec本文が空です。" };
  }

  const requiredHeadings = ["背景/目的", "受入条件", "非対象"];
  const missing = requiredHeadings.filter(
    (heading) => !new RegExp(`^#{1,6}\\s*${heading}\\s*$`, "m").test(content),
  );

  if (missing.length > 0) {
    return {
      valid: false,
      reason: `spec本文に必須見出しが不足しています: ${missing.join(", ")}`,
    };
  }

  return { valid: true, content };
}

async function main() {
  const comments = await listIssueComments();
  const newestFirst = [...comments].reverse();

  let lastInvalidReason = "";

  for (const comment of newestFirst) {
    const actor = comment.user?.login || "";
    if (!allowedActors.includes(actor)) {
      continue;
    }

    const parsed = parseSpecComment(comment.body || "");
    if (!parsed) {
      continue;
    }

    if (!parsed.valid) {
      lastInvalidReason = `@${actor} の /codex spec が無効です: ${parsed.reason}`;
      continue;
    }

    fs.mkdirSync(path.dirname(outputPath), { recursive: true });
    fs.writeFileSync(outputPath, parsed.content, "utf8");

    appendOutput("status", "PASS");
    appendOutput("spec_path", outputPath);
    appendOutput("source_comment_id", String(comment.id));
    appendOutput("source_actor", actor);
    appendOutput("reason", "");

    console.log(
      `Spec extracted from comment ${comment.id} by @${actor}: ${outputPath}`,
    );
    return;
  }

  appendOutput("status", "BLOCK");
  appendOutput("spec_path", "");
  appendOutput("source_comment_id", "");
  appendOutput("source_actor", "");
  appendOutput(
    "reason",
    lastInvalidReason ||
      "allowlist登録者による有効な /codex spec コメントが見つかりませんでした。",
  );
  console.log("Spec extraction: BLOCK");
}

main().catch((error) => {
  console.error(error?.stack || String(error));
  process.exit(1);
});
