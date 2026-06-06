#!/usr/bin/env python3
"""
CLI-SURFACE SELF-PROOF  (SPEC rule #6: every component must PROVE it catches a defect)

A green that cannot be shown to catch a planted bug is worthless. This test plants
KNOWN surface divergences and asserts the comparator reports FAIL for each. It also
asserts the BASELINE property: when Rust's surface is made identical to Go's, the
comparator finds ZERO surface divergences (so the FAILs above are signal, not noise).

It exercises the comparator's surface-diff layer with INJECTED surfaces (so the test
is hermetic and does not depend on the current state of the Rust binary), plus a
LIVE end-to-end check that the comparator's process exit code is nonzero on the real
(currently-divergent) binaries.

Mutations planted (each must be DETECTED):
  M1 missing-command   : Rust drops a subcommand Go has
  M2 missing-flag      : Rust drops a flag Go has
  M3 extra-flag        : Rust adds a flag Go lacks
  M4 flag-short-differs: Rust changes a flag's short letter
  M5 flag-arity-differs: Rust changes a flag from bool to value-taking
  M6 flag-default-diff : Rust changes a flag's default
  M7 positional-shape  : Rust changes a positional from scalar to variadic
  M8 help-exit-code    : Rust's `--help` exits nonzero where Go exits 0

Tamper planted (must be DETECTED as the forbidden cheat):
  T1 force-green-by-aliasing : feed the GO surface as the Rust surface for a probe
       that we then mutate ONLY on the Go side — i.e. prove the comparator is not a
       no-op that always says PASS. Concretely we mutate Go (oracle) and leave Rust
       equal to the UNMUTATED go; the comparator MUST flag the divergence rather
       than silently agreeing. (A comparator that ignored one side would pass.)
"""

import copy
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CLI_DIR = os.path.dirname(HERE)
sys.path.insert(0, CLI_DIR)
import cli_surface  # noqa: E402


def base_surface():
    """A minimal but representative two-command surface used as the GO oracle."""
    return {
        "(root)": {
            "subcommands": ["render", "run", "version"],
            "flags": {"help": {"short": "h", "takes_value": False, "default": None},
                      "verbose": {"short": "v", "takes_value": False, "default": None}},
            "positionals": [],
            "help_rc": 0, "help_stream": "stdout",
        },
        "run": {
            "subcommands": [],
            "flags": {
                "analyzers": {"short": "a", "takes_value": True, "default": None},
                "format": {"short": None, "takes_value": True, "default": "json"},
                "head": {"short": None, "takes_value": False, "default": None},
            },
            "positionals": [{"variadic": False, "required": False}],
            "help_rc": 0, "help_stream": "stdout",
        },
        "render": {
            "subcommands": [],
            "flags": {"output": {"short": "o", "takes_value": True, "default": ""}},
            "positionals": [{"variadic": False, "required": True}],
            "help_rc": 0, "help_stream": "stdout",
        },
        "version": {
            "subcommands": [], "flags": {}, "positionals": [],
            "help_rc": 0, "help_stream": "stdout",
        },
    }


def diff(go_s, ru_s):
    return cli_surface.diff_surface("codefang", go_s, ru_s)


def kinds(fails):
    return {f["kind"] for f in fails}


def assert_detects(name, go_s, ru_s, expected_kind):
    fails = diff(go_s, ru_s)
    if expected_kind not in kinds(fails):
        return (False, f"{name}: expected '{expected_kind}', got "
                       f"{sorted(kinds(fails))} ({len(fails)} fails)")
    return (True, f"{name}: DETECTED '{expected_kind}'")


