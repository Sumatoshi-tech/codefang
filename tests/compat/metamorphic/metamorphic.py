#!/usr/bin/env python3
"""
METAMORPHIC / ANTI-SIMULATION LAYER
(SPEC: specs/go-compat-testing/SPEC.md §3.6, roadmap 4)

Per-analyzer RELATIONAL checks that FAIL hardcoded stubs. Unlike the oracle
(which compares Rust bytes to Go bytes directly), these checks assert RELATIONS
between Rust outputs across DIFFERENT invocations -- the relations a real
analyzer obeys and a constant/stub does not.

THE GO BINARY IS THE ORACLE FOR THE PREMISE OF EVERY CHECK. We never assert
"Rust output must differ" from a re-derived expectation; we assert "Rust output
must differ BECAUSE Go's output differs on these same two inputs". If Go does
NOT exhibit the relation (e.g. Go does not grow with --limit on this cell, Go is
empty here), the premise is absent and the check is recorded as N/A -- it never
fakes a verdict. This keeps the live Go binary as the source of truth (rule #1)
and uses freshly-substituted corpus inputs the author never hand-picked (rule #2).

Properties implemented (task brief §3.6 a-e):

  (a) VARY-INPUT     : output DIFFERS for two different inputs WHERE GO DIFFERS.
                       Rust giving identical bytes for two inputs that Go
                       distinguishes is the hardcoded-constant signature => SIM.
  (b) GROW-LIMIT     : output GROWS monotonically with --limit (10 vs 500)
                       WHERE GO GROWS. Rust constant across limits while Go grows
                       is the closed-form-stub signature => SIM.
  (c) DETERMINISM    : identical args => identical Rust bytes across repeated runs.
                       A faithful port is deterministic; differing bytes => FAIL.
  (d) NON-EMPTY      : Rust is NON-EMPTY wherever GO is NON-EMPTY. A 0-byte stub
                       where Go computes something => FAIL.
  (e) GOLDEN-DRIFT   : once the INPUT changed, Rust output never equals a
                       previously-recorded golden constant. A frozen golden that
                       survives an input change is a memorized constant => SIM.

Any failure prints, for SIM, the TWO INPUTS and the TWO EQUAL RUST OUTPUTS that
triggered it (task brief: "printing the two inputs and the two equal Rust
outputs"); for FAIL, the failing invocation + the missing/inconsistent property.

Verdicts: PASS (relation holds), SIM (simulation suspect), FAIL (determinism /
non-emptiness violated), NA (Go did not exhibit the premise here -> not a probe).

Execution reuses oracle.run_once so BOTH binaries run under the SAME pinned env
(TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800) and we compare
STDOUT only (stderr is progress) -- exactly rule #5. We do NOT re-implement
process launching here; the oracle module is the single launcher.
"""

import argparse
import hashlib
import importlib.util
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ORACLE_PY = os.path.join(HERE, "..", "oracle", "oracle.py")

# Import the oracle module: we reuse its LIVE-binary launcher (run_once) and sha
# so there is exactly ONE place that decides how to invoke Go/Rust under the
# pinned env. Re-deriving expected output is forbidden; we only run the binaries.
_spec = importlib.util.spec_from_file_location("oracle", ORACLE_PY)
oracle = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(oracle)

run_once = oracle.run_once   # (side, argv) -> (rc, stdout_bytes); side in {go,rust}
sha = oracle.sha

CORPUS = os.path.join(HERE, "..", "corpus")
MANIFEST = os.path.join(CORPUS, "manifest.json")
GOLDEN = os.path.join(HERE, "golden_constants.json")

# Repos for analyzer (run) checks: small first for speed. Inputs the author never
# hand-curated as golden args -- mined corpus repos from the manifest.
HERCULES = "/home/dmitriy/sources/hercules"
KUBE = "/home/dmitriy/sources/kubernetes"

RUN_BASE = ["run", "--checkpoint=false", "--resume=false", "--no-cache",
            "--workers", "1"]
