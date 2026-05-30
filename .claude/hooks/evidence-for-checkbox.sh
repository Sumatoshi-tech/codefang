#!/usr/bin/env bash
# promptkit hook: refuse to tick `- [x]` boxes on a roadmap/FRD/journey unless
# the most recent `make lint` AND `make test` in this session exited 0 *after*
# the most recent code write. State is read from .claude/state/gates.json,
# which is written by track-gates.sh on PostToolUse Bash.
#
# Wired to Claude Code PreToolUse on Write|Edit|MultiEdit.
# Exit 0 = allow, exit 2 = block.

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  printf 'promptkit hook: jq not installed; skipping evidence-for-checkbox\n' >&2
  exit 0
fi

INPUT="$(cat)"
TOOL="$(printf '%s' "$INPUT" | jq -r '.tool_name // ""')"
FILE="$(printf '%s' "$INPUT" | jq -r '.tool_input.file_path // ""')"
CWD="$(printf '%s' "$INPUT" | jq -r '.cwd // ""')"

case "$TOOL" in
  Write|Edit|MultiEdit) ;;
  *) exit 0 ;;
esac

# Only police roadmaps / FRDs / journeys / run logs.
case "$FILE" in
  */ROADMAP.md|*/specs/frds/FRD-*.md|*/specs/journeys/JOURNEY-*.md|*/specs/runs/RUN-*.md) ;;
  *) exit 0 ;;
esac

case "$TOOL" in
  Write)     NEW="$(printf '%s' "$INPUT" | jq -r '.tool_input.content // ""')" ;;
  Edit)      NEW="$(printf '%s' "$INPUT" | jq -r '.tool_input.new_string // ""')" ;;
  MultiEdit) NEW="$(printf '%s' "$INPUT" | jq -r '[.tool_input.edits[]?.new_string] | join("\n")')" ;;
esac

# Count newly-introduced checked boxes.
TICKS="$(printf '%s\n' "$NEW" | grep -cE '^[[:space:]]*-[[:space:]]*\[[xX]\]' || true)"
if [ "${TICKS:-0}" -eq 0 ]; then
  exit 0
fi

STATE_FILE="${CWD:-.}/.claude/state/gates.json"
if [ ! -f "$STATE_FILE" ]; then
  cat >&2 <<EOF
promptkit hook (evidence-for-checkbox): refusing to tick $TICKS box(es) in $FILE.

No gate-state recorded for this session ($STATE_FILE missing).
Run \`make lint\` and \`make test\` first; the track-gates hook will record
their exit codes, then the tick is allowed.
EOF
  exit 2
fi

LINT="$(jq -r '.lint.exit // "missing"' "$STATE_FILE")"
TEST="$(jq -r '.test.exit // "missing"' "$STATE_FILE")"
LINT_SEQ="$(jq -r '.lint.seq // 0' "$STATE_FILE")"
TEST_SEQ="$(jq -r '.test.seq // 0' "$STATE_FILE")"
CODE_SEQ="$(jq -r '.last_code_write_seq // 0' "$STATE_FILE")"

if [ "$LINT" != "0" ] || [ "$TEST" != "0" ]; then
  cat >&2 <<EOF
promptkit hook (evidence-for-checkbox): refusing to tick $TICKS box(es) in $FILE.

Gate state: make lint exit=$LINT, make test exit=$TEST.
Both must be 0. Fix the failing gate, re-run, then tick.
EOF
  exit 2
fi

if [ "$LINT_SEQ" -lt "$CODE_SEQ" ] || [ "$TEST_SEQ" -lt "$CODE_SEQ" ]; then
  cat >&2 <<EOF
promptkit hook (evidence-for-checkbox): refusing to tick $TICKS box(es) in $FILE.

Code has been written since the last green gate run (last code write seq=$CODE_SEQ,
last lint seq=$LINT_SEQ, last test seq=$TEST_SEQ). Re-run \`make lint\` and
\`make test\` so the green is on the current code.
EOF
  exit 2
fi

exit 0
