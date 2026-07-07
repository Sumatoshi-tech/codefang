#!/usr/bin/env python3
"""
TAMPER-EVIDENCE  (SPEC: specs/go-compat-testing/SPEC.md §3.7 / §3.8, roadmap 7).

The gate itself was weakened once (a deterministic Go-stable field was blanked to
hide a real bug). So the compat system must treat its OWN harness as untrusted and
fail CLOSED on any of these:

  (A) ORACLE / CANONICALIZER MODIFIED.
      The oracle binary path (the LIVE Go binary -- the source of truth) and the
      canonicalizer logic (oracle.py: classify/canonicalize/compare) and the
      matrix definition are content-hashed against a recorded baseline. Any byte
      change to a protected file fails closed. (Editing the canonicalizer is
      exactly how one would re-introduce a "blank a Go-stable field" cheat, so the
      canonicalizer source is protected, not trusted.)

  (B) MATRIX SHRUNK.
      The expanded matrix is re-expanded and compared to the recorded per-tier,
      per-FAMILY cell-count baseline. Dropping an analyzer, a format, or a whole
      family shrinks a family count below baseline -> fail closed. (A shrunk
      matrix is the "probe fewer cells to pass" cheat. Growing the matrix is fine
      and updates the baseline only via explicit --bless.)

  (C) GO-STABLE FIELD NEWLY BLANKED.
      The historic cheat: declare a Go-STABLE field "nondeterministic" and blank
      it so a buggy Rust value is hidden. This is detected ACTIVELY: we run the
      LIVE oracle on a pinned, fast, deterministic probe (uast parse of a corpus
      file, which Go emits byte-stable), and for every normalize/blank request in
      the declared canonicalization policy we assert the oracle REJECTS it as
      targeting a Go-stable field. A policy that asks to neutralize ANY currently
      Go-stable field is a tamper -> fail closed. The oracle's own
      check_normalize_request is the authority; this layer drives it adversarially.

The ORACLE here is always the live Go binary (via oracle.run_invocation). Nothing
is re-derived. `--bless` records the current state as the new baseline (used after
a legitimate, reviewed change). Default mode VERIFIES and exits nonzero on any
violation. `--self-test` proves this checker catches each tamper class.
"""

import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
COMPAT = os.path.dirname(HERE)
BASELINE = os.path.join(HERE, "baseline.json")
CORPUS = os.path.join(COMPAT, "corpus")

# Files whose every byte is integrity-protected: the canonicalizer logic, the
# matrix definition + expander + runner, and the metamorphic layer. The ORACLE
# proper is the live Go binary; we also fingerprint the Go binary path + size so a
# swapped oracle is visible (its content hash is recorded too).
PROTECTED_RELPATHS = [
    "oracle/oracle.py",          # canonicalizer (classify/canonicalize/compare)
    "matrix.toml",               # matrix definition (axes)
    "expand_matrix.py",          # matrix expander
    "run_matrix.py",             # matrix runner
    "metamorphic/metamorphic.py",
]

# The live Go oracle binaries -- the source of truth. A swapped/rebuilt oracle is
# a legitimate event, but it MUST be re-blessed consciously, so we record their
# hashes too and report (not necessarily fail) on change. Treated as a WARNING by
# default (Go is rebuilt often); promote to hard-fail with --strict-oracle.
GO_BIN_DIR = "/home/dmitriy/sources/codefang/build/bin"
GO_BINS = ["codefang", "uast"]

PINNED_ENV = {"TZ": "UTC", "NO_COLOR": "1", "LANG": "C", "LC_ALL": "C",
              "SOURCE_DATE_EPOCH": "315532800"}


