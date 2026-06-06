# CoverageLedger (SPEC §3.6/§3.7, roadmap 5)

Three coverage-REACH facets + the machine-readable gap ledger. Coverage is
reported **alongside** the differential oracle verdict, **never as a substitute**
for it (SPEC §6: "coverage % gives false confidence"). A high number here means
"a test reached this code/cell/language", not "Rust matches Go" — that is the
oracle's job.

## Components

| File | Produces | How it is measured (never declared) |
|------|----------|--------------------------------------|
| `rust_cov.py` | `rust_cov.json` — Rust line/region/function/branch coverage | Runs `cargo-llvm-cov` and reads back the audited `data[].totals` from llvm-cov's own `--json` export. `--branch` is nightly-only; on a stable toolchain branch is recorded **unavailable**, never fabricated (region is the stable branch-reach proxy). Default scope = representative analyzer + serializer crates, recorded in the output so the % is honest about what it measured. |
| `matrix_cell_cov.py` | `matrix_cell_cov.json` — matrix-cell coverage (cells exercised / total) **and** per-language parse coverage | Denominator comes from the **live expander** (`expand_matrix.py`), so a shrunk/sampled run cannot inflate the %. Per-language parse runs the **live Go + Rust `uast parse`** per corpus language; a language Go parses but Rust does not is a measured GAP. |
| `build_ledger.py` | `../ledger.json` | Unifies the three facets + the **verdict tally** (from `results/<tier>.json`, so coverage never stands alone) + untested cells + known divergences + Go-VARIANT captures harvested **live** through the oracle with the differing Go observations stored as evidence. |
| `selftest/self_test.py` | self-proof | Plants each cheat/defect class and asserts it is caught. |

## ledger.json shape

```
verdict_tally            # PASS/FAIL/SIM/EXPECTED_EMPTY — coverage is NEVER reported alone
coverage:
  rust_llvm_cov          # lines/regions/functions/branches(+unavailable marker)
  matrix_cell            # exercised/total, per_family, percent
  per_language_parse     # rust-parses/total, gaps[]
untested_matrix_cells[]  # every enumerated cell with no recorded verdict — surfaced
known_divergences[]      # corpus/known_gaps.json + every FAIL/SIM in results
go_variant_captures[]    # MEASURED Go-variant fields WITH evidence (differing Go values)
```

## Run

```sh
# fast: reuse warm llvm-cov counters, live variant + parse probes
python3 coverage/build_ledger.py --tier smoke --probe-variants --probe-parse --no-rust-cov

# full refresh (re-runs cargo-llvm-cov; slower)
python3 coverage/build_ledger.py --tier full --probe-variants --probe-parse

# self-proof (REQUIRED gate: proves the ledger catches planted cheats)
python3 coverage/selftest/self_test.py
```

## Why each self-proof exists (rule #6)

- **D1 shrunk-results cheat** — denominator is the live expander, so claiming a few
  cells PASS cannot inflate the %; untested cells stay listed.
- **D2 empty results** — a missing run reads 0%/all-untested, never a silent 100%.
- **D3 per-language parse gap** — a language Go parses but Rust does not is reported
  as a GAP, measured against the live Go oracle (currently 11/12: grammar vendoring
  pending — independently confirmed against `corpus/known_gaps.json`).
- **D4 fabricated branch %** — branch is recorded `unavailable`, never invented.
- **D5 coverage ≠ done** — even with a planted 100% coverage json, the ledger keeps
  the verdict tally (with its FAILs) visible.
- **D6 variant evidence measured** — the harvest goes through the oracle whose
  cheat-detector refuses to blank a Go-STABLE field, and a real variant capture
  stores the differing Go observations as proof.
