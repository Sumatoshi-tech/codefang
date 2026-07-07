#!/usr/bin/env bash
# promptkit hook: block scope-cutting markers in code files.
# Wired to Claude Code PreToolUse on Write|Edit|MultiEdit.
#
# Exit 0 = allow, exit 2 = block (stderr is sent back to the model).
# If jq is missing, the hook degrades to allow (exit 0) — install jq to enforce.

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  printf 'promptkit hook: jq not installed; skipping no-stub-markers\n' >&2
  exit 0
fi

INPUT="$(cat)"
TOOL="$(printf '%s' "$INPUT" | jq -r '.tool_name // ""')"
FILE="$(printf '%s' "$INPUT" | jq -r '.tool_input.file_path // ""')"

case "$TOOL" in
  Write|Edit|MultiEdit) ;;
  *) exit 0 ;;
esac

# Skip files where stub markers are legitimate (docs, specs, hook source,
# instruction templates, test fixtures, the run log).
case "$FILE" in
  *.md|*.markdown|*.txt|*.rst)                          exit 0 ;;
  */docs/*|*/specs/*)                                   exit 0 ;;
  */testdata/*|*/fixtures/*|*/__fixtures__/*)           exit 0 ;;
  */.claude/hooks/*|*/_shared/hooks/*)                  exit 0 ;;
  */_shared/instructions/*|*/.agents/instructions/*)    exit 0 ;;
esac

case "$TOOL" in
  Write)     NEW="$(printf '%s' "$INPUT" | jq -r '.tool_input.content // ""')" ;;
  Edit)      NEW="$(printf '%s' "$INPUT" | jq -r '.tool_input.new_string // ""')" ;;
  MultiEdit) NEW="$(printf '%s' "$INPUT" | jq -r '[.tool_input.edits[]?.new_string] | join("\n")')" ;;
esac

# Banned patterns. Case-sensitive on purpose so "todo" inside a word
# (e.g. "todoist") does not trigger; the markers must be uppercase.
PATTERNS=(
  '\bTODO\b'
  '\bFIXME\b'
  '\bXXX\b'
  '\bHACK\b'
  '\bMVP\b'
  '\bMLP\b'
  'implement later'
  '\bfor now\b'
  'as a first pass'
  'unimplemented!\(\)'
  'panic!\("(not implemented|unimplemented)'
  'NotImplementedError'
  '# *(stub|placeholder)\b'
)

HITS=""
for pat in "${PATTERNS[@]}"; do
  if match="$(printf '%s\n' "$NEW" | grep -nE "$pat" || true)"; [ -n "$match" ]; then
    HITS="${HITS}  /${pat}/:
${match}
"
  fi
done

if [ -n "$HITS" ]; then
  cat >&2 <<EOF
promptkit hook (no-stub-markers): refusing $TOOL on $FILE.

Scope-cutting markers are banned by AGENTS.md and instr-implement.md:
"Deliver complete, runnable code at every loop. No TODO, no // implement later,
no zero-value stubs, no truncation."

Hits:
${HITS}
Banned vocabulary: TODO FIXME XXX HACK MVP MLP "implement later"
"for now" "as a first pass" unimplemented!() NotImplementedError.

Finish the work, do not stub. If a real follow-up exists, file a
bug or roadmap item instead of leaving a marker.
EOF
  exit 2
fi

exit 0
