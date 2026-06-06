#!/usr/bin/env bash
# =============================================================================
# MUTATION SELF-TEST  --  THE META-GATE
# (SPEC: specs/go-compat-testing/SPEC.md §4 Testing-Strategy "E2E / self-test",
#  roadmap 7; task brief PHASE: TamperProof, requirement #6 + integrity #2.)
# =============================================================================
# The compat system is only trustworthy if it PROVABLY catches both
#   (1) a PRODUCT bug  -- a real behavioral defect in a Rust analyzer, and
#   (2) a HARNESS cheat -- a tamper that blanks a Go-STABLE field to hide a bug.
# A green that cannot be shown to go red on a planted defect is worthless.
#
# This script is a mutation-testing-style meta-test.
#
#   PHASE A (product bug):
#     A0. Pick a probe cell that is GREEN at baseline (Rust already matches Go) so
#         the bug signal is isolable. We use `uast parse` of the Go corpus file --
#         a deterministic, Go-byte-stable invocation the live oracle confirms PASS.
#         (The full matrix may contain other already-divergent cells from the WIP
#         port; those are tracked by run_matrix/ledger, NOT by this meta-test. We
#         do NOT shrink the matrix -- matrix-shrink protection lives in
#         tamper_check.py. Here we deliberately isolate ONE baseline-green cell so
#         "red" can only mean "the planted bug was detected".)
#     A1. Inject a deliberate behavioral bug: perturb the Go-STABLE `end_col`
#         position metric in cf-uast-node (end_col -> end_col + 1).
#     A2. Rebuild the Rust `uast` binary.
#     A3. Run the LIVE oracle (Go is the source of truth, never re-derived) on the
#         probe cell and ASSERT it now reports FAIL. If a planted Go-stable metric
#         bug slips through GREEN, the whole compat system is untrustworthy.
#     A4. Revert + rebuild + ASSERT the probe cell is GREEN again (proves the FAIL
#         was caused by the bug, not by a stuck-red gate).
#
#   PHASE B (harness cheat):
#     B1. Copy the real oracle and TAMPER the copy to BLANK a Go-STABLE field
#         (`end_col`) -- the exact historic cheat -- AND disable the stable-leaf
#         guard that would otherwise still catch it (a full canonicalizer
#         weakening).
#     B2. Drive the tampered-copy oracle on a Rust output WRONG only on that
#         stable field and ASSERT the tampered oracle WRONGLY returns PASS while
#         the REAL oracle returns FAIL -- demonstrating the cheat hides the bug.
#     B3. ASSERT the tamper-evidence checker DETECTS the canonicalizer weakening
#         and fails CLOSED (both via its self-test and end-to-end with the
#         tampered oracle swapped into place).
#
# Everything mutates a COPY or is reverted under a trap, so the real port and the
# real harness are restored even on error/interrupt.
set -u
ROOT=/home/dmitriy/sources/codefang
RUST=$ROOT/rust
COMPAT=$RUST/tests/compat
INTEG=$COMPAT/integrity
ORACLE=$COMPAT/oracle/oracle.py
# The Rust analyzer source that serializes `uast parse` JSON positions -- the
# code path the probe cell actually exercises (verified: this is where end_col is
# emitted for `uast parse`, not the unused tomap.rs helper).
TARGET=$RUST/bins/uast/src/govalue_bridge.rs
GO_FILE="$COMPAT/corpus/files/44740f6a69ae1eb14a52794bc2543aebf0612d8319ae3c20ea124211c6297fbc.go"
export TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800

BACKUP="$(mktemp)"
TMPDIR_B=""
FAILS=0
ok()   { printf '  OK   %s\n' "$1"; }
bad()  { printf '  XX   %s\n' "$1"; FAILS=$((FAILS+1)); }

cleanup() {
  if [ -f "$BACKUP" ]; then
    cp "$BACKUP" "$TARGET" 2>/dev/null || true
    rm -f "$BACKUP"
  fi
  [ -n "$TMPDIR_B" ] && rm -rf "$TMPDIR_B"
  ( cd "$RUST" && cargo build --release --bin uast >/dev/null 2>&1 ) || true
}
trap cleanup EXIT INT TERM

# Probe the ONE baseline-green cell through the LIVE oracle. Returns oracle exit
# code: 0=PASS, 1=FAIL, 3=SIM. stdout of the oracle is captured to $1.
probe_cell() {
  python3 "$ORACLE" --n-go 3 --quiet -- \
      uast parse --format json "$GO_FILE" >"$1" 2>&1
  return $?
}

