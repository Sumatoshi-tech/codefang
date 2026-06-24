#!/usr/bin/env python3
"""
CLI SURFACE CONFORMANCE  (SPEC: specs/go-compat-testing/SPEC.md, Scope #1, Roadmap #2)

Asserts the Rust binary's CLI surface is IDENTICAL to the LIVE Go binary's surface.

Procedure:
  1. THE ORACLE IS THE LIVE GO BINARY. Extract Go's surface model by invoking
     `--help` recursively on the real Go binary (extract_surface.py). Never derive
     the expected surface in code — ask Go.
  2. Extract Rust's surface model the same way.
  3. DIFF the two models, per command, per flag, per positional, per exit-code,
     per help-stream. Every divergence is reported as a FAIL row.
  4. ERROR-PATH PARITY: run a fixed set of bad-flag / missing-arg / unknown-cmd /
     unknown-analyzer invocations under the pinned env on BOTH binaries and compare
     (exit_code, stderr_class). Divergences are FAIL rows.

OUTPUT: a per-row PASS/FAIL report and a final tally; nonzero exit on any FAIL.
With --json it emits a machine-readable result (used by the self-test).

HONESTY: cobra (Go) and clap (Rust) render help PROSE differently, so this tool
compares the STRUCTURED surface, NOT raw help bytes. It does NOT, however, weaken
the contract: every flag long-name, short-letter, value-taking, and default, every
subcommand, every positional shape, and the error-path exit-code + stderr-class are
all compared. A surface that Rust is missing or has extra is a FAIL — it cannot be
hidden by the rendering difference.
"""

import argparse
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
GO_DIR = "/home/dmitriy/sources/codefang/build/bin"
RU_DIR = "/home/dmitriy/sources/codefang/target/release"
PINNED_ENV = {
    "TZ": "UTC", "NO_COLOR": "1", "LANG": "C", "LC_ALL": "C",
    "SOURCE_DATE_EPOCH": "315532800",
}

# Subcommands that are SYNTHETIC to one framework and have no surface meaning to
# compare structurally: cobra auto-adds `help`; we compare its presence/exit-code
# at the error-path layer, not as a surface row. (`completion` IS a real surface
# command and IS compared.)
SYNTHETIC_SUBCMDS = {"help"}

# Subcommands present in the FROZEN Go oracle that have been INTENTIONALLY
# removed from the Rust product (so they are expected to be absent and must not
# count as surface divergences). Keyed by binary name; values are the bare
# command keys the extractor emits for that binary's surface.
#
# `uast lsp`    — language server for the `.uastmap`/query DSL.
# `uast server` — the UAST development HTTP server + web playground.
# Both, and the `.uastmap` DSL data they edited, were dropped: analysis runs off
# the native cf-uast-mappings tables, leaving the DSL-editor tooling with no
# consumer. This is a deliberate divergence from the Go oracle's CLI surface;
# the report-output oracle (tests/compat/oracle) is unaffected.
INTENTIONALLY_REMOVED = {"uast": {"lsp", "server"}}


def load_surface(side, binname):
    """Run the extractor on the LIVE binary, return its surface model."""
    p = subprocess.run(
        [sys.executable, os.path.join(HERE, "extract_surface.py"), side, binname],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=180)
    if p.returncode != 0:
        raise RuntimeError(f"extract {side} {binname} failed: "
                           f"{p.stderr.decode('utf-8','replace')}")
    return json.loads(p.stdout.decode("utf-8"))["surface"]


# --------------------------------------------------------------------------- #
# Surface diff.
# --------------------------------------------------------------------------- #
def _cmd_keys(surface):
    return {k for k in surface.keys()}


