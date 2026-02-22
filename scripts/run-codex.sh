#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/run-codex.sh --mode <implement|review|fix> --prompt <prompt-file> [--output <output-file>]

Examples:
  ./scripts/run-codex.sh --mode implement --prompt /tmp/implement.md
  ./scripts/run-codex.sh --mode review --prompt /tmp/review.md --output /tmp/review-result.md
EOF
}

MODE=""
PROMPT_FILE=""
OUTPUT_FILE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      MODE="${2:-}"
      shift 2
      ;;
    --prompt)
      PROMPT_FILE="${2:-}"
      shift 2
      ;;
    --output)
      OUTPUT_FILE="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$MODE" || -z "$PROMPT_FILE" ]]; then
  echo "--mode and --prompt are required." >&2
  usage >&2
  exit 2
fi

case "$MODE" in
  implement|review|fix) ;;
  *)
    echo "Invalid mode: $MODE (expected: implement|review|fix)" >&2
    exit 2
    ;;
esac

if [[ ! -f "$PROMPT_FILE" ]]; then
  echo "Prompt file not found: $PROMPT_FILE" >&2
  exit 2
fi

if command -v codex >/dev/null 2>&1; then
  CODEX_CMD=(codex)
elif command -v npx >/dev/null 2>&1; then
  # Fallback for hosted runners where global npm bin may not be in PATH.
  CODEX_CMD=(npx -y @openai/codex)
else
  echo "Neither 'codex' nor 'npx' command is available in PATH." >&2
  exit 127
fi

COMMON_ARGS=(
  exec
  --dangerously-bypass-approvals-and-sandbox
  --color
  never
)

if [[ "$MODE" == "review" ]]; then
  if [[ -z "$OUTPUT_FILE" ]]; then
    echo "--output is required when --mode review is used." >&2
    exit 2
  fi
  mkdir -p "$(dirname "$OUTPUT_FILE")"
  "${CODEX_CMD[@]}" "${COMMON_ARGS[@]}" --output-last-message "$OUTPUT_FILE" - < "$PROMPT_FILE"
else
  "${CODEX_CMD[@]}" "${COMMON_ARGS[@]}" - < "$PROMPT_FILE"
fi
