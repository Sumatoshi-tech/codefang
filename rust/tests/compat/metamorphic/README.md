# Metamorphic / Anti-Simulation Layer

SPEC: `specs/go-compat-testing/SPEC.md` §3.6, roadmap item 4.

Per-analyzer **relational** checks that fail hardcoded stubs. Where the oracle
(`../oracle/oracle.py`) compares Rust bytes to Go bytes directly, this layer
asserts relations *between* Rust outputs across different invocations — the
relations a real analyzer obeys and a constant/stub does not.

## The Go binary is the oracle for every PREMISE

We never assert "Rust must differ" from a re-derived expectation. We assert
"Rust must differ **because Go differs** on these same two inputs." If Go does
not exhibit the relation here (Go identical on both inputs, Go does not grow with
`--limit`, Go empty), the premise is absent and the check is recorded `NA` — it
never fakes a verdict. This keeps the live Go binary as the source of truth
(rule #1) and uses freshly-substituted mined corpus inputs (rule #2). Process
launching is delegated to `oracle.run_once`, so both binaries run under the same
pinned env (`TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800`) and
only STDOUT is compared (rule #5).

## Properties (task brief §3.6 a–e)

| id | property | failure verdict | signature it catches |
|----|----------|-----------------|----------------------|
| a | **vary-input** — Rust differs on 2 inputs **where Go differs** | SIM | hardcoded constant |
| b | **grow-limit** — Rust grows with `--limit` 10→500 **where Go grows** | SIM (constant) / FAIL (shrinks) | closed-form stub |
| c | **determinism** — identical args ⇒ identical Rust bytes | FAIL | nondeterministic port |
| d | **non-empty** — Rust non-empty **where Go non-empty** | FAIL | 0-byte stub / not ported |
| e | **golden-drift** — Rust ≠ recorded golden once the input changed (and Go changed) | SIM | memorized golden constant |

Every SIM prints the **two inputs and the two equal Rust outputs** that triggered
it (task brief requirement). FAILs print the failing invocation + the violated
property. Results: `results.json`. Recorded golden constants (for drift):
`golden_constants.json`, each with input provenance.

## Run

```sh
python3 metamorphic.py --tier smoke          # against the live binaries
python3 selftest/self_test.py                # scripted self-proof (rule #6)
python3 selftest/live_constant_test.py       # LIVE constant-stub self-proof
```

## Self-proof (rule #6 — "must SELF-PROVE it catches a defect")

`selftest/self_test.py` monkeypatches the live-binary launcher
(`oracle.run_once`) with scripted Go/Rust outputs and asserts each planted defect
class is reported non-PASS, and each honest control is PASS/NA (so the FAILs are
meaningful, not a stuck "always FAIL"):

- P-a vary-input constant → SIM · P-b grow-limit constant → SIM ·
  P-b2 grow-limit shrink → FAIL · P-c nondeterminism → FAIL ·
  P-d empty stub → FAIL · P-e golden-drift → SIM
- Controls: honest vary-input → PASS, go-identical → NA, honest grow → PASS,
  go-saturates → NA, deterministic → PASS, non-empty → PASS, honest drift → PASS

Current scripted self-proof: **13/13 checks correct**.

`selftest/live_constant_test.py` is the brief's required end-to-end proof: it keeps
the **LIVE Go binary** as the real oracle (Go genuinely distinguishes the mined
`hercules` vs `kubernetes` repos) and replaces ONLY the Rust side with a throwaway
constant-emitting executable. It asserts (1) the constant stub is flagged **SIM**
by vary-input, (2) the stub is non-empty (so it is the *relation*, not emptiness,
that catches it), and (3) the **real** `history/devs` analyzer **PASSes** on the
same inputs (no cry-wolf). Current live self-proof: **3/3 checks correct**.

## Current live result (snapshot)

`PASS=26 SIM=2 FAIL=15 NA=3` → layer is **RED**, correctly: it surfaces genuine
Rust port gaps it must not hide —
- `uast parse` only has the Go grammar wired; C/Python/Rust/TS/TSX/JS/JSON/YAML/
  C++/shell error "no tree-sitter grammar wired" and emit empty stdout while Go
  produces full UAST (FAIL non-empty; the c-vs-rust and ts-vs-js pairs also trip
  vary-input SIM because both sides are empty);
- analyzers `static/clones`, `static/cohesion`, `history/anomaly`,
  `history/file-history` emit nothing where Go computes large reports.

These are real incompleteness, verified directly against the binaries — not
harness artifacts.
