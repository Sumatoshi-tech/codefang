# FRD-20260327: Summary Statistics Utility

**Date:** 2026-03-27
**Author:** Agent
**Status:** In Progress
**Roadmap:** specs/filestats/ROADMAP.md — Step 1.1
**Spec:** specs/filestats/SPEC.md — Feature 1

## Problem

Feature 1 (per-file output mode) requires a `summary_stats` object on each JSON section containing `{min, p25, p50, p75, p95, max, avg}` computed across per-file metric values. No such composite computation exists today. `pkg/alg/stats/` has `Percentile()`, `Mean()`, `Min()`, `Max()` individually, but there is no single function that produces the full 7-stat distribution from a `[]float64`.

## Solution

Create a `Summary` struct and `ComputeSummary(values []float64) Summary` function in a new package `internal/analyzers/common/stats/`. This package wraps `pkg/alg/stats` functions into a single call that produces all 7 statistics. Placing it under `internal/analyzers/common/` follows the existing pattern for shared analyzer utilities (e.g., `common/reportutil`, `common/plotpage`).

## Type Definition

```go
// Summary holds the 7-stat distribution for a set of numeric values.
type Summary struct {
    Min float64
    P25 float64
    P50 float64
    P75 float64
    P95 float64
    Max float64
    Avg float64
}
```

## Function Contract

```go
// ComputeSummary computes the 7-stat distribution from values.
// Returns a zero Summary for an empty slice.
// Returns all fields equal to the single value for a one-element slice.
// The input slice is not modified.
func ComputeSummary(values []float64) Summary
```

**Preconditions:** None (empty slice is valid).
**Postconditions:** `Min <= P25 <= P50 <= P75 <= P95 <= Max` and `Min <= Avg <= Max`.
**Invariants:** Input slice is not modified. No allocations beyond the sorted copy inside `pkg/alg/stats.Percentile`.

## Percentile Constants

```go
const (
    P25 = 0.25
    P50 = 0.50
    P75 = 0.75
    P95 = 0.95
)
```

## Edge Cases

| Input | Expected |
|-------|----------|
| `[]float64{}` | Zero `Summary` |
| `[]float64{42}` | All fields = 42 |
| `[]float64{1, 2}` | Min=1, Max=2, Avg=1.5, percentiles interpolated |
| `[]float64{1, 2, 3, 4, 5}` | Standard distribution |

## Test Plan

- Table-driven tests covering: 0, 1, 2, 5, 100 values.
- Ordering invariant asserted for every case: `Min <= P25 <= P50 <= P75 <= P95 <= Max`.
- Average bounds asserted: `Min <= Avg <= Max`.
- Exact values asserted for small known inputs.
- `go test -race` clean.

## Performance

Pure computation on a sorted copy. For the expected use case (N < 10000 files), sub-millisecond. No optimization needed.

## Implementation

**Status:** Complete

**Files created:**
- `internal/analyzers/common/stats/summary.go` — `Summary` struct, `ComputeSummary()` function
- `internal/analyzers/common/stats/summary_test.go` — 11 test cases, 100% coverage

**Coverage:** 100% of statements.
**Race detector:** Clean.
**Lint:** Clean (zero issues in new files).
