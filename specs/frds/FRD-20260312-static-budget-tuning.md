# FRD: Static Phase Budget Auto-Tuning (Roadmap perf30/5.1)

**ID**: FRD-20260312-static-budget-tuning
**Roadmap**: [specs/perf30/ROADMAP.md](../perf30/ROADMAP.md) — Step 5.1
**Date**: 2026-03-12

## Problem

When `--memory-budget` is set, the static phase only applies `debug.SetMemoryLimit` (Step 1.3).
It does not adjust `MaxWorkers` or `SpillThreshold` — both remain at fixed defaults
(`DefaultStaticMaxWorkers=8`, `defaultSpillThreshold=10000`).

On a memory-constrained machine (e.g., 512 MB budget), 8 concurrent workers each holding a
UAST parse tree (~5-50 MiB) plus 10K in-memory items per aggregator can still exhaust the
budget. Conversely, with a 4 GB budget, the defaults are unnecessarily conservative.

## Decision

Add `SolveStaticBudget(budgetBytes int64) StaticBudgetConfig` to `internal/budget/`.
This function derives `MaxWorkers` and `SpillThreshold` from the memory budget using
empirically measured per-component costs.

### Cost Model

```
perWorkerFootprint = 50 MiB   (parser + tree-sitter + Go nodes + file content)
staticBaseOverhead = 150 MiB  (Go runtime + loaded analyzers)
avgItemBytes       = 512      (average gob-encoded size of a report item)
numAnalyzers       = 6        (complexity, halstead, comments, cohesion, clones, imports)
```

### Formulas

```
usable      = budget * 95 / 100                (5% slack)
available   = usable - staticBaseOverhead
maxWorkers  = clamp(available / perWorkerFootprint, 1, min(NumCPU, 16))
workerAlloc = maxWorkers * perWorkerFootprint
remaining   = available - workerAlloc
perAnalyzer = remaining / numAnalyzers
spillThreshold = clamp(perAnalyzer / avgItemBytes, 1000, 100000)
```

### Wiring

1. `StaticBudgetConfig` has fields: `MaxWorkers int`, `SpillThreshold int`.
2. `StaticService` gets a new `SpillThreshold int` field (0 = default).
3. `initAggregators` calls `SetSpillThreshold` on aggregators when `SpillThreshold > 0`.
4. In `cmd/codefang/commands/run.go`, `runStaticPhase` calls `SolveStaticBudget` and
   applies the config to the service (only when `--memory-budget` is set and `--static-workers`
   is not explicitly overridden).
5. When no budget is set, all defaults are preserved (zero-value config = no override).

### Interface: `SpillThresholdSetter`

To set spill threshold on aggregators without importing `common`, define an interface
in the `analyze` package:

```go
type SpillThresholdSetter interface {
    SetSpillThreshold(threshold int)
}
```

`initAggregators` checks for this interface after `AggregationModeAware`.

## Contract

- `SolveStaticBudget(0)` returns zero-value `StaticBudgetConfig` (no override).
- `SolveStaticBudget(budget)` where `budget < MinStaticBudget` returns zero-value config.
- `MaxWorkers >= 1` when budget is sufficient.
- `SpillThreshold >= MinSpillThreshold (1000)` when budget is sufficient.
- Explicit `--static-workers` overrides budget-derived `MaxWorkers`.
- Without `--memory-budget`, behavior is identical to current code.

## Acceptance Criteria

- [x] `budget.SolveStaticBudget` computes `MaxWorkers` and `SpillThreshold`
- [x] `StaticService.SpillThreshold` field added
- [x] `initAggregators` applies spill threshold via `SpillThresholdSetter` interface
- [x] `runStaticPhase` wires budget config to service
- [x] Zero budget produces zero config (no override)
- [x] Budget below minimum produces zero config
- [x] Tests cover boundary conditions (zero, minimum, large budgets)
- [x] `go test ./internal/budget/...` passes
- [x] `go test ./internal/analyzers/analyze/...` passes
- [x] `go test ./cmd/codefang/commands/...` passes
- [x] `make lint` passes

## Implementation

Files created:
- `internal/budget/static_solver.go` — `StaticBudgetConfig`, `SolveStaticBudget`, cost model constants
- `internal/budget/static_solver_test.go` — 9 boundary tests

Files modified:
- `internal/analyzers/analyze/analyzer.go` — `SpillThresholdSetter` interface
- `internal/analyzers/analyze/static.go` — `SpillThreshold` field, `initAggregators` early-continue refactor
- `internal/analyzers/analyze/static_test.go` — `TestStaticService_SpillThreshold_AppliedToAggregators`
- `internal/analyzers/common/aggregator.go` — compile-time `SpillThresholdSetter` check
- `cmd/codefang/commands/run.go` — `applyStaticBudgetConfig`, `staticExecutor` with `memoryBudget` param
- `cmd/codefang/commands/run_test.go` — 3 tests for `applyStaticBudgetConfig`, updated mock signatures
- `cmd/codefang/commands/run_plot_test.go` — updated mock signatures
- `cmd/codefang/commands/run_config_test.go` — updated mock signatures
