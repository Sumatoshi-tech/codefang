#!/usr/bin/env python3
"""
MATRIX RUNNER  (SPEC: specs/go-compat-testing/SPEC.md §3.2/§3.4, roadmap 3).

Wires EVERY expanded matrix cell to the LIVE GO ORACLE (oracle/oracle.py):

  for each cell:
      run the oracle on cell.argv  ->  verdict PASS / FAIL / SIM
      if the LIVE Go binary produced NO stdout for this cell, the cell is recorded
      as an EXPECTED-EMPTY CONTRACT (kind="expected-empty") -- a contract, NOT a
      skip. (A later run where Go starts producing output, or Rust diverges from
      Go's emptiness, breaks the contract.)

The oracle is the ONLY source of truth: this runner never re-derives expected
output. It only dispatches cell argvs to the oracle and tallies verdicts. The
matrix definition is hashed so a shrunk matrix is detectable (tamper layer).

Output: results/<tier>.json  with per-cell verdicts + a coverage summary.
"""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import threading

HERE = os.path.dirname(os.path.abspath(__file__))
ORACLE = os.path.join(HERE, "oracle", "oracle.py")
EXPAND = os.path.join(HERE, "expand_matrix.py")
RESULTS_DIR = os.path.join(HERE, "results")

GO_DIR = "/home/dmitriy/sources/codefang/build/bin"
PINNED_ENV = {"TZ": "UTC", "NO_COLOR": "1", "LANG": "C", "LC_ALL": "C",
              "SOURCE_DATE_EPOCH": "315532800"}


def matrix_hash():
    """Content hash of the matrix definition + expander + corpus manifest.
    The tamper layer (later phase) compares this against a recorded baseline; we
    surface it here so a shrunk matrix changes the hash."""
    h = hashlib.sha256()
    for p in (os.path.join(HERE, "matrix.toml"), EXPAND,
              os.path.join(HERE, "corpus", "manifest.json")):
        with open(p, "rb") as f:
            h.update(f.read())
    return h.hexdigest()


def expand(tier):
    out = subprocess.run([sys.executable, EXPAND, tier],
                         stdout=subprocess.PIPE, check=True)
    return json.loads(out.stdout)["cells"]


def _resolve_go(argv):
    if argv[0] == "uast":
        return [os.path.join(GO_DIR, "uast")] + argv[1:]
    return [os.path.join(GO_DIR, "codefang"), argv[0]] + argv[1:]


def go_is_empty(argv):
    """Run the LIVE Go binary once; True iff it produced no stdout bytes.
    This is how an EXPECTED-EMPTY contract is MEASURED from the oracle, not
    declared."""
    env = dict(os.environ); env.update(PINNED_ENV)
    try:
        p = subprocess.run(_resolve_go(argv), env=env,
                           stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                           timeout=600)
    except Exception:
        return False
    return len(p.stdout) == 0


def run_oracle(argv, n_go):
    p = subprocess.run([sys.executable, ORACLE, "--n-go", str(n_go), "--quiet",
                        "--"] + argv, stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE)
    verdict = {0: "PASS", 1: "FAIL", 3: "SIM"}.get(p.returncode, "FAIL")
    return verdict, (p.stdout + p.stderr).decode("utf-8", "replace")


