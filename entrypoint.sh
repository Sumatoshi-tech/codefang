#!/bin/bash
set -euo pipefail

ANALYZERS="${INPUT_ANALYZERS:-static/*}"
TARGET_PATH="${INPUT_PATH:-.}"
FORMAT="${INPUT_FORMAT:-json}"
CONFIG_PATH="${INPUT_CONFIG_PATH:-}"
FAIL_ON_ERROR="${INPUT_FAIL_ON_ERROR:-false}"

args=("run")
args+=("-a" "$ANALYZERS")
args+=("--format" "$FORMAT")

if [ -n "$CONFIG_PATH" ]; then
    args+=("--config" "$CONFIG_PATH")
fi

args+=("$TARGET_PATH")

echo "Running: codefang ${args[*]}"

REPORT=""
EXIT_CODE=0
REPORT=$(codefang "${args[@]}" 2>&1) || EXIT_CODE=$?

PASS="true"
if [ "$EXIT_CODE" -ne 0 ]; then
    PASS="false"
fi

# Write outputs for GitHub Actions
if [ -n "${GITHUB_OUTPUT:-}" ]; then
    {
        echo "pass=$PASS"
        echo "report<<CODEFANG_EOF"
        echo "$REPORT"
        echo "CODEFANG_EOF"
    } >> "$GITHUB_OUTPUT"
fi

echo "$REPORT"

if [ "$FAIL_ON_ERROR" = "true" ] && [ "$PASS" = "false" ]; then
    echo "Analysis failed with exit code $EXIT_CODE"
    exit 1
fi