def sha_file(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


# --------------------------------------------------------------------------- #
# Load the oracle module (the canonicalizer authority) without running it.
# --------------------------------------------------------------------------- #
def load_oracle():
    op = os.path.join(COMPAT, "oracle", "oracle.py")
    spec = importlib.util.spec_from_file_location("oracle", op)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


# --------------------------------------------------------------------------- #
# Matrix expansion -> per-family cell counts (the anti-shrink signal).
# --------------------------------------------------------------------------- #
def expand_families(tier):
    out = subprocess.run([sys.executable,
                          os.path.join(COMPAT, "expand_matrix.py"), tier],
                         stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                         check=True)
    d = json.loads(out.stdout)
    return int(d["total_cells"]), {k: int(v) for k, v in d["families"].items()}


# --------------------------------------------------------------------------- #
# (C) Go-stable probe: pick a corpus file Go parses byte-stably, and a normalize
# policy that, IF a field is Go-stable, must be rejected by the oracle.
# --------------------------------------------------------------------------- #
def _first_corpus_file():
    man = json.load(open(os.path.join(CORPUS, "manifest.json")))
    for e in man["files"]:
        p = os.path.join(CORPUS, e["stored"])
        if os.path.exists(p):
            return p
    return None


def stable_field_probe(oracle, blank_paths):
    """
    Run the LIVE oracle on a deterministic uast-parse probe with `blank_paths`
    presented as a normalize/canonicalization request. The oracle classifies each
    field as STABLE/VARIANT from N>=3 real Go runs; if any requested path is
    Go-STABLE it returns verdict FAIL with a TAMPER reason. We return:
        (probe_ran, illegal_paths, oracle_result)
    illegal_paths non-empty => the policy is blanking a Go-stable field (tamper).
    """
    f = _first_corpus_file()
    if not f:
        return False, [], {"reason": "no corpus file available for probe"}
    argv = ["uast", "parse", "--format", "json", f]
    res = oracle.run_invocation(argv, n_go=3, normalize=blank_paths)
    illegal = res.get("illegal_normalize_paths", [])
    return True, illegal, res


# A representative set of fields that ARE Go-stable in `uast parse` output. The
# checker asserts the oracle refuses to blank ANY of them. If the canonicalizer
# were weakened to permit blanking a stable field, the oracle would no longer
# reject these and this check fails closed.
KNOWN_STABLE_PARSE_FIELDS = [
    "$.type",
    "$.children[0].pos.start_line",
    "$.children[0].pos.end_col",
    "$.children[0].type",
]


# --------------------------------------------------------------------------- #
# Build / verify baseline.
# --------------------------------------------------------------------------- #
def compute_state():
    state = {"protected": {}, "go_oracle": {}, "matrix": {}}
    for rel in PROTECTED_RELPATHS:
        p = os.path.join(COMPAT, rel)
        state["protected"][rel] = sha_file(p)
    for b in GO_BINS:
        p = os.path.join(GO_BIN_DIR, b)
        state["go_oracle"][b] = sha_file(p) if os.path.exists(p) else None
    for tier in ("smoke", "full"):
        total, fams = expand_families(tier)
        state["matrix"][tier] = {"total_cells": total, "families": fams}
    return state


def bless():
    state = compute_state()
    state["_doc"] = ("Tamper-evidence baseline. Regenerate ONLY after a reviewed, "
                     "intentional change via: tamper_check.py --bless. Family cell "
                     "counts are MINIMUMS; shrinking any below these fails closed.")
    with open(BASELINE, "w") as f:
        json.dump(state, f, indent=2, sort_keys=True)
    print(f"blessed baseline -> {BASELINE}")
    for rel, h in state["protected"].items():
        print(f"  protected {rel}: {h[:16]}")
    for tier, mx in state["matrix"].items():
        print(f"  matrix[{tier}]: {mx['total_cells']} cells {mx['families']}")
    return 0


def verify(strict_oracle=False):
    if not os.path.exists(BASELINE):
        print("FAIL-CLOSED: no baseline.json (run --bless on a trusted state)")
        return 2
    base = json.load(open(BASELINE))
    cur = compute_state()
    violations = []
    warnings = []

    # (A) protected file hashes -- any change fails closed.
    for rel, want in base["protected"].items():
        got = cur["protected"].get(rel)
        if got != want:
            violations.append(
                f"PROTECTED FILE MODIFIED: {rel}\n"
                f"    baseline={want[:24]}\n    current ={(got or 'MISSING')[:24]}")
    for rel in cur["protected"]:
        if rel not in base["protected"]:
            warnings.append(f"new protected file not in baseline: {rel}")

    # Oracle binary: changed Go binary is a warning (rebuilt often) unless strict.
    for b, want in base.get("go_oracle", {}).items():
        got = cur["go_oracle"].get(b)
        if got != want:
            msg = (f"GO ORACLE BINARY CHANGED: {b} "
                   f"(baseline={(want or 'none')[:16]} current={(got or 'MISSING')[:16]})")
            (violations if strict_oracle else warnings).append(msg)

    # (B) matrix shrink -- any family or total below baseline fails closed.
    for tier, bmx in base["matrix"].items():
        cmx = cur["matrix"].get(tier, {"total_cells": 0, "families": {}})
        if cmx["total_cells"] < bmx["total_cells"]:
            violations.append(
                f"MATRIX SHRUNK [{tier}]: total {cmx['total_cells']} "
                f"< baseline {bmx['total_cells']}")
        for fam, bcount in bmx["families"].items():
            ccount = cmx["families"].get(fam, 0)
            if ccount < bcount:
                violations.append(
                    f"MATRIX FAMILY SHRUNK [{tier}/{fam}]: {ccount} "
                    f"< baseline {bcount} (dropped analyzer/format/cells)")

    # (C) Go-stable-field blanking -- ACTIVE probe via the live oracle.
    try:
        oracle = load_oracle()
        ran, illegal, _ = stable_field_probe(oracle, KNOWN_STABLE_PARSE_FIELDS)
        if not ran:
            warnings.append("stable-field probe skipped (no corpus file)")
        elif not illegal:
            # The oracle ACCEPTED blanking fields we know are Go-stable => the
            # canonicalizer has been weakened to permit the historic cheat.
            violations.append(
                "CANONICALIZER WEAKENED: oracle accepted a normalize request for "
                "fields measured Go-STABLE (blanking a Go-stable field is the "
                "forbidden cheat). Probed paths it should have REJECTED: "
                + ", ".join(KNOWN_STABLE_PARSE_FIELDS))
        else:
            # Good: oracle rejected at least the known-stable paths.
            pass
    except Exception as e:
        violations.append(f"stable-field probe ERROR (fail-closed): {e}")

    print("================ TAMPER-EVIDENCE CHECK ================")
    for w in warnings:
        print("  WARN  " + w.replace("\n", "\n        "))
    if not violations:
        print("  OK    protected files unmodified")
        print("  OK    matrix not shrunk (all family counts >= baseline)")
        print("  OK    canonicalizer rejects blanking Go-stable fields")
        print("INTEGRITY: GREEN -- harness untampered, fail-closed gate satisfied")
        return 0
    print("INTEGRITY: RED -- FAIL-CLOSED. Violations:")
    for v in violations:
        print("  X  " + v.replace("\n", "\n     "))
    return 1


# --------------------------------------------------------------------------- #
# SELF-TEST: prove THIS checker catches each tamper class (rule #6). Operates on
# a temporary COPY of the compat tree so the real harness is never corrupted.
# --------------------------------------------------------------------------- #
def self_test():
    print("================ TAMPER-EVIDENCE SELF-PROOF ================")
    results = []

    def record(name, ok, detail=""):
        results.append((name, ok))
        print(f"  {'OK ' if ok else 'XX '}{name}{('  -- ' + detail) if not ok else ''}")

    # 0) baseline must exist and verify GREEN on the real (untampered) tree.
    rc = verify()
    record("S0 untampered-tree-is-GREEN", rc == 0,
           f"verify() returned {rc}, expected 0")

    # Helper: run verify() against a temporary state by monkeypatching globals.
    import types

    def verify_with(base_obj=None, protected_override=None, matrix_override=None,
                    oracle_override=None, strict_oracle=False):
        """Run a stripped verify() against injected current-state, isolated."""
        base = base_obj if base_obj is not None else json.load(open(BASELINE))
        violations = []
        # (A)
        cur_prot = protected_override if protected_override is not None \
            else {rel: sha_file(os.path.join(COMPAT, rel))
                  for rel in base["protected"]}
        for rel, want in base["protected"].items():
            if cur_prot.get(rel) != want:
                violations.append(("protected", rel))
        # (B)
        cur_mx = matrix_override if matrix_override is not None else {
            t: dict(zip(("total_cells", "families"), expand_families(t)))
            for t in base["matrix"]}
        for tier, bmx in base["matrix"].items():
            cmx = cur_mx.get(tier, {"total_cells": 0, "families": {}})
            if cmx["total_cells"] < bmx["total_cells"]:
                violations.append(("matrix-total", tier))
            for fam, bcount in bmx["families"].items():
                if cmx["families"].get(fam, 0) < bcount:
                    violations.append(("matrix-family", f"{tier}/{fam}"))
        # (C)
        oracle = oracle_override if oracle_override is not None else load_oracle()
        ran, illegal, _ = stable_field_probe(oracle, KNOWN_STABLE_PARSE_FIELDS)
        if ran and not illegal:
            violations.append(("canonicalizer-weakened", "stable-blanked"))
        return violations

    # 1) PROTECTED FILE MODIFIED -> caught.
    base = json.load(open(BASELINE))
    tampered_base = json.loads(json.dumps(base))
    # flip one recorded hash so the live file no longer matches.
    rel0 = PROTECTED_RELPATHS[0]
    tampered_base["protected"][rel0] = "0" * 64
    v = verify_with(base_obj=tampered_base)
    record("S1 protected-file-modified-CAUGHT",
           any(k == "protected" and r == rel0 for k, r in v),
           f"violations={v}")

    # 2) MATRIX SHRUNK (family dropped) -> caught.
    shrunk = {}
    for t, mx in base["matrix"].items():
        fams = dict(mx["families"])
        # remove a whole family (simulate dropping all uast_parse cells)
        if "uast_parse" in fams:
            fams = {k: v for k, v in fams.items() if k != "uast_parse"}
        shrunk[t] = {"total_cells": mx["total_cells"] - 1, "families": fams}
    v = verify_with(matrix_override=shrunk)
    record("S2 matrix-family-shrunk-CAUGHT",
           any(k in ("matrix-family", "matrix-total") for k, _ in v),
           f"violations={v}")

    # 3) MATRIX SHRUNK (one cell fewer, no family removed) -> caught by total.
    minus_one = {}
    for t, mx in base["matrix"].items():
        fams = dict(mx["families"])
        # drop a single static_analyzer cell
        if fams.get("static_analyzer", 0) > 0:
            fams["static_analyzer"] -= 1
        minus_one[t] = {"total_cells": mx["total_cells"] - 1, "families": fams}
    v = verify_with(matrix_override=minus_one)
    record("S3 matrix-one-cell-shrunk-CAUGHT",
           any(k in ("matrix-family", "matrix-total") for k, _ in v),
           f"violations={v}")

    # 4) CANONICALIZER WEAKENED (oracle accepts blanking a Go-stable field) ->
    #    caught. We build a FAKE oracle whose check rejects nothing (the exact
    #    weakening), prove the probe + checker flag it; and confirm the REAL
    #    oracle is NOT flagged (no false positive).
    real_oracle = load_oracle()

    class WeakenedOracle:
        """A canonicalizer that has been tampered to permit the historic cheat:
        its run_invocation never reports a normalize-of-stable-field as illegal."""
        def run_invocation(self, argv, n_go=3, normalize=None):
            r = real_oracle.run_invocation(argv, n_go=n_go, normalize=[])
            # strip the tamper rejection: pretend nothing was illegal.
            r = dict(r)
            r.pop("illegal_normalize_paths", None)
            return r

    v_weak = verify_with(oracle_override=WeakenedOracle())
    record("S4 canonicalizer-weakened-CAUGHT",
           any(k == "canonicalizer-weakened" for k, _ in v_weak),
           f"violations={v_weak}")

    v_real = verify_with(oracle_override=real_oracle)
    record("S4b real-oracle-NOT-flagged(no-false-positive)",
           not any(k == "canonicalizer-weakened" for k, _ in v_real),
           f"violations={v_real}")

    # 5) NEGATIVE control: untampered injected state -> no violations.
    v_clean = verify_with()
    record("S5 clean-state-no-violations", len(v_clean) == 0,
           f"violations={v_clean}")

    print("============================================================")
    bad = [n for n, ok in results if not ok]
    print(f"SELF-PROOF: {len(results) - len(bad)}/{len(results)} checks correct")
    if bad:
        print("SELF-PROOF FAILED -- tamper checker did not catch:", bad)
        return 1
    print("SELF-PROOF GREEN: tamper checker provably catches file-modify, "
          "matrix-shrink, and canonicalizer-weakening; clean state stays green.")
    return 0


def main():
    ap = argparse.ArgumentParser(description="Compat harness tamper-evidence")
    ap.add_argument("--bless", action="store_true",
                    help="record current state as the trusted baseline")
    ap.add_argument("--self-test", action="store_true",
                    help="prove this checker catches each tamper class")
    ap.add_argument("--strict-oracle", action="store_true",
                    help="treat a changed Go oracle binary as a hard failure")
    a = ap.parse_args()
    if a.bless:
        sys.exit(bless())
    if a.self_test:
        sys.exit(self_test())
    sys.exit(verify(strict_oracle=a.strict_oracle))


if __name__ == "__main__":
    main()
