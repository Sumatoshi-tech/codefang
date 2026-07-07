#!/usr/bin/env bash
# Bounded differential fuzz session driver for the Go<->Rust compat system.
#
# Runs each per-stage Go-native (testing/F) differential fuzz target for a short,
# bounded wall-clock budget. Each target feeds the SAME input to the LIVE Go
# binary (the oracle) and the Rust binary under the pinned env and FAILS on any
# divergence; divergence-finding inputs are distilled into
#   tests/compat/corpus/fuzzfinds/
# with the differing Go outputs stored as EVIDENCE.
#
# Usage:
#   run_fuzz.sh [seconds-per-target]   (default 20)
#   FUZZ_ONLY=FuzzParse run_fuzz.sh    (limit to one target)
#
# Exit 0 only if NO target found a divergence in its budget. (The seed pass — a
# plain `go test` over the corpus — runs first and surfaces seed-level
# divergences immediately.)
set -u
ROOT=/home/dmitriy/sources/codefang
PKG=./tests/compat/fuzz/
BUDGET="${1:-20}"
cd "$ROOT" || exit 2

TARGETS=(FuzzParse FuzzMap FuzzQuery FuzzSerializerJSON FuzzSerializerYAML FuzzSerializerCFB1 FuzzComputeAllMetrics)
[ -n "${FUZZ_ONLY:-}" ] && TARGETS=("$FUZZ_ONLY")

echo "================ DIFFERENTIAL FUZZ SESSION ================"
echo "budget=${BUDGET}s/target  oracle=build/bin  candidate=target/release"
echo

# 1) self-proving meta-tests FIRST: the layer must be shown to catch a bug.
echo "-- self-check (must catch planted defects) --"
if ! go test "$PKG" -run 'TestSelfCheck' -count=1 >/tmp/cffuzz_self.log 2>&1; then
  echo "SELF-CHECK FAILED — the fuzz layer cannot be trusted:"
  cat /tmp/cffuzz_self.log
  exit 1
fi
echo "  self-check OK (constant-stub, byte-flip, blank-stable-field all caught)"
echo

# 2) seed pass: run every target's SEED corpus as ordinary subtests (no mutation)
#    to surface corpus-level divergences immediately.
echo "-- seed pass (corpus, no mutation) --"
SEED_FAIL=0
for tgt in "${TARGETS[@]}"; do
  if go test "$PKG" -run "^${tgt}\$" -count=1 >/tmp/cffuzz_${tgt}.log 2>&1; then
    echo "  PASS  ${tgt} (all seeds match)"
  else
    nf=$(grep -c 'DIVERGENCE' /tmp/cffuzz_${tgt}.log 2>/dev/null || echo 0)
    echo "  FAIL  ${tgt} (${nf} seed divergence(s)) — see /tmp/cffuzz_${tgt}.log"
    SEED_FAIL=1
  fi
done
echo

# 3) bounded mutation fuzz per target.
echo "-- mutation fuzz (${BUDGET}s each) --"
FUZZ_FAIL=0
for tgt in "${TARGETS[@]}"; do
  printf '  %-22s ' "$tgt"
  if go test "$PKG" -run '^$' -fuzz "^${tgt}\$" -fuzztime "${BUDGET}s" -count=1 \
       >/tmp/cffuzz_fuzz_${tgt}.log 2>&1; then
    echo "OK (no new divergence in ${BUDGET}s)"
  else
    echo "DIVERGENCE — see /tmp/cffuzz_fuzz_${tgt}.log + corpus/fuzzfinds/"
    FUZZ_FAIL=1
  fi
done

echo
echo "================ RESULT ================"
FINDS=$(ls "$ROOT/tests/compat/corpus/fuzzfinds/" 2>/dev/null | grep -vcE '\.(evidence\.json|rust_out)$|\.go_run_' || echo 0)
echo "distilled divergence inputs: ${FINDS} (tests/compat/corpus/fuzzfinds/)"
if [ "$SEED_FAIL" -eq 0 ] && [ "$FUZZ_FAIL" -eq 0 ]; then
  echo "FUZZ GATE: GREEN — Go and Rust agree on every probed input"
  exit 0
fi
echo "FUZZ GATE: RED — differential divergence(s) recorded above"
exit 1
