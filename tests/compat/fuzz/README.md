# Per-stage differential fuzzing (compat layer 6 / SPEC roadmap §6)

Go-native (`testing/F`) differential fuzz targets, **one per PURE stage** of the
codefang/uast pipeline. Each target feeds the SAME input to the **LIVE Go
binary** (the oracle, `build/bin/{codefang,uast}`) and to the **Rust binary**
(`target/release/{codefang,uast}`) under the pinned env, and **FAILS on any
divergence**. Divergence-finding inputs are distilled back into
`../corpus/fuzzfinds/` with the differing Go outputs stored as evidence.

## Why this exists (the threat model)

The prior "32/32 golden" signal was gameable: analyzers emitted golden bytes as
constants, and a gate was weakened to hide a real bug. This layer defeats that
because a **freshly mutated input cannot be memorized**: the corpus is structure
-aware (mutations of real source), the oracle is the live Go binary, and the
self-proving tests assert the harness catches planted defects.

## Pure stages → targets

| Stage | Target | Invocation diffed |
|-------|--------|-------------------|
| tree-sitter parse | `FuzzParse`, `FuzzParseGo` | `uast parse --format json` |
| UAST map | `FuzzMap`, `FuzzQuery` | `uast analyze` / `uast query` |
| cf-gojson serializer | `FuzzSerializerJSON` | `run … --format json` |
| cf-goyaml serializer | `FuzzSerializerYAML` | `run … --format yaml` |
| CFB1 serializer | `FuzzSerializerCFB1` | `run … --format bin` |
| analyzer ComputeAllMetrics | `FuzzComputeAllMetrics`, `FuzzComputeAllMetricsGo` | `run --analyzers static/{complexity,composition}` |

`*Go` variants restrict mutation to Go source — the ONE language whose
tree-sitter grammar is currently wired in the Rust port — so the coverage-guided
mutator digs DEEP into a supported language instead of bouncing off the
"grammar not wired" gap that the all-language targets expose for every other
language.

## Non-negotiable design (encoded in `harness.go`)

- **Oracle = live Go binary.** No re-derivation in Go or Rust; we run the actual
  binaries as subprocesses.
- **Canonicalization is MEASURED, never declared.** For each input, Go is run
  N≥3×; only fields that VARY across Go's own runs may be neutralized
  (`measureGoVariance` → `canonByMeasure`), and the differing Go outputs are
  stored as evidence. `canonByMeasure` **refuses to blank a Go-stable field** —
  the exact cheat that hid a real bug before (proved by
  `TestSelfCheck_TamperBlankStableField`).
- **Pinned env:** `TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800`,
  argv passed as a list (no shell glob), STDOUT compared (stderr is progress).
- **Distillation keeps the SMALLEST set preserving distinct findings:**
  `distill` dedups by divergence CLASS (the set of differing JSON field keys, or
  the raw-bytes shape), so fuzzer minimization's hundreds of near-duplicates
  collapse to one representative per bug.

## Self-proving (rule 6 — a green that can't catch a bug is worthless)

`selfcheck_test.go` injects known defects via a shim "Rust" (`CFFUZZ_RUST_DIR`)
and asserts the harness reports FAIL:

- `TestSelfCheck_DetectsConstantStub` — Rust emits a constant → caught.
- `TestSelfCheck_DetectsByteFlip` — Rust flips one Go-stable byte → caught.
- `TestSelfCheck_TamperBlankStableField` — `canonByMeasure` will NOT equate a
  differing Go-stable field; only MEASURED-variant fields canonicalize.
- `TestSelfCheck_VarianceMeasuredNotDeclared` — measurement yields 0 variant
  fields on a deterministic analyzer (evidence stored).
- `TestSelfCheck_RealRustPasses` — the REAL Rust binary passes the baseline (so
  the gate is not stuck-red).

## Running

```bash
# self-checks + seed pass + bounded mutation fuzz (20s/target)
tests/compat/fuzz/run_fuzz.sh 20

# one target, longer:
FUZZ_ONLY=FuzzParseGo tests/compat/fuzz/run_fuzz.sh 60

# raw go test (seed pass only, no mutation):
go test ./tests/compat/fuzz/ -run FuzzParseGo -count=1
# active mutation fuzz of one target:
go test ./tests/compat/fuzz/ -run '^$' -fuzz '^FuzzParseGo$' -fuzztime 30s
```

## Divergences found (recorded, NOT fixed — fixing Rust is out of scope here)

All recorded under `../corpus/fuzzfinds/` with `.evidence.json` + the N Go runs.

1. **Rust has only the Go tree-sitter grammar wired.** For C/C++/Python/TS/TSX/
   JS/Rust/shell/etc., `uast parse|analyze|query` errors
   ("no tree-sitter grammar wired for language X") and produces empty output,
   while Go parses them. Surfaced by `FuzzParse`/`FuzzMap`/`FuzzQuery` seeds.
2. **`static/complexity` (and the serializers downstream) are Go-only.** On
   non-Go files the Rust analyzer reports 0 functions / "No complexity data
   available" (a CONSTANT 756B JSON / 329B CFB1 / 339B YAML) while Go computes
   real metrics. This is the classic simulation signature, caught by
   `FuzzSerializer*` / `FuzzComputeAllMetrics`.
3. **Invalid-UTF-8 token serialization differs (Go input).** When source
   contains non-UTF-8 bytes, Go's `uast parse` JSON-escapes them as `�`
   while Rust emits the raw replacement char `�`; the downstream node `id`
   content-hash differs as a consequence. Found by `FuzzParseGo` mutation in
   ~12s and minimized automatically.
