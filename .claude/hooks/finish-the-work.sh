#!/usr/bin/env bash
# promptkit hook: refuse to yield while the agent is mid-task.
#
# Wired to Claude Code Stop hook. Two checks:
#   (a) scan the assistant's last message for soft-stop phrases banned by
#       instr-implement.md ("good stopping point", "let me know if you want
#       me to continue", "I'll continue next turn", "for now", "as a first
#       pass", "MVP", "MLP"). If hit, refuse the stop.
#   (b) if a roadmap exists under specs/ and has unchecked `- [ ]` items,
#       refuse the stop and tell the model to keep walking.
#
# Exit 0 = allow the stop, exit 2 = block (model resumes with stderr as
# context). Graceful degrade when jq is missing.

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  exit 0
fi

INPUT="$(cat)"
CWD="$(printf '%s' "$INPUT" | jq -r '.cwd // ""')"
TRANSCRIPT="$(printf '%s' "$INPUT" | jq -r '.transcript_path // ""')"
STOP_ACTIVE="$(printf '%s' "$INPUT" | jq -r '.stop_hook_active // false')"

# Don't recurse: if Claude restarted because of a previous Stop refusal,
# allow this stop so we don't spin forever.
if [ "$STOP_ACTIVE" = "true" ]; then
  exit 0
fi

# (a) Inspect the last assistant message for soft-stop language.
LAST_TEXT=""
if [ -n "$TRANSCRIPT" ] && [ -f "$TRANSCRIPT" ]; then
  LAST_TEXT="$(jq -rs '
    [.[] | select(.type == "assistant")] | last
    | .message.content
    | if type == "array"
      then map(select(.type == "text") | .text) | join("\n")
      else (. // "")
      end
  ' "$TRANSCRIPT" 2>/dev/null || true)"
fi

SOFT_STOPS=(
  'good stopping point'
  'natural stopping point'
  'let me know if you want me to (continue|proceed)'
  "I'll continue next turn"
  'shall I continue'
  '\bMVP\b'
  '\bMLP\b'
  '\bfor now\b'
  'as a first pass'
  'in the interest of time'
)

SOFT_HIT=""
for pat in "${SOFT_STOPS[@]}"; do
  if printf '%s' "$LAST_TEXT" | grep -qiE "$pat"; then
    SOFT_HIT="${SOFT_HIT}  - matched /${pat}/
"
  fi
done

# (b) Walk specs/ for any roadmap with unchecked items.
UNCHECKED=0
ROADMAPS=""
if [ -d "${CWD:-.}/specs" ]; then
  while IFS= read -r r; do
    n="$(grep -cE '^[[:space:]]*-[[:space:]]*\[ \]' "$r" || true)"
    if [ "${n:-0}" -gt 0 ]; then
      UNCHECKED=$((UNCHECKED + n))
      ROADMAPS="${ROADMAPS}  - ${r} ($n unchecked)
"
    fi
  done < <(find "${CWD:-.}/specs" -type f -name 'ROADMAP.md' 2>/dev/null)
fi

if [ -z "$SOFT_HIT" ] && [ "$UNCHECKED" = "0" ]; then
  exit 0
fi

{
  printf 'promptkit hook (finish-the-work): refusing to yield.\n\n'
  if [ -n "$SOFT_HIT" ]; then
    printf 'Soft-stop language detected in your last message:\n%s\n' "$SOFT_HIT"
    printf 'instr-implement.md: "Good stopping point", "I will continue next turn",\n'
    printf 'and "let me know if you want me to proceed" are not valid stop conditions.\n\n'
  fi
  if [ "$UNCHECKED" != "0" ]; then
    printf 'Roadmap is not done — %s unchecked DoD item(s):\n%s\n' "$UNCHECKED" "$ROADMAPS"
    printf 'Per instr-march.md and instr-implement.md, keep walking until every DoD\n'
    printf 'bullet has on-disk evidence of completion or you hit one of the hard-stop\n'
    printf 'conditions (toolchain missing, ambiguous DoR, second red gate, destructive\n'
    printf 'action needing user permission).\n'
  fi
} >&2

exit 2
