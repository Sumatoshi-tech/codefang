#!/usr/bin/env bash
# CLI SURFACE CONFORMANCE — entry point
# (SPEC: specs/go-compat-testing/SPEC.md, Scope #1 / Roadmap #2)
#
# Runs, under the pinned env, the full CLI-surface conformance check (Go oracle vs
# Rust) for BOTH codefang and uast, plus error-path parity, then the self-proof
# that the comparator catches planted defects.
#
# Exit 0 only when the Rust surface matches Go AND the self-proof passes.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# pinned run env (rule #5); set -f so the shell never globs argv.
set -f
export TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800

echo "##### CLI SURFACE: self-proof (must catch planted defects) #####"
python3 "$HERE/selftest/self_test.py"
SELF=$?

echo
echo "##### CLI SURFACE: live conformance (Go oracle vs Rust) #####"
python3 "$HERE/cli_surface.py" "$@"
LIVE=$?

echo
if [ "$SELF" -ne 0 ]; then
  echo "CLI-SURFACE: SELF-PROOF FAILED — the comparator cannot be trusted"
  exit 2
fi
if [ "$LIVE" -ne 0 ]; then
  echo "CLI-SURFACE: RED — Rust surface diverges from Go (see rows above)"
  exit 1
fi
echo "CLI-SURFACE: GREEN — Rust surface matches Go and self-proof passed"
exit 0