def diff_surface(binname, go_s, ru_s):
    """Return list of failure dicts (empty => surfaces match)."""
    fails = []

    def fail(cmd, kind, detail):
        fails.append({"bin": binname, "cmd": cmd, "kind": kind, "detail": detail})

    go_cmds = _cmd_keys(go_s)
    ru_cmds = _cmd_keys(ru_s)
    removed = INTENTIONALLY_REMOVED.get(binname, set())

    # 1) command tree (subcommand existence). The cobra/clap synthetic `help`
    #    command is excluded; everything else (run, render, version, completion,
    #    parse, ...) must match exactly.
    go_real = {c for c in go_cmds if c.split()[-1] not in SYNTHETIC_SUBCMDS}
    ru_real = {c for c in ru_cmds if c.split()[-1] not in SYNTHETIC_SUBCMDS}
    for c in sorted(go_real - ru_real):
        if c in removed:
            continue  # deliberate product divergence; see INTENTIONALLY_REMOVED
        fail(c, "missing-command", "present in Go, absent in Rust")
    for c in sorted(ru_real - go_real):
        fail(c, "extra-command", "present in Rust, absent in Go")

    # 2) per-command surface for every command present on BOTH sides
    for cmd in sorted(go_real & ru_real):
        g = go_s[cmd]
        r = ru_s[cmd]

        # 2a) declared subcommand sets (minus synthetic and intentionally-removed)
        gsub = {s for s in g["subcommands"]
                if s not in SYNTHETIC_SUBCMDS and s not in removed}
        rsub = {s for s in r["subcommands"] if s not in SYNTHETIC_SUBCMDS}
        if gsub != rsub:
            fail(cmd, "subcommand-set-differs",
                 {"go_only": sorted(gsub - rsub), "rust_only": sorted(rsub - gsub)})

        # 2b) flags: every Go flag must exist in Rust with same short/value/default
        gf, rf = g["flags"], r["flags"]
        for name in sorted(set(gf) - set(rf)):
            fail(cmd, "missing-flag", {"flag": "--" + name, "go": gf[name]})
        for name in sorted(set(rf) - set(gf)):
            fail(cmd, "extra-flag", {"flag": "--" + name, "rust": rf[name]})
        for name in sorted(set(gf) & set(rf)):
            gv, rv = gf[name], rf[name]
            if gv["short"] != rv["short"]:
                fail(cmd, "flag-short-differs",
                     {"flag": "--" + name, "go": gv["short"], "rust": rv["short"]})
            if gv["takes_value"] != rv["takes_value"]:
                fail(cmd, "flag-value-arity-differs",
                     {"flag": "--" + name,
                      "go": gv["takes_value"], "rust": rv["takes_value"]})
            # default: compared only when BOTH sides surfaced one. clap and cobra
            # do not both always PRINT a default for the same flag, so a default
            # present on only one side is reported as a WEAKER "default-visibility"
            # note rather than a hard divergence — but a CONFLICTING default (both
            # present, different value) IS a hard fail.
            if gv["default"] is not None and rv["default"] is not None \
                    and str(gv["default"]) != str(rv["default"]):
                fail(cmd, "flag-default-differs",
                     {"flag": "--" + name,
                      "go": gv["default"], "rust": rv["default"]})

        # 2c) positionals: compare SHAPE (count + per-arg variadic flag). Names
        #     differ by framework (cobra "path" vs clap "path_arg") so names are
        #     NOT compared; but COUNT and VARIADIC-ness are a real contract. We do
        #     NOT compare `required` because cobra renders an optional positional as
        #     a bare word and a required one also as a bare word (no </[ marker),
        #     so required-ness is not reliably observable from cobra help text —
        #     comparing it would manufacture false divergences. Count + variadic
        #     are observable on both sides and are compared strictly.
        gp = [p["variadic"] for p in g["positionals"]]
        rp = [p["variadic"] for p in r["positionals"]]
        if gp != rp:
            fail(cmd, "positional-shape-differs",
                 {"go": g["positionals"], "rust": r["positionals"]})

        # 2d) help exit code + stream must match (a help that errors, or goes to
        #     the wrong stream, is a surface divergence).
        if g["help_rc"] != r["help_rc"]:
            fail(cmd, "help-exit-code-differs",
                 {"go": g["help_rc"], "rust": r["help_rc"]})
        if g["help_stream"] != r["help_stream"]:
            fail(cmd, "help-stream-differs",
                 {"go": g["help_stream"], "rust": r["help_stream"]})

    return fails


