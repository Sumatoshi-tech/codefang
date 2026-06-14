# Compatibility corpus + invocation matrix (MatrixCorpus phase)

SPEC: `specs/go-compat-testing/SPEC.md` §3.2 (invocation matrix), §3.3 (corpus),
roadmap item 3. This phase builds the content-addressed input corpus and the
meaningful invocation cross-product, and wires **every** matrix cell to the live
Go oracle (`../oracle/oracle.py`). It does not re-derive any expected output: the
Go binary is the sole source of truth.

## Layout

- `build_corpus.py` — mines real inputs, content-addresses them, records
  provenance. Re-runnable; deterministic file selection.
- `files/<sha256>.<ext>` — content-addressed source files (one per language).
- `manifest.json` — provenance per entry: language, sha256, origin path, size,
  and the **live-oracle-measured** UAST byte/node count (so a dead cell can never
  be admitted silently).
- `known_gaps.json` — measured Go↔Rust divergences with evidence (see Findings).
- `../matrix.toml` — the invocation cross-product (axes only; declares nothing
  about field stability — that is measured by the oracle).
- `../expand_matrix.py` — cross-multiplies the axes, substitutes corpus inputs,
  emits one oracle invocation per cell.
- `../run_matrix.py` — runs each cell through the oracle; records Go-empty cells
  as `EXPECTED_EMPTY` contracts (not skips). Writes `../results/<tier>.json`.
- `../selftest_matrix.py` — proves the corpus+matrix catch a planted defect.

## Corpus contents

12 tree-sitter languages mined from real local repos (not just Go):
go, python, c, c-header, rust, typescript, tsx, javascript, json, yaml, cpp,
shell. Each file is 500–25 000 bytes of real source and parses to non-trivial
UAST on the **live Go binary** (verified at build time).

3 real git repos recorded for the analyzer cells, **2 beyond kubernetes**:
- `hercules` — Go, ~1006 commits (medium).
- `ioq3` — C, ~3784 commits (larger, different language).
- `kubernetes` — Go, ~135k commits (the large reference repo).

## Matrix

Axes (see `matrix.toml`): all 17 analyzers (7 static + 10 history) × output
formats (json, yaml, bin, compact, text, ndjson, timeseries, plus the
timeseries+ndjson combo; uast: json/compact/tree, json/text, json/compact/count)
× key flags (--head, --limit, --first-parent, --since, --per-file,
--include-vendored, --include-generated) × analyzer sets (`*`, `static/*`,
`history/*`, and pairs) × per-language uast subcommands (parse/analyze/query).

Expansion: **smoke = 155 cells**, **full = 486 cells**. Every cell is dispatched
to `oracle.py`. Cells where the live Go binary emits no stdout (e.g. `uast parse
--format tree`) are recorded as `EXPECTED_EMPTY` contracts.

## Findings (recorded, not hidden)

Running the multi-language `uast parse` cells through the oracle immediately
surfaced a real port gap that the all-Go recorded goldens could never expose:

- **Go parses 12/12 languages; Rust parses only Go.** For the other 11
  languages the Rust `uast` binary emits **0 bytes** with
  `"no tree-sitter grammar wired for language <X> (grammar vendoring pending)"`.
  Evidence (per-language Go byte counts + Rust stderr) is in `known_gaps.json`.

This is exactly the unseen-input class the SPEC targets: a fresh, multi-language
corpus is not memorizable, and it caught a gap a Go-only golden set hid.

## Run

```
python3 corpus/build_corpus.py                 # (re)build the corpus
python3 expand_matrix.py full                   # inspect the cell list
python3 run_matrix.py --tier smoke --family uast_parse   # wire a family to oracle
python3 selftest_matrix.py                      # prove it catches a planted bug
```
