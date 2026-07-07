#!/usr/bin/env python3
"""
METAMORPHIC SELF-PROOF  (non-negotiable rule #6: every component must SELF-PROVE
it catches a defect). A green that cannot be shown to catch a planted bug is
worthless. This harness PLANTS each simulation/relational defect class and
asserts the metamorphic layer reports the correct non-PASS verdict.

We exercise the REAL metamorphic decision functions (metamorphic.check_*) but
feed them scripted Go/Rust binary outputs by monkeypatching the LIVE-binary
launcher (oracle.run_once, which metamorphic imports as run_once). Faking the
binary outputs is the honest way to plant a defect without corrupting the real
port: the binaries ARE the oracle, so a controlled fake oracle lets us prove the
relational logic catches each stub class.

Planted defects (mirror the simulation failure modes the SPEC warns about):

  P-a  VARY-INPUT constant   Rust same bytes for 2 inputs Go distinguishes  -> SIM
  P-b  GROW-LIMIT constant   Rust constant across limits while Go grows     -> SIM
  P-b2 GROW-LIMIT shrink     Rust shrinks with limit while Go grows         -> FAIL
  P-c  NONDETERMINISM        identical Rust args produce differing bytes    -> FAIL
  P-d  EMPTY-STUB            Rust empty where Go non-empty                   -> FAIL
  P-e  GOLDEN-DRIFT          Rust echoes a recorded golden after input chg  -> SIM

Controls prove the layer does NOT cry wolf (so the FAILs are meaningful):

  C-a  honest vary-input     Rust differs where Go differs                  -> PASS
  C-na go-identical          Go same on both inputs -> no premise           -> NA
  C-b  honest grow           Rust grows where Go grows                      -> PASS
  C-c  deterministic         identical Rust args -> identical bytes         -> PASS
  C-d  non-empty             Rust non-empty where Go non-empty              -> PASS
  C-e  honest drift          Rust changes when input changes                -> PASS
"""

import importlib.util
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
META_PY = os.path.join(HERE, "..", "metamorphic.py")

spec = importlib.util.spec_from_file_location("metamorphic", META_PY)
meta = importlib.util.module_from_spec(spec)
spec.loader.exec_module(meta)


def b(o):
    return json.dumps(o).encode()


class ScriptedBins:
    """
    Replaces oracle.run_once. Returns scripted (rc, bytes) keyed by a matcher
    function over (side, argv). First matching rule wins; matched rules with a
    list pop in order, scalars repeat. This lets us script Go vs Rust per input
    and per --limit value precisely.
    """
    def __init__(self, rules):
        # rules: list of (predicate(side, argv) -> bool, value)
        # value: bytes (repeat) | list[bytes] (pop) | callable(side,argv)->bytes
        self.rules = rules

    def __call__(self, side, argv):
        for pred, val in self.rules:
            if pred(side, argv):
                if callable(val):
                    return (0, val(side, argv))
                if isinstance(val, list):
                    return (0, val.pop(0))
                return (0, val)
        return (0, b"")


def install(rules):
    meta.run_once = ScriptedBins(rules)


def fresh_report():
    return meta.Report()


RESULTS = []


def expect(name, rep, prop, want):
    rec = [r for r in rep.records if r["property"] == prop]
    got = rec[-1]["verdict"] if rec else "MISSING"
    ok = got == want
    RESULTS.append((name, ok, got, want))
    flag = "OK " if ok else "XX "
    print(f"  {flag}{name}: [{prop}] got {got}, want {want}")
    return ok


def has_arg(argv, val):
    return val in argv


def limit_is(argv, n):
    return "--limit" in argv and argv[argv.index("--limit") + 1] == str(n)


# argv ends in a positional path; use it as the "input" discriminator.
def path_is(argv, p):
    return argv and argv[-1] == p