def run_cell(c, n_go, dry_run):
    """Process one matrix cell (parallel-safe: pure subprocess dispatch to the
    LIVE oracle / Go binary). Returns (record, detail_or_None)."""
    argv = c["argv"]
    if go_is_empty(argv):
        return ({**c, "verdict": "EXPECTED_EMPTY",
                 "contract": "go-produces-no-stdout"}, None)
    if dry_run:
        return ({**c, "verdict": "NOT_RUN(go-nonempty)"}, None)
    verdict, detail = run_oracle(argv, n_go)
    return ({**c, "verdict": verdict}, detail)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tier", default="smoke", choices=["smoke", "full"])
    ap.add_argument("--n-go", type=int, default=3)
    ap.add_argument("--sample", type=int, default=0,
                    help="run only the first N cells (for fast self-check)")
    ap.add_argument("--family", default="", help="only cells of this family")
    ap.add_argument("--jobs", type=int, default=1,
                    help="parallel cells (each cell is an independent oracle "
                         "subprocess; speeds the smoke tier WITHOUT shrinking the "
                         "matrix -- every enumerated cell still runs)")
    ap.add_argument("--dry-run", action="store_true",
                    help="only enumerate + classify expected-empty; no oracle")
    ap.add_argument("--no-resume", dest="resume", action="store_false",
                    help="ignore + clear any prior partial results; clean run")
    ap.set_defaults(resume=True)
    a = ap.parse_args()

    cells = expand(a.tier)
    if a.family:
        cells = [c for c in cells if c["family"] == a.family]
    if a.sample > 0:
        cells = cells[:a.sample]

    os.makedirs(RESULTS_DIR, exist_ok=True)
    tally = {"PASS": 0, "FAIL": 0, "SIM": 0, "EXPECTED_EMPTY": 0}
    records = [None] * len(cells)

    # --- incremental + resume -------------------------------------------------
    # Each completed cell is appended (label-keyed) to results/<tier>.partial.jsonl
    # and fsync'd, so a long full run is observable live and survives a crash. On
    # startup, already-completed labels are reloaded and their cells skipped, so a
    # restart NEVER repeats work already done. --no-resume forces a clean run.
    partial_path = os.path.join(RESULTS_DIR, f"{a.tier}.partial.jsonl")
    done = {}
    if a.resume and os.path.exists(partial_path):
        with open(partial_path) as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    r = json.loads(line)
                    done[r["label"]] = r
                except Exception:
                    pass  # tolerate a torn last line from a crash mid-write
    elif not a.resume and os.path.exists(partial_path):
        os.remove(partial_path)

    pf = open(partial_path, "a")
    wlock = threading.Lock()

    def absorb(idx, rec, detail):
        v = rec["verdict"]
        tally[v] = tally.get(v, 0) + 1
        mark = {"PASS": "PASS ", "FAIL": "FAIL!", "SIM": "SIM! ",
                "EXPECTED_EMPTY": "EMPTY"}.get(v, v)
        nd = sum(1 for r in records if r is not None) + 1
        print(f"  [{nd}/{len(cells)}] {mark} {cells[idx]['label']}", flush=True)
        if v not in ("PASS", "EXPECTED_EMPTY") and detail:
            for line in detail.splitlines()[:4]:
                print("        " + line, flush=True)
        records[idx] = rec
        with wlock:
            pf.write(json.dumps(rec, default=str) + "\n")
            pf.flush()
            os.fsync(pf.fileno())

    # split cells into already-done (replayed) and pending (to run)
    # Only replay SETTLED-GREEN cells (PASS / EXPECTED_EMPTY). A prior FAIL/SIM is
    # NEVER trusted on resume -- after a Rust fix+rebuild those cells must be
    # re-measured against the live oracle. This makes resume safe for the burndown
    # loop: expensive green kubernetes-static cells stay cached, red cells re-run.
    GREEN = ("PASS", "EXPECTED_EMPTY")
    pending = []
    for i, c in enumerate(cells):
        prior = done.get(c["label"])
        if prior is not None and prior.get("verdict") in GREEN:
            v = prior["verdict"]
            tally[v] = tally.get(v, 0) + 1
            records[i] = prior
        else:
            pending.append((i, c))
    if done:
        print(f"== resume: {len(done)} cell(s) already done, "
              f"{len(pending)} pending", flush=True)

    if a.jobs <= 1:
        for i, c in pending:
            rec, detail = run_cell(c, a.n_go, a.dry_run)
            absorb(i, rec, detail)
    else:
        from concurrent.futures import ThreadPoolExecutor, as_completed
        with ThreadPoolExecutor(max_workers=a.jobs) as ex:
            futs = {ex.submit(run_cell, c, a.n_go, a.dry_run): i
                    for i, c in pending}
            for fut in as_completed(futs):
                i = futs[fut]
                rec, detail = fut.result()
                absorb(i, rec, detail)

    pf.close()

    summary = {
        "tier": a.tier,
        "matrix_hash": matrix_hash(),
        "total_cells": len(cells),
        "tally": tally,
        "records": records,
    }
    outp = os.path.join(RESULTS_DIR, f"{a.tier}.json")
    with open(outp, "w") as f:
        json.dump(summary, f, indent=2, default=str)
    print(f"\n== {a.tier}: {tally}  matrix_hash={summary['matrix_hash'][:12]}")
    print(f"wrote {outp}")
    # nonzero exit on any real divergence
    if tally["FAIL"] or tally["SIM"]:
        sys.exit(1)


if __name__ == "__main__":
    main()
