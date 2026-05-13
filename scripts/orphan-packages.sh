#!/bin/bash
# orphan-packages.sh - Detect Go packages that exist on disk but are not
# imported by any other package.
#
# A package is orphan when no other package in the module imports it —
# even if it has its own tests. Self-contained test-only packages that
# nothing depends on are still dead weight in the repo.
#
# Entry points (main packages) are excluded from orphan detection since
# they are invoked by the Go toolchain directly.
#
# These packages are invisible to deadcode analysis.
# Common cause: speculatively written code with no importer yet.
#
# Whitelist: .orphan-packages-whitelist (one package path per line, # comments ok)
#
# Usage: ./scripts/orphan-packages.sh [package-patterns...]
#   Default patterns: ./cmd/... ./pkg/... ./internal/...

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
WHITELIST_FILE="$ROOT_DIR/.orphan-packages-whitelist"

cd "$ROOT_DIR"

PATTERNS=("$@")
if [ ${#PATTERNS[@]} -eq 0 ]; then
    PATTERNS=("./cmd/..." "./pkg/..." "./internal/...")
fi

MOD=$(go list -m)

# All packages on disk within the given patterns.
ALL_PKGS=$(go list "${PATTERNS[@]}" 2>/dev/null | sort -u)

# Collect every intra-module import from OTHER packages.
# Production imports, test imports, and external test imports all count
# as evidence that the target package is needed.
IMPORTED_BY_OTHERS=$(
    go list -json "${PATTERNS[@]}" 2>/dev/null \
    | jq -r --arg mod "$MOD" '
        .ImportPath as $self |
        ((.Imports // []) + (.TestImports // []) + (.XTestImports // []))[] |
        select(startswith($mod + "/")) |
        select(. != $self)
    ' \
    | sort -u
)

# Main packages are entry points — they can't be orphans.
MAIN_PKGS=$(
    go list -json "${PATTERNS[@]}" 2>/dev/null \
    | jq -r 'select(.Name == "main") | .ImportPath' \
    | sort -u
)

# A package is NOT orphan if: it is imported by another package, OR it is a main package.
NOT_ORPHAN=$(printf '%s\n%s\n' "$IMPORTED_BY_OTHERS" "$MAIN_PKGS" | sort -u)

ORPHANS=$(comm -23 <(echo "$ALL_PKGS") <(echo "$NOT_ORPHAN"))

# Apply whitelist if present.
if [ -f "$WHITELIST_FILE" ]; then
    WHITELIST=$(grep -v '^\s*#' "$WHITELIST_FILE" | grep -v '^\s*$' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' | sed "s|^|${MOD}/|" | sort -u)
    WHITELIST_COUNT=$(echo "$WHITELIST" | grep -c . || true)
    ORPHANS_FILTERED=$(comm -23 <(echo "$ORPHANS") <(echo "$WHITELIST"))
    FILTERED_COUNT=$(( $(echo "$ORPHANS" | grep -c . || true) - $(echo "$ORPHANS_FILTERED" | grep -c . || true) ))
    ORPHANS="$ORPHANS_FILTERED"
else
    WHITELIST_COUNT=0
    FILTERED_COUNT=0
fi

if [ -z "$ORPHANS" ]; then
    if [ "$FILTERED_COUNT" -gt 0 ]; then
        echo "✓ No orphan packages found (excluding $FILTERED_COUNT/$WHITELIST_COUNT whitelisted)"
    else
        echo "✓ No orphan packages found"
    fi
    exit 0
fi

echo "Orphan packages (not imported by any other package):"
echo ""

COUNT=0
while IFS= read -r pkg; do
    [ -z "$pkg" ] && continue
    rel="${pkg#${MOD}/}"
    echo "  $rel"
    COUNT=$((COUNT + 1))
done <<< "$ORPHANS"

echo ""
if [ "$FILTERED_COUNT" -gt 0 ]; then
    echo "$COUNT orphan package(s) found ($FILTERED_COUNT/$WHITELIST_COUNT whitelisted excluded)."
else
    echo "$COUNT orphan package(s) found."
fi
echo "Either import them or delete them."
exit 1
