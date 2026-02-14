#!/bin/bash
# GitHub Action entrypoint for Codefang.
# Runs code analysis and writes results to GITHUB_OUTPUT.
set -euo pipefail

readonly INPUT_ANALYZERS="${INPUT_ANALYZERS:-static/complexity}"
readonly INPUT_PATH="${INPUT_PATH:-.}"
readonly INPUT_CONFIG_PATH="${INPUT_CONFIG_PATH:-}"
readonly INPUT_FORMAT="${INPUT_FORMAT:-json}"
readonly INPUT_FAIL_ON_ERROR="${INPUT_FAIL_ON_ERROR:-false}"

build_args() {
    local args=("run" "-a" "${INPUT_ANALYZERS}" "--format" "${INPUT_FORMAT}" "--silent")

    if [ -n "${INPUT_CONFIG_PATH}" ]; then
        args+=("--config" "${INPUT_CONFIG_PATH}")
    fi

    args+=("${INPUT_PATH}")
    printf '%s\n' "${args[@]}"
}

main() {
    local exit_code=0
    local output=""

    mapfile -t args < <(build_args)

    output=$(codefang "${args[@]}" 2>/dev/null) || exit_code=$?

    local pass="true"
    if [ "${exit_code}" -ne 0 ]; then
        pass="false"
    fi

    if [ -n "${GITHUB_OUTPUT:-}" ]; then
        {
            echo "pass=${pass}"
            echo "report<<CODEFANG_EOF"
            echo "${output}"
            echo "CODEFANG_EOF"
        } >> "${GITHUB_OUTPUT}"
    fi

    if [ "${pass}" = "true" ]; then
        echo "Codefang analysis completed successfully."
    else
        echo "Codefang analysis detected issues."
    fi

    echo "${output}"

    if [ "${INPUT_FAIL_ON_ERROR}" = "true" ] && [ "${pass}" = "false" ]; then
        exit 1
    fi
}

main
