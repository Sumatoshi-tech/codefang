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
RU=$ROOT/target/release
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

# canonicalizing differential check for analyzers whose Go JSON output is
# INTRINSICALLY NONDETERMINISTIC (Go map-iteration order in the issues list and
# in the "first average" status message — see golden MANIFEST.json, which marks
# static_comments.json / static_imports.json / static_halstead.json nonBinding
# for exactly this reason). A naive byte diff cannot pass because Go's OWN output
# varies run-to-run. Per the MANIFEST methodology we compare after CANONICALIZING:
# sort every section's `issues` array and neutralize the nondeterministic `status`
# message, then require byte-identical canonical JSON. This still fails on any
# real divergence (metrics, distribution, score, issue SET/values) and the
# simulation probes below still catch hardcoded constants.
canon_json() { # reads JSON on stdin, writes canonical JSON on stdout
  python3 -c '
import json,sys
def canon(o):
    if isinstance(o,dict):
        o={k:canon(v) for k,v in o.items()}
        # neutralize Go-nondeterministic "first average" status labels
        if "status" in o: o["status"]="<status>"
        if "message" in o: o["message"]="<message>"
        if "sections" in o and isinstance(o["sections"],list):
            for s in o["sections"]:
                if isinstance(s,dict) and isinstance(s.get("issues"),list):
                    s["issues"]=sorted(s["issues"],key=lambda x:json.dumps(x,sort_keys=True))
        # history/typos: ONLY the list ORDER is Go-nondeterministic (verified: 3
        # Go runs at identical args produce the SAME set AND the SAME commit
        # attribution, differing only in element order). Therefore we sort the
        # lists but DO NOT neutralize the commit — commit attribution is
        # deterministic in Go and a real port must match it. (An earlier version
        # of this gate wrongly blanked the commit, which masked a real Rust
        # commit-attribution bug. Do not reintroduce that.)
        if isinstance(o.get("typo_list"),list):
            o["typo_list"]=sorted(o["typo_list"],key=lambda x:json.dumps(x,sort_keys=True))
        if isinstance(o.get("file_typos"),list):
            o["file_typos"]=sorted(o["file_typos"],key=lambda x:json.dumps(x,sort_keys=True))
        if isinstance(o.get("patterns"),list):
            o["patterns"]=sorted(o["patterns"],key=lambda x:json.dumps(x,sort_keys=True))
        return o
    if isinstance(o,list): return [canon(x) for x in o]
    return o
try:
    d=json.load(sys.stdin)
except Exception:
    sys.exit(2)
json.dump(canon(d),sys.stdout,sort_keys=True,separators=(",",":"))
'
}
diffcheck_canon() { # $1 label  $2.. argv  — compare canonical JSON (Go-nondet ok)
  local label="$1"; shift
  [ -n "$FILTER" ] && [[ "$label" != *"$FILTER"* ]] && return 0
  local graw rraw g r
  graw=$(go_out "$@"); rraw=$(ru_out "$@")
  g=$(printf '%s' "$graw" | canon_json); r=$(printf '%s' "$rraw" | canon_json)
  if [ "$g" = "$r" ] && [ -n "$g" ]; then
    PASS=$((PASS+1)); printf '  PASS  %s (%dB, canonical)\n' "$label" "${#graw}"
  else
    FAIL=$((FAIL+1)); FAILED+=("$label")
    printf '  FAIL  %s : canonical JSON differs (go=%dB rust=%dB)\n' "$label" "${#graw}" "${#rraw}"
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

# REAL-COMPUTATION probe for analyzers whose Go output is INTRINSICALLY
# CONTENT-NONDETERMINISTIC (not merely byte order) and therefore CANNOT be byte-
# diffed against Go — the recorded golden is itself non-reproducible. These are
# exactly the run captures the MANIFEST marks nonBinding/stable=false:
# history/shotness, history/couples, history/file-history. (For shotness the Go
# streaming pipeline never assigns stable node IDs, so reverseNodeMap collapses
# on the empty id and the SELECTED NODE SET is Go-map-order random run-to-run —
# two Go runs at the same args produce disjoint node sets of differing size. No
# deterministic port can match those bytes; canonicalization does not help
# because the *set* differs, not just the order.)
#
# A byte diff is impossible, so "done" for these is proven structurally:
#   (1) Rust output is NON-EMPTY and grows with --limit (NOT a hardcoded
#       constant — the simulation signature), proven on two off-golden limits;
#   (2) Rust is itself DETERMINISTIC (same args ⇒ identical bytes twice), the
#       correctness property a real port has and Go lacks; and
#   (3) Go also produces non-empty, growing output (sanity: the analyzer does
#       compute something at these inputs).
# This FAILS a 0-byte stub (not ported) and a constant stub (faked) while not
# demanding byte parity against an irreproducible Go reference.
realprobe() { # $1 label  $2 analyzerId
  local label="$1" aid="$2"
  [ -n "$FILTER" ] && [[ "$label" != *"$FILTER"* ]] && return 0
  local r10a r10b r500 g50 g50b
  r10a=$(ru_out run --checkpoint=false --resume=false --no-cache --workers 1 --analyzers "$aid" --format json --limit 10  "$KUBE")
  r10b=$(ru_out run --checkpoint=false --resume=false --no-cache --workers 1 --analyzers "$aid" --format json --limit 10  "$KUBE")
  r500=$(ru_out run --checkpoint=false --resume=false --no-cache --workers 1 --analyzers "$aid" --format json --limit 500 "$KUBE")
  g50=$(go_out  run --checkpoint=false --resume=false --no-cache --workers 1 --analyzers "$aid" --format json --limit 50  "$KUBE")
  # (1) non-empty + grows with limit (not a constant / not a 0-byte stub)
  if [ -z "$r10a" ] || [ "$r10a" = "$r500" ]; then
    FAIL=$((FAIL+1)); FAILED+=("$label")
    printf '  FAIL  %s : rust empty-or-constant (limit10=%dB limit500=%dB)\n' "$label" "${#r10a}" "${#r500}"
    return 0
  fi
  # (2) Rust deterministic on identical args (real port property)
  if [ "$r10a" != "$r10b" ]; then
    FAIL=$((FAIL+1)); FAILED+=("$label")
    printf '  FAIL  %s : rust NONDETERMINISTIC across runs (not a faithful port)\n' "$label"
    return 0
  fi
  # (3) Go computes something here too (sanity)
  if [ -z "$g50" ]; then
    FAIL=$((FAIL+1)); FAILED+=("$label")
    printf '  FAIL  %s : go produced no output at limit50 (probe invalid)\n' "$label"
    return 0
  fi
  PASS=$((PASS+1))
  printf '  PASS  %s (REAL: rust grows %dB->%dB, deterministic; go=%dB nondet golden)\n' \
    "$label" "${#r10a}" "${#r500}" "${#g50}"
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
# complexity / composition are fully deterministic in Go ⇒ strict byte diff.
for A in static/complexity static/composition; do
  diffcheck "$A@framework" $SRUN "$SDIR" --analyzers $A --format json
done
# halstead / comments / imports JSON are Go-nondeterministic (map-iteration order
# in issues + "first average" status); compare after canonicalization. The bin /
# yaml BINDING formats for these analyzers are byte-exact (verified separately).
for A in static/halstead static/comments static/imports; do
  diffcheck_canon "$A@framework" $SRUN "$SDIR" --analyzers $A --format json
done
# simulation probes: same analyzer, two different dirs
SDIR2="$KUBE/pkg/apis/core"
for A in static/complexity static/halstead; do
  simprobe "$A:const?" \
    "run|--checkpoint=false|--resume=false|--no-cache|--head|--workers|1|--static-workers|1|-p|$SDIR|--analyzers|$A|--format|json" \
    "run|--checkpoint=false|--resume=false|--no-cache|--head|--workers|1|--static-workers|1|-p|$SDIR2|--analyzers|$A|--format|json"
done

echo "-- history analyzers (off-golden limits) --"
# DETERMINISTIC Go output ⇒ strict byte diff (MANIFEST binding/stable=true).
for A in history/imports history/devs history/burndown; do
  diffcheck "$A@limit50" $RUN --analyzers $A --format json --limit 50 "$KUBE"
done
# history/typos: Go output is ORDER- and COMMIT-ATTRIBUTION-nondeterministic
# (two Go runs at identical args disagree on typo_list/file_typos order AND on
# which commit each typo-fix is credited to — Go reorders commits before the
# aggregator and the within-tick first-seen dedup winner is map/goroutine-order
# random). A strict byte diff is therefore impossible. Compare CANONICALLY
# (commit neutralized, lists sorted): this still fails on any real divergence in
# the typo SET, files, lines, counts, or aggregates, while not demanding parity
# with Go`s intrinsic nondeterminism. The simprobe below still catches a
# hardcoded constant.
diffcheck_canon "history/typos@limit50" $RUN --analyzers history/typos --format json --limit 50 "$KUBE"
# INTRINSICALLY CONTENT-NONDETERMINISTIC Go output (MANIFEST nonBinding/
# stable=false) ⇒ a byte diff against Go is impossible; prove REAL computation
# structurally (non-empty, grows with --limit, Rust deterministic). See realprobe.
for A in history/couples history/shotness history/file-history; do
  realprobe "$A@real" "$A"
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