STATIC_BASE = ["run", "--checkpoint=false", "--resume=false", "--no-cache",
               "--head", "--workers", "1", "--static-workers", "1"]


def go(argv):
    return run_once("go", argv)[1]


def ru(argv):
    return run_once("rust", argv)[1]


# --------------------------------------------------------------------------- #
# Verdict accumulation.
# --------------------------------------------------------------------------- #
class Report:
    def __init__(self):
        self.records = []
        self.tally = {"PASS": 0, "SIM": 0, "FAIL": 0, "NA": 0}

    def add(self, prop, label, verdict, detail):
        self.tally[verdict] = self.tally.get(verdict, 0) + 1
        self.records.append({"property": prop, "label": label,
                             "verdict": verdict, **detail})
        mark = {"PASS": "PASS ", "SIM": "SIM! ", "FAIL": "FAIL!",
                "NA": "n/a  "}[verdict]
        print(f"  {mark}[{prop}] {label}")
        if verdict in ("SIM", "FAIL"):
            for k, v in detail.items():
                vs = v if isinstance(v, str) else json.dumps(v, default=str)
                print(f"        {k}: {vs[:400]}")

    def failed(self):
        return self.tally["SIM"] + self.tally["FAIL"]


# --------------------------------------------------------------------------- #
# (a) VARY-INPUT: Rust must DIFFER on two inputs WHERE GO DIFFERS.
# --------------------------------------------------------------------------- #
def check_vary_input(rep, prop_label, argvA, argvB, inputA, inputB):
    gA, gB = go(argvA), go(argvB)
    if not gA or not gB:
        rep.add("vary-input", prop_label, "NA",
                {"reason": "go empty on one side -> no differ premise",
                 "go_a_len": len(gA), "go_b_len": len(gB)})
        return
    if sha(gA) == sha(gB):
        # Go does not distinguish these two inputs -> no premise to demand Rust
        # differ. (Never re-derive: if Go doesn't differ, we don't claim Rust must.)
        rep.add("vary-input", prop_label, "NA",
                {"reason": "go IDENTICAL on both inputs -> no differ premise",
                 "input_a": inputA, "input_b": inputB})
        return
    rA, rB = ru(argvA), ru(argvB)
    if sha(rA) == sha(rB):
        # THE SIMULATION SIGNATURE: Rust constant where Go varies.
        rep.add("vary-input", prop_label, "SIM", {
            "SIMULATION_SUSPECT": "rust emits identical bytes for two inputs Go "
                                  "distinguishes (hardcoded-constant signature)",
            "input_a": inputA, "input_b": inputB,
            "rust_out_a_sha": sha(rA), "rust_out_b_sha": sha(rB),
            "rust_out_a_head": rA[:200].decode("utf-8", "replace"),
            "rust_out_b_head": rB[:200].decode("utf-8", "replace"),
            "go_a_sha": sha(gA), "go_b_sha": sha(gB),
        })
        return
    rep.add("vary-input", prop_label, "PASS",
            {"go_differs": True, "rust_differs": True})


# --------------------------------------------------------------------------- #
# (b) GROW-LIMIT: Rust output GROWS with --limit (10 vs 500) WHERE GO GROWS.
# --------------------------------------------------------------------------- #
def _with_limit(argv, n):
    a = list(argv)
    if "--limit" in a:
        i = a.index("--limit")
        a[i + 1] = str(n)
        return a
    # insert before the trailing positional repo path
    return a[:-1] + ["--limit", str(n)] + a[-1:]


