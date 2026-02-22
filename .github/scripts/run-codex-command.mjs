#!/usr/bin/env node

import { execSync } from "node:child_process";
import path from "node:path";

const mode = process.argv[2];
const promptPathArg = process.argv[3];

if (!mode || !promptPathArg) {
  console.error(
    "Usage: node .github/scripts/run-codex-command.mjs <mode> <prompt-file>",
  );
  process.exit(1);
}

const promptPath = path.resolve(promptPathArg);
const outputPath = process.env.CODEX_OUTPUT_PATH
  ? path.resolve(process.env.CODEX_OUTPUT_PATH)
  : "";

const cmdTemplate = process.env.CODEX_RUNNER_COMMAND || "";
if (!cmdTemplate.trim()) {
  console.error("CODEX_RUNNER_COMMAND is empty.");
  console.error(
    "Set repository variable CODEX_RUNNER_COMMAND. Available placeholders: {mode}, {prompt_file}, {output_file}",
  );
  process.exit(1);
}

function shellQuote(value) {
  return `'${String(value).replace(/'/g, `'\"'\"'`)}'`;
}

const expanded = cmdTemplate
  .replaceAll("{mode}", mode)
  .replaceAll("{prompt_file}", shellQuote(promptPath))
  .replaceAll("{output_file}", shellQuote(outputPath));

console.log(`[codex] mode=${mode}`);
console.log(`[codex] command=${expanded}`);

try {
  execSync(expanded, {
    stdio: "inherit",
    shell: true,
    env: {
      ...process.env,
      CODEX_MODE: mode,
      CODEX_PROMPT_FILE: promptPath,
      CODEX_OUTPUT_FILE: outputPath,
    },
  });
} catch (error) {
  const status =
    typeof error?.status === "number" ? String(error.status) : "unknown";
  console.error(
    `[codex] command failed (mode=${mode}, status=${status}). Check CODEX_RUNNER_COMMAND and runner logs.`,
  );
  throw error;
}