def main():
    print("================ METAMORPHIC SELF-PROOF ================")

    A = "/repoA"
    B = "/repoB"
    argvA = meta.history_argv("history/devs", A, limit=20)
    argvB = meta.history_argv("history/devs", B, limit=20)

    # ---- P-a VARY-INPUT constant: Rust same for 2 inputs Go distinguishes -> SIM
    install([
        (lambda s, a: s == "go" and path_is(a, A), b({"r": "A"})),
        (lambda s, a: s == "go" and path_is(a, B), b({"r": "B"})),
        (lambda s, a: s == "rust", b({"CONST": 1})),   # constant regardless of input
    ])
    rep = fresh_report()
    meta.check_vary_input(rep, "P-a", argvA, argvB, A, B)
    expect("P-a vary-input-constant", rep, "vary-input", "SIM")

    # ---- C-a honest vary-input: Rust differs where Go differs -> PASS
    install([
        (lambda s, a: s == "go" and path_is(a, A), b({"r": "gA"})),
        (lambda s, a: s == "go" and path_is(a, B), b({"r": "gB"})),
        (lambda s, a: s == "rust" and path_is(a, A), b({"r": "rA"})),
        (lambda s, a: s == "rust" and path_is(a, B), b({"r": "rB"})),
    ])
    rep = fresh_report()
    meta.check_vary_input(rep, "C-a", argvA, argvB, A, B)
    expect("C-a honest-vary-input", rep, "vary-input", "PASS")

    # ---- C-na go-identical: Go same on both inputs -> NA (no premise)
    install([
        (lambda s, a: s == "go", b({"r": "same"})),
        (lambda s, a: s == "rust" and path_is(a, A), b({"r": "rA"})),
        (lambda s, a: s == "rust" and path_is(a, B), b({"r": "rB"})),
    ])
    rep = fresh_report()
    meta.check_vary_input(rep, "C-na", argvA, argvB, A, B)
    expect("C-na go-identical-no-premise", rep, "vary-input", "NA")

    # ---- P-b GROW-LIMIT constant: Go grows, Rust constant -> SIM
    base = meta.history_argv("history/shotness", A)   # has --limit 20 default
    install([
        # Go grows with limit (len scales with limit value)
        (lambda s, a: s == "go", lambda s, a:
            b({"nodes": ["n"] * int(a[a.index("--limit") + 1])})),
        (lambda s, a: s == "rust", b({"nodes": ["c"]})),   # constant
    ])
    rep = fresh_report()
    meta.check_grow_limit(rep, "P-b", base, lo=10, hi=500)
    expect("P-b grow-limit-constant", rep, "grow-limit", "SIM")

    # ---- P-b2 GROW-LIMIT shrink: Go grows, Rust shrinks -> FAIL
    install([
        (lambda s, a: s == "go", lambda s, a:
            b({"nodes": ["n"] * int(a[a.index("--limit") + 1])})),
        (lambda s, a: s == "rust", lambda s, a:
            b({"nodes": ["n"] * (1000 - int(a[a.index("--limit") + 1]))})),
    ])
    rep = fresh_report()
    meta.check_grow_limit(rep, "P-b2", base, lo=10, hi=500)
    expect("P-b2 grow-limit-shrink", rep, "grow-limit", "FAIL")

    # ---- C-b honest grow: Go grows, Rust grows -> PASS
    install([
        (lambda s, a: True, lambda s, a:
            b({"nodes": ["n"] * int(a[a.index("--limit") + 1])})),
    ])
    rep = fresh_report()
    meta.check_grow_limit(rep, "C-b", base, lo=10, hi=500)
    expect("C-b honest-grow", rep, "grow-limit", "PASS")

    # ---- NA grow: Go does not grow -> NA (no premise, never invents growth)
    install([
        (lambda s, a: s == "go", b({"agg": 7})),   # constant regardless of limit
        (lambda s, a: s == "rust", b({"agg": 7})),
    ])
    rep = fresh_report()
    meta.check_grow_limit(rep, "C-b-na", base, lo=10, hi=500)
    expect("C-b-na go-saturates-no-premise", rep, "grow-limit", "NA")

    # ---- P-c NONDETERMINISM: identical Rust args -> differing bytes -> FAIL
    seq = [b({"x": 1}), b({"x": 2}), b({"x": 3})]
    install([(lambda s, a: s == "rust", list(seq))])
    rep = fresh_report()
    meta.check_determinism(rep, "P-c", argvA, reps=3)
    expect("P-c nondeterminism", rep, "determinism", "FAIL")

    # ---- C-c deterministic -> PASS
    install([(lambda s, a: s == "rust", b({"x": 1}))])
    rep = fresh_report()
    meta.check_determinism(rep, "C-c", argvA, reps=3)
    expect("C-c deterministic", rep, "determinism", "PASS")

    # ---- P-d EMPTY-STUB: Rust empty where Go non-empty -> FAIL
    install([
        (lambda s, a: s == "go", b({"r": 1})),
        (lambda s, a: s == "rust", b""),
    ])
    rep = fresh_report()
    meta.check_nonempty(rep, "P-d", argvA)
    expect("P-d empty-stub", rep, "non-empty", "FAIL")

    # ---- C-d non-empty -> PASS
    install([
        (lambda s, a: s == "go", b({"r": 1})),
        (lambda s, a: s == "rust", b({"r": 2})),
    ])
    rep = fresh_report()
    meta.check_nonempty(rep, "C-d", argvA)
    expect("C-d non-empty", rep, "non-empty", "PASS")

    # ---- P-e GOLDEN-DRIFT: Rust echoes recorded golden after input changed -> SIM
    # Go distinguishes A and B; the golden was recorded for A; Rust still emits the
    # A-golden bytes for B (memorized constant).
    golden = {}
    install([
        (lambda s, a: s == "go" and path_is(a, A), b({"r": "gA"})),
        (lambda s, a: s == "go" and path_is(a, B), b({"r": "gB"})),
        (lambda s, a: s == "rust", b({"FROZEN": "golden"})),   # constant
    ])
    rep = fresh_report()
    meta.check_golden_drift(rep, "P-e", "k", argvA, golden, argvB, A, B)
    expect("P-e golden-drift", rep, "golden-drift", "SIM")

    # ---- C-e honest drift: Rust changes when input changes -> PASS
    golden = {}
    install([
        (lambda s, a: s == "go" and path_is(a, A), b({"r": "gA"})),
        (lambda s, a: s == "go" and path_is(a, B), b({"r": "gB"})),
        (lambda s, a: s == "rust" and path_is(a, A), b({"r": "rA"})),
        (lambda s, a: s == "rust" and path_is(a, B), b({"r": "rB"})),
    ])
    rep = fresh_report()
    meta.check_golden_drift(rep, "C-e", "k", argvA, golden, argvB, A, B)
    expect("C-e honest-drift", rep, "golden-drift", "PASS")

    print("=======================================================")
    failed = [r for r in RESULTS if not r[1]]
    n = len(RESULTS)
    print(f"SELF-PROOF: {n - len(failed)}/{n} checks correct")
    if failed:
        print("SELF-PROOF FAILED -- the metamorphic layer did NOT catch a planted "
              "defect (or cried wolf on an honest control):")
        for name, _, got, want in failed:
            print(f"   - {name}: got {got}, want {want}")
        sys.exit(1)
    print("SELF-PROOF GREEN: metamorphic layer provably catches vary-input "
          "constants, grow-limit stubs/shrinks, nondeterminism, empty stubs, and "
          "golden-drift memorization -- and does NOT cry wolf on honest output.")
    sys.exit(0)


if __name__ == "__main__":
    main()
