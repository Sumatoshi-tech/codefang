#!/usr/bin/env python3
"""
Go<->Rust differential ORACLE  (SPEC: specs/go-compat-testing/SPEC.md, layer 3 / roadmap 1)

THE ORACLE IS THE LIVE GO BINARY. This program NEVER re-derives expected output;
it executes the real Go binary as the source of truth and the real Rust binary as
the candidate, under a pinned environment, then compares them.

Per-invocation procedure (rules from the task brief, encoded here):

  1. Run Go N>=3 times  -> classify every output field as
        GO-STABLE  : identical across ALL Go runs
        GO-VARIANT : differs across Go runs
     Store the DIFFERING Go outputs as EVIDENCE in the manifest. Canonicalization
     is MEASURED, never declared.
  2. Run Rust 2 times   -> if the two Rust runs differ, Rust is NONDETERMINISTIC
     -> FAIL (a faithful port is deterministic).
  3. Compare Rust vs Go:
        - GO-STABLE  fields : BYTE-EXACT (Rust leaf must equal the one stable Go value)
        - GO-VARIANT fields : CANONICAL-EQUAL only (sorted / neutralized), and ONLY
                              for fields measured variant. Blanking a Go-STABLE field
                              is the exact cheat that hid a real bug before; it is
                              FORBIDDEN and detected (see check_normalize_request).
  4. Emit verdict: PASS / FAIL / SIM, the field classification, and the evidence.

Run env is pinned by the caller-equivalent here:
    set -f; TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800
We compare STDOUT only (stderr is progress).
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

GO_DIR = "/home/dmitriy/sources/codefang/build/bin"
RU_DIR = "/home/dmitriy/sources/codefang/rust/target/release"

PINNED_ENV = {
    "TZ": "UTC",
    "NO_COLOR": "1",
    "LANG": "C",
    "LC_ALL": "C",
    "SOURCE_DATE_EPOCH": "315532800",
}


# --------------------------------------------------------------------------- #
# Process execution: the binaries ARE the oracle. No re-derivation anywhere.
# --------------------------------------------------------------------------- #
def _resolve(side, bin_name):
    base = GO_DIR if side == "go" else RU_DIR
    # argv[0] of the codefang/uast surface: "uast ..." -> uast binary,
    # anything else (e.g. "run ...", "version") -> codefang binary with the
    # subcommand kept as the first argv element. This mirrors parity_gate.sh.
    if bin_name == "uast":
        return os.path.join(base, "uast"), []
    return os.path.join(base, "codefang"), [bin_name]


def run_once(side, argv):
    """Run the LIVE binary once under the pinned env; return (rc, stdout_bytes)."""
    exe, prefix = _resolve(side, argv[0])
    cmd = [exe] + prefix + list(argv[1:])
    env = dict(os.environ)
    env.update(PINNED_ENV)
    # set -f equivalent: we pass argv as a list so the shell never globs.
    p = subprocess.run(
        cmd, env=env, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        timeout=600,
    )
    return p.returncode, p.stdout


def sha(b):
    return hashlib.sha256(b).hexdigest()


# --------------------------------------------------------------------------- #
# Field model: a "field" is a JSON leaf path. For non-JSON output we fall back
# to a single synthetic field "<bytes>" carrying the whole output.
# --------------------------------------------------------------------------- #
def try_json(b):
    try:
        return json.loads(b.decode("utf-8"))
    except Exception:
        return None


def leaf_paths(obj, prefix="$"):
    """
    Yield (path, value) for every JSON leaf. Lists use a structural index so the
    classifier can detect order-only variation distinctly from set variation:
    we record list MEMBERSHIP under "<prefix>[]" (a multiset of canonical leaves)
    AND each positional leaf under "<prefix>[i]".
    """
    if isinstance(obj, dict):
        for k in sorted(obj.keys()):
            yield from leaf_paths(obj[k], f"{prefix}.{k}")
    elif isinstance(obj, list):
        # positional leaves (captures order)
        for i, v in enumerate(obj):
            yield from leaf_paths(v, f"{prefix}[{i}]")
    else:
        yield (prefix, obj)


def canon_str(obj):
    return json.dumps(obj, sort_keys=True, separators=(",", ":"))


def field_map(b):
    """Return {path: value} for a single output. JSON -> leaf paths; else one blob."""
    j = try_json(b)
    if j is None:
        return {"$<bytes>": b.decode("utf-8", "replace")}, False
    return {p: v for p, v in leaf_paths(j)}, True


# --------------------------------------------------------------------------- #
# Measured classification: GO-STABLE vs GO-VARIANT, with stored evidence.
# --------------------------------------------------------------------------- #
def classify(go_outputs):
    """
    go_outputs: list of stdout bytes from N Go runs.
    Returns (classification, evidence, is_json, numeric_envelope).

    classification[path] = "STABLE" | "VARIANT"
       - STABLE  : the path exists with the SAME value in every Go run
       - VARIANT : the value differs across runs, OR the path is present in some
                   runs and absent in others (membership instability)

    evidence: for every VARIANT path, the list of the distinct Go values observed
              (this is the stored proof that Go itself varies here).
    """
    maps = []
    is_json = True
    for b in go_outputs:
        m, j = field_map(b)
        is_json = is_json and j
        maps.append(m)

    all_paths = set()
    for m in maps:
        all_paths.update(m.keys())

    classification = {}
    evidence = {}
    numeric_envelope = {}   # path -> (min, max) of Go's observed numeric values
    SENTINEL = object()
    for path in sorted(all_paths):
        vals = [m.get(path, SENTINEL) for m in maps]
        canon = [("<absent>" if v is SENTINEL else canon_str(v)) for v in vals]
        if len(set(canon)) == 1:
            classification[path] = "STABLE"
        else:
            classification[path] = "VARIANT"
            # store the DISTINCT differing Go observations as evidence
            evidence[path] = sorted(set(canon))
            # If every present Go value is numeric, record the OBSERVED envelope
            # [min,max]. A variant float is NOT blanked: Rust must land inside
            # Go's own measured spread (the strongest non-weakening rule for a
            # field Go itself wobbles on, e.g. summation-order float noise).
            present = [v for v in vals if v is not SENTINEL]
            if present and all(isinstance(v, (int, float))
                               and not isinstance(v, bool) for v in present):
                numeric_envelope[path] = (min(present), max(present))
    return classification, evidence, is_json, numeric_envelope


# --------------------------------------------------------------------------- #
# Canonicalization of VARIANT-only structure (sorted / neutralized).
# Applied identically to Go and Rust so order-only / membership variation that
# Go itself exhibits is neutralized -- but ONLY where measured variant.
# --------------------------------------------------------------------------- #
def canonicalize(obj, variant_lists, numeric_envelope=None):
    """
    Two MEASURED neutralizations, applied identically to Go and Rust:

      * Sort any list whose path is in variant_lists (its ELEMENT ORDER or
        MEMBERSHIP was measured Go-variant). Stable lists keep their order so a
        real ordering bug is still caught.

      * For a variant numeric SCALAR with a recorded Go [min,max] envelope, map
        the value to the token "<num:envelope>" IFF it falls inside Go's OWN
        observed spread (so Go's summation-order float wobble is neutralized) and
        otherwise leave the raw number (so a value OUTSIDE Go's measured range --
        a real Rust bug like couples' avg_coupling_strength -- still diverges).
        This is NOT blanking: the envelope is Go's measured variance, never a
        declared tolerance.

    numeric_envelope keys are positional leaf paths ($.a.b or $.x[3].y). After a
    variant list is sorted, positional indices no longer align, so envelopes only
    bite OUTSIDE variant lists; inside variant lists the structure equality on the
    sorted multiset is the authority.
    """
    numeric_envelope = numeric_envelope or {}

    def walk(o, prefix="$"):
        if isinstance(o, dict):
            return {k: walk(o[k], f"{prefix}.{k}") for k in o}
        if isinstance(o, list):
            elems = [walk(v, f"{prefix}[{i}]") for i, v in enumerate(o)]
            # the list CONTAINER's measured-variant marker is "<prefix>[]"
            collapsed = __import__("re").sub(r"\[\d+\]", "[]", prefix) + "[]"
            if collapsed in variant_lists:
                elems = sorted(elems, key=lambda x: canon_str(x))
            return elems
        # scalar leaf: apply Go-measured numeric envelope if present for this path
        if prefix in numeric_envelope and isinstance(o, (int, float)) \
                and not isinstance(o, bool):
            lo, hi = numeric_envelope[prefix]
            if lo <= o <= hi:
                return "<num:in-go-envelope>"
        return o
    return walk(obj)


def variant_list_prefixes(classification):
    """
    Derive which LIST containers were Go-variant: any path of the form
    "<base>[i]..." or "<base>[]" that is classified VARIANT implies the list at
    <base> varied. We collapse the positional index to "<base>[]" so the same
    canonicalization rule applies to both Go and Rust regardless of length.
    """
    import re
    prefixes = set()
    for path, cls in classification.items():
        if cls != "VARIANT":
            continue
        # Replace every "[<n>]" with "[]" and record each list-container prefix.
        # e.g. "$.node_hotness[3].name" -> list base "$.node_hotness[]"
        collapsed = re.sub(r"\[\d+\]", "[]", path)
        parts = collapsed.split("[]")
        acc = ""
        for k in range(len(parts) - 1):
            acc += parts[k]
            prefixes.add(acc + "[]")
            acc += "[]"
    return prefixes


# --------------------------------------------------------------------------- #
# TAMPER / CHEAT detector.
# The historic cheat: blank a Go-STABLE field so a buggy Rust value is hidden.
# We accept an optional external "normalize request" (paths the caller wants
# neutralized). If ANY requested path is classified GO-STABLE, that is the cheat
# -> hard error, fail-closed.
# --------------------------------------------------------------------------- #
def check_normalize_request(normalize_paths, classification):
    illegal = [p for p in normalize_paths
               if classification.get(p, "STABLE") == "STABLE"]
    return illegal


# --------------------------------------------------------------------------- #
# Comparison: STABLE -> byte-exact leaf; VARIANT -> canonical-equal only.
# --------------------------------------------------------------------------- #
def _in_variant_list(path, var_lists):
    """True if this leaf path lives inside a measured-variant list container."""
    import re
    collapsed = re.sub(r"\[\d+\]", "[]", path)
    for vl in var_lists:               # vl ends in "[]"
        if collapsed == vl or collapsed.startswith(vl):
            return True
    return False


def compare(go_canonical_b, rust_canonical_b, classification, go_field_map,
            is_json, var_lists, numeric_envelope=None):
    """
    Returns (ok, diffs). Compares Rust against the CANONICALIZED Go reference.

    For JSON:
      - STABLE leaves OUTSIDE any variant list  : byte/value-exact, present in Rust.
        (A dropped/blanked stable field cannot pass via structural coincidence.)
      - Leaves INSIDE a measured-variant list   : NOT checked positionally (index
        alignment is meaningless after the list is sorted); their content is fully
        policed by the whole-structure canonical equality below, which compares the
        sorted multiset and so still catches any real value divergence.
      - VARIANT leaves                          : canonical-equal only.
    """
    diffs = []
    if not is_json:
        if go_canonical_b != rust_canonical_b:
            diffs.append({
                "path": "$<bytes>", "kind": "non-json-bytes-differ",
                "go_sha": sha(go_canonical_b), "rust_sha": sha(rust_canonical_b),
            })
        return (len(diffs) == 0), diffs

    # canonical structural equality covers VARIANT lists (sorted) AND everything
    # else. We additionally assert every STABLE leaf is present & equal in Rust so
    # a dropped/blanked stable field cannot pass via structural coincidence.
    go_j = json.loads(go_canonical_b)
    ru_j = json.loads(rust_canonical_b)
    ru_fields = {p: v for p, v in leaf_paths(ru_j)}

    # 1) STABLE leaves: byte-exact value match against the stable Go value,
    #    EXCEPT leaves inside a variant list (policed by structure-equality #2).
    for path, cls in classification.items():
        if cls != "STABLE":
            continue
        if _in_variant_list(path, var_lists):
            continue
        gv = go_field_map.get(path)
        if path not in ru_fields:
            diffs.append({"path": path, "kind": "stable-field-missing-in-rust",
                          "go_value": gv})
        elif canon_str(ru_fields[path]) != canon_str(gv):
            diffs.append({"path": path, "kind": "stable-field-differs",
                          "go_value": gv, "rust_value": ru_fields[path]})

    # 2) Whole-structure canonical equality (catches VARIANT-list content drift,
    #    extra rust fields, type changes the leaf scan would miss).
    if canon_str(go_j) != canon_str(ru_j):
        diffs.append({"path": "$", "kind": "canonical-structure-differs",
                      "go_sha": sha(canon_str(go_j).encode()),
                      "rust_sha": sha(canon_str(ru_j).encode())})

    return (len(diffs) == 0), diffs


# --------------------------------------------------------------------------- #
# File-output cells (--format plot): the contract is the FILES the binary
# writes into --output, not stdout. Both binaries run into fresh temp dirs;
# the file SETS and per-file bytes are compared with the same measured-variance
# discipline as the stdout path:
#   * a file whose bytes are identical across all N Go runs must match Rust
#     byte-for-byte;
#   * a file that varies is first canonicalized by neutralizing the go-echarts
#     random chart ids (12-char [A-Za-z] tokens in `id="..."` / `goecharts_*` /
#     `option_*` sites) — the substitution is only TRUSTED when it provably
#     makes all Go runs agree (measurement-driven, like variant-list sorting);
#   * a file that stays unstable even canonicalized is Go-content-
#     nondeterministic: the fallback requires the Rust file to be non-empty and
#     deterministic (the structural realcheck contract).
# Rust determinism (two identical-arg runs, raw bytes) is REQUIRED throughout.
# --------------------------------------------------------------------------- #
_CHART_ID_SITES = [
    (re.compile(rb'id="[A-Za-z]{12}"'), b'id="CHARTID"'),
    (re.compile(rb'goecharts_[A-Za-z]{12}'), b'goecharts_CHARTID'),
    (re.compile(rb'option_[A-Za-z]{12}'), b'option_CHARTID'),
]


def _canon_chart_ids(data):
    for pat, repl in _CHART_ID_SITES:
        data = pat.sub(repl, data)
    return data


def run_once_files(side, argv):
    """Run the LIVE binary with --output pointed at a fresh temp dir; return
    (rc, {relpath: bytes}) over every regular file the run wrote."""
    out_dir = tempfile.mkdtemp(prefix=f"oracle-plot-{side}-")
    try:
        rc, _ = run_once(side, list(argv) + ["--output", out_dir])
        files = {}
        for root, _dirs, names in os.walk(out_dir):
            for name in names:
                p = os.path.join(root, name)
                rel = os.path.relpath(p, out_dir)
                with open(p, "rb") as f:
                    files[rel] = f.read()
        return rc, files
    finally:
        shutil.rmtree(out_dir, ignore_errors=True)


def is_file_output_cell(argv):
    """True when the invocation's contract is --output files (plot format,
    no caller-supplied --output)."""
    if "--output" in argv:
        return False
    for i, tok in enumerate(argv):
        if tok == "--format" and i + 1 < len(argv) and argv[i + 1] == "plot":
            return True
        if tok == "--format=plot":
            return True
    return False


def run_invocation_files(argv, n_go=3):
    """File-set differential verdict for a --format plot cell."""
    go_runs = [run_once_files("go", argv) for _ in range(n_go)]
    go_sets = [set(files) for _rc, files in go_runs]

    base = {"argv": argv, "mode": "files",
            "go_rcs": [rc for rc, _ in go_runs],
            "go_file_sets": sorted(go_sets[0]) if go_sets else []}

    if all(not files for _rc, files in go_runs):
        base["verdict"] = "FAIL"
        base["reason"] = "go wrote no files (probe invalid / no Go contract)"
        return base

    ru_a = run_once_files("rust", argv)
    ru_b = run_once_files("rust", argv)
    base["rust_rcs"] = [ru_a[0], ru_b[0]]

    # Rust determinism is non-negotiable: raw byte equality across two runs.
    if ru_a[1] != ru_b[1]:
        unstable = sorted(set(ru_a[1]) ^ set(ru_b[1])) or [
            p for p in ru_a[1] if ru_a[1][p] != ru_b[1].get(p)]
        base["verdict"] = "FAIL"
        base["reason"] = "RUST NONDETERMINISTIC (file outputs differ across runs)"
        base["unstable_rust_files"] = unstable[:10]
        return base
    rust_files = ru_a[1]

    # Go file-set stability gates the per-file comparison.
    if any(s != go_sets[0] for s in go_sets[1:]):
        base["verdict"] = "PASS" if rust_files else "FAIL"
        base["reason"] = ("Go file SET nondeterministic; structural fallback: "
                          + ("rust wrote files deterministically" if rust_files
                             else "rust wrote NO files"))
        base["structural"] = True
        return base

    if set(rust_files) != go_sets[0]:
        base["verdict"] = "FAIL"
        base["reason"] = "file set mismatch"
        base["only_go"] = sorted(go_sets[0] - set(rust_files))[:10]
        base["only_rust"] = sorted(set(rust_files) - go_sets[0])[:10]
        return base

    diffs = []
    file_modes = {}
    for rel in sorted(go_sets[0]):
        go_variants = [files[rel] for _rc, files in go_runs]
        if all(v == go_variants[0] for v in go_variants[1:]):
            # Byte-stable in Go -> exact byte contract.
            file_modes[rel] = "stable"
            if rust_files[rel] != go_variants[0]:
                diffs.append({"file": rel, "kind": "bytes-differ",
                              "go_sha": sha(go_variants[0]),
                              "rust_sha": sha(rust_files[rel])})
            continue
        canon = [_canon_chart_ids(v) for v in go_variants]
        if all(c == canon[0] for c in canon[1:]):
            # Variance fully explained by chart ids (measured) -> canonical
            # byte contract.
            file_modes[rel] = "chart-id-canonical"
            if _canon_chart_ids(rust_files[rel]) != canon[0]:
                diffs.append({"file": rel, "kind": "canonical-bytes-differ",
                              "go_sha": sha(canon[0]),
                              "rust_sha": sha(_canon_chart_ids(rust_files[rel]))})
            continue
        # Content-nondeterministic even canonicalized: structural contract.
        file_modes[rel] = "go-content-nondeterministic"
        if not rust_files[rel]:
            diffs.append({"file": rel, "kind": "rust-empty-vs-nondet-go"})

    base["file_modes"] = file_modes
    base["diffs"] = diffs
    base["verdict"] = "PASS" if not diffs else "FAIL"
    if diffs:
        base["reason"] = "file divergence"
    return base


# --------------------------------------------------------------------------- #
# Top-level per-invocation oracle.
# --------------------------------------------------------------------------- #
def run_invocation(argv, n_go=3, normalize=None):
    normalize = normalize or []
    verdict = "PASS"
    notes = []

    # ---- Go N runs (the oracle) ----
    go_runs = []
    for _ in range(n_go):
        rc, out = run_once("go", argv)
        go_runs.append({"rc": rc, "out": out, "sha": sha(out), "len": len(out)})
    go_outputs = [r["out"] for r in go_runs]

    if all(len(o) == 0 for o in go_outputs):
        # Go produced nothing -> this cell has no Go contract here.
        return {
            "argv": argv, "verdict": "FAIL",
            "reason": "go-empty-output (probe invalid / no Go contract)",
            "go_shas": [r["sha"] for r in go_runs],
        }

    classification, evidence, is_json, numeric_envelope = classify(go_outputs)

    # ---- cheat detector: refuse to neutralize a Go-STABLE field ----
    illegal = check_normalize_request(normalize, classification)
    if illegal:
        return {
            "argv": argv, "verdict": "FAIL",
            "reason": "TAMPER: normalize request targets Go-STABLE field(s) "
                      "(blanking a stable field is the forbidden cheat)",
            "illegal_normalize_paths": illegal,
            "classification": classification,
        }

    # ---- Rust 2 runs (determinism + candidate) ----
    ru_runs = []
    for _ in range(2):
        rc, out = run_once("rust", argv)
        ru_runs.append({"rc": rc, "out": out, "sha": sha(out), "len": len(out)})

    if ru_runs[0]["sha"] != ru_runs[1]["sha"]:
        return {
            "argv": argv, "verdict": "FAIL",
            "reason": "RUST NONDETERMINISTIC (two identical-arg Rust runs differ)",
            "rust_shas": [r["sha"] for r in ru_runs],
            "classification": classification, "evidence": evidence,
        }

    # ---- build canonicalization rule strictly from MEASURED variance ----
    var_lists = variant_list_prefixes(classification)

    # Numeric envelopes are only meaningful OUTSIDE variant lists: a variant list
    # is sorted, so its positional indices scramble and a per-index envelope would
    # be an artifact of different elements landing at the same index. Drop those;
    # inside variant lists the sorted-multiset structure equality is the authority.
    numeric_envelope = {p: v for p, v in numeric_envelope.items()
                        if not _in_variant_list(p, var_lists)}

    # pick a Go reference run (run 0). Build the go field map for STABLE checks.
    go_ref = go_outputs[0]
    go_field_map, _ = field_map(go_ref)

    n_stable = sum(1 for c in classification.values() if c == "STABLE")
    n_variant = sum(1 for c in classification.values() if c == "VARIANT")
    base = {
        "argv": argv,
        "is_json": is_json,
        "go_runs": [{"sha": r["sha"], "len": r["len"], "rc": r["rc"]} for r in go_runs],
        "rust_runs": [{"sha": r["sha"], "len": r["len"], "rc": r["rc"]} for r in ru_runs],
        "field_counts": {"stable": n_stable, "variant": n_variant},
        "variant_list_prefixes": sorted(var_lists),
        "classification": classification,
        "evidence": evidence,
    }

    if is_json:
        # MEASURE canonical stability: are all N Go runs equal AFTER canonicalizing
        # the measured-variant lists? If yes, Go's nondeterminism is order-only and
        # canonical parity is a legitimate, enforceable contract. If NO, Go's output
        # is CONTENT-nondeterministic (the member SET differs run-to-run, e.g.
        # history/shotness) and byte/canonical parity is MEASURABLY IMPOSSIBLE --
        # the recorded reference is itself irreproducible. We do NOT silently pass:
        # we fall back to the STRUCTURAL realprobe (non-empty + Rust deterministic),
        # and record that the parity contract is unavailable WITH the evidence.
        # Go is JSON. If Rust's output is NOT parseable JSON (e.g. empty stdout,
        # an error string, or truncated bytes) that is a GENUINE divergence -- Go
        # produced a UAST/report and Rust did not. Report a clean FAIL with the
        # evidence instead of crashing the oracle on json.loads. (Strengthening:
        # an unparseable Rust output can never byte/canonical-match a JSON Go
        # output, so this can only ever be a real FAIL, never a hidden pass.)
        try:
            ru_obj = json.loads(ru_runs[0]["out"])
        except (json.JSONDecodeError, ValueError):
            base["verdict"] = "FAIL"
            base["reason"] = ("Go emitted JSON but Rust output is not parseable "
                              "JSON (empty/error/truncated) -- genuine divergence")
            base["rust_out_len"] = ru_runs[0]["len"]
            base["rust_out_head"] = ru_runs[0]["out"][:200].decode(
                "utf-8", "replace")
            base["go_out_len"] = len(go_ref)
            return base
        go_canons = [canon_str(canonicalize(json.loads(o), var_lists,
                                            numeric_envelope)) for o in go_outputs]
        canonical_stable = len(set(go_canons)) == 1
        base["canonical_go_stable"] = canonical_stable
        base["numeric_envelopes"] = {k: list(v) for k, v in numeric_envelope.items()}

        if canonical_stable:
            go_canon = go_canons[0].encode()
            ru_canon = canon_str(canonicalize(ru_obj,
                                              var_lists, numeric_envelope)).encode()
            ok, diffs = compare(go_canon, ru_canon, classification,
                                go_field_map, is_json, var_lists, numeric_envelope)
            base["verdict"] = "PASS" if ok else "FAIL"
            base["diffs"] = diffs
            return base

        # content-nondeterministic Go: structural proof of real computation.
        base["canonical_go_evidence"] = sorted(
            {sha(c.encode()) for c in go_canons})
        ok, why = _structural_realcheck(argv, ru_runs[0]["out"])
        base["verdict"] = "PASS" if ok else "FAIL"
        base["reason"] = ("Go content-nondeterministic (canonical parity impossible); "
                          "verified structurally: " + why)
        base["structural"] = True
        return base

    # non-JSON: require byte-stability of Go, else structural fallback.
    go_byte_stable = len({r["sha"] for r in go_runs}) == 1
    base["canonical_go_stable"] = go_byte_stable
    if go_byte_stable:
        ok, diffs = compare(go_ref, ru_runs[0]["out"], classification,
                            go_field_map, is_json, var_lists)
        base["verdict"] = "PASS" if ok else "FAIL"
        base["diffs"] = diffs
        return base
    ok, why = _structural_realcheck(argv, ru_runs[0]["out"])
    base["verdict"] = "PASS" if ok else "FAIL"
    base["reason"] = "Go byte-nondeterministic; verified structurally: " + why
    base["structural"] = True
    return base


def _structural_realcheck(argv, rust_out):
    """
    For invocations where Go output is content-nondeterministic and therefore not
    byte/canonical-comparable, prove the Rust port does REAL work (defeats a
    0-byte stub and a constant stub) without demanding parity to an irreproducible
    reference. Mirrors parity_gate.sh `realprobe`:
      (1) Rust output is NON-EMPTY;
      (2) Rust output GROWS when --limit is raised (not a hardcoded constant);
      (3) Rust is DETERMINISTIC at the raised limit.
    Returns (ok, reason_string). Only applies to `run ... --limit N` invocations.
    """
    if len(rust_out) == 0:
        return False, "rust output EMPTY (0-byte stub)"
    if "--limit" not in argv:
        # cannot run the grow probe; accept non-emptiness + determinism already
        # established by the 2 Rust runs in the caller.
        return True, "rust non-empty; determinism established (no --limit to grow-probe)"
    # build a raised-limit argv
    hi = list(argv)
    i = hi.index("--limit")
    try:
        lo_val = int(hi[i + 1])
    except Exception:
        return True, "rust non-empty; --limit not numeric, skipped grow-probe"
    hi[i + 1] = str(lo_val * 20 + 50)
    rc_a, out_hi_a = run_once("rust", hi)
    rc_b, out_hi_b = run_once("rust", hi)
    if sha(out_hi_a) != sha(out_hi_b):
        return False, "rust NONDETERMINISTIC at raised limit"
    if sha(out_hi_a) == sha(rust_out):
        return False, "rust CONSTANT across limits (hardcoded-output signature)"
    return True, (f"rust grows {len(rust_out)}B->{len(out_hi_a)}B with --limit "
                  f"and is deterministic")


def main():
    ap = argparse.ArgumentParser(description="Go<->Rust differential oracle")
    ap.add_argument("--n-go", type=int, default=3, help="Go runs (>=3)")
    ap.add_argument("--normalize", action="append", default=[],
                    help="JSON leaf path to neutralize; REJECTED if Go-stable")
    ap.add_argument("--manifest", help="write full manifest JSON to this path")
    ap.add_argument("--quiet", action="store_true")
    ap.add_argument("argv", nargs=argparse.REMAINDER,
                    help="invocation: <bin> <args...>  (bin = codefang-subcmd or uast)")
    a = ap.parse_args()
    if a.n_go < 3:
        print("n-go must be >= 3 (measured canonicalization needs >=3 runs)",
              file=sys.stderr)
        sys.exit(2)
    argv = a.argv
    if argv and argv[0] == "--":
        argv = argv[1:]
    if not argv:
        print("no invocation given", file=sys.stderr)
        sys.exit(2)

    if is_file_output_cell(argv):
        # --format plot: the contract is the --output files, not stdout.
        res = run_invocation_files(argv, n_go=a.n_go)
    else:
        res = run_invocation(argv, n_go=a.n_go, normalize=a.normalize)
    if a.manifest:
        with open(a.manifest, "w") as f:
            json.dump(res, f, indent=2, default=str)
    if not a.quiet:
        # human line
        v = res["verdict"]
        fc = res.get("field_counts", {})
        print(f"{v}  {' '.join(argv)}  "
              f"[stable={fc.get('stable','?')} variant={fc.get('variant','?')}]")
        if v == "FAIL":
            print("  reason:", res.get("reason", "field divergence"))
            for d in res.get("diffs", [])[:8]:
                print("   diff:", json.dumps(d, default=str)[:300])
    # exit code: PASS=0, FAIL=1, SIM=3
    sys.exit({"PASS": 0, "FAIL": 1, "SIM": 3}.get(res["verdict"], 1))


if __name__ == "__main__":
    main()
