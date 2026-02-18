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
    # Write report to a file to avoid exceeding GitHub expression memory limits.
    # Use a relative filename in the output so host-side steps can resolve it
    # against their own GITHUB_WORKSPACE (container path differs from host path).
    REPORT_FILENAME=".codefang-report.${FORMAT}"
    echo "$REPORT" > "${GITHUB_WORKSPACE:-.}/${REPORT_FILENAME}"

    {
        echo "pass=$PASS"
        echo "report-file=$REPORT_FILENAME"
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
