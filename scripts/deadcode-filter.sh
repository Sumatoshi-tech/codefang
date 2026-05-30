#!/bin/bash
# deadcode-filter.sh - Filter deadcode output using whitelist
#
# Whitelist format: exact qualified function names as printed by deadcode.
# One entry per line. Lines starting with # are comments.
# Examples:
#   SomeType.MethodName
#   UnusedHelper
#   Pretty

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
WHITELIST_FILE="$ROOT_DIR/.deadcode-whitelist"

# Set up CGO environment

# Run deadcode analysis and capture output
DEADCODE_CMD="deadcode"
if ! command -v deadcode &> /dev/null; then
	if [ -f ~/go/bin/deadcode ]; then
		DEADCODE_CMD=~/go/bin/deadcode
	fi
fi

DEADCODE_OUTPUT=$($DEADCODE_CMD "$@" 2>&1 || true)

# If whitelist doesn't exist, just show all results
if [ ! -f "$WHITELIST_FILE" ]; then
    echo "$DEADCODE_OUTPUT"
    exit 0
fi

# Build associative array of whitelisted function names for O(1) lookup.
# Each entry is the exact qualified name as printed by deadcode
# (e.g. "SomeType.MethodName", "Pretty").
declare -A WHITELIST_SET
WHITELIST_COUNT=0
while IFS= read -r whitelist_line; do
    # Skip comments and empty lines
    [[ "$whitelist_line" =~ ^[[:space:]]*# ]] && continue
    [[ -z "${whitelist_line// /}" ]] && continue

    # Trim whitespace
    func_name=$(echo "$whitelist_line" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
    if [ -n "$func_name" ]; then
        WHITELIST_SET["$func_name"]=1
        WHITELIST_COUNT=$((WHITELIST_COUNT + 1))
    fi
done < "$WHITELIST_FILE"

# Filter out whitelisted functions using exact match on the qualified name.
# deadcode output format: "path/to/file.go:line:col: unreachable func: QualifiedName"
FILTERED_OUTPUT=""
FILTERED_COUNT=0

while IFS= read -r line; do
    [ -z "$line" ] && continue

    # Extract the qualified function name after "unreachable func: "
    dead_func="${line##*unreachable func: }"

    if [ -n "${WHITELIST_SET[$dead_func]+x}" ]; then
        FILTERED_COUNT=$((FILTERED_COUNT + 1))
    else
        FILTERED_OUTPUT="$FILTERED_OUTPUT$line"$'\n'
    fi
done <<< "$DEADCODE_OUTPUT"

# Show filtered results
if [ -n "$FILTERED_OUTPUT" ]; then
    echo "$FILTERED_OUTPUT"
    echo ""
    echo "Note: $FILTERED_COUNT/$WHITELIST_COUNT whitelisted functions excluded"
    echo "Tip: Use 'deadcode -whylive <function>' to understand why a function is considered reachable"
    exit 1
else
    echo "✓ No dead code found (excluding $FILTERED_COUNT/$WHITELIST_COUNT whitelisted functions)"
    exit 0
fi