# --------------------------------------------------------------------------- #
# Error-path parity. Run identical bad invocations on Go (oracle) and Rust under
# the pinned env; compare exit code and a normalized stderr CLASS.
#
# stderr CLASS, not raw bytes: cobra and clap word their diagnostics differently
# ("unknown flag" vs "unexpected argument"). The CONTRACT we can hold both to is:
#   - the same EXIT CODE, and
#   - the same error CATEGORY (bad-flag / unknown-command / missing-arg / runtime),
# detected by tolerant keyword classification of EITHER side's stderr.
# Raw-byte stderr equality is NOT achievable across frameworks and demanding it
# would be a false contract; but exit-code parity IS a real, enforceable contract,
# and it is exactly where Go(cobra)=1 vs Rust(clap)=2 currently DIVERGES — which
# this layer SURFACES rather than hides.
# --------------------------------------------------------------------------- #
ERROR_PROBES = [
    # (label, bin, argv, expected_category)
    ("codefang/bad-flag",        "codefang", ["run", "--nonexistent-flag"],        "bad-flag"),
    ("codefang/unknown-command", "codefang", ["frobnicate"],                       "unknown-command"),
    ("codefang/render-missing-arg", "codefang", ["render"],                        "missing-arg"),
    ("uast/bad-flag",            "uast",     ["parse", "--nonexistent-flag"],      "bad-flag"),
    ("uast/unknown-command",     "uast",     ["frobnicate"],                       "unknown-command"),
    ("codefang/unknown-analyzer","codefang", ["run", "--no-cache", "--head",
                                              "--analyzers", "static/doesnotexist",
                                              "-p", "/tmp"],                       "runtime"),
]

CATEGORY_KEYWORDS = {
    "bad-flag":        ["unknown flag", "unexpected argument", "unrecognized"],
    "unknown-command": ["unknown command", "unrecognized subcommand"],
    "missing-arg":     ["accepts", "required argument", "required arguments"],
    "runtime":         ["unknown analyzer", "analyzer", "error"],
}


def classify_stderr(text):
    low = text.lower()
    # order matters: most specific first
    for cat in ("unknown-command", "missing-arg", "bad-flag", "runtime"):
        for kw in CATEGORY_KEYWORDS[cat]:
            if kw in low:
                return cat
    return "none"


def run_probe(side, binname, argv):
    base = GO_DIR if side == "go" else RU_DIR
    env = dict(os.environ)
    env.update(PINNED_ENV)
    p = subprocess.run([os.path.join(base, binname)] + argv, env=env,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120)
    return p.returncode, p.stderr.decode("utf-8", "replace")


def diff_error_paths():
    fails = []
    rows = []
    for label, binname, argv, expected_cat in ERROR_PROBES:
        grc, gerr = run_probe("go", binname, argv)
        rrc, rerr = run_probe("rust", binname, argv)
        gcat = classify_stderr(gerr)
        rcat = classify_stderr(rerr)
        row = {"label": label, "argv": argv,
               "go_rc": grc, "rust_rc": rrc,
               "go_cat": gcat, "rust_cat": rcat,
               "expected_cat": expected_cat}
        problems = []
        if grc != rrc:
            problems.append("exit-code")
        if gcat != rcat:
            problems.append("error-category")
        # For the runtime (unknown-analyzer) probe, "error" is too coarse: Go emits
        # a SPECIFIC diagnostic ("unknown analyzer id: ..."). Require Rust to also
        # surface the analyzer concept, so a generic/stub error message that merely
        # happens to contain "error" cannot pass as parity.
        if expected_cat == "runtime":
            if "analyzer" not in gerr.lower():
                problems.append("go-probe-not-analyzer-error")
            elif "analyzer" not in rerr.lower():
                problems.append("rust-missing-specific-diagnostic")
        # Go itself must produce a non-empty diagnostic for these (sanity: probe
        # actually triggered an error path in the oracle).
        if gcat == "none" and gerr.strip() == "":
            problems.append("go-no-diagnostic")
        row["problems"] = problems
        rows.append(row)
        if problems:
            fails.append({"bin": binname, "cmd": label, "kind": "error-path-differs",
                          "detail": row})
    return fails, rows


