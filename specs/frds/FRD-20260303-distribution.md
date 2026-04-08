# FRD: Add Distribution[T] counting utility to pkg/alg/stats (Roadmap F5.3)

**ID**: FRD-20260303-distribution
**Roadmap**: [specs/dedup/ROADMAP.md](../dedup/ROADMAP.md) — Item F5.3
**Spec**: [specs/dedup/SPEC.md](../dedup/SPEC.md) — Cluster 3: Statistics & Numerical Analysis

## Problem

4 analyzers (complexity, halstead, cohesion, and their plot files) implement identical
"classify items by threshold, count per bucket" loops:

```go
dist := SomeDistributionStruct{}
for _, item := range items {
    switch {
    case item.Value >= thresholdA: dist.BucketA++
    case item.Value >= thresholdB: dist.BucketB++
    default: dist.Default++
    }
}
```

The classification logic varies per analyzer, but the "iterate → classify → count" loop
is identical boilerplate. Additionally, the `metrics.go` files use typed structs
(`DistributionData`) while `plot.go` files independently re-implement the same logic
returning `map[string]int`.

## Feature

Add `Distribution[T any](items []T, classify func(T) string) map[string]int` to
`pkg/alg/stats/stats.go`. The function iterates `items`, calls `classify` on each,
and returns the count per label.

Then migrate complexity and cohesion analyzers:
1. **metrics.go**: Replace `DistributionData` struct with `map[string]int`, replace
   the manual loop in `Compute()` with `stats.Distribution()`, add distribution label constants.
2. **metrics_test.go**: Update assertions from struct field access to map key lookup.
3. **plot.go**: Replace `countXxxDistribution()` helper with `stats.Distribution()` call.

### Design Decisions

- **Placement in `stats.go`**: Distribution is a counting/statistical operation,
  consistent with the package's purpose.
- **Returns `map[string]int`**: Matches the existing plot.go convention and is
  JSON-serializable. Struct fields serialized via `json:"simple"` produce the same
  JSON as `map[string]int{"simple": N}`.
- **Nil-in → nil-out**: Follows the package convention. A nil slice returns nil.
  An empty (non-nil) slice returns an empty (non-nil) map.
- **Caller provides `classify`**: Keeps threshold logic in the analyzer where it
  belongs. `stats.Distribution` owns only the count loop.
- **Label constants use lowercase**: Match existing JSON struct tags (`json:"simple"`)
  in metrics.go to preserve JSON output backward compatibility.
- **Plot.go uses display labels**: Plot distribution functions use title-case labels
  ("Simple", "Excellent") for chart display. These remain title-case.

### Migration Scope

| File | Action |
|------|--------|
| `pkg/alg/stats/stats.go` | Add `Distribution[T]` function |
| `pkg/alg/stats/stats_test.go` | Add `TestDistribution` (6 cases) |
| `internal/analyzers/complexity/metrics.go` | Replace `DistributionData` struct with `map[string]int`, add label constants, use `stats.Distribution` |
| `internal/analyzers/complexity/metrics_test.go` | Update distribution assertions to map key lookup |
| `internal/analyzers/complexity/plot.go` | Replace `countComplexityDistribution` with `stats.Distribution` call |
| `internal/analyzers/cohesion/metrics.go` | Replace `DistributionData` struct with `map[string]int`, add label constants, use `stats.Distribution` |
| `internal/analyzers/cohesion/metrics_test.go` | Update distribution assertions to map key lookup |
| `internal/analyzers/cohesion/plot.go` | Replace `countCohesionDistribution` with `stats.Distribution` call |

### Not Migrated

- `halstead/metrics.go` — same pattern, deferred to keep scope minimal (2 analyzer minimum)
- `file_history/metrics.go` — does not have a distribution counting pattern

## Acceptance Criteria

- [x] `stats.Distribution[T any](items []T, classify func(T) string) map[string]int` exists in `pkg/alg/stats/stats.go`
- [x] Unit tests in `pkg/alg/stats/stats_test.go` (6 cases)
- [x] complexity/metrics.go migrated: `DistributionData` → `map[string]int`, `Compute` uses `stats.Distribution`
- [x] cohesion/metrics.go migrated: `DistributionData` → `map[string]int`, `Compute` uses `stats.Distribution`
- [x] complexity/plot.go: `countComplexityDistribution` replaced with `stats.Distribution` call
- [x] cohesion/plot.go: `countCohesionDistribution` replaced with `stats.Distribution` call
- [x] All existing tests pass
- [x] `go vet` clean, `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files modified:**
- `pkg/alg/stats/stats.go` — added `Distribution[T any](items []T, classify func(T) string) map[string]int`
- `pkg/alg/stats/stats_test.go` — added `TestDistribution` (6 cases: nil_returns_nil, empty_returns_empty_map, single_item, multiple_buckets, all_same_bucket, string_items)
- `internal/analyzers/complexity/metrics.go` — replaced `DistributionData` struct with `MetricDistSimple`/`MetricDistModerate`/`MetricDistComplex` constants, `Compute` uses `stats.Distribution` + `classifyComplexityLevel`
- `internal/analyzers/complexity/metrics_test.go` — updated distribution assertions from struct fields to map key lookup
- `internal/analyzers/complexity/plot.go` — replaced `countComplexityDistribution` with `classifyComplexityForPlot` + `stats.Distribution`, added `plotLabel*` constants
- `internal/analyzers/complexity/plot_test.go` — updated to use `stats.Distribution` + `classifyComplexityForPlot`
- `internal/analyzers/cohesion/metrics.go` — replaced `DistributionData` struct with `MetricDistExcellent`/`MetricDistGood`/`MetricDistFair`/`MetricDistPoor` constants, `Compute` uses `stats.Distribution` + `classifyCohesionLevel`
- `internal/analyzers/cohesion/metrics_test.go` — updated distribution assertions from struct fields to map key lookup
- `internal/analyzers/cohesion/plot.go` — replaced `countCohesionDistribution` with `classifyCohesionForPlot` + `stats.Distribution`, added `plotLabel*` constants
- `internal/analyzers/cohesion/plot_test.go` — updated to use `stats.Distribution` + `classifyCohesionForPlot`
