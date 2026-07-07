#!/usr/bin/env bash
# =============================================================================
# COMPAT TEST SYSTEM -- SINGLE ENTRY POINT   (SPEC roadmap 8 / task PHASE: CI)
# =============================================================================
# Two tiers:
#   smoke  -- pre-commit. Full 155-cell invocation matrix vs the LIVE Go oracle
#             (serial = deterministic verdicts; NO matrix shrink), CLI-surface
#             conformance, metamorphic/anti-sim, fast tamper-verify + tamper
#             self-test, gap ledger (no slow llvm-cov). Target: fast feedback.
#   full   -- scheduled. Everything in smoke PLUS the full 486-cell matrix, the
#             MUTATION SELF-TEST (rebuilds Rust, proves a planted bug is caught),
#             per-stage differential fuzzing, and llvm-cov-backed ledger.
#
# Pinned run env (rule #5) is applied by the oracle to BOTH binaries; we also set
# it here so every child sees it.  Compare STDOUT only (stderr is progress).
#
# Output: per-cell PASS/FAIL/SIM/EXPECTED_EMPTY + final tallies + the gap ledger.
# Exit code: NONZERO on any real (un-allowlisted) divergence. A known-divergence
# allowlist (allowlist.json) only neutralizes a cell when it carries a written
# reason + Go-nondeterminism evidence; an excuse without evidence fails CLOSED.
# =============================================================================
set -u
set -f
export TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TIER="${1:-smoke}"
case "$TIER" in
  smoke|full) ;;
  *) echo "usage: run.sh [smoke|full]"; exit 2 ;;
esac

PY=python3
RC=0
step() { printf '\n\n############## %s ##############\n' "$1"; }
fail() { echo ">>> STAGE FAILED: $1"; RC=1; }

T0=$(date +%s)
echo "================================================================"
echo " COMPAT TEST SYSTEM   tier=$TIER"
echo " oracle (truth) : /home/dmitriy/sources/codefang/build/bin/{codefang,uast}"
echo " candidate      : $(dirname "$HERE")/../target/release/{codefang,uast}"
echo " env            : TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800"
echo "================================================================"

# -----------------------------------------------------------------------------
# 1. INTEGRITY / TAMPER-EVIDENCE  (fail-closed; runs FIRST so a tampered harness
#    can never report a trustworthy green).
#      smoke: tamper self-test + live tamper verify  (fast)
#      full : + the MUTATION SELF-TEST (rebuilds Rust, proves it catches a bug)
# -----------------------------------------------------------------------------
step "1. TAMPER-EVIDENCE (fail-closed)"
$PY "$HERE/integrity/tamper_check.py" --self-test || fail "tamper self-test"
$PY "$HERE/integrity/tamper_check.py"             || fail "tamper verify (fail-closed)"

# -----------------------------------------------------------------------------
# 2. CLI SURFACE CONFORMANCE  (recursive Go-vs-Rust flags/defaults/help/exit)
# -----------------------------------------------------------------------------
step "2. CLI SURFACE CONFORMANCE"
bash "$HERE/cli_surface/run.sh" || fail "cli-surface"

# -----------------------------------------------------------------------------
# 3. DIFFERENTIAL INVOCATION MATRIX vs the LIVE Go oracle
#    smoke = 155 cells (serial, deterministic), full = 486 cells.
#    run_matrix exits nonzero on FAIL/SIM; we do NOT gate on that here -- the
#    allowlist-aware FINAL GATE (step 6) owns the exit decision.
# -----------------------------------------------------------------------------
step "3. DIFFERENTIAL MATRIX (live Go oracle, N>=3 per cell)"
$PY "$HERE/run_matrix.py" --tier "$TIER" --n-go 3 || true

# -----------------------------------------------------------------------------
# 4. METAMORPHIC / ANTI-SIMULATION  (vary-input, grow-with-limit, determinism,
#    non-empty, golden-drift => SIM). SIM verdicts are real divergences.
# -----------------------------------------------------------------------------
step "4. METAMORPHIC / ANTI-SIMULATION"
$PY "$HERE/metamorphic/metamorphic.py" --tier smoke || true   # writes results.json

# -----------------------------------------------------------------------------
# 5. GAP LEDGER + COVERAGE
#    smoke: matrix-cell coverage + live Go-variant evidence harvest (no llvm-cov)
#    full : + cargo-llvm-cov line/region/branch
# -----------------------------------------------------------------------------
step "5. GAP LEDGER + COVERAGE ACCOUNTING"
if [ "$TIER" = "full" ]; then
  $PY "$HERE/coverage/build_ledger.py" --tier full --probe-variants \
      --probe-parse || fail "ledger(full)"
else
  $PY "$HERE/coverage/build_ledger.py" --tier smoke --no-rust-cov \
      --probe-variants || fail "ledger(smoke)"
fi

# -----------------------------------------------------------------------------
# FULL-ONLY heavy stages: mutation self-test + differential fuzzing.
# -----------------------------------------------------------------------------
if [ "$TIER" = "full" ]; then
  step "F1. MUTATION SELF-TEST (META-GATE: prove the system catches a bug)"
  bash "$HERE/integrity/mutation_self_test.sh" || fail "mutation self-test"

  step "F2. PER-STAGE DIFFERENTIAL FUZZING"
  bash "$HERE/fuzz/run_fuzz.sh" "${FUZZ_SECS:-20}" || fail "differential fuzz"
fi

# -----------------------------------------------------------------------------
# 6. FINAL GATE  (allowlist-aware; the single CI exit decision + honest tally)
# -----------------------------------------------------------------------------
step "6. FINAL GATE (allowlist-aware honest tally)"
$PY "$HERE/gate.py" --tier "$TIER" || fail "final gate (real divergence)"

T1=$(date +%s)
echo
echo "================================================================"
echo " tier=$TIER  wall=$((T1 - T0))s  ledger=$HERE/ledger.json"
if [ "$RC" -eq 0 ]; then
  echo " RESULT: GREEN -- Rust matches Go across the measured matrix (no"
  echo "         un-allowlisted divergence) and the harness is untampered."
else
  echo " RESULT: RED -- see failed stages above. The gap ledger lists every"
  echo "         divergence; this is the HONEST tally, not a suppressed green."
fi
echo "================================================================"
exit $RC
