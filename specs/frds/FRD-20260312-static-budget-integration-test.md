# FRD: Memory Budget Integration Test (Roadmap perf30/5.3)

**ID**: FRD-20260312-static-budget-integration-test
**Roadmap**: [specs/perf30/ROADMAP.md](../perf30/ROADMAP.md) — Step 5.3
**Date**: 2026-03-12

## Problem

All memory optimizations (Steps 1.1–5.2) have been implemented and unit-tested individually,
but there is no end-to-end verification that static analysis on a large synthetic codebase
actually stays within a given memory budget. Without this safety net, regressions in any
optimization could silently reintroduce OOM on large repositories.

## Decision

Add `TestStaticAnalyzers_MemoryBudget` in `internal/analyzers/analyze/budget_static_test.go`.
This is a pass/fail gate (using `testing.T`), not a comparative benchmark.

### Test Design

1. Generate a temp directory with 5000 `.go` files, each containing 50 functions (250K
   functions total — comparable to ~/sources/kubernetes).
2. Create a `StaticService` with all production analyzers.
3. Set `SpillThreshold` via `budget.SolveStaticBudget(budgetBytes)`.
4. Set `AggregationMode = SummaryOnly` (text output mode — most common for large runs).
5. Set `debug.SetMemoryLimit(budgetBytes)` to engage Go GC self-regulation.
6. Start a `heapSampler` goroutine sampling `HeapInuse` every 50ms.
7. Run `AnalyzeFolder`.
8. Assert:
   - No error
   - Result map has entries for all enabled analyzers
   - Peak `HeapInuse` < 2× budget (1 GiB for 512 MiB budget)

### Build Tag

Use `//go:build integration` so CI can opt in. The test takes ~30s and generates
temporary files, so it should not run on every `go test ./...`.

## Contract

- Test must pass with `go test -tags integration ./internal/analyzers/analyze/...`.
- Peak heap must stay below 2× the memory budget.
- All analyzers must produce results (non-nil report).
- Test cleans up temp files via `t.TempDir()`.

## Acceptance Criteria

- [x] Test in `internal/analyzers/analyze/budget_static_test.go`
- [x] Build tag `//go:build integration`
- [x] Generates 5000 `.go` files × 50 functions
- [x] Runs `StaticService.AnalyzeFolder` with 512 MiB budget
- [x] Asserts peak heap < 1 GiB
- [x] Asserts analysis completes without error
- [x] Asserts result map has entries for all enabled analyzers
- [x] `make lint` passes

## Implementation

Files created:
- `internal/analyzers/analyze/budget_static_test.go` — integration test with `//go:build integration` tag

Reuses existing test infrastructure:
- `setupHeavyBenchDir` — synthetic Go file generator from `static_bench_test.go`
- `heapSampler` — peak HeapInuse sampler from `static_bench_test.go`
- `testStaticAnalyzers` — production analyzer set from `static_bench_test.go`
- `budget.SolveStaticBudget` — budget solver from `internal/budget/`

Test results:
- Peak heap: 62 MiB (limit: 1024 MiB) — 94% headroom
- All 5 analyzers produced results
- Analysis completed without error
