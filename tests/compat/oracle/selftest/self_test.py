#!/usr/bin/env python3
"""
ORACLE SELF-PROOF  (non-negotiable rule #6: every component must SELF-PROVE it
catches a defect). A green that cannot be shown to catch a planted bug is
worthless. This harness PLANTS each defect class and asserts the oracle reports
the correct non-PASS verdict.

We exercise the REAL oracle decision logic (oracle.run_invocation) but feed it
controlled binary outputs by monkeypatching oracle.run_once -- the binaries are
the oracle, so faking the binary outputs is the honest way to plant a defect
without corrupting the real port. Each planted defect mirrors a real failure mode:

  T1 GO-STABLE divergence      Rust returns a wrong value on a Go-stable field   -> FAIL
  T2 hardcoded-constant stub   Rust emits the same bytes regardless of input
                               while Go varies (the simulation signature)        -> SIM
  T3 TAMPER (blank stable)     caller asks to normalize a Go-STABLE field
                               (the exact cheat that hid a real bug)             -> FAIL/TAMPER
  T4 Rust nondeterminism       two identical-arg Rust runs differ                -> FAIL
  T5 dropped Go-stable field   Rust omits a field Go always emits                -> FAIL
  T6 honest order-only nondet  Go varies only in list ORDER; Rust matches the
                               canonical content                                 -> PASS (control)
  T7 NEGATIVE control          everything agrees                                 -> PASS

Also includes a CONTROL proving the oracle does NOT cry wolf: a genuinely
order-only Go-variant case where Rust is canonically correct must PASS, so the
FAIL verdicts above are meaningful, not a stuck "always FAIL".
"""

import importlib.util
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ORACLE_PY = os.path.join(HERE, "..", "oracle.py")

spec = importlib.util.spec_from_file_location("oracle", ORACLE_PY)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)


class FakeBins:
    """Drives oracle.run_once with scripted Go/Rust outputs per side."""
    def __init__(self, go_seq, rust_seq):
        # go_seq / rust_seq: lists of (rc, bytes) consumed in call order
        self.go = list(go_seq)
        self.rust = list(rust_seq)
        self.extra_rust = []   # for structural grow-probe at raised limit

    def __call__(self, side, argv):
        if side == "go":
            return self.go.pop(0)
        # rust: if the grow-probe raised the limit, serve from extra_rust
        if "--limit" in argv and self.extra_rust and self._is_raised(argv):
            return self.extra_rust.pop(0)
        return self.rust.pop(0)

    @staticmethod
    def _is_raised(argv):
        return True  # self-tests that use structural path set extra_rust explicitly


def b(o):
    return json.dumps(o).encode()


RESULTS = []


def expect(name, got_verdict, want_verdict, res):
    ok = got_verdict == want_verdict
    RESULTS.append((name, ok, got_verdict, want_verdict))
    flag = "OK " if ok else "XX "
    print(f"  {flag}{name}: got {got_verdict}, want {want_verdict}"
          + ("" if ok else f"   <-- SELF-TEST FAILURE  reason={res.get('reason','')}"))
    return ok


def run(go_runs, rust_runs, normalize=None, argv=None, extra_rust=None):
    argv = argv or ["run", "--analyzers", "x", "--format", "json",
                    "--limit", "10", "/repo"]
    fb = FakeBins(go_runs, rust_runs)
    if extra_rust:
        fb.extra_rust = list(extra_rust)
    orig = oracle.run_once
    oracle.run_once = fb
    try:
        return oracle.run_invocation(argv, n_go=3, normalize=normalize or [])
    finally:
        oracle.run_once = orig


