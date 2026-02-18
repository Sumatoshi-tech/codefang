#!/bin/bash
# update-third-party.sh - update libgit2 and tree-sitter related dependencies.
#
# This script intentionally avoids git operations so it can run in restricted
# environments and reproducible automation.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

MODE="all"
DRY_RUN=0

LIBGIT2_REF=""
TREE_SITTER_BARE_REF=""
SITTER_FOREST_REF=""
SITTER_FOREST_GO_REF=""

usage() {
	echo "Usage: $0 [options]"
	echo ""
	echo "Options:"
	echo "  --mode MODE                    Update mode: all|libgit2|treesitter (default: all)"
	echo "  --libgit2-ref REF              libgit2 tag/SHA/branch ref for archive download"
	echo "  --tree-sitter-bare-ref REF     Ref for github.com/alexaandru/go-tree-sitter-bare"
	echo "  --sitter-forest-ref REF        Ref for github.com/alexaandru/go-sitter-forest"
	echo "  --sitter-forest-go-ref REF     Ref for github.com/alexaandru/go-sitter-forest/go"
	echo "  --dry-run                      Print planned commands without applying changes"
	echo "  -h, --help                     Show help"
}

run_cmd() {
	local cmd="$1"

	if [[ "$DRY_RUN" == "1" ]]; then
		echo "[dry-run] $cmd"
		return 0
	fi

	eval "$cmd"
}

require_tool() {
	local tool_name="$1"

	if ! command -v "$tool_name" >/dev/null 2>&1; then
		echo "error: required tool not found: $tool_name" >&2
		exit 2
	fi
}

update_libgit2() {
	if [[ -z "$LIBGIT2_REF" ]]; then
		echo "error: --libgit2-ref is required for libgit2 update" >&2
		exit 2
	fi

	require_tool curl
	require_tool tar

	local tmp_dir
	tmp_dir="$(mktemp -d)"
	local archive="$tmp_dir/libgit2.tar.gz"
	local extract_dir="$tmp_dir/extracted"
	local source_dir=""
	local target_dir="$ROOT_DIR/third_party/libgit2"
	local url="https://codeload.github.com/libgit2/libgit2/tar.gz/${LIBGIT2_REF}"

	echo "Updating libgit2 from ref: $LIBGIT2_REF"
	run_cmd "mkdir -p \"$extract_dir\""
	run_cmd "curl -fsSL \"$url\" -o \"$archive\""
	run_cmd "tar -xzf \"$archive\" -C \"$extract_dir\""

	if [[ "$DRY_RUN" == "1" ]]; then
		run_cmd "rm -rf \"$target_dir\""
		run_cmd "mkdir -p \"$target_dir\""
		run_cmd "cp -a \"<extracted-libgit2>/.\" \"$target_dir/\""
		run_cmd "printf '%s\n' \"$LIBGIT2_REF\" > \"$ROOT_DIR/third_party/libgit2.ref\""
		return 0
	fi

	source_dir="$(find "$extract_dir" -mindepth 1 -maxdepth 1 -type d | head -n 1)"
	if [[ -z "$source_dir" ]]; then
		echo "error: unable to locate extracted libgit2 directory" >&2
		exit 1
	fi

	run_cmd "rm -rf \"$target_dir\""
	run_cmd "mkdir -p \"$target_dir\""
	run_cmd "cp -a \"$source_dir/.\" \"$target_dir/\""
	run_cmd "printf '%s\n' \"$LIBGIT2_REF\" > \"$ROOT_DIR/third_party/libgit2.ref\""
	rm -rf "$tmp_dir"
}

update_tree_sitter_modules() {
	if [[ -z "$TREE_SITTER_BARE_REF" || -z "$SITTER_FOREST_REF" || -z "$SITTER_FOREST_GO_REF" ]]; then
		echo "error: tree-sitter module update requires all refs:" >&2
		echo "  --tree-sitter-bare-ref --sitter-forest-ref --sitter-forest-go-ref" >&2
		exit 2
	fi

	require_tool go

	echo "Updating tree-sitter modules:"
	echo "  go-tree-sitter-bare: $TREE_SITTER_BARE_REF"
	echo "  go-sitter-forest:    $SITTER_FOREST_REF"
	echo "  go-sitter-forest/go: $SITTER_FOREST_GO_REF"

	run_cmd "cd \"$ROOT_DIR\" && go get github.com/alexaandru/go-tree-sitter-bare@\"$TREE_SITTER_BARE_REF\""
	run_cmd "cd \"$ROOT_DIR\" && go get github.com/alexaandru/go-sitter-forest@\"$SITTER_FOREST_REF\""
	run_cmd "cd \"$ROOT_DIR\" && go get github.com/alexaandru/go-sitter-forest/go@\"$SITTER_FOREST_GO_REF\""
	run_cmd "cd \"$ROOT_DIR\" && go mod tidy"
}

while [[ $# -gt 0 ]]; do
	case "$1" in
		--mode)
			MODE="$2"
			shift 2
			;;
		--libgit2-ref)
			LIBGIT2_REF="$2"
			shift 2
			;;
		--tree-sitter-bare-ref)
			TREE_SITTER_BARE_REF="$2"
			shift 2
			;;
		--sitter-forest-ref)
			SITTER_FOREST_REF="$2"
			shift 2
			;;
		--sitter-forest-go-ref)
			SITTER_FOREST_GO_REF="$2"
			shift 2
			;;
		--dry-run)
			DRY_RUN=1
			shift
			;;
		-h|--help)
			usage
			exit 0
			;;
		*)
			echo "error: unknown option: $1" >&2
			usage >&2
			exit 2
			;;
	esac
done

case "$MODE" in
	all)
		update_libgit2
		update_tree_sitter_modules
		;;
	libgit2)
		update_libgit2
		;;
	treesitter)
		update_tree_sitter_modules
		;;
	*)
		echo "error: invalid mode: $MODE (expected all|libgit2|treesitter)" >&2
		exit 2
		;;
esac

echo "Third-party update workflow completed (mode=$MODE, dry_run=$DRY_RUN)."
