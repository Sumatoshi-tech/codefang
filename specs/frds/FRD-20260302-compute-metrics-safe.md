# FRD: Shared computeMetricsSafe (Roadmap 3.1)

**ID**: FRD-20260302-compute-metrics-safe
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 3.1
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster 3c

## Problem

Six history analyzers (`devs`, `anomaly`, `sentiment`, `shotness`, `typos`, `quality`) each define an identical local `computeMetricsSafe` function:

```go
func computeMetricsSafe(report analyze.Report) (*ComputedMetrics, error) {
    if len(report) == 0 {
        return &ComputedMetrics{}, nil
    }
    return ComputeAllMetrics(report)
}
```

The quality analyzer uses an inline closure variant of the same pattern. All six follow the identical contract: guard against empty reports by returning a zero-value metrics struct, otherwise delegate to the real computation.

## Solution

Create a generic factory function `SafeMetricComputer[M any]` in the `analyze` package that wraps any `MetricComputer[M]` with the empty-report guard. This returns a `MetricComputer[M]` that can be assigned directly to `BaseHistoryAnalyzer.ComputeMetricsFn`, eliminating all six local copies.

### Placement

`internal/analyzers/analyze/metrics_safe.go` — alongside `MetricComputer[M]` and `BaseHistoryAnalyzer[M]` in the same package.

### API

```go
// SafeMetricComputer wraps a MetricComputer to return empty on empty reports.
func SafeMetricComputer[M any](compute MetricComputer[M], empty M) MetricComputer[M]
```

### Migration (per analyzer)

Before:
```go
ComputeMetricsFn: computeMetricsSafe,
```

After:
```go
ComputeMetricsFn: analyze.SafeMetricComputer(ComputeAllMetrics, &ComputedMetrics{}),
```

Then delete the local `computeMetricsSafe` function.

## Acceptance Criteria

- [x] `SafeMetricComputer[M]` defined in `internal/analyzers/analyze/metrics_safe.go`
- [x] Unit test in `internal/analyzers/analyze/metrics_safe_test.go` covering:
  - Empty report returns `empty` value with nil error
  - Non-empty report delegates to wrapped compute function
  - Wrapped compute error is propagated
- [x] All 6 local `computeMetricsSafe` functions removed
- [x] Quality analyzer inline closure replaced
- [x] `go vet` clean
- [x] `go test ./internal/analyzers/...` passes
- [x] `make lint` passes — zero issues, zero dead code

## Risk

Low. The function is a trivial generic wrapper. Each migration is a mechanical replacement of one line plus deletion of a local function.

## Implementation

### Files Created

- `internal/analyzers/analyze/metrics_safe.go`
- `internal/analyzers/analyze/metrics_safe_test.go`

### Files Modified

- `internal/analyzers/devs/analyzer.go` — remove `computeMetricsSafe`, use `SafeMetricComputer`
- `internal/analyzers/anomaly/analyzer.go` — remove `computeMetricsSafe`, use `SafeMetricComputer`
- `internal/analyzers/sentiment/analyzer.go` — remove `computeMetricsSafe`, use `SafeMetricComputer`
- `internal/analyzers/shotness/analyzer.go` — remove `computeMetricsSafe`, use `SafeMetricComputer`
- `internal/analyzers/typos/analyzer.go` — remove `computeMetricsSafe`, use `SafeMetricComputer`
- `internal/analyzers/quality/analyzer.go` — replace inline closure with `SafeMetricComputer`
- `internal/analyzers/devs/analyzer_test.go` — update test to use `SafeMetricComputer`
- `internal/analyzers/shotness/analyzer_test.go` — update tests to use `SafeMetricComputer`

### Lines Eliminated

~42 lines of duplicate `computeMetricsSafe` functions removed across 6 packages.

### Verification

- `go vet` — clean
- `go test ./internal/analyzers/...` — all pass
- `make lint` — zero issues, zero dead code
