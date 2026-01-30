#!/bin/bash
# deadcode-filter.sh - Filter deadcode output using whitelist

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
WHITELIST_FILE="$ROOT_DIR/.deadcode-whitelist"

# Set up CGO environment for libgit2
export PKG_CONFIG_PATH="$ROOT_DIR/third_party/libgit2/install/lib64/pkgconfig:$ROOT_DIR/third_party/libgit2/install/lib/pkgconfig:$PKG_CONFIG_PATH"
export CGO_CFLAGS="-I$ROOT_DIR/third_party/libgit2/install/include"
export CGO_LDFLAGS="-L$ROOT_DIR/third_party/libgit2/install/lib64 -L$ROOT_DIR/third_party/libgit2/install/lib -lgit2 -lz -lssl -lcrypto -lpthread"

# Run deadcode analysis and capture output
# Try to find deadcode in common locations
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

# Extract function names from whitelist
WHITELIST_FUNCTIONS=()
while IFS= read -r whitelist_line; do
    # Skip comments and empty lines
    if [[ "$whitelist_line" =~ ^[[:space:]]*# ]] || [[ -z "$whitelist_line" ]]; then
        continue
    fi

    # Extract function name (last part after the last colon)
    func_name=$(echo "$whitelist_line" | sed 's/.*://' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
    if [ -n "$func_name" ]; then
        WHITELIST_FUNCTIONS+=("$func_name")
    fi
done < "$WHITELIST_FILE"

# Filter out whitelisted functions
FILTERED_OUTPUT=""
FILTERED_COUNT=0

while IFS= read -r line; do
    # Skip empty lines
    if [ -z "$line" ]; then
        continue
    fi

    # Check if this line contains any whitelisted function
    WHITELISTED=false
    for func_name in "${WHITELIST_FUNCTIONS[@]}"; do
        if [[ "$line" == *"$func_name"* ]]; then
            WHITELISTED=true
            FILTERED_COUNT=$((FILTERED_COUNT + 1))
            break
        fi
    done

    # If not whitelisted, include in output
    if [ "$WHITELISTED" = false ]; then
        FILTERED_OUTPUT="$FILTERED_OUTPUT$line"$'\n'
    fi
done <<< "$DEADCODE_OUTPUT"

# Show filtered results
if [ -n "$FILTERED_OUTPUT" ]; then
    echo "$FILTERED_OUTPUT"
    echo ""
    echo "Note: $FILTERED_COUNT functions whitelisted (interface requirements)"
    echo "Tip: Use 'deadcode -whylive <function>' to understand why a function is considered reachable"
    exit 1
else
    echo "✓ No dead code found (excluding $FILTERED_COUNT whitelisted functions)"
    exit 0
fi
