#!/usr/bin/env python3
"""
COVERAGE-LEDGER SELF-PROOF  (non-negotiable rule #6: every component must
SELF-PROVE it catches a defect). A green that cannot be shown to catch a planted
bug is worthless. This harness PLANTS each cheat/defect class the ledger must be
immune to and asserts the component reports the gap rather than hiding it.

Planted defects (each mirrors a real way the prior effort was gamed):

  D1 SHRUNK-RESULTS CHEAT       A results file falsely claims cells were exercised
                                while the live expander enumerates more cells.
                                matrix_cell_coverage MUST take the denominator
                                from the LIVE expander, so the untested cells stay
                                visible -- the cheat cannot inflate the %.
  D2 EMPTY-RESULTS              No results -> coverage MUST be 0%/all-untested,
                                never a silent 100%. (A missing run is a gap, not
                                a pass.)
  D3 PER-LANGUAGE PARSE GAP     A language where the LIVE Go binary parses but the
                                LIVE Rust binary does not MUST be reported as a
                                GAP (measured against the Go oracle, not declared).
  D4 FABRICATED BRANCH NUMBER   rust_cov MUST NOT invent a branch % when the
                                toolchain can't measure it; it records
                                'unavailable', never a number.
  D5 COVERAGE-NOT-A-SUBSTITUTE  Even with a planted 100% coverage json, the ledger
                                MUST still carry the differential verdict tally
                                and that tally still shows the FAILs -- coverage
                                green is NOT 'done'.
  D6 VARIANT-EVIDENCE MEASURED  The Go-variant harvest goes through the LIVE
                                oracle whose cheat-detector REFUSES to normalize a
                                Go-STABLE field; and a measured-variant capture
                                MUST store the differing Go observations as
                                evidence (proof, not a declaration).

D1/D2/D4/D5 run offline (pure logic on planted inputs). D3/D6 need the live
binaries; they are skipped with a clear notice if a binary is missing, but run by
default in this environment.
"""

import importlib.util
import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
COV = os.path.abspath(os.path.join(HERE, ".."))
COMPAT = os.path.abspath(os.path.join(COV, ".."))
GO = "/home/dmitriy/sources/codefang/build/bin"
RU = "/home/dmitriy/sources/codefang/rust/target/release"


def load(name):
    p = os.path.join(COV, name)
    spec = importlib.util.spec_from_file_location(name[:-3], p)
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


mcc_mod = load("matrix_cell_cov.py")

RESULTS = []


def check(name, cond, detail=""):
    RESULTS.append((name, bool(cond)))
    print(f"  {'OK ' if cond else 'XX '}{name}"
          + ("" if cond else f"   <-- SELF-TEST FAILURE  {detail}"))
    return bool(cond)


