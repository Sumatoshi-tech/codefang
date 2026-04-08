# FRD: Extract DetailedDataCollector mixin (Roadmap F3.1)

**ID**: FRD-20260303-detailed-data-collector
**Roadmap**: [specs/dedup/ROADMAP.md](../dedup/ROADMAP.md) — Item F3.1
**Spec**: [specs/dedup/SPEC.md](../dedup/SPEC.md) — Cluster 3: Analyzer mixin extraction

## Problem

3 aggregators (complexity, halstead, comments) implement identical patterns for
collecting detailed per-item data from individual file reports and merging them
into the final aggregated result:

```go
// Each aggregator has:
detailedFunctions []map[string]any  // (comments also has detailedComments)

// Three methods per collection key:
collectDetailedFunctions(results)    // loop reports, skip nil, call extract
extractFunctionsFromReport(report)   // type-assert report[key], append to slice
addDetailedFunctionsToResult(result) // set result[key] = slice if non-empty
```

This adds ~20 lines per collection key per aggregator (~80 lines total across 3 files).

## Feature

Add `DetailedDataCollector` to `internal/analyzers/common/` that supports
configurable collection keys. Each aggregator embeds or holds a
`*DetailedDataCollector` and delegates the collect/add operations to it.

### API

```go
// NewDetailedDataCollector creates a collector for the given report keys.
func NewDetailedDataCollector(keys ...string) *DetailedDataCollector

// CollectFromReports extracts data for all keys from all non-nil reports.
func (d *DetailedDataCollector) CollectFromReports(results map[string]analyze.Report)

// AddToResult adds all non-empty collections to the result report.
func (d *DetailedDataCollector) AddToResult(result analyze.Report)
```

### Design Decisions

- **Named field, not embedding**: Use `detailed *common.DetailedDataCollector`
  as a named field to avoid method promotion conflicts with the base
  `common.Aggregator`.
- **Multi-key support**: The collector accepts variadic keys, supporting both
  single-key (complexity: "functions", halstead: "functions") and multi-key
  (comments: "comments", "functions") use cases.
- **No deduplication**: Unlike `common.DataCollector`, this collector appends
  all items from all reports without deduplication. This matches the existing
  behavior of the three aggregators.
- **Ordered keys**: Keys are stored in insertion order for deterministic
  iteration when adding to results.

### Migration Scope

| File | Before | After |
|------|--------|-------|
| complexity/aggregator.go | `detailedFunctions` field + 3 methods | `detailed` field + 2 delegation calls |
| halstead/aggregator.go | `detailedFunctions` field + 3 methods | `detailed` field + 2 delegation calls |
| comments/aggregator.go | `detailedComments` + `detailedFunctions` fields + 4 methods | `detailed` field + 2 delegation calls |

### Test Updates

Tests that directly access internal fields/methods
(`extractFunctionsFromReport`, `collectDetailedFunctions`,
`addDetailedFunctionsToResult`, `detailedFunctions`) will be deleted from
complexity and halstead test files. This behavior is now tested by:
1. The mixin's own unit tests in `common/detailed_data_collector_test.go`
2. The existing public-facing tests (`TestAggregator_Aggregate_*`,
   `TestAggregator_DetailedFunctionsCollection`) which exercise the same
   behavior through `Aggregate()` + `GetResult()`.

## Acceptance Criteria

- [x] `common.DetailedDataCollector` struct exists in `common/detailed_data_collector.go`
- [x] `NewDetailedDataCollector(keys ...string)` constructor
- [x] `CollectFromReports(results map[string]analyze.Report)` method
- [x] `AddToResult(result analyze.Report)` method
- [x] Unit tests in `common/detailed_data_collector_test.go` (single key, multi key, nil reports, empty, no matching key)
- [x] complexity/aggregator.go uses `detailed *common.DetailedDataCollector`
- [x] halstead/aggregator.go uses `detailed *common.DetailedDataCollector`
- [x] comments/aggregator.go uses `detailed *common.DetailedDataCollector` (keys: "comments", "functions")
- [x] Internal test methods deleted from complexity/aggregator_test.go and halstead/aggregator_test.go
- [x] All existing tests pass
- [x] `go vet` clean, `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files created:**
- `internal/analyzers/common/detailed_data_collector.go` — `DetailedDataCollector` struct with `CollectFromReports` and `AddToResult`
- `internal/analyzers/common/detailed_data_collector_test.go` — 10 test cases

**Files modified:**
- `internal/analyzers/complexity/aggregator.go` — replaced `detailedFunctions` field + 3 methods with `detailed *common.DetailedDataCollector` + 2 delegation calls
- `internal/analyzers/complexity/aggregator_test.go` — updated `TestNewAggregator`, deleted 3 internal tests
- `internal/analyzers/halstead/aggregator.go` — replaced `detailedFunctions` field + 3 methods with `detailed *common.DetailedDataCollector` + 2 delegation calls
- `internal/analyzers/halstead/aggregator_test.go` — updated `TestNewAggregator`, deleted 3 internal tests
- `internal/analyzers/comments/aggregator.go` — replaced `detailedComments` + `detailedFunctions` fields + 4 methods with `detailed *common.DetailedDataCollector` + 2 delegation calls
