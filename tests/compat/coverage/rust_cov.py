#!/usr/bin/env python3
"""
RUST COVERAGE  (SPEC: specs/go-compat-testing/SPEC.md §3.6, roadmap 5)

Produces Rust line / region / (best-effort) branch coverage via cargo-llvm-cov
and emits a machine-readable summary. This is the COVERAGE-REACH facet of the
ledger; per SPEC §6 it is reported ALONGSIDE the differential verdict, NEVER as a
substitute for it. A high % here means "the tests reached this code", NOT "the
code matches Go" -- that is the oracle's job.

Design notes:
  * cargo-llvm-cov's `--branch` is nightly-only (unstable). We try it; if it
    fails we fall back to line+region coverage (region is llvm's stable proxy for
    branch reach) and record that branch was UNAVAILABLE -- we never fabricate a
    branch number.
  * We do NOT silently "declare" coverage: every number here is read back out of
    llvm-cov's own --json export (the `data[].totals` block), so it is auditable.
  * Scope is parameterizable. Running instrumented tests over all 75 crates is
    slow; the default scope is a representative set of analyzer crates (the
    compat target surface). The exact scope is RECORDED in the output so the
    number is honest about what it measured.

Usage:
  rust_cov.py --out coverage/rust_cov.json [-p CRATE ...] [--all] [--no-run]
"""

import argparse
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
RUST_ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))

# Default representative scope: the analyzer + serializer crates that the compat
# matrix actually exercises. Honest, fast, and recorded in the output.
DEFAULT_SCOPE = [
    "cf-gojson", "cf-goyaml",          # the byte-identical serializers (keystones)
    "cf-complexity", "cf-halstead", "cf-comments", "cf-imports",
    "cf-composition", "cf-cohesion", "cf-clones",
    "cf-devs", "cf-burndown-core", "cf-couples", "cf-shotness",
]


def run(cmd, **kw):
    return subprocess.run(cmd, cwd=RUST_ROOT,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kw)


def llvm_cov_json(scope, branch, do_run):
    """Run instrumented tests for `scope` and return the parsed llvm-cov --json
    export plus whether branch coverage was actually enabled."""
    branch_used = False
    if do_run:
        # 1) clean prior counters so the number reflects THIS run's scope only.
        run(["cargo", "llvm-cov", "clean", "--workspace"])
        # 2) run instrumented tests, no report yet (merge later).
        base = ["cargo", "llvm-cov", "--no-report"]
        for c in scope:
            base += ["-p", c]
        if branch:
            br = base + ["--branch"]
            p = run(br)
            if p.returncode == 0:
                branch_used = True
            else:
                # branch unavailable (stable toolchain) -> fall back, recorded.
                p = run(base)
                if p.returncode != 0:
                    return None, False, p.stderr.decode("utf-8", "replace")
        else:
            p = run(base)
            if p.returncode != 0:
                return None, False, p.stderr.decode("utf-8", "replace")
    # 3) export the JSON report from the merged counters.
    rep = ["cargo", "llvm-cov", "report", "--json"]
    if branch_used:
        rep += ["--branch"]
    p = run(rep)
    if p.returncode != 0:
        return None, branch_used, p.stderr.decode("utf-8", "replace")
    try:
        return json.loads(p.stdout), branch_used, None
    except Exception as e:
        return None, branch_used, f"parse llvm-cov json: {e}"


def summarize(cov_json, branch_used):
    """Pull the audited totals out of llvm-cov's own export."""
    data = cov_json.get("data", [])
    if not data:
        return None
    totals = data[0].get("totals", {})

    def pct(block):
        b = totals.get(block, {})
        return {"covered": b.get("covered"), "count": b.get("count"),
                "percent": b.get("percent")}

    out = {
        "lines": pct("lines"),
        "regions": pct("regions"),
        "functions": pct("functions"),
    }
    if branch_used and "branches" in totals:
        out["branches"] = pct("branches")
    else:
        out["branches"] = {"covered": None, "count": None, "percent": None,
                           "unavailable": "cargo-llvm-cov --branch is nightly-"
                           "only; region coverage is the stable branch-reach "
                           "proxy. Branch number NOT fabricated."}
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=os.path.join(HERE, "rust_cov.json"))
    ap.add_argument("-p", dest="scope", action="append", default=[],
                    help="crate to include (repeatable); default = representative set")
    ap.add_argument("--all", action="store_true",
                    help="cover the WHOLE workspace (slow)")
    ap.add_argument("--branch", action="store_true", default=True,
                    help="attempt branch coverage (falls back if unavailable)")
    ap.add_argument("--no-run", action="store_true",
                    help="skip the instrumented run; only re-export existing counters")
    a = ap.parse_args()

    if a.all:
        scope = ["--workspace"]
        scope_desc = "workspace(all crates)"
        # llvm-cov uses --workspace not -p list
        # handled by passing the literal token below
    elif a.scope:
        scope = a.scope
        scope_desc = ",".join(scope)
    else:
        scope = DEFAULT_SCOPE
        scope_desc = "representative-analyzers(" + ",".join(scope) + ")"

    t0 = time.time()
    if a.all:
        # special-case: --workspace as a single token, not "-p --workspace"
        branch_used = False
        if not a.no_run:
            run(["cargo", "llvm-cov", "clean", "--workspace"])
            base = ["cargo", "llvm-cov", "--no-report", "--workspace"]
            p = run(base + (["--branch"] if a.branch else []))
            branch_used = a.branch and p.returncode == 0
            if not branch_used:
                run(base)
        rep = ["cargo", "llvm-cov", "report", "--json"] + (
            ["--branch"] if branch_used else [])
        pr = run(rep)
        cov = json.loads(pr.stdout) if pr.returncode == 0 else None
        err = None if cov else pr.stderr.decode("utf-8", "replace")
    else:
        cov, branch_used, err = llvm_cov_json(scope, a.branch, not a.no_run)

    elapsed = round(time.time() - t0, 1)

    if cov is None:
        result = {"ok": False, "error": err, "scope": scope_desc,
                  "elapsed_s": elapsed}
        with open(a.out, "w") as f:
            json.dump(result, f, indent=2)
        print(f"rust_cov: FAILED ({err[:200] if err else '?'})", file=sys.stderr)
        sys.exit(1)

    totals = summarize(cov, branch_used)
    result = {
        "ok": True,
        "scope": scope_desc,
        "scope_kind": "workspace" if a.all else "subset",
        "branch_coverage_available": branch_used,
        "elapsed_s": elapsed,
        "totals": totals,
        "note": ("Coverage measures REACH, not correctness. Reported alongside "
                 "the differential verdict per SPEC §6, never as a substitute."),
    }
    with open(a.out, "w") as f:
        json.dump(result, f, indent=2)
    ln = totals["lines"]
    rg = totals["regions"]
    print(f"rust_cov: lines {ln['percent']:.1f}% ({ln['covered']}/{ln['count']}) "
          f"regions {rg['percent']:.1f}% branch_avail={branch_used} "
          f"scope={scope_desc[:40]} wrote {a.out}")


if __name__ == "__main__":
    main()