def check_grow_limit(rep, label, base_argv, lo=10, hi=500):
    aLo, aHi = _with_limit(base_argv, lo), _with_limit(base_argv, hi)
    gLo, gHi = go(aLo), go(aHi)
    if not gLo or not gHi:
        rep.add("grow-limit", label, "NA",
                {"reason": "go empty at a limit -> no grow premise",
                 "go_lo_len": len(gLo), "go_hi_len": len(gHi)})
        return
    if len(gHi) <= len(gLo) or sha(gLo) == sha(gHi):
        # Go does NOT grow with limit here (analyzer saturates / aggregate-only).
        # No premise to demand Rust grow. We do not invent a growth expectation.
        rep.add("grow-limit", label, "NA",
                {"reason": "go does NOT grow with --limit here -> no grow premise",
                 "go_lo_len": len(gLo), "go_hi_len": len(gHi)})
        return
    rLo, rHi = ru(aLo), ru(aHi)
    if sha(rLo) == sha(rHi):
        # Rust constant across limits while Go grows: closed-form-stub signature.
        rep.add("grow-limit", label, "SIM", {
            "SIMULATION_SUSPECT": "rust CONSTANT across --limit while Go GROWS "
                                  "(closed-form-stub signature)",
            "input_a": " ".join(aLo), "input_b": " ".join(aHi),
            "rust_out_a_sha": sha(rLo), "rust_out_b_sha": sha(rHi),
            "rust_out_a_len": len(rLo), "rust_out_b_len": len(rHi),
            "go_lo_len": len(gLo), "go_hi_len": len(gHi),
        })
        return
    if len(rHi) < len(rLo):
        # Rust SHRINKS where Go grows: not the constant signature but still a real
        # relational divergence -> FAIL (the monotonicity contract Go obeys).
        rep.add("grow-limit", label, "FAIL", {
            "reason": "rust output SHRINKS with --limit while Go GROWS",
            "input_a": " ".join(aLo), "input_b": " ".join(aHi),
            "rust_lo_len": len(rLo), "rust_hi_len": len(rHi),
            "go_lo_len": len(gLo), "go_hi_len": len(gHi),
        })
        return
    rep.add("grow-limit", label, "PASS",
            {"go_grows": f"{len(gLo)}->{len(gHi)}",
             "rust_grows": f"{len(rLo)}->{len(rHi)}"})


# --------------------------------------------------------------------------- #
# (c) DETERMINISM: identical args => identical Rust bytes.
# --------------------------------------------------------------------------- #
def check_determinism(rep, label, argv, reps=3):
    shas = [sha(ru(argv)) for _ in range(reps)]
    if len(set(shas)) == 1:
        rep.add("determinism", label, "PASS", {"sha": shas[0], "runs": reps})
    else:
        rep.add("determinism", label, "FAIL", {
            "reason": "rust NONDETERMINISTIC: identical args produced differing "
                      "bytes (a faithful port is deterministic)",
            "input": " ".join(argv), "distinct_shas": sorted(set(shas)),
        })


# --------------------------------------------------------------------------- #
# (d) NON-EMPTY: Rust non-empty WHERE GO is non-empty.
# --------------------------------------------------------------------------- #
def check_nonempty(rep, label, argv):
    g = go(argv)
    if not g:
        rep.add("non-empty", label, "NA",
                {"reason": "go empty here -> no non-empty premise"})
        return
    r = ru(argv)
    if not r:
        rep.add("non-empty", label, "FAIL", {
            "reason": "rust EMPTY where Go is NON-EMPTY (0-byte stub / not ported)",
            "input": " ".join(argv), "go_len": len(g), "rust_len": len(r),
        })
        return
    rep.add("non-empty", label, "PASS", {"go_len": len(g), "rust_len": len(r)})


# --------------------------------------------------------------------------- #
# (e) GOLDEN-DRIFT: once the input changed, Rust output never equals a
#     previously-recorded golden constant.
# --------------------------------------------------------------------------- #
def load_golden():
    if os.path.exists(GOLDEN):
        with open(GOLDEN) as f:
            return json.load(f)
    return {}


def save_golden(g):
    with open(GOLDEN, "w") as f:
        json.dump(g, f, indent=2, sort_keys=True)


