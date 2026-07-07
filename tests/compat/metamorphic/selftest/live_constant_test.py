#!/usr/bin/env python3
"""
LIVE END-TO-END SELF-PROOF  (non-negotiable rule #6, and the VERIFY-phase brief:
"create a throwaway analyzer/path that emits a CONSTANT regardless of input and
confirm the metamorphic layer flags it SIM; then confirm a real analyzer (e.g.
history/devs) passes").

Unlike self_test.py (which scripts BOTH sides), this test keeps the LIVE Go
binary as the real oracle and only replaces the RUST side with a throwaway
constant-emitting binary. So the premise (Go distinguishes two real mined repos)
is established by the genuine Go binary on disk -- not faked -- and we prove the
metamorphic layer flags the constant Rust path SIM. Then, with the constant
emitter removed, the SAME check on the SAME inputs against the REAL Rust binary
must PASS, proving the layer does not cry wolf on an honest port.

This is the planted-defect proof required by the brief, run end-to-end against
the binaries (Go live; Rust = a deliberate constant stub), reusing the layer's
own decision functions (meta.check_vary_input / meta.check_nonempty).
"""

import importlib.util
import os
import stat
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
META_PY = os.path.join(HERE, "..", "metamorphic.py")

spec = importlib.util.spec_from_file_location("metamorphic", META_PY)
meta = importlib.util.module_from_spec(spec)
spec.loader.exec_module(meta)

HERCULES = meta.HERCULES
KUBE = meta.KUBE

# Real launcher (live Go + live Rust) is meta.run_once via the oracle module.
REAL_RUN_ONCE = meta.run_once

CONST_BYTES = b'{"devs":[{"name":"CONSTANT","commits":1}],"_planted":"stub"}'


def make_constant_emitter():
    """Write a throwaway 'analyzer' that emits CONST_BYTES regardless of argv."""
    fd, path = tempfile.mkstemp(prefix="const_analyzer_", suffix=".sh")
    os.write(fd, b"#!/bin/sh\nprintf '%s' '" + CONST_BYTES + b"'\n")
    os.close(fd)
    os.chmod(path, os.stat(path).st_mode | stat.S_IEXEC | stat.S_IXGRP |
             stat.S_IXOTH)
    return path


def routed_run_once(const_path):
    """
    LIVE Go oracle + CONSTANT Rust stub. Go calls go to the real binary on disk;
    Rust calls go to the throwaway constant emitter -> the planted defect.
    """
    def _run(side, argv):
        if side == "go":
            return REAL_RUN_ONCE("go", argv)        # genuine live oracle
        # side == rust: throwaway analyzer, ignores argv -> constant
        import subprocess
        p = subprocess.run([const_path], stdout=subprocess.PIPE,
                           stderr=subprocess.DEVNULL, timeout=30)
        return p.returncode, p.stdout
    return _run


RESULTS = []


def expect(name, rep, prop, want):
    rec = [r for r in rep.records if r["property"] == prop]
    got = rec[-1]["verdict"] if rec else "MISSING"
    ok = got == want
    RESULTS.append((name, ok, got, want))
    print(f"  {'OK ' if ok else 'XX '}{name}: [{prop}] got {got}, want {want}")
    return ok


def main():
    print("============ LIVE CONSTANT-STUB SELF-PROOF ============")
    print("(live Go oracle; Rust replaced by a throwaway CONSTANT emitter)")

    argvA = meta.history_argv("history/devs", HERCULES, limit=10)
    argvB = meta.history_argv("history/devs", KUBE, limit=10)

    # ---- PLANTED DEFECT: Rust = constant emitter -> vary-input must SIM ------
    const_path = make_constant_emitter()
    try:
        meta.run_once = routed_run_once(const_path)
        print("-- planted constant stub: vary-input hercules-vs-kube --")
        rep = meta.Report()
        meta.check_vary_input(rep, "PLANTED const history/devs",
                              argvA, argvB, HERCULES, KUBE)
        expect("planted-constant vary-input", rep, "vary-input", "SIM")

        # non-empty still PASSes (the stub emits bytes) -> shows vary-input is
        # what catches the constant, not a trivially-empty failure.
        print("-- planted constant stub: non-empty (stub is non-empty) --")
        rep = meta.Report()
        meta.check_nonempty(rep, "PLANTED const history/devs", argvA)
        expect("planted-constant non-empty-is-PASS", rep, "non-empty", "PASS")
    finally:
        os.unlink(const_path)
        meta.run_once = REAL_RUN_ONCE

    # ---- HONEST CONTROL: real Rust binary on the SAME inputs -> PASS --------
    print("-- real Rust analyzer history/devs: vary-input must PASS --")
    rep = meta.Report()
    meta.check_vary_input(rep, "REAL history/devs hercules-vs-kube",
                          argvA, argvB, HERCULES, KUBE)
    expect("real-analyzer vary-input", rep, "vary-input", "PASS")

    print("=======================================================")
    failed = [r for r in RESULTS if not r[1]]
    n = len(RESULTS)
    print(f"LIVE SELF-PROOF: {n - len(failed)}/{n} checks correct")
    if failed:
        print("LIVE SELF-PROOF FAILED:")
        for name, _, got, want in failed:
            print(f"   - {name}: got {got}, want {want}")
        sys.exit(1)
    print("LIVE SELF-PROOF GREEN: the metamorphic layer flags a throwaway "
          "constant-emitting Rust path SIM against the LIVE Go oracle, and "
          "PASSes the real history/devs analyzer on the same inputs.")
    sys.exit(0)


if __name__ == "__main__":
    main()
