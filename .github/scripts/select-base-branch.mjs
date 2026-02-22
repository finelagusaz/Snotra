#!/usr/bin/env node

import { execSync } from "node:child_process";
import fs from "node:fs";

function parseVersion(name) {
  const m = name.match(/^origin\/v(\d+)(?:\.(\d+))?(?:\.(\d+))?$/);
  if (!m) {
    return null;
  }
  return {
    ref: name,
    branch: name.replace(/^origin\//, ""),
    major: Number(m[1] || 0),
    minor: Number(m[2] || 0),
    patch: Number(m[3] || 0),
  };
}

function compareDesc(a, b) {
  if (a.major !== b.major) {
    return b.major - a.major;
  }
  if (a.minor !== b.minor) {
    return b.minor - a.minor;
  }
  return b.patch - a.patch;
}

const refsRaw = execSync(
  "git for-each-ref --format=%(refname:short) refs/remotes/origin",
  { encoding: "utf8" },
);

const candidates = refsRaw
  .split(/\r?\n/)
  .map((s) => s.trim())
  .filter(Boolean)
  .map(parseVersion)
  .filter(Boolean)
  .sort(compareDesc);

if (candidates.length === 0) {
  console.error("No origin/v* branch found.");
  process.exit(1);
}

const selected = candidates[0];
console.log(`Selected base branch: ${selected.branch}`);

const ghOutput = process.env.GITHUB_OUTPUT;
if (ghOutput) {
  fs.appendFileSync(ghOutput, `base_branch=${selected.branch}\n`, "utf8");
}