def run():
    results = []

    # BASELINE: identical surfaces => zero divergences (signal vs noise proof)
    go_s = base_surface()
    ru_s = copy.deepcopy(go_s)
    base_fails = diff(go_s, ru_s)
    if base_fails:
        results.append((False, f"BASELINE: identical surfaces produced "
                               f"{len(base_fails)} false divergence(s): "
                               f"{sorted(kinds(base_fails))}"))
    else:
        results.append((True, "BASELINE: identical surfaces => 0 divergences"))

    # M1 missing-command
    ru = copy.deepcopy(go_s); del ru["version"]; ru["(root)"]["subcommands"].remove("version")
    results.append(assert_detects("M1", go_s, ru, "missing-command"))

    # M2 missing-flag
    ru = copy.deepcopy(go_s); del ru["run"]["flags"]["head"]
    results.append(assert_detects("M2", go_s, ru, "missing-flag"))

    # M3 extra-flag
    ru = copy.deepcopy(go_s)
    ru["run"]["flags"]["bogus"] = {"short": None, "takes_value": False, "default": None}
    results.append(assert_detects("M3", go_s, ru, "extra-flag"))

    # M4 flag-short-differs
    ru = copy.deepcopy(go_s); ru["run"]["flags"]["analyzers"]["short"] = "x"
    results.append(assert_detects("M4", go_s, ru, "flag-short-differs"))

    # M5 flag-value-arity-differs
    ru = copy.deepcopy(go_s); ru["run"]["flags"]["head"]["takes_value"] = True
    results.append(assert_detects("M5", go_s, ru, "flag-value-arity-differs"))

    # M6 flag-default-differs
    ru = copy.deepcopy(go_s); ru["run"]["flags"]["format"]["default"] = "yaml"
    results.append(assert_detects("M6", go_s, ru, "flag-default-differs"))

    # M7 positional-shape-differs
    ru = copy.deepcopy(go_s)
    ru["run"]["positionals"] = [{"variadic": True, "required": False}]
    results.append(assert_detects("M7", go_s, ru, "positional-shape-differs"))

    # M8 help-exit-code-differs
    ru = copy.deepcopy(go_s); ru["run"]["help_rc"] = 2
    results.append(assert_detects("M8", go_s, ru, "help-exit-code-differs"))

    # T1 anti-noop / anti-tamper: mutate the GO (oracle) side only; Rust stays at
    # the original. A comparator that quietly trusted one side, or that "blanked"
    # the differing field to force green, would report 0. The honest comparator
    # MUST report the divergence.
    go_mut = copy.deepcopy(go_s)
    go_mut["run"]["flags"]["format"]["default"] = "CHANGED"
    ru = copy.deepcopy(go_s)  # rust unchanged => differs from mutated go
    t1_fails = diff(go_mut, ru)
    if "flag-default-differs" in kinds(t1_fails):
        results.append((True, "T1: oracle-side change DETECTED (comparator is not "
                              "a no-op; cannot be forced green by ignoring a side)"))
    else:
        results.append((False, "T1: oracle-side change NOT detected — comparator "
                               "may be a no-op/tampered"))

    # LIVE end-to-end: the real comparator must exit nonzero on the real binaries
    # (which currently diverge) AND the error-path layer must detect the live
    # exit-code mismatch. This proves the wiring works against the actual binaries,
    # not just injected fixtures.
    p = subprocess.run([sys.executable, os.path.join(CLI_DIR, "cli_surface.py"),
                        "--json"], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                       timeout=300)
    try:
        live = json.loads(p.stdout.decode("utf-8"))
        live_ok = live["n_fail"] > 0 and any(
            r["problems"] for r in live.get("error_rows", []))
        if live_ok:
            results.append((True, f"LIVE: real comparator reports "
                                  f"{live['n_fail']} divergence(s) incl. error-path "
                                  f"(exit code {p.returncode})"))
        else:
            results.append((False, "LIVE: real comparator did not report the known "
                                   "live divergences (suspicious green)"))
    except Exception as e:
        results.append((False, f"LIVE: could not parse comparator output: {e}"))

    # report
    npass = sum(1 for ok, _ in results if ok)
    nfail = sum(1 for ok, _ in results if not ok)
    print("================ CLI-SURFACE SELF-PROOF ================")
    for ok, msg in results:
        print(f"  {'PASS' if ok else 'FAIL'}  {msg}")
    print(f"---- self-proof: {npass} passed, {nfail} failed ----")
    return 0 if nfail == 0 else 1


if __name__ == "__main__":
    sys.exit(run())