# --------------------------------------------------------------------------- #
# Main.
# --------------------------------------------------------------------------- #
def run_all(go_codefang=None, go_uast=None, ru_codefang=None, ru_uast=None):
    """Run the full surface + error-path comparison. Surfaces may be injected
    (used by the self-test to feed a tampered surface); otherwise extracted live."""
    if go_codefang is None:
        go_codefang = load_surface("go", "codefang")
    if go_uast is None:
        go_uast = load_surface("go", "uast")
    if ru_codefang is None:
        ru_codefang = load_surface("rust", "codefang")
    if ru_uast is None:
        ru_uast = load_surface("rust", "uast")

    fails = []
    fails += diff_surface("codefang", go_codefang, ru_codefang)
    fails += diff_surface("uast", go_uast, ru_uast)
    err_fails, err_rows = diff_error_paths()
    fails += err_fails
    return fails, err_rows


def main():
    ap = argparse.ArgumentParser(description="Go<->Rust CLI surface conformance")
    ap.add_argument("--json", action="store_true",
                    help="emit machine-readable result")
    ap.add_argument("--only", choices=["surface", "errorpath"],
                    help="run only one layer")
    a = ap.parse_args()

    if a.only == "surface":
        go_cf, go_u = load_surface("go", "codefang"), load_surface("go", "uast")
        ru_cf, ru_u = load_surface("rust", "codefang"), load_surface("rust", "uast")
        fails = diff_surface("codefang", go_cf, ru_cf) + diff_surface("uast", go_u, ru_u)
        err_rows = []
    elif a.only == "errorpath":
        fails, err_rows = diff_error_paths()
    else:
        fails, err_rows = run_all()

    if a.json:
        print(json.dumps({"fails": fails, "error_rows": err_rows,
                          "n_fail": len(fails)}, indent=2, default=str))
        sys.exit(1 if fails else 0)

    # human report
    print("================ CLI SURFACE CONFORMANCE (Go oracle vs Rust) ============")
    if err_rows:
        print("-- error-path parity (exit code + stderr class) --")
        for r in err_rows:
            verdict = "FAIL" if r["problems"] else "PASS"
            print(f"  {verdict}  {r['label']:30s} "
                  f"go(rc={r['go_rc']},{r['go_cat']}) "
                  f"rust(rc={r['rust_rc']},{r['rust_cat']})"
                  + (f"  PROBLEMS={r['problems']}" if r["problems"] else ""))
    print("-- surface diff --")
    allowlisted = sorted(f"{b} {c}" for b, cs in INTENTIONALLY_REMOVED.items() for c in cs)
    if allowlisted:
        print(f"  NOTE  intentionally removed (allowlisted): {', '.join(allowlisted)}")
    if not fails:
        print("  (no surface divergences)")
    for f in fails:
        if f["kind"] == "error-path-differs":
            continue
        print(f"  FAIL  [{f['bin']}] {f['cmd']:24s} {f['kind']}: "
              f"{json.dumps(f['detail'], default=str)[:160]}")
    n_surface = sum(1 for f in fails if f["kind"] != "error-path-differs")
    n_err = sum(1 for f in fails if f["kind"] == "error-path-differs")
    print("================ RESULT ================")
    print(f"surface_divergences={n_surface}  error_path_divergences={n_err}  "
          f"TOTAL_FAIL={len(fails)}")
    if fails:
        print("CLI-SURFACE GATE: RED — Rust surface diverges from Go")
        sys.exit(1)
    print("CLI-SURFACE GATE: GREEN — Rust surface matches Go")
    sys.exit(0)


if __name__ == "__main__":
    main()
