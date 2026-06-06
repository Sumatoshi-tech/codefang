#!/usr/bin/env bash
# ANTI-SIMULATION PARITY GATE
# ===========================
# "Done" must mean: the Rust port reproduces the Go binary on inputs the GOLDEN
# NEVER SAW — not just the recorded golden args. This gate runs every analyzer on
# OFF-GOLDEN inputs (different dirs, different limits, multiple repos) and:
#   (1) FAILs if Rust output != Go output, and
#   (2) FLAGs "SIMULATION SUSPECT" if Rust emits the SAME bytes for two different
#       inputs while Go does not (the hardcoded-constant signature).
#
# Exit 0 only when every probed analyzer matches Go on every off-golden input.
# Usage: parity_gate.sh [analyzer-substring-filter]
set -u
ROOT=/home/dmitriy/sources/codefang
GO=$ROOT/build/bin
RU=$ROOT/rust/target/release
KUBE=/home/dmitriy/sources/kubernetes
export TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800
FILTER="${1:-}"

PASS=0; FAIL=0; SIM=0
declare -a FAILED SIMSUS

# run a codefang/uast invocation, echo stdout, suppress stderr.
# $1 is the binary name (codefang|uast|run...), but our call sites pass the
# subcommand as $1 (e.g. "uast" or "run"); resolve the actual binary from it.
go_out()  { local bin="$1"; shift; case "$bin" in uast) "$GO/uast" "$@";; *) "$GO/codefang" "$bin" "$@";; esac 2>/dev/null; }
ru_out()  { local bin="$1"; shift; case "$bin" in uast) "$RU/uast" "$@";; *) "$RU/codefang" "$bin" "$@";; esac 2>/dev/null; }

# differential check: same command must give identical bytes Go vs Rust
diffcheck() { # $1 label  $2.. argv
  local label="$1"; shift
  [ -n "$FILTER" ] && [[ "$label" != *"$FILTER"* ]] && return 0
  local g r
  g=$(go_out "$@"); r=$(ru_out "$@")
  if [ "$g" = "$r" ] && [ -n "$g" ]; then
    PASS=$((PASS+1)); printf '  PASS  %s (%dB)\n' "$label" "${#g}"
  else
    FAIL=$((FAIL+1)); FAILED+=("$label")
    printf '  FAIL  %s : go=%dB rust=%dB\n' "$label" "${#g}" "${#r}"
  fi
}

# simulation probe: two DIFFERENT inputs; if Rust gives identical bytes for both
# but Go gives different bytes => Rust is emitting a constant (faked).
simprobe() { # $1 label  $2 argvA(|-sep)  $3 argvB(|-sep)
  local label="$1"; shift
  [ -n "$FILTER" ] && [[ "$label" != *"$FILTER"* ]] && return 0
  local IFS='|'; local -a A=($1) B=($2); unset IFS
  local rA rB gA gB
  rA=$(ru_out "${A[@]}"); rB=$(ru_out "${B[@]}")
  gA=$(go_out "${A[@]}"); gB=$(go_out "${B[@]}")
  if [ "$rA" = "$rB" ] && [ "$gA" != "$gB" ]; then
    SIM=$((SIM+1)); SIMSUS+=("$label")
    printf '  SIM!  %s : rust CONSTANT (%dB==%dB) while go varies (%dB!=%dB)\n' \
      "$label" "${#rA}" "${#rB}" "${#gA}" "${#gB}"
  fi
}

echo "================ ANTI-SIMULATION PARITY GATE ================"
echo "(differential vs Go on OFF-GOLDEN inputs + constant-output probes)"
echo

RUN="run --checkpoint=false --resume=false --no-cache --workers 1"

echo "-- uast (off-golden files) --"
for f in $(find "$KUBE/pkg/scheduler" -name '*.go' 2>/dev/null | head -3); do
  diffcheck "uast/parse:$(basename "$f")"   uast parse   --format json "$f"
  diffcheck "uast/analyze:$(basename "$f")" uast analyze --format json "$f"
  diffcheck "uast/query:$(basename "$f")"   uast query 'filter(.roles has "Function")' --format json "$f"
done

echo "-- static analyzers (off-golden dir) --"
SDIR="$KUBE/pkg/scheduler/framework"
SRUN="run --checkpoint=false --resume=false --no-cache --head --workers 1 --static-workers 1 -p"
for A in static/complexity static/composition static/halstead static/comments static/imports; do
  diffcheck "$A@framework" $SRUN "$SDIR" --analyzers $A --format json
done
# simulation probes: same analyzer, two different dirs
SDIR2="$KUBE/pkg/apis/core"
for A in static/complexity static/halstead; do
  simprobe "$A:const?" \
    "run|--checkpoint=false|--resume=false|--no-cache|--head|--workers|1|--static-workers|1|-p|$SDIR|--analyzers|$A|--format|json" \
    "run|--checkpoint=false|--resume=false|--no-cache|--head|--workers|1|--static-workers|1|-p|$SDIR2|--analyzers|$A|--format|json"
done

echo "-- history analyzers (off-golden limits) --"
for A in history/imports history/typos history/devs history/burndown history/couples history/shotness history/file-history; do
  diffcheck "$A@limit50" $RUN --analyzers $A --format json --limit 50 "$KUBE"
done
# simulation probes: limit 10 vs limit 500 — output MUST grow for real analyzers
for A in history/imports history/typos history/devs history/burndown; do
  simprobe "$A:const?" \
    "run|--checkpoint=false|--resume=false|--no-cache|--workers|1|--analyzers|$A|--format|json|--limit|10|$KUBE" \
    "run|--checkpoint=false|--resume=false|--no-cache|--workers|1|--analyzers|$A|--format|json|--limit|500|$KUBE"
done

echo
echo "================ RESULT ================"
echo "PASS=$PASS  FAIL=$FAIL  SIMULATION_SUSPECT=$SIM"
[ "$FAIL" -gt 0 ] && { echo "FAILED:";  printf '   - %s\n' "${FAILED[@]}"; }
[ "$SIM"  -gt 0 ] && { echo "SIMULATION SUSPECTS (hardcoded constants):"; printf '   - %s\n' "${SIMSUS[@]}"; }
if [ "$FAIL" -eq 0 ] && [ "$SIM" -eq 0 ]; then echo "GATE: GREEN — real parity on off-golden inputs"; exit 0; fi
echo "GATE: RED — port is incomplete or simulated"; exit 1
