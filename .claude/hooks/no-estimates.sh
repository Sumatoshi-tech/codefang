#!/usr/bin/env bash
# promptkit hook: block estimation language, MVP/MLP scoping, and date/timestamp
# leaks in roadmaps, FRDs, journeys, run logs, and any other artifact.
#
# Wired to Claude Code PreToolUse on Write|Edit|MultiEdit.
# Exit 0 = allow, exit 2 = block. Graceful degrade when jq is missing.

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  printf 'promptkit hook: jq not installed; skipping no-estimates\n' >&2
  exit 0
fi

INPUT="$(cat)"
TOOL="$(printf '%s' "$INPUT" | jq -r '.tool_name // ""')"
FILE="$(printf '%s' "$INPUT" | jq -r '.tool_input.file_path // ""')"

case "$TOOL" in
  Write|Edit|MultiEdit) ;;
  *) exit 0 ;;
esac

# Skip the hook source itself and the docs that explain the banned vocabulary.
case "$FILE" in
  */.claude/hooks/*|*/_shared/hooks/*) exit 0 ;;
  */docs/hooks*.md)                    exit 0 ;;
  */_shared/instructions/*|*/.agents/instructions/*) exit 0 ;;
esac

case "$TOOL" in
  Write)     NEW="$(printf '%s' "$INPUT" | jq -r '.tool_input.content // ""')" ;;
  Edit)      NEW="$(printf '%s' "$INPUT" | jq -r '.tool_input.new_string // ""')" ;;
  MultiEdit) NEW="$(printf '%s' "$INPUT" | jq -r '[.tool_input.edits[]?.new_string] | join("\n")')" ;;
esac

# Block date-stamped artifact filenames (e.g. FRD-2026-05-21.md, RUN-20260521.md).
# Slug-based names are required by instr-march.md and instr-implement.md.
case "$FILE" in
  *[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]*) DATE_IN_NAME=1 ;;
  *[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]T[0-9]*) DATE_IN_NAME=1 ;;
  *) DATE_IN_NAME=0 ;;
esac

PATTERNS=(
  # Effort/time estimates
  '\b[0-9]+\s*(hours?|hrs?|days?|weeks?|months?|quarters?)\b'
  '\bstory\s*points?\b'
  '\bt-?shirt\s*(size|sizing)\b'
  '\bETA\b'
  '\bestimated\s*(effort|time|duration|cost)\b'
  # Scope/MVP language
  '\bMVP\b'
  '\bMLP\b'
  '\bPhase\s+[0-9]+\b'
  '\b[Vv][0-9]+(\.[0-9]+)?\s+(release|milestone|scope|cut)\b'
  # Clock leaks
  '\b(today|tomorrow|yesterday|tonight)\b'
  '\bnext\s+(week|month|quarter|sprint)\b'
  '\b(Monday|Tuesday|Wednesday|Thursday|Friday|Saturday|Sunday)\b'
  '\b(January|February|March|April|May|June|July|August|September|October|November|December)\s+[0-9]'
)

HITS=""
for pat in "${PATTERNS[@]}"; do
  if match="$(printf '%s\n' "$NEW" | grep -nE "$pat" || true)"; [ -n "$match" ]; then
    HITS="${HITS}  /${pat}/:
${match}
"
  fi
done

if [ -z "$HITS" ] && [ "$DATE_IN_NAME" = "0" ]; then
  exit 0
fi

cat >&2 <<EOF
promptkit hook (no-estimates): refusing $TOOL on $FILE.

AGENTS.md, instr-implement.md, and instr-march.md ban:
  - effort/time estimates (hours, days, weeks, story points, t-shirt sizes, ETAs)
  - MVP/MLP scoping language and "Phase N" / "vN scope"
  - clock references (today/tomorrow/weekdays/months)
  - date or timestamp tokens in artifact filenames (use slugs)

EOF

if [ "$DATE_IN_NAME" = "1" ]; then
  printf 'Date-stamped filename: %s\n  → rename using a topic slug (e.g. FRD-001-payment-retries.md).\n\n' "$FILE" >&2
fi

if [ -n "$HITS" ]; then
  printf 'Hits in content:\n%s\n' "$HITS" >&2
fi

printf 'State scope, gates, and risks — never forecasted duration.\n' >&2
exit 2
