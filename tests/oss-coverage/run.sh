#!/usr/bin/env bash
# Real-world language-coverage sweep for the complexity/cohesion analyzers.
#
# Clones one representative open-source project per supported language
# (manifest.tsv), runs `codefang run -a static/complexity,static/cohesion
# --per-file` over each, and prints a per-language function-detection matrix
# via summarize.py. Zero-function rates flag uastmap gaps on real code that
# the synthetic corpus test (internal/analyzers/language_coverage_test.go)
# cannot catch.
#
# Usage:
#   tests/oss-coverage/run.sh [workdir]
#
# Requires: a built codefang binary on PATH or CODEFANG=/path/to/codefang.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKDIR="${1:-${TMPDIR:-/tmp}/codefang-oss-coverage}"
CODEFANG="${CODEFANG:-codefang}"
REPOS="$WORKDIR/repos"
OUT="$WORKDIR/out"

command -v "$CODEFANG" >/dev/null || { echo "codefang binary not found (set CODEFANG=...)" >&2; exit 1; }

mkdir -p "$REPOS" "$OUT"

echo "Cloning repos into $REPOS ..."
while IFS=$'\t' read -r lang url; do
  [ -z "$lang" ] && continue
  name=$(basename "$url")
  [ -d "$REPOS/$name" ] || git clone --depth 1 --quiet "$url" "$REPOS/$name" &
done < "$HERE/manifest.tsv"
wait

echo "Analyzing ..."
for d in "$REPOS"/*/; do
  name=$(basename "$d")
  timeout -k 10 300 "$CODEFANG" run -a static/complexity,static/cohesion \
    --format json --per-file "$d" \
    > "$OUT/$name.json" 2> "$OUT/$name.err" \
    || echo "WARN: $name exited non-zero" >&2
done

python3 "$HERE/summarize.py" "$OUT"
