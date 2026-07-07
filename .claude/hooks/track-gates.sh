#!/usr/bin/env bash
# promptkit hook: bookkeeping for evidence-for-checkbox.sh.
#
# Wired to Claude Code PostToolUse on Bash and on Write|Edit|MultiEdit.
# Updates .claude/state/gates.json with:
#   - lint.exit / lint.seq   : exit code and monotonic sequence of last `make lint`
#   - test.exit / test.seq   : same for `make test`
#   - last_code_write_seq    : monotonic sequence of last Write/Edit to a code file
#
# Always exits 0 — this hook never blocks; it only records.

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  exit 0
fi

INPUT="$(cat)"
TOOL="$(printf '%s' "$INPUT" | jq -r '.tool_name // ""')"
CWD="$(printf '%s' "$INPUT" | jq -r '.cwd // ""')"

STATE_DIR="${CWD:-.}/.claude/state"
STATE_FILE="$STATE_DIR/gates.json"
mkdir -p "$STATE_DIR"
[ -f "$STATE_FILE" ] || printf '{}' > "$STATE_FILE"

NEXT_SEQ="$(jq -r '((.seq // 0) + 1)' "$STATE_FILE")"

case "$TOOL" in
  Bash)
    CMD="$(printf '%s' "$INPUT" | jq -r '.tool_input.command // ""')"
    EXIT="$(printf '%s' "$INPUT" | jq -r '.tool_response.exit_code // .tool_response.exitCode // .tool_response.returncode // 0')"
    case "$CMD" in
      *"make lint"*|*"make  lint"*)
        jq --argjson e "$EXIT" --argjson s "$NEXT_SEQ" \
          '.lint = {exit: $e, seq: $s} | .seq = $s' \
          "$STATE_FILE" > "$STATE_FILE.tmp" && mv "$STATE_FILE.tmp" "$STATE_FILE"
        ;;
      *"make test"*|*"make  test"*|*"go test"*|*"cargo test"*|*"zig build test"*)
        jq --argjson e "$EXIT" --argjson s "$NEXT_SEQ" \
          '.test = {exit: $e, seq: $s} | .seq = $s' \
          "$STATE_FILE" > "$STATE_FILE.tmp" && mv "$STATE_FILE.tmp" "$STATE_FILE"
        ;;
    esac
    ;;
  Write|Edit|MultiEdit)
    FILE="$(printf '%s' "$INPUT" | jq -r '.tool_input.file_path // ""')"
    case "$FILE" in
      # Code files only — markdown/spec writes don't invalidate the gate watermark.
      *.go|*.rs|*.zig|*.py|*.ts|*.tsx|*.js|*.jsx|*.java|*.kt|*.swift|*.c|*.h|*.cpp|*.hpp|*.cc|*.rb|*.ex|*.exs|*.sh)
        jq --argjson s "$NEXT_SEQ" \
          '.last_code_write_seq = $s | .seq = $s' \
          "$STATE_FILE" > "$STATE_FILE.tmp" && mv "$STATE_FILE.tmp" "$STATE_FILE"
        ;;
    esac
    ;;
esac

exit 0
