#!/usr/bin/env python3
"""
GAP LEDGER BUILDER  (SPEC §3.7 / §4 ledger.json, roadmap 5)

Assembles the machine-readable gap ledger at tests/compat/ledger.json. It unifies:

  1. COVERAGE (reach, three facets) -- read from coverage/rust_cov.json and
     coverage/matrix_cell_cov.json:
        a. Rust line / region / (best-effort) branch coverage (cargo-llvm-cov)
        b. matrix-cell coverage (cells exercised / total)
        c. per-language parse coverage
     Coverage is reported ALONGSIDE the differential verdict, NEVER instead of it
     (SPEC §6 risk: "coverage % gives false confidence"). The ledger therefore
     ALSO carries the verdict tallies and refuses to call a state "done" on
     coverage alone.

  2. UNTESTED MATRIX CELLS -- every enumerated cell with no recorded verdict,
     surfaced (never hidden). Taken from matrix_cell_cov (full-tier denominator).

  3. KNOWN DIVERGENCES -- (a) the recorded corpus/known_gaps.json entries, and
     (b) every FAIL/SIM verdict in the current results/<tier>.json. Each carries
     enough to triage.

  4. GO-VARIANT CAPTURES WITH EVIDENCE -- MEASURED here by running the LIVE oracle
     (the Go binary, N>=3x) on a probe set; every field the oracle classifies
     VARIANT is recorded WITH the differing Go observations the oracle stored as
     evidence. Canonicalization is measured, never declared (SPEC §3.3); the
     evidence is the proof that Go itself varies. (If a probe shows ZERO variant
     fields, that is recorded too -- a Go-STABLE capture is NOT a license to
     normalize anything.)

The ledger is the honest "how close to all + where the gaps are" artifact. It is
NOT a pass/fail gate by itself; run_matrix.py / the oracle own the verdict.

Usage:
  build_ledger.py --tier smoke
  build_ledger.py --tier full --probe-variants   # live Go N-run variant harvest
  build_ledger.py --tier smoke --no-cov-refresh   # reuse existing cov json
"""

import argparse
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
COMPAT = os.path.abspath(os.path.join(HERE, ".."))
ORACLE = os.path.join(COMPAT, "oracle", "oracle.py")
RUST_COV = os.path.join(HERE, "rust_cov.json")
MCC = os.path.join(HERE, "matrix_cell_cov.json")
KNOWN_GAPS = os.path.join(COMPAT, "corpus", "known_gaps.json")
LEDGER = os.path.join(COMPAT, "ledger.json")

CORPUS = "/home/dmitriy/sources/hercules"

# Probe set for the Go-VARIANT evidence harvest: analyzers the parity gate notes
# are (or may be) Go map/goroutine-order nondeterministic. The oracle MEASURES
# whether they actually vary; we never assume. Kept small for the smoke tier.
VARIANT_PROBES = [
    ("history/couples", ["run", "--checkpoint=false", "--resume=false",
                         "--no-cache", "--workers", "1", "--analyzers",
                         "history/couples", "--format", "json", "--limit", "30",
                         CORPUS]),
    ("history/shotness", ["run", "--checkpoint=false", "--resume=false",
                          "--no-cache", "--workers", "1", "--analyzers",
                          "history/shotness", "--format", "json", "--limit",
                          "30", CORPUS]),
    ("history/typos", ["run", "--checkpoint=false", "--resume=false",
                       "--no-cache", "--workers", "1", "--analyzers",
                       "history/typos", "--format", "json", "--limit", "30",
                       CORPUS]),
    ("static/comments", ["run", "--checkpoint=false", "--resume=false",
                         "--no-cache", "--head", "--workers", "1",
                         "--static-workers", "1", "-p", CORPUS, "--analyzers",
                         "static/comments", "--format", "json"]),
]


def refresh_coverage(tier, probe_parse, do_rust_cov):
    if do_rust_cov:
        subprocess.run([sys.executable, os.path.join(HERE, "rust_cov.py"),
                        "--out", RUST_COV], check=False)
    cmd = [sys.executable, os.path.join(HERE, "matrix_cell_cov.py"),
           "--tier", tier, "--out", MCC]
    if probe_parse:
        cmd.append("--probe-parse")
    subprocess.run(cmd, check=False)


def load_json(path):
    if not os.path.exists(path):
        return None
    with open(path) as f:
        return json.load(f)