def main():
    print("================ ORACLE SELF-PROOF ================")

    # Shared "Go-stable" document.
    STABLE_DOC = {"score": 7, "name": "alpha", "items": [1, 2, 3]}

    # ---- T7 NEGATIVE control: everything agrees -> PASS ----
    res = run(
        go_runs=[(0, b(STABLE_DOC))] * 3,
        rust_runs=[(0, b(STABLE_DOC))] * 2,
    )
    expect("T7 all-agree", res["verdict"], "PASS", res)

    # ---- T1 GO-STABLE divergence: Rust wrong on a stable field -> FAIL ----
    WRONG = dict(STABLE_DOC, score=999)
    res = run(
        go_runs=[(0, b(STABLE_DOC))] * 3,
        rust_runs=[(0, b(WRONG))] * 2,
    )
    expect("T1 stable-field-divergence", res["verdict"], "FAIL", res)

    # ---- T5 dropped Go-stable field -> FAIL ----
    DROPPED = {"name": "alpha", "items": [1, 2, 3]}   # 'score' removed
    res = run(
        go_runs=[(0, b(STABLE_DOC))] * 3,
        rust_runs=[(0, b(DROPPED))] * 2,
    )
    expect("T5 dropped-stable-field", res["verdict"], "FAIL", res)

    # ---- T4 Rust nondeterminism: two Rust runs differ -> FAIL ----
    res = run(
        go_runs=[(0, b(STABLE_DOC))] * 3,
        rust_runs=[(0, b(STABLE_DOC)), (0, b(WRONG))],
    )
    expect("T4 rust-nondeterministic", res["verdict"], "FAIL", res)

    # ---- T3 TAMPER: ask to normalize a Go-STABLE field -> FAIL/TAMPER ----
    res = run(
        go_runs=[(0, b(STABLE_DOC))] * 3,
        rust_runs=[(0, b(WRONG))] * 2,
        normalize=["$.score"],   # score is Go-stable; blanking it is the cheat
    )
    ok = expect("T3 tamper-blank-stable", res["verdict"], "FAIL", res)
    if ok and "TAMPER" not in res.get("reason", ""):
        RESULTS.append(("T3 tamper-reason", False, res.get("reason"), "TAMPER"))
        print("  XX T3 tamper-reason: reason did not mention TAMPER")

    # ---- T6 honest order-only Go nondeterminism: Rust canonically correct -> PASS ----
    # Go varies only list ORDER across runs; the SET is identical every run, so
    # canonicalization makes Go canonical-stable and Rust (a permutation) PASSes.
    GO_A = {"meta": "k", "items": [{"v": 3}, {"v": 1}, {"v": 2}]}
    GO_B = {"meta": "k", "items": [{"v": 1}, {"v": 2}, {"v": 3}]}
    GO_C = {"meta": "k", "items": [{"v": 2}, {"v": 3}, {"v": 1}]}
    RUST = {"meta": "k", "items": [{"v": 1}, {"v": 3}, {"v": 2}]}  # another perm, same set
    res = run(
        go_runs=[(0, b(GO_A)), (0, b(GO_B)), (0, b(GO_C))],
        rust_runs=[(0, b(RUST))] * 2,
    )
    expect("T6 order-only-nondet-canonical-pass", res["verdict"], "PASS", res)

    # ---- T6b order-only nondet BUT Rust has WRONG content -> FAIL ----
    # Same list paths are Go-variant, but Rust's SET differs (a 4 instead of a 3):
    # canonicalization must NOT hide this -> FAIL. Proves we don't over-neutralize.
    RUST_BAD = {"meta": "k", "items": [{"v": 1}, {"v": 4}, {"v": 2}]}
    res = run(
        go_runs=[(0, b(GO_A)), (0, b(GO_B)), (0, b(GO_C))],
        rust_runs=[(0, b(RUST_BAD))] * 2,
    )
    expect("T6b variant-list-content-bug-still-caught", res["verdict"], "FAIL", res)

    # ---- T2 hardcoded-constant stub (content-nondet Go => structural realprobe) ----
    # Go is CONTENT-nondeterministic (member set differs every run) so byte/canonical
    # parity is impossible and the oracle uses the structural realprobe. A constant
    # Rust stub returns the SAME bytes at the raised limit => CONSTANT signature
    # => FAIL. This is the simulation defect the realprobe must catch.
    GO_S1 = {"nodes": [{"n": "a"}, {"n": "b"}]}
    GO_S2 = {"nodes": [{"n": "c"}, {"n": "d"}]}
    GO_S3 = {"nodes": [{"n": "e"}]}
    CONST = {"nodes": [{"n": "CONST"}]}
    res = run(
        go_runs=[(0, b(GO_S1)), (0, b(GO_S2)), (0, b(GO_S3))],
        rust_runs=[(0, b(CONST))] * 2,         # limit=10 runs
        extra_rust=[(0, b(CONST)), (0, b(CONST))],  # raised-limit grow-probe: SAME
    )
    expect("T2 constant-stub-structural", res["verdict"], "FAIL", res)

    # ---- T2b structural HONEST: Rust grows with limit + deterministic -> PASS ----
    GROWN = {"nodes": [{"n": "e"}, {"n": "f"}, {"n": "g"}]}
    res = run(
        go_runs=[(0, b(GO_S1)), (0, b(GO_S2)), (0, b(GO_S3))],
        rust_runs=[(0, b(CONST))] * 2,
        extra_rust=[(0, b(GROWN)), (0, b(GROWN))],  # grows at raised limit, det.
    )
    expect("T2b honest-growing-structural", res["verdict"], "PASS", res)

    # ---- T8 empty-Rust stub on content-nondet Go -> FAIL ----
    res = run(
        go_runs=[(0, b(GO_S1)), (0, b(GO_S2)), (0, b(GO_S3))],
        rust_runs=[(0, b"")] * 2,
        extra_rust=[(0, b"")] * 2,
    )
    expect("T8 empty-rust-stub-structural", res["verdict"], "FAIL", res)

    print("==================================================")
    failed = [r for r in RESULTS if not r[1]]
    n = len(RESULTS)
    print(f"SELF-PROOF: {n - len(failed)}/{n} checks correct")
    if failed:
        print("SELF-PROOF FAILED -- the oracle did NOT catch a planted defect:")
        for name, _, got, want in failed:
            print(f"   - {name}: got {got}, want {want}")
        sys.exit(1)
    print("SELF-PROOF GREEN: oracle provably catches divergence, constants, "
          "tamper, dropped fields, and nondeterminism.")
    sys.exit(0)


if __name__ == "__main__":
    main()