def check_golden_drift(rep, label, key, base_argv, golden, mutate_argv,
                       input_orig, input_mut):
    """
    Record (or compare against) a golden constant for `key` on base_argv. Then run
    Rust on a DIFFERENT input (mutate_argv). If Go's output ALSO changed between
    the two inputs (so the golden is genuinely stale for the new input) but Rust
    STILL emits the recorded golden bytes, Rust memorized a constant => SIM.
    """
    # establish/refresh the golden for the ORIGINAL input
    r_orig = ru(base_argv)
    g_orig = go(base_argv)
    if not g_orig:
        rep.add("golden-drift", label, "NA",
                {"reason": "go empty on original input"})
        return
    recorded = golden.get(key)
    cur = sha(r_orig)
    if recorded is None:
        golden[key] = {"sha": cur, "input": input_orig,
                       "len": len(r_orig)}
        recorded = golden[key]

    # now the INPUT CHANGES. Does Go distinguish the two inputs?
    g_mut = go(mutate_argv)
    if not g_mut or sha(g_orig) == sha(g_mut):
        rep.add("golden-drift", label, "NA", {
            "reason": "go did NOT change across the input mutation -> golden may "
                      "legitimately persist; no drift premise",
            "input_orig": input_orig, "input_mut": input_mut})
        return
    r_mut = ru(mutate_argv)
    if sha(r_mut) == recorded["sha"]:
        # Rust still emits the golden constant after the input changed AND Go
        # changed: a memorized golden constant.
        rep.add("golden-drift", label, "SIM", {
            "SIMULATION_SUSPECT": "rust output EQUALS the recorded golden constant "
                                  "even though the INPUT changed and Go changed "
                                  "(memorized-golden signature)",
            "input_a": input_orig, "input_b": input_mut,
            "rust_out_a_sha": recorded["sha"], "rust_out_b_sha": sha(r_mut),
            "rust_out_a_head": r_orig[:200].decode("utf-8", "replace"),
            "rust_out_b_head": r_mut[:200].decode("utf-8", "replace"),
            "golden_key": key,
        })
        return
    rep.add("golden-drift", label, "PASS", {
        "drifted_from_golden": True, "golden_sha": recorded["sha"],
        "rust_new_sha": sha(r_mut)})


# --------------------------------------------------------------------------- #
# Cell builders: which analyzers/inputs to probe. Inputs are mined corpus
# (files + repos), NOT recorded golden args.
# --------------------------------------------------------------------------- #
def history_argv(analyzer, repo, fmt="json", limit=20):
    return RUN_BASE + ["--analyzers", analyzer, "--format", fmt,
                       "--limit", str(limit), repo]


def static_argv(analyzer, path, fmt="json"):
    return STATIC_BASE + ["-p", path, "--analyzers", analyzer, "--format", fmt]


def uast_argv(sub, stored, fmt="json", dsl=None):
    if sub == "query":
        return ["uast", "query", dsl or 'filter(.roles has "Function")',
                "--format", fmt, "-i", stored]
    if sub == "analyze":
        return ["uast", "analyze", "--format", fmt, "-i", stored]
    return ["uast", "parse", "--format", fmt, stored]


def corpus_files():
    with open(MANIFEST) as f:
        m = json.load(f)
    out = []
    for e in m["files"]:
        out.append((e["language"], os.path.join(CORPUS, e["stored"])))
    return out


HISTORY_ANALYZERS = ["history/devs", "history/burndown", "history/imports",
                     "history/couples", "history/shotness", "history/typos",
                     "history/file-history", "history/sentiment",
                     "history/quality", "history/anomaly"]
STATIC_ANALYZERS = ["static/complexity", "static/composition",
                    "static/halstead", "static/comments", "static/imports",
                    "static/clones", "static/cohesion"]