def main():
    print("================ COVERAGE-LEDGER SELF-PROOF ================")

    # Build the ground-truth cell set from the LIVE expander for a tier.
    exp = mcc_mod.expand("smoke")
    all_labels = [c["label"] for c in exp["cells"]]
    total = len(all_labels)
    check("expander enumerates a non-trivial matrix", total > 50,
          f"only {total} cells")

    # --- D1 SHRUNK-RESULTS CHEAT -------------------------------------------
    # A cheating results file claims ONLY 3 cells but marks them PASS; if the
    # coverage took its denominator from the results it would read 100%. It must
    # instead read total from the live expander -> 3/total, untested listed.
    # We monkeypatch load_results to return the cheat without touching real files.
    cheat_labels = all_labels[:3]
    orig_load = mcc_mod.load_results
    mcc_mod.load_results = lambda tier: {
        "records": [{"label": l, "verdict": "PASS"} for l in cheat_labels]}
    try:
        cov = mcc_mod.matrix_cell_coverage("smoke")
    finally:
        mcc_mod.load_results = orig_load
    check("D1 denominator from live expander (cheat cannot inflate)",
          cov["total_cells"] == total and cov["exercised_cells"] == 3,
          f"total={cov['total_cells']} exercised={cov['exercised_cells']}")
    check("D1 untested cells surfaced (not hidden)",
          cov["untested_cells"] == total - 3 and len(cov["untested"]) == total - 3,
          f"untested={cov['untested_cells']}")

    # --- D2 EMPTY-RESULTS --------------------------------------------------
    mcc_mod.load_results = lambda tier: None
    try:
        cov0 = mcc_mod.matrix_cell_coverage("smoke")
    finally:
        mcc_mod.load_results = orig_load
    check("D2 no results -> 0% (a missing run is a gap, not a pass)",
          cov0["exercised_cells"] == 0 and cov0["percent"] == 0.0
          and cov0["untested_cells"] == total,
          f"exercised={cov0['exercised_cells']} pct={cov0['percent']}")

    # --- D4 FABRICATED BRANCH NUMBER ---------------------------------------
    # The committed rust_cov.json (from a stable toolchain) must record branch as
    # unavailable, NOT a number. (If branch ever becomes available the field is a
    # real number AND branch_coverage_available is true -- also acceptable.)
    rc_path = os.path.join(COV, "rust_cov.json")
    if os.path.exists(rc_path):
        rc = json.load(open(rc_path))
        br = rc.get("totals", {}).get("branches", {})
        avail = rc.get("branch_coverage_available", False)
        honest = (avail and isinstance(br.get("percent"), (int, float))) or \
                 ((not avail) and br.get("percent") is None
                  and "unavailable" in br)
        check("D4 branch number not fabricated (recorded or honestly N/A)",
              honest, f"branches={br} avail={avail}")
    else:
        check("D4 (skipped: no rust_cov.json yet)", True)

    # --- D5 COVERAGE-NOT-A-SUBSTITUTE --------------------------------------
    # Plant a 100% coverage json + a verdict tally that still has FAILs; the
    # ledger builder must keep the verdict tally visible alongside coverage so a
    # 100% number cannot mask FAILs ("done" requires the oracle verdict).
    led = load("build_ledger.py")
    with tempfile.TemporaryDirectory() as td:
        fake_rc = os.path.join(td, "rust_cov.json")
        json.dump({"ok": True, "branch_coverage_available": False,
                   "totals": {"lines": {"percent": 100.0, "covered": 1,
                                        "count": 1},
                              "regions": {"percent": 100.0},
                              "branches": {"percent": None,
                                           "unavailable": "x"}}},
                  open(fake_rc, "w"))
        fake_mcc = os.path.join(td, "mcc.json")
        json.dump({"matrix_cell": {"total_cells": 10, "exercised_cells": 10,
                                   "untested_cells": 0, "percent": 100.0,
                                   "per_family": {}, "untested": []},
                   "per_language_parse": {"languages_total": 1,
                                          "probed": False, "table": []}},
                  open(fake_mcc, "w"))
        # Plant a results file with FAILs under a private tier name and run the
        # REAL build_ledger main path against it; the emitted ledger MUST carry
        # the verdict tally (with its 3 FAILs) NEXT TO the 100% coverage.
        fake_tier = "_selftest_d5"
        fake_res = os.path.join(COMPAT, "results", f"{fake_tier}.json")
        json.dump({"tier": fake_tier, "tally": {"PASS": 1, "FAIL": 3, "SIM": 1},
                   "records": [{"label": "x", "verdict": "FAIL",
                                "argv": ["run"], "family": "f"}]},
                  open(fake_res, "w"))
        out_ledger = os.path.join(td, "ledger.json")
        orig_paths = (led.RUST_COV, led.MCC)
        led.RUST_COV, led.MCC = fake_rc, fake_mcc
        try:
            sys.argv = ["build_ledger.py", "--tier", fake_tier,
                        "--no-cov-refresh", "--out", out_ledger]
            try:
                led.main()
            except SystemExit:
                pass
            emitted = json.load(open(out_ledger))
        finally:
            led.RUST_COV, led.MCC = orig_paths
            os.remove(fake_res)
        cov_is_100 = emitted["coverage"]["rust_llvm_cov"]["totals"]["lines"][
            "percent"] == 100.0
        tally = emitted.get("verdict_tally") or {}
        check("D5 ledger keeps verdict tally (3 FAILs) alongside 100% coverage",
              cov_is_100 and tally.get("FAIL") == 3 and tally.get("SIM") == 1,
              f"cov100={cov_is_100} tally={tally}")

    # --- D3 PER-LANGUAGE PARSE GAP (live) ----------------------------------
    if os.path.exists(os.path.join(GO, "uast")) and \
       os.path.exists(os.path.join(RU, "uast")):
        plc = mcc_mod.per_language_parse_coverage(probe=True)
        # There is at least one language Go parses but Rust does not (grammar
        # vendoring pending). The facet MUST report it as a GAP.
        check("D3 live per-language parse GAP detected (measured vs Go oracle)",
              plc["probed"] and len(plc.get("gaps", [])) >= 1,
              f"gaps={plc.get('gaps')}")
        # And it must NOT falsely call a Rust-failure a pass: every gap language
        # has rust_parse_bytes<=0 while go_parse_bytes>0 in the table.
        gap_rows = [r for r in plc["table"] if r["status"].startswith("GAP")]
        sound = all((r.get("go_parse_bytes", 0) > 0 and
                     r.get("rust_parse_bytes", 0) <= 0) for r in gap_rows)
        check("D3 gap rows are genuine (go-parses & rust-empty)",
              len(gap_rows) >= 1 and sound,
              f"gap_rows={len(gap_rows)} sound={sound}")
    else:
        check("D3 (skipped: live binaries missing)", True)

    # --- D6 VARIANT-EVIDENCE MEASURED (live oracle cheat-detector) ----------
    oracle_py = os.path.join(COMPAT, "oracle", "oracle.py")
    if os.path.exists(oracle_py) and os.path.exists(os.path.join(GO, "uast")):
        ospec = importlib.util.spec_from_file_location("oracle", oracle_py)
        oracle = importlib.util.module_from_spec(ospec)
        ospec.loader.exec_module(oracle)
        # Plant: a fabricated Go output that is STABLE on '$.score', then ask the
        # oracle to normalize '$.score'. The cheat-detector MUST refuse (this is
        # the exact blank-a-stable-field cheat the variant harvest must be immune
        # to). We drive it with scripted bins, never corrupting the real port.
        STABLE = {"score": 7, "items": [1, 2, 3]}

        class FakeBins:
            def __init__(self, go, rust):
                self.go = list(go); self.rust = list(rust)

            def __call__(self, side, argv):
                return (self.go if side == "go" else self.rust).pop(0)
        fb = FakeBins([(0, json.dumps(STABLE).encode())] * 3,
                      [(0, json.dumps(STABLE).encode())] * 2)
        orig = oracle.run_once
        oracle.run_once = fb
        try:
            res = oracle.run_invocation(
                ["run", "--analyzers", "x", "--format", "json", "--limit", "10",
                 "/r"], n_go=3, normalize=["$.score"])
        finally:
            oracle.run_once = orig
        check("D6 cheat-detector refuses to blank a Go-STABLE field",
              res["verdict"] == "FAIL" and "TAMPER" in res.get("reason", ""),
              f"verdict={res['verdict']} reason={res.get('reason')}")

        # And: a genuinely Go-variant capture stores the differing observations.
        GO1 = {"items": [{"v": 1}], "tag": "a"}
        GO2 = {"items": [{"v": 2}], "tag": "b"}
        GO3 = {"items": [{"v": 3}], "tag": "c"}
        fb2 = FakeBins([(0, json.dumps(GO1).encode()),
                        (0, json.dumps(GO2).encode()),
                        (0, json.dumps(GO3).encode())],
                       [(0, json.dumps(GO1).encode())] * 2)
        oracle.run_once = fb2
        try:
            res2 = oracle.run_invocation(
                ["run", "--analyzers", "x", "--format", "json", "/r"],
                n_go=3, normalize=[])
        finally:
            oracle.run_once = orig
        ev = res2.get("evidence", {})
        has_proof = "$.tag" in ev and len(ev["$.tag"]) >= 2
        check("D6 measured-variant capture stores differing Go observations",
              has_proof, f"evidence={list(ev.items())[:3]}")
    else:
        check("D6 (skipped: oracle/live Go missing)", True)

    print("===========================================================")
    failed = [r for r in RESULTS if not r[1]]
    n = len(RESULTS)
    print(f"SELF-PROOF: {n - len(failed)}/{n} checks correct")
    if failed:
        print("SELF-PROOF FAILED -- a planted cheat/defect was NOT caught:")
        for name, _ in failed:
            print(f"   - {name}")
        sys.exit(1)
    print("SELF-PROOF GREEN: the ledger provably surfaces shrunk-results cheats, "
          "empty runs, per-language parse gaps, refuses fabricated branch %, keeps "
          "the verdict alongside coverage, and harvests measured variant evidence.")
    sys.exit(0)


if __name__ == "__main__":
    main()