echo "================ MUTATION SELF-TEST (META-GATE) ================"
echo "Target Rust analyzer source: $TARGET"
echo "Probe cell (baseline-green): uast parse --format json <go corpus file>"
echo

echo "-- baseline rebuild (ensure clean starting binary) --"
if ! ( cd "$RUST" && cargo build --release --bin uast >/dev/null 2>&1 ); then
  echo "FATAL: baseline build failed; cannot run mutation test"; exit 2
fi
cp "$TARGET" "$BACKUP"

# -----------------------------------------------------------------------------
# PHASE A
# -----------------------------------------------------------------------------
echo
echo "== PHASE A: inject a behavioral bug, prove the probe cell goes RED =="

PRE_LOG="$(mktemp)"
probe_cell "$PRE_LOG"; pre_rc=$?
if [ "$pre_rc" -eq 0 ]; then
  ok "A0 baseline probe cell is GREEN via live oracle (control)"
else
  bad "A0 baseline probe cell NOT green (oracle rc=$pre_rc) -- cannot isolate \
the bug signal. Oracle said:"; sed 's/^/        /' "$PRE_LOG" | head -4
fi

# A1: perturb a Go-STABLE metric.
python3 - "$TARGET" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
needle = '("end_col".to_string(), GoValue::Uint(pos.end_col)),'
repl   = '("end_col".to_string(), GoValue::Uint(pos.end_col + 1)), // MUTATION-SELFTEST'
assert needle in s, "mutation anchor not found in " + p
open(p, "w").write(s.replace(needle, repl, 1))
print("injected: end_col -> end_col + 1")
PY

echo "-- rebuilding Rust uast with the injected bug --"
if ! ( cd "$RUST" && cargo build --release --bin uast >/dev/null 2>&1 ); then
  bad "A2 mutated build failed to compile (cannot assess detection)"
else
  ok "A2 mutated binary built"
  MUT_LOG="$(mktemp)"
  probe_cell "$MUT_LOG"; mut_rc=$?
  if [ "$mut_rc" -ne 0 ]; then
    ok "A3 oracle DETECTED the planted Go-stable bug (verdict via rc=$mut_rc)"
    sed 's/^/        /' "$MUT_LOG" | head -4
  else
    bad "A3 oracle stayed GREEN despite a planted Go-stable metric bug -- \
THE SYSTEM IS UNTRUSTWORTHY"
    sed 's/^/        /' "$MUT_LOG" | head -4
  fi
  rm -f "$MUT_LOG"
fi

echo "-- reverting source and rebuilding --"
cp "$BACKUP" "$TARGET"
if ! ( cd "$RUST" && cargo build --release --bin uast >/dev/null 2>&1 ); then
  bad "A4 revert build failed (tree left in a bad state -- investigate)"
else
  POST_LOG="$(mktemp)"
  probe_cell "$POST_LOG"; post_rc=$?
  if [ "$post_rc" -eq 0 ]; then
    ok "A4 probe cell GREEN again after revert (the RED was bug-driven, not stuck)"
  else
    bad "A4 probe cell still RED after revert -- gate is stuck-red, not bug-driven"
    sed 's/^/        /' "$POST_LOG" | head -4
  fi
  rm -f "$POST_LOG"
fi
rm -f "$PRE_LOG"

# -----------------------------------------------------------------------------
# PHASE B
# -----------------------------------------------------------------------------
echo
echo "== PHASE B: inject a HARNESS cheat (blank a Go-stable field), prove fail-closed =="

TMPDIR_B="$(mktemp -d)"
cp "$ORACLE" "$TMPDIR_B/oracle.py"

# B1: tamper the COPY -- full canonicalizer weakening that blanks the Go-stable
# `end_col` field (the historic cheat). To actually HIDE the bug we must defeat
# BOTH compare() paths: (i) the whole-structure canonical equality (handled by
# blanking in canonicalize), and (ii) the explicit stable-leaf guard (we make it
# skip any path ending in end_col). This is the maximal, realistic weakening.
python3 - "$TMPDIR_B/oracle.py" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()

# (i) blank end_col inside canonicalize().walk for dicts
anchor1 = '    def walk(o, prefix="$"):\n        if isinstance(o, dict):\n'
inject1 = ('    def walk(o, prefix="$"):\n        if isinstance(o, dict):\n'
           '            o = {k: v for k, v in o.items() if k != "end_col"}  '
           '# HARNESS-CHEAT: blank a Go-stable field\n')
