#!/usr/bin/env python3
"""
MATRIX EXPANDER  (SPEC: specs/go-compat-testing/SPEC.md §3.2, roadmap 3).

Cross-multiplies the axes in matrix.toml, substitutes content-addressed corpus
inputs from corpus/manifest.json, and emits one CELL per meaningful combination.
Each cell is an oracle invocation:  {"label","argv","family","tier"}.

This module DOES NOT compare and DOES NOT decide pass/fail. The oracle
(oracle/oracle.py) is the sole source of truth; the runner (run_matrix.py) feeds
each cell argv to it. Cells the LIVE Go binary leaves empty are recorded as
expected-empty contracts BY THE RUNNER (it asks the oracle / Go), never here --
the expander only enumerates what to probe so that the probed set is auditable
and cannot be silently shrunk (the matrix file is hashed by the tamper layer).
"""

import json
import os
import sys

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    print("python>=3.11 required (tomllib)", file=sys.stderr)
    sys.exit(2)

HERE = os.path.dirname(os.path.abspath(__file__))
MATRIX = os.path.join(HERE, "matrix.toml")
CORPUS_MANIFEST = os.path.join(HERE, "corpus", "manifest.json")


def load():
    with open(MATRIX, "rb") as f:
        m = tomllib.load(f)
    with open(CORPUS_MANIFEST) as f:
        c = json.load(f)
    return m, c


def repo_path(corpus, repo_id):
    for r in corpus["repos"]:
        if r["id"] == repo_id:
            return r["path"]
    return None


def expand(tier="smoke"):
    m, corpus = load()
    files = corpus["files"]
    tcfg = m["tiers"][tier]
    cells = []

    def add(label, argv, family):
        cells.append({"label": label, "argv": argv, "family": family,
                      "tier": tier})

    # ---- 1) uast surface x corpus languages (per-language parse contract) ----
    qdsl = m["uast"]["query_dsl"]
    for entry in files:
        stored = os.path.join(HERE, "corpus", entry["stored"])
        lang = entry["language"]
        for fmt in m["formats"]["uast_parse"]:
            add(f"uast/parse[{fmt}]/{lang}",
                ["uast", "parse", "--format", fmt, stored], "uast_parse")
        for fmt in m["formats"]["uast_analyze"]:
            add(f"uast/analyze[{fmt}]/{lang}",
                ["uast", "analyze", "--format", fmt, "-i", stored], "uast_analyze")
        for fmt in m["formats"]["uast_query"]:
            add(f"uast/query[{fmt}]/{lang}",
                ["uast", "query", qdsl, "--format", fmt, "-i", stored], "uast_query")

    # ---- 2) static analyzers x formats (single-analyzer cells) ----
    sbase = m["meta"]["static_base"]
    for repo_id in tcfg["repos"]:
        rp = repo_path(corpus, repo_id)
        if not rp:
            continue
        for an in m["analyzers"]["static"]:
            for fmt in tcfg["run_formats"]:
                add(f"static:{an}[{fmt}]@{repo_id}",
                    sbase + ["-p", rp, "--analyzers", an, "--format", fmt],
                    "static_analyzer")
        # static analyzer SETS (only the static-relevant sets) on this repo
        for setspec in m["analyzer_sets"]["sets"]:
            if not setspec.startswith("static") and setspec != "*":
                continue
            add(f"static-set:{setspec}[json]@{repo_id}",
                sbase + ["-p", rp, "--analyzers", setspec, "--format", "json"],
                "analyzer_set")

    # ---- 3) history analyzers x formats x limit (single-analyzer cells) ----
    rbase = m["meta"]["run_base"]
    lim = str(tcfg["history_limit"])
    for repo_id in tcfg["repos"]:
        rp = repo_path(corpus, repo_id)
        if not rp:
            continue
        for an in m["analyzers"]["history"]:
            for fmt in tcfg["run_formats"]:
                add(f"history:{an}[{fmt}]@{repo_id}/lim{lim}",
                    rbase + ["--analyzers", an, "--format", fmt,
                             "--limit", lim, rp],
                    "history_analyzer")
        # history analyzer SETS
        for setspec in m["analyzer_sets"]["sets"]:
            if not (setspec.startswith("history") or setspec == "*"):
                continue
            add(f"history-set:{setspec}[json]@{repo_id}/lim{lim}",
                rbase + ["--analyzers", setspec, "--format", "json",
                         "--limit", lim, rp],
                "analyzer_set")

    # ---- 4) timeseries + --ndjson COMBINATION cell ----
    for repo_id in tcfg["repos"]:
        rp = repo_path(corpus, repo_id)
        if not rp:
            continue
        add(f"history:history/devs[timeseries+ndjson]@{repo_id}/lim{lim}",
            rbase + ["--analyzers", "history/devs", "--format", "timeseries",
                     "--ndjson", "--limit", lim, rp], "format_combo")

    # ---- 4b) PLOT (file-output) cells: --format plot writes index.html +
    # per-analyzer <id>.html + report.json into --output; the oracle's file
    # mode (triggered by `--format plot` with no --output in the cell argv)
    # runs both binaries into temp dirs and compares file sets + per-file
    # bytes (go-echarts chart-id canonicalization, measured). Scope: every
    # analyzer on the FIRST tier repo (hercules — the cheap one), plus the
    # static/* and * set pages there.
    plot_repo = tcfg["repos"][0]
    rp = repo_path(corpus, plot_repo)
    if rp:
        for an in m["analyzers"]["static"]:
            add(f"plot:{an}@{plot_repo}",
                sbase + ["-p", rp, "--analyzers", an, "--format", "plot"],
                "plot")
        for an in m["analyzers"]["history"]:
            add(f"plot:{an}@{plot_repo}/lim{lim}",
                rbase + ["--analyzers", an, "--format", "plot",
                         "--limit", lim, rp],
                "plot")
        for setspec in ("static/*", "*"):
            add(f"plot:{setspec}@{plot_repo}",
                sbase + ["-p", rp, "--analyzers", setspec, "--format", "plot"],
                "plot")

    # ---- 5) KEY-FLAG axis (full tier only): each flag is one cell ----
    if tcfg.get("include_flags"):
        rp = repo_path(corpus, tcfg["repos"][0])
        for flag in m["flags"]["history"]:
            name, _, val = flag.partition(":")
            extra = [name] + ([val] if val else [])
            add(f"flag:history/devs{flag}@{tcfg['repos'][0]}",
                rbase + ["--analyzers", "history/devs", "--format", "json"]
                + extra + ([] if name == "--head" else ["--limit", lim]) + [rp],
                "flag")
        for flag in m["flags"]["static"]:
            name, _, val = flag.partition(":")
            extra = [name] + ([val] if val else [])
            add(f"flag:static/complexity{flag}@{tcfg['repos'][0]}",
                sbase + ["-p", rp, "--analyzers", "static/complexity",
                         "--format", "json"] + extra, "flag")

    return cells


def main():
    tier = sys.argv[1] if len(sys.argv) > 1 else "smoke"
    cells = expand(tier)
    # families summary
    fams = {}
    for c in cells:
        fams[c["family"]] = fams.get(c["family"], 0) + 1
    out = {"tier": tier, "total_cells": len(cells),
           "families": fams, "cells": cells}
    json.dump(out, sys.stdout, indent=2)
    print(file=sys.stderr)
    print(f"tier={tier}: {len(cells)} cells  {fams}", file=sys.stderr)


if __name__ == "__main__":
    main()
