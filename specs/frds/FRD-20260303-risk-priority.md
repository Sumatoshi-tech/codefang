# FRD: Extract shared RiskPriority to pkg/metrics (Roadmap F1.3)

**ID**: FRD-20260303-risk-priority
**Roadmap**: [specs/dedup/ROADMAP.md](../dedup/ROADMAP.md) — Item F1.3
**Spec**: [specs/dedup/SPEC.md](../dedup/SPEC.md) — Cluster 1: Shared Constants

## Problem

4 analyzers define identical `riskPriority(level string) int` functions that map
risk level strings to sortable integers for `sort.Slice` comparisons:

```go
func riskPriority(level string) int {
    switch level {
    case "CRITICAL": return 0
    case "HIGH":     return 1
    case "MEDIUM":   return 2
    default:         return 3
    }
}
```

Each also duplicates `riskPriorityCritical/High/Medium/Default` constants.
The `pkg/metrics` package already defines `RiskLevel` type and the corresponding
constants (`RiskCritical`, `RiskHigh`, `RiskMedium`, `RiskLow`).

## Feature

Add `RiskPriority(level RiskLevel) int` to `pkg/metrics/metrics.go` and migrate
all 4 callers. Remove local `riskPriority()` functions and their priority constants.

### API

```go
// RiskPriority returns a sortable integer for a risk level.
// Lower values indicate higher priority: CRITICAL < HIGH < MEDIUM < LOW/unknown.
func RiskPriority(level RiskLevel) int
```

### Design Decisions

- **Placed in `pkg/metrics`**: Co-located with `RiskLevel` type and constants.
- **Accepts `RiskLevel` (not `string`)**: Provides type safety. Callers with `string`
  fields convert via `metrics.RiskLevel(field)`. F1.4 will change field types to
  `RiskLevel`, eliminating casts.
- **Priority constants unexported**: Only the function is public; the mapping values
  (0, 1, 2, 3) are implementation details.
- **`comments` behavior preserved**: Comments only uses HIGH/MEDIUM (never CRITICAL).
  Priorities are only used for relative sort ordering, so adding the CRITICAL case
  doesn't change behavior.

### Migration Scope

| Analyzer | Call site | Notes |
|----------|----------|-------|
| devs | `metrics.go:648` — `sort.Slice` by `BusFactorData.RiskLevel` | Already imports `pkg/metrics` |
| file_history | `metrics.go:247` — `sort.Slice` by `HotspotData.RiskLevel` | Needs `pkg/metrics` import |
| complexity | `metrics.go:369` — `sort.Slice` by `HighRiskFunctionData.RiskLevel` | Already imports `pkg/metrics` |
| comments | `metrics.go:379` — `sort.Slice` by `UndocumentedFunctionData.RiskLevel` | Already imports `pkg/metrics` |

**Removed from each:** local `riskPriority()` function + `riskPriorityCritical/High/Medium/Default` constants.

## Acceptance Criteria

- [x] `RiskPriority(level RiskLevel) int` exists in `pkg/metrics/metrics.go`
- [x] Unit tests in `pkg/metrics/metrics_test.go` cover: all 4 levels + unknown
- [x] `devs/metrics.go` uses `metrics.RiskPriority`, local function + constants removed
- [x] `file_history/metrics.go` uses `metrics.RiskPriority`, local function + constants removed
- [x] `complexity/metrics.go` uses `metrics.RiskPriority`, local function + constants removed
- [x] `comments/metrics.go` uses `metrics.RiskPriority`, local function + constants removed
- [x] All existing tests pass unchanged
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files modified:**
- `pkg/metrics/metrics.go` — added `RiskPriority(level RiskLevel) int` with unexported priority constants
- `pkg/metrics/metrics_test.go` — added `TestRiskPriority_AllLevels` (table-driven, 4 levels + ordering) and `TestRiskPriority_UnknownLevel`
- `internal/analyzers/devs/metrics.go` — removed `riskPriorityCritical/High/Medium/Default` constants, removed `riskPriority()` function, updated sort call site
- `internal/analyzers/file_history/metrics.go` — added `pkg/metrics` import, removed local `RiskCritical/High/Medium/Low` priority constants, removed `riskPriority()` function, updated sort call site
- `internal/analyzers/complexity/metrics.go` — removed `riskPriorityCritical/High/Medium/Default` constants, removed `riskPriority()` function, updated sort call site
- `internal/analyzers/comments/metrics.go` — removed `riskPriorityHigh/Medium/Default` constants, removed `riskPriority()` function, updated sort call site
- `internal/analyzers/devs/metrics_test.go` — removed redundant `TestRiskPriority` (now tested centrally)
- `internal/analyzers/file_history/metrics_test.go` — removed redundant `TestRiskPriority`
- `internal/analyzers/complexity/metrics_test.go` — removed redundant `TestRiskPriority`
- `internal/analyzers/comments/metrics_test.go` — removed redundant `TestRiskPriority`
