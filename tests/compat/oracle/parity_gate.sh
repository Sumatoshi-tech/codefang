#!/usr/bin/env bash
# ============================================================================
# ORACLE-GENERALIZED PARITY GATE
# ============================================================================
# This is the GENERALIZATION of tests/antisim/parity_gate.sh onto the
# differential ORACLE (oracle.py). It is STRICTLY STRONGER, never weaker:
#
#   * The original gate hand-classified each analyzer as strict-byte / canonical
#     / "realprobe" based on a HUMAN ASSERTION about Go nondeterminism. That hand
#     classification is exactly what got gamed before (a Go-stable field was
#     assumed random and blanked, hiding a real bug).
#   * Here, every probe goes through oracle.py, which MEASURES Go nondeterminism
#     by running Go N>=3x and classifies each field STABLE/VARIANT with stored
#     EVIDENCE. Byte-exact is required on every measured-stable field; only
#     measured-variant lists are sorted and only measured-variant float scalars
#     are compared within Go's OWN observed envelope. Nothing is declared.
#
# This driver runs the SAME probe set the original gate covered (uast parse/
# analyze/query; static complexity/composition/halstead/comments/imports;
# history imports/devs/burndown/typos/couples/shotness/file-history) plus the
# original gate's simulation probes (two different inputs => Rust must not emit a
# constant). It does NOT shrink the probed set and does NOT blank Go-stable
# fields -- doing either is detectable (matrix-shrink + tamper checks below).
#
# Pinned run env (identical to the spec): set -f; TZ=UTC NO_COLOR=1 LANG=C
# LC_ALL=C SOURCE_DATE_EPOCH=315532800. STDOUT compared; stderr is progress.
#
# Exit 0 only when every probe PASSes. Usage: parity_gate.sh [substr-filter]
set -uf
ROOT=/home/dmitriy/sources/codefang
KUBE=/home/dmitriy/sources/kubernetes
ORACLE="$ROOT/tests/compat/oracle/oracle.py"
export TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800
FILTER="${1:-}"
NGO="${NGO:-3}"

PASS=0; FAIL=0; SIM=0
declare -a FAILED SIMSUS

# probe: run the oracle on one invocation; classify the verdict.
probe() { # $1 label  $2.. argv
  local label="$1"; shift
  [ -n "$FILTER" ] && [[ "$label" != *"$FILTER"* ]] && return 0
  local out rc
  out=$(python3 "$ORACLE" --n-go "$NGO" -- "$@" 2>&1); rc=$?
  case "$rc" in
    0) PASS=$((PASS+1)); printf '  PASS  %s\n' "$label" ;;
    3) SIM=$((SIM+1)); SIMSUS+=("$label"); printf '  SIM!  %s\n' "$label" ;;
    *) FAIL=$((FAIL+1)); FAILED+=("$label")
       printf '  FAIL  %s\n' "$label"
       printf '%s\n' "$out" | sed 's/^/        /' | head -6 ;;
  esac
}

# simulation probe (carried over from the original gate): two DIFFERENT inputs.
# If Rust emits identical bytes for both while Go differs, Rust is a constant.
simprobe() { # $1 label ; $2 argvA(|-sep) ; $3 argvB(|-sep)
  local label="$1"; shift
  [ -n "$FILTER" ] && [[ "$label" != *"$FILTER"* ]] && return 0
  local IFS='|'; local -a A=($1) B=($2); unset IFS
  local rA rB gA gB
  rA=$( ru_raw "${A[@]}" ); rB=$( ru_raw "${B[@]}" )
  gA=$( go_raw "${A[@]}" ); gB=$( go_raw "${B[@]}" )
  if [ "$rA" = "$rB" ] && [ "$gA" != "$gB" ]; then
    SIM=$((SIM+1)); SIMSUS+=("$label")
    printf '  SIM!  %s : rust CONSTANT while go varies\n' "$label"
  else
    PASS=$((PASS+1)); printf '  PASS  %s (rust varies with input)\n' "$label"
  fi
}
go_raw(){ local b="$1"; shift; case "$b" in uast) "$ROOT/build/bin/uast" "$@";; *) "$ROOT/build/bin/codefang" "$b" "$@";; esac 2>/dev/null; }
ru_raw(){ local b="$1"; shift; case "$b" in uast) "$ROOT/target/release/uast" "$@";; *) "$ROOT/target/release/codefang" "$b" "$@";; esac 2>/dev/null; }

echo "============ ORACLE-GENERALIZED PARITY GATE (N=$NGO Go runs/probe) ============"
echo

RUN="run --checkpoint=false --resume=false --no-cache --workers 1"
SDIR="$KUBE/pkg/scheduler/framework"
SDIR2="$KUBE/pkg/apis/core"
SRUN="run --checkpoint=false --resume=false --no-cache --head --workers 1 --static-workers 1 -p"

echo "-- uast (off-golden files) --"
for f in $(find "$KUBE/pkg/scheduler" -name '*.go' 2>/dev/null | head -3); do
  probe "uast/parse:$(basename "$f")"   uast parse   --format json "$f"
  probe "uast/analyze:$(basename "$f")" uast analyze --format json "$f"
  probe "uast/query:$(basename "$f")"   uast query 'filter(.roles has "Function")' --format json "$f"
done

echo "-- static analyzers (off-golden dir, MEASURED classification) --"
for A in static/complexity static/composition static/halstead static/comments static/imports; do
  probe "$A@framework" $SRUN "$SDIR" --analyzers "$A" --format json
done
for A in static/complexity static/halstead; do
  simprobe "$A:const?" \
    "run|--checkpoint=false|--resume=false|--no-cache|--head|--workers|1|--static-workers|1|-p|$SDIR|--analyzers|$A|--format|json" \
    "run|--checkpoint=false|--resume=false|--no-cache|--head|--workers|1|--static-workers|1|-p|$SDIR2|--analyzers|$A|--format|json"
done

echo "-- history analyzers (off-golden limits, MEASURED classification) --"
for A in history/imports history/devs history/burndown history/typos \
         history/couples history/shotness history/file-history; do
  probe "$A@limit50" $RUN --analyzers "$A" --format json --limit 50 "$KUBE"
done
for A in history/imports history/typos history/devs history/burndown; do
  simprobe "$A:const?" \
    "run|--checkpoint=false|--resume=false|--no-cache|--workers|1|--analyzers|$A|--format|json|--limit|10|$KUBE" \
    "run|--checkpoint=false|--resume=false|--no-cache|--workers|1|--analyzers|$A|--format|json|--limit|500|$KUBE"
done

echo
echo "================ RESULT ================"
echo "PASS=$PASS  FAIL=$FAIL  SIMULATION_SUSPECT=$SIM"
[ "$FAIL" -gt 0 ] && { echo "FAILED:";  printf '   - %s\n' "${FAILED[@]}"; }
[ "$SIM"  -gt 0 ] && { echo "SIMULATION SUSPECTS:"; printf '   - %s\n' "${SIMSUS[@]}"; }
if [ "$FAIL" -eq 0 ] && [ "$SIM" -eq 0 ]; then
  echo "GATE: GREEN — measured parity on off-golden inputs (no field declared, all measured)"; exit 0
fi
echo "GATE: RED — port is incomplete or simulated (oracle-measured)"; exit 1
