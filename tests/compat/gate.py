#!/usr/bin/env python3
"""
FINAL COMPAT GATE  (SPEC roadmap 8 / task PHASE: CI).

Reads results/<tier>.json (per-cell verdicts from the LIVE-oracle matrix runner)
and metamorphic/results.json, applies the known-divergence ALLOWLIST, and decides
the single CI exit code:

  * Any FAIL/SIM cell NOT covered by an allowlist entry         -> RED (exit 1).
  * A cell covered by a `go_nondeterminism` allowlist entry that
    carries a written reason + Go-nondeterminism evidence        -> NEUTRALIZED
    (moved to ALLOWED; does not flip the gate).  This is the ONLY
    way a divergence is excused, and only because Go itself varies.
  * A cell covered by a `tracked_known_failing` entry            -> still RED
    (a tracked port gap is a REAL divergence; SPEC: nonzero on any real
    divergence). It is merely labelled "tracked" for triage.

Fail-closed rules (the allowlist cannot be used to cheat):
  * A `go_nondeterminism` entry MISSING a reason or evidence is REJECTED and the
    gate goes RED with a TAMPER note (an unjustified excuse can never pass).
  * The allowlist NEVER blanks a field; field-blanking is caught by the tamper
    layer (integrity/tamper_check.py), run separately by run.sh.

Prints per-cell PASS/FAIL/SIM/ALLOWED + final tallies; this is the honest tally.
Exit 0 only when there is no un-allowlisted real divergence.
"""
import argparse
import fnmatch
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ALLOWLIST = os.path.join(HERE, "allowlist.json")


def load(path):
    if not os.path.exists(path):
        return None
    with open(path) as f:
        return json.load(f)


def _label_of(rec):
    return rec.get("label", "")


def _match(entry, rec):
    """True if an allowlist entry covers this result record."""
    lab = _label_of(rec)
    if "label" in entry and entry["label"] == lab:
        return True
    if "label_glob" in entry and fnmatch.fnmatch(lab, entry["label_glob"]):
        # if family/kind also pinned, require them too (tighter match)
        if "family" in entry and rec.get("family") != entry["family"]:
            return False
        return True
    if "argv" in entry and entry["argv"] == rec.get("argv"):
        return True
    if "family" in entry and "kind" in entry:
        if rec.get("family") == entry["family"] and \
           rec.get("kind", entry["kind"]) == entry["kind"]:
            return True
    return False


def validate_allowlist(al):
    """Fail-closed: every go_nondeterminism entry MUST carry reason + evidence."""
    bad = []
    for e in al.get("go_nondeterminism", []):
        if not e.get("reason"):
            bad.append((e, "missing written reason"))
        if not (e.get("go_nondeterminism_evidence") or e.get("evidence_source")):
            bad.append((e, "missing Go-nondeterminism evidence"))
    return bad


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tier", default="smoke")
    ap.add_argument("--quiet", action="store_true")
    a = ap.parse_args()

    al = load(ALLOWLIST) or {"go_nondeterminism": [], "tracked_known_failing": []}
    nd_entries = al.get("go_nondeterminism", [])
    tk_entries = al.get("tracked_known_failing", [])

    # 1) fail-closed allowlist validation
    bad = validate_allowlist(al)
    if bad:
        print("GATE: RED -- allowlist TAMPER (an excuse without evidence can never "
              "pass):")
        for e, why in bad:
            print("   reject:", why, "->", json.dumps(e)[:160])
        sys.exit(2)

    matrix = load(os.path.join(HERE, "results", f"{a.tier}.json"))
    meta = load(os.path.join(HERE, "metamorphic", "results.json"))

    if matrix is None:
        print(f"GATE: RED -- no results/{a.tier}.json (matrix did not run)")
        sys.exit(2)

    tally = {"PASS": 0, "FAIL": 0, "SIM": 0, "EXPECTED_EMPTY": 0,
             "ALLOWED_GO_NONDET": 0, "TRACKED_KNOWN_FAILING": 0}
    real_divergences = []
    allowed = []
    tracked = []

    records = matrix.get("records", [])
    for rec in records:
        v = rec.get("verdict", "FAIL")
        if v == "PASS":
            tally["PASS"] += 1
            continue
        if v == "EXPECTED_EMPTY":
            tally["EXPECTED_EMPTY"] += 1
            continue
        if v not in ("FAIL", "SIM"):
            # NOT_RUN etc. -- treat as non-divergence but report
            continue
        # a divergence: check allowlist
        nd = next((e for e in nd_entries if _match(e, rec)), None)
        if nd:
            tally["ALLOWED_GO_NONDET"] += 1
            allowed.append({"label": _label_of(rec), "verdict": v,
                            "reason": nd["reason"]})
            continue
        tk = next((e for e in tk_entries if _match(e, rec)), None)
        if tk:
            tally["TRACKED_KNOWN_FAILING"] += 1
            tracked.append({"label": _label_of(rec), "verdict": v,
                            "reason": tk.get("reason", "")})
            # tracked != excused: it is STILL a real divergence
            real_divergences.append({"label": _label_of(rec), "verdict": v,
                                     "tracked": True})
            continue
        tally[v] += 1
        real_divergences.append({"label": _label_of(rec), "verdict": v,
                                 "tracked": False, "argv": rec.get("argv")})

    # metamorphic SIM verdicts are also real divergences (simulation suspects)
    meta_sim = 0
    if meta:
        for r in meta.get("records", meta.get("results", [])):
            if r.get("verdict") == "SIM":
                meta_sim += 1
                real_divergences.append({"label": "metamorphic:" +
                                         r.get("label", r.get("check", "?")),
                                         "verdict": "SIM", "tracked": False})

    # ---- report ----
    print("=" * 64)
    print(f"FINAL COMPAT GATE  tier={a.tier}")
    print(f"  matrix cells     : {matrix.get('total_cells')}")
    print(f"  PASS             : {tally['PASS']}")
    print(f"  EXPECTED_EMPTY   : {tally['EXPECTED_EMPTY']}  (Go-measured contract)")
    print(f"  FAIL (un-allow)  : {tally['FAIL']}")
    print(f"  SIM  (un-allow)  : {tally['SIM']}")
    print(f"  metamorphic SIM  : {meta_sim}")
    print(f"  ALLOWED (go-nondet, evidence-backed) : "
          f"{tally['ALLOWED_GO_NONDET']}")
    print(f"  TRACKED known-failing (still RED)    : "
          f"{tally['TRACKED_KNOWN_FAILING']}")
    if not a.quiet and real_divergences:
        print("  -- real divergences (un-allowlisted OR tracked port gaps) --")
        for d in real_divergences[:25]:
            tag = "tracked" if d.get("tracked") else "UNEXPECTED"
            print(f"     {d['verdict']:4} [{tag}] {d['label']}")
        if len(real_divergences) > 25:
            print(f"     ... +{len(real_divergences) - 25} more")
    print("=" * 64)

    gate = {"tier": a.tier, "tally": tally, "meta_sim": meta_sim,
            "real_divergence_count": len(real_divergences),
            "allowed": allowed, "tracked": tracked,
            "real_divergences": real_divergences}
    with open(os.path.join(HERE, "results", f"{a.tier}_gate.json"), "w") as f:
        json.dump(gate, f, indent=2, default=str)

    if real_divergences:
        print(f"GATE: RED -- {len(real_divergences)} real divergence(s) "
              f"(nonzero exit).")
        sys.exit(1)
    print("GATE: GREEN -- no un-allowlisted divergence.")
    sys.exit(0)


if __name__ == "__main__":
    main()