def harvest_variant_captures(n_go, do_probe):
    """Run the LIVE oracle N>=3x on the probe set and collect every measured
    Go-VARIANT field WITH the differing Go observations stored as evidence."""
    captures = []
    if not do_probe:
        return captures, "skipped (pass --probe-variants to harvest live)"
    for label, argv in VARIANT_PROBES:
        mf = f"/tmp/ledger_variant_{abs(hash(label)) % 99999}.json"
        p = subprocess.run([sys.executable, ORACLE, "--n-go", str(n_go),
                            "--manifest", mf, "--quiet", "--"] + argv,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        m = load_json(mf)
        if m is None:
            captures.append({"label": label, "argv": argv,
                             "error": "oracle produced no manifest"})
            continue
        evidence = m.get("evidence", {})
        fc = m.get("field_counts", {})
        cap = {
            "label": label,
            "argv": argv,
            "oracle_verdict": m.get("verdict"),
            "field_counts": fc,
            "n_variant_fields": len(evidence),
            "canonical_go_stable": m.get("canonical_go_stable"),
            "go_run_shas": [r.get("sha") for r in m.get("go_runs", [])],
            # store a CAPPED slice of the evidence so the ledger stays readable
            # but the proof (differing Go values) is present and auditable.
            "evidence_sample": dict(list(evidence.items())[:12]),
            "evidence_field_paths": sorted(evidence.keys()),
        }
        if not evidence:
            cap["note"] = ("Go MEASURED STABLE here across N runs -> NOTHING may "
                           "be normalized. Recorded so a future blank is detectable.")
        else:
            cap["note"] = ("Go MEASURED VARIANT on these fields (evidence = the "
                           "differing Go observations). Canonicalization here is "
                           "measured, not declared.")
        try:
            os.remove(mf)
        except OSError:
            pass
        captures.append(cap)
    return captures, "harvested live"


def known_divergences(tier):
    out = []
    kg = load_json(KNOWN_GAPS)
    if kg:
        for d in kg.get("known_divergences", []):
            out.append({"source": "corpus/known_gaps.json", **d})
    res = load_json(os.path.join(COMPAT, "results", f"{tier}.json"))
    if res:
        for r in res.get("records", []):
            if r.get("verdict") in ("FAIL", "SIM"):
                out.append({"source": f"results/{tier}.json",
                            "label": r.get("label"),
                            "verdict": r.get("verdict"),
                            "family": r.get("family"),
                            "argv": r.get("argv")})
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tier", default="smoke",
                    help="results/<tier>.json + expander tier (smoke|full; other "
                         "names allowed only with --no-cov-refresh, for self-test)")
    ap.add_argument("--n-go", type=int, default=3)
    ap.add_argument("--probe-variants", action="store_true",
                    help="run the LIVE oracle to harvest Go-variant evidence")
    ap.add_argument("--probe-parse", action="store_true",
                    help="run LIVE per-language parse coverage")
    ap.add_argument("--no-cov-refresh", action="store_true",
                    help="reuse existing coverage/*.json (skip re-running)")
    ap.add_argument("--no-rust-cov", action="store_true",
                    help="skip the (slow) llvm-cov refresh; keep matrix-cell")
    ap.add_argument("--out", default=LEDGER)
    a = ap.parse_args()

    t0 = time.time()
    if not a.no_cov_refresh:
        refresh_coverage(a.tier, a.probe_parse, not a.no_rust_cov)

    rust_cov = load_json(RUST_COV)
    mcc = load_json(MCC)
    captures, harvest_status = harvest_variant_captures(a.n_go, a.probe_variants)
    divergences = known_divergences(a.tier)

    # verdict tally pulled from results so coverage is NEVER reported alone.
    res = load_json(os.path.join(COMPAT, "results", f"{a.tier}.json"))
    verdict_tally = res.get("tally") if res else None

    untested = mcc["matrix_cell"]["untested"] if mcc else []

    ledger = {
        "spec": "specs/go-compat-testing/SPEC.md",
        "phase": "CoverageLedger (roadmap 5)",
        "tier": a.tier,
        "generated_epoch": int(time.time()),
        "elapsed_s": round(time.time() - t0, 1),

        # --- the honest headline: coverage REACH + the differential VERDICT,
        #     side by side. Coverage is never a substitute for the verdict. ---
        "verdict_tally": verdict_tally,
        "verdict_source": f"results/{a.tier}.json" if res else None,
        "coverage": {
            "rust_llvm_cov": rust_cov,
            "matrix_cell": mcc["matrix_cell"] if mcc else None,
            "per_language_parse": mcc["per_language_parse"] if mcc else None,
        },

        # --- the gaps: surfaced, never hidden ---
        "untested_matrix_cells": untested,
        "untested_matrix_cell_count": len(untested),
        "known_divergences": divergences,
        "known_divergence_count": len(divergences),
        "go_variant_captures": captures,
        "go_variant_capture_status": harvest_status,

        "honesty_note": (
            "Coverage % measures REACH (did a test touch this code/cell/language). "
            "It is reported ALONGSIDE the differential oracle verdict, never as a "
            "substitute (SPEC §6). 'Done' requires the oracle verdict GREEN on "
            "off-corpus inputs, NOT a high coverage number. Every Go-variant "
            "capture stores the differing Go observations as evidence so "
            "canonicalization is measured, not declared."),
    }

    with open(a.out, "w") as f:
        json.dump(ledger, f, indent=2, default=str)

    # console summary
    print("=" * 64)
    print(f"GAP LEDGER  tier={a.tier}  -> {a.out}")
    if rust_cov and rust_cov.get("ok"):
        t = rust_cov["totals"]
        br = t["branches"]["percent"]
        print(f"  rust coverage : lines {t['lines']['percent']:.1f}%  "
              f"regions {t['regions']['percent']:.1f}%  "
              f"branch {('%.1f%%' % br) if br is not None else 'N/A(stable-toolchain)'}")
    if mcc:
        m = mcc["matrix_cell"]
        print(f"  matrix cells  : {m['exercised_cells']}/{m['total_cells']} "
              f"({m['percent']}%) exercised  |  {m['untested_cells']} UNTESTED")
        pl = mcc["per_language_parse"]
        if pl.get("probed"):
            print(f"  per-language  : {pl['rust_parse_ok']}/{pl['languages_total']} "
                  f"({pl['percent']}%) rust-parses  gaps={pl.get('gaps')}")
        else:
            print(f"  per-language  : {pl['languages_total']} langs (static; "
                  f"--probe-parse for live)")
    print(f"  verdict tally : {verdict_tally}  (NOT replaced by coverage)")
    print(f"  divergences   : {len(divergences)} known")
    nv = sum(1 for c in captures if c.get('n_variant_fields'))
    print(f"  variant caps  : {len(captures)} probed, {nv} with measured evidence "
          f"({harvest_status})")
    print("=" * 64)


if __name__ == "__main__":
    main()