assert anchor1 in s, "canonicalize anchor not found"
s = s.replace(anchor1, inject1, 1)

# (ii) neuter the explicit stable-leaf guard so it ignores end_col paths
anchor2 = '    for path, cls in classification.items():\n        if cls != "STABLE":\n            continue\n'
inject2 = ('    for path, cls in classification.items():\n        if cls != "STABLE":\n            continue\n'
           '        if path.endswith("end_col"):  # HARNESS-CHEAT: skip stable guard\n            continue\n')
assert anchor2 in s, "stable-leaf guard anchor not found"
s = s.replace(anchor2, inject2, 1)

open(p, "w").write(s)
print("tampered copy: canonicalizer blanks Go-stable end_col AND skips its stable guard")
PY

# B2: demonstrate the cheat HIDES the bug. tampered=PASS, real=FAIL.
python3 - "$TMPDIR_B/oracle.py" "$ORACLE" <<'PY'
import importlib.util, json, sys

def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m); return m

tampered = load(sys.argv[1], "tampered")
real     = load(sys.argv[2], "realo")

# Go-stable doc; Rust differs ONLY on end_col (the exact historic-bug shape).
GO   = {"type":"File","children":[{"type":"Id","pos":{"start_line":1,"end_col":13}}]}
RUST = {"type":"File","children":[{"type":"Id","pos":{"start_line":1,"end_col":99}}]}

class Bins:
    def __init__(self, go, rust):
        self.go=[(0,json.dumps(go).encode())]*3
        self.rust=[(0,json.dumps(rust).encode())]*2
    def __call__(self, side, argv):
        return (self.go if side=="go" else self.rust).pop(0)

def verdict(mod):
    orig = mod.run_once
    mod.run_once = Bins(GO, RUST)
    try:
        return mod.run_invocation(["uast","parse","--format","json","/x"], n_go=3)["verdict"]
    finally:
        mod.run_once = orig

vt = verdict(tampered); vr = verdict(real)
print(f"tampered-oracle verdict on end_col bug = {vt}")
print(f"real-oracle     verdict on end_col bug = {vr}")
# Dangerous iff the cheat turns a real FAIL into a PASS.
sys.exit(0 if (vt == "PASS" and vr == "FAIL") else 1)
PY
cheat_rc=$?
if [ "$cheat_rc" -eq 0 ]; then
  ok "B2 confirmed the cheat HIDES the bug (tampered oracle=PASS, real oracle=FAIL)"
else
  bad "B2 could not demonstrate the cheat hiding the bug (tampered did not flip PASS)"
fi

# B3: integrity layer must detect canonicalizer weakening (self-test proof).
echo "-- integrity layer must detect canonicalizer weakening (fail-closed) --"
if python3 "$INTEG/tamper_check.py" --self-test >/dev/null 2>&1; then
  ok "B3 tamper_check self-test GREEN (file-modify + matrix-shrink + canonicalizer-weakening all caught)"
else
  bad "B3 tamper_check self-test FAILED -- integrity layer does not catch the cheat"
fi

# B3b: end-to-end -- swap the tampered copy in as the LIVE oracle, run the real
# tamper_check verify(), assert it goes RED (fail-closed), then restore.
echo "-- end-to-end: live oracle replaced with tampered copy must fail-closed --"
ORACLE_BK="$(mktemp)"; cp "$ORACLE" "$ORACLE_BK"
cp "$TMPDIR_B/oracle.py" "$ORACLE"
python3 "$INTEG/tamper_check.py" >/dev/null 2>&1; ttc_rc=$?
cp "$ORACLE_BK" "$ORACLE"; rm -f "$ORACLE_BK"
if [ "$ttc_rc" -ne 0 ]; then
  ok "B3b live tamper_check FAILED-CLOSED on the swapped/tampered oracle (rc=$ttc_rc)"
else
  bad "B3b live tamper_check stayed GREEN with a tampered oracle in place -- NOT fail-closed"
fi

echo
echo "================ META-GATE RESULT ================"
if [ "$FAILS" -eq 0 ]; then
  echo "META-GATE GREEN: compat PROVABLY catches a product bug (probe red->green->red)"
  echo "AND the integrity layer PROVABLY fails-closed on a harness cheat (blanked"
  echo "Go-stable field). Both a product defect and a harness cheat are caught."
  exit 0
fi
echo "META-GATE RED: $FAILS assertion(s) failed -- the compat system is NOT proven"
echo "to catch the planted defect(s). DO NOT TRUST a green compat run until fixed."
exit 1