def run_smoke(rep, golden):
    files = corpus_files()
    # pick two DIFFERENT files of the SAME language family where possible for the
    # vary-input premise; here we use two different languages' files which Go
    # certainly distinguishes.
    by_lang = {l: p for l, p in files}

    print("-- (a) vary-input: history analyzers, two different repos --")
    # hercules vs kubernetes (Go distinguishes; both real mined repos)
    for an in ["history/devs", "history/burndown", "history/imports"]:
        argvA = history_argv(an, HERCULES, limit=20)
        argvB = history_argv(an, KUBE, limit=20)
        check_vary_input(rep, f"{an} hercules-vs-kube",
                         argvA, argvB, HERCULES, KUBE)

    print("-- (a) vary-input: uast parse, two different files --")
    pairs = [("go", "python"), ("c", "rust"), ("typescript", "javascript")]
    for la, lb in pairs:
        if la in by_lang and lb in by_lang:
            fa, fb = by_lang[la], by_lang[lb]
            check_vary_input(rep, f"uast/parse {la}-vs-{lb}",
                             uast_argv("parse", fa), uast_argv("parse", fb),
                             fa, fb)

    print("-- (b) grow-limit: history analyzers (10 vs 500) on kubernetes --")
    for an in ["history/devs", "history/imports", "history/burndown",
               "history/typos", "history/couples", "history/shotness"]:
        check_grow_limit(rep, f"{an}@kube", history_argv(an, KUBE), lo=10, hi=500)

    print("-- (c) determinism: history + static + uast --")
    check_determinism(rep, "history/devs@hercules",
                      history_argv("history/devs", HERCULES, limit=20))
    check_determinism(rep, "static/complexity@hercules",
                      static_argv("static/complexity", HERCULES))
    f_go = by_lang.get("go")
    if f_go:
        check_determinism(rep, "uast/parse go", uast_argv("parse", f_go))

    print("-- (d) non-empty where Go non-empty: every analyzer --")
    for an in HISTORY_ANALYZERS:
        check_nonempty(rep, f"{an}@hercules",
                       history_argv(an, HERCULES, limit=20))
    for an in STATIC_ANALYZERS:
        check_nonempty(rep, f"{an}@hercules", static_argv(an, HERCULES))
    for lang, path in files:
        check_nonempty(rep, f"uast/parse {lang}", uast_argv("parse", path))

    print("-- (e) golden-drift: input changes, Rust must not echo old golden --")
    # original = hercules limit 20; mutated = kubernetes limit 20 (Go distinguishes)
    for an in ["history/devs", "history/imports"]:
        check_golden_drift(
            rep, f"{an} drift", f"{an}|hercules->kube",
            history_argv(an, HERCULES, limit=20), golden,
            history_argv(an, KUBE, limit=20),
            f"{HERCULES}@lim20", f"{KUBE}@lim20")


def main():
    ap = argparse.ArgumentParser(description="Metamorphic / anti-simulation layer")
    ap.add_argument("--tier", default="smoke", choices=["smoke"])
    ap.add_argument("--out", default=os.path.join(HERE, "results.json"))
    ap.add_argument("--no-golden-write", action="store_true",
                    help="do not persist newly-recorded golden constants")
    a = ap.parse_args()

    print("================ METAMORPHIC / ANTI-SIMULATION LAYER ================")
    print("(relational checks vs the LIVE Go premise; SIM => simulation suspect)")
    print()
    golden = load_golden()
    rep = Report()
    run_smoke(rep, golden)
    if not a.no_golden_write:
        save_golden(golden)

    summary = {"tier": a.tier, "tally": rep.tally, "records": rep.records}
    with open(a.out, "w") as f:
        json.dump(summary, f, indent=2, default=str)
    print()
    print("================ RESULT ================")
    print(f"PASS={rep.tally['PASS']}  SIM={rep.tally['SIM']}  "
          f"FAIL={rep.tally['FAIL']}  NA={rep.tally['NA']}")
    print(f"wrote {a.out}")
    if rep.failed():
        print("METAMORPHIC: RED -- simulation suspect or relational divergence")
        sys.exit(1)
    print("METAMORPHIC: GREEN -- all relational premises Go exhibits, Rust honors")
    sys.exit(0)


if __name__ == "__main__":
    main()
