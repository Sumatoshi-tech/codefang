#!/usr/bin/env python3
"""
MATRIX-CELL + PER-LANGUAGE COVERAGE  (SPEC §3.6 / §3.2, roadmap 5)

Two coverage facets that the differential oracle's REACH is measured by:

  A) MATRIX-CELL coverage: of every cell the expander enumerates for a tier
     (the full meaningful cross-product, see expand_matrix.py), how many were
     actually EXERCISED by a results run (results/<tier>.json)? An exercised cell
     is one with a recorded verdict (PASS/FAIL/SIM/EXPECTED_EMPTY). A cell present
     in the expansion but absent from results is an UNTESTED cell -- listed, never
     hidden (SPEC §3.7).

     The TOTAL is taken from the LIVE expander over the FULL tier, so it cannot be
     shrunk by sampling: if a run probed only `--sample N`, the denominator is
     still the full enumerated matrix and the gap is visible.

  B) PER-LANGUAGE parse coverage: for every tree-sitter language in the corpus,
     did the Rust `uast parse` produce a non-empty UAST where the LIVE Go binary
     does? This is measured by re-invoking the oracle's runner primitive on one
     parse cell per language (the Go binary is the oracle; we never re-derive).
     A language where Go parses but Rust does not is a parse GAP.

Output: coverage/matrix_cell_cov.json with exercised/total per family, the list
of untested cells, and the per-language parse table.

Usage:
  matrix_cell_cov.py --tier smoke --out coverage/matrix_cell_cov.json
  matrix_cell_cov.py --tier full  --probe-parse   # also run live per-lang parse
"""

import argparse
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
COMPAT = os.path.abspath(os.path.join(HERE, ".."))
EXPAND = os.path.join(COMPAT, "expand_matrix.py")
RESULTS_DIR = os.path.join(COMPAT, "results")
CORPUS_MANIFEST = os.path.join(COMPAT, "corpus", "manifest.json")

GO_DIR = "/home/dmitriy/sources/codefang/build/bin"
RU_DIR = "/home/dmitriy/sources/codefang/rust/target/release"
PINNED_ENV = {"TZ": "UTC", "NO_COLOR": "1", "LANG": "C", "LC_ALL": "C",
              "SOURCE_DATE_EPOCH": "315532800"}


def expand(tier):
    p = subprocess.run([sys.executable, EXPAND, tier],
                       stdout=subprocess.PIPE, check=True)
    return json.loads(p.stdout)


def load_results(tier):
    path = os.path.join(RESULTS_DIR, f"{tier}.json")
    if not os.path.exists(path):
        return None
    with open(path) as f:
        return json.load(f)


def matrix_cell_coverage(tier):
    exp = expand(tier)
    cells = exp["cells"]
    res = load_results(tier)
    exercised_labels = set()
    if res:
        for r in res.get("records", []):
            exercised_labels.add(r["label"])

    per_family = {}
    untested = []
    for c in cells:
        fam = c["family"]
        d = per_family.setdefault(fam, {"total": 0, "exercised": 0})
        d["total"] += 1
        if c["label"] in exercised_labels:
            d["exercised"] += 1
        else:
            untested.append({"label": c["label"], "family": fam,
                             "argv": c["argv"]})

    total = len(cells)
    exercised = total - len(untested)
    for fam, d in per_family.items():
        d["percent"] = round(100.0 * d["exercised"] / d["total"], 1) \
            if d["total"] else 0.0

    return {
        "tier": tier,
        "total_cells": total,
        "exercised_cells": exercised,
        "untested_cells": len(untested),
        "percent": round(100.0 * exercised / total, 1) if total else 0.0,
        "per_family": per_family,
        "results_present": res is not None,
        "untested": untested,
    }


def _run(side, argv):
    base = GO_DIR if side == "go" else RU_DIR
    if argv[0] == "uast":
        cmd = [os.path.join(base, "uast")] + argv[1:]
    else:
        cmd = [os.path.join(base, "codefang"), argv[0]] + argv[1:]
    env = dict(os.environ); env.update(PINNED_ENV)
    try:
        p = subprocess.run(cmd, env=env, stdout=subprocess.PIPE,
                           stderr=subprocess.PIPE, timeout=600)
        return len(p.stdout), p.stdout
    except Exception as e:
        return -1, str(e).encode()


def per_language_parse_coverage(probe):
    with open(CORPUS_MANIFEST) as f:
        man = json.load(f)
    table = []
    n_lang = 0
    n_rust_ok = 0
    for entry in man["files"]:
        lang = entry["language"]
        n_lang += 1
        stored = os.path.join(COMPAT, "corpus", entry["stored"])
        row = {"language": lang, "ext": entry["ext"],
               "oracle_uast_bytes": entry.get("oracle_uast_bytes")}
        if probe:
            gn, _ = _run("go", ["uast", "parse", "--format", "json", stored])
            rn, rb = _run("rust", ["uast", "parse", "--format", "json", stored])
            row["go_parse_bytes"] = gn
            row["rust_parse_bytes"] = rn
            go_ok = gn > 0
            rust_ok = rn > 0
            row["go_parses"] = go_ok
            row["rust_parses"] = rust_ok
            if go_ok and rust_ok:
                row["status"] = "both-parse"
                n_rust_ok += 1
            elif go_ok and not rust_ok:
                row["status"] = "GAP:rust-fails-go-parses"
            elif not go_ok:
                row["status"] = "go-does-not-parse(not-a-rust-gap)"
                n_rust_ok += 1  # not counted against Rust
        else:
            # static facet from manifest only (no live probe): Go parsed it when
            # the corpus was mined (oracle_uast_bytes>0). Rust status unknown.
            row["go_parses"] = (entry.get("oracle_uast_bytes") or 0) > 0
            row["status"] = "static(manifest-only;use --probe-parse for live)"
        table.append(row)
    out = {"languages_total": n_lang, "probed": probe, "table": table}
    if probe:
        out["rust_parse_ok"] = n_rust_ok
        out["percent"] = round(100.0 * n_rust_ok / n_lang, 1) if n_lang else 0.0
        out["gaps"] = [r["language"] for r in table
                       if r["status"].startswith("GAP")]
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tier", default="smoke", choices=["smoke", "full"])
    ap.add_argument("--out", default=os.path.join(HERE, "matrix_cell_cov.json"))
    ap.add_argument("--probe-parse", action="store_true",
                    help="run LIVE Go+Rust uast parse per language (slower)")
    a = ap.parse_args()

    mcc = matrix_cell_coverage(a.tier)
    plc = per_language_parse_coverage(a.probe_parse)

    result = {"matrix_cell": mcc, "per_language_parse": plc,
              "note": ("Coverage measures REACH only; correctness is the "
                       "oracle's verdict. Untested cells are listed, not hidden.")}
    with open(a.out, "w") as f:
        json.dump(result, f, indent=2)
    print(f"matrix_cell[{a.tier}]: {mcc['exercised_cells']}/{mcc['total_cells']} "
          f"({mcc['percent']}%) exercised, {mcc['untested_cells']} untested")
    if a.probe_parse:
        print(f"per_language_parse: {plc['rust_parse_ok']}/{plc['languages_total']} "
              f"({plc['percent']}%) gaps={plc.get('gaps')}")
    else:
        print(f"per_language_parse: {plc['languages_total']} langs (static; "
              f"pass --probe-parse for live Rust status)")
    print(f"wrote {a.out}")


if __name__ == "__main__":
    main()
