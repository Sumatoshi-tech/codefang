# FRD: Typed Report Items — Reduce per-function map allocations (Roadmap perf30/3.2)

**ID**: FRD-20260311-typed-report-items
**Roadmap**: [specs/perf30/ROADMAP.md](../perf30/ROADMAP.md) — Step 3.2
**Date**: 2026-03-11

## Problem

Each per-file analyzer builds `[]map[string]any` for per-function data. For example,
complexity creates one `map[string]any` with 8 keys per function; halstead creates one
with 20 keys. On kubernetes (~150K functions), this produces ~1M+ small map allocations.

Each `map[string]any` incurs:
- Map header: ~100 bytes
- Bucket array + overflow: ~32 bytes per key-value pair
- String key headers: ~16 bytes each

A typed struct with the same fields uses ~3-5x less memory (contiguous, no bucket overhead).

These maps flow through `DetailedDataCollector` which appends ALL items for the entire run,
keeping them alive until `AddToResult()`. This is the largest long-lived allocation pool
in Full aggregation mode.

## Decision

Introduce `analyze.TypedCollection` — a wrapper that carries typed struct slices through
the report pipeline, deferring `map[string]any` conversion to the serialization boundary.

### 1. `analyze.TypedCollection` type

```go
type TypedCollection struct {
    Items      any                                      // concrete typed slice
    SourceFile string                                   // stamped by StampSourceFile
    ToMaps     func(items any, sourceFile string) []map[string]any
}
```

### 2. Per-file analyzers return `TypedCollection`

Each analyzer's `buildResult()` puts a `TypedCollection` in the report instead of
`[]map[string]any`. The typed structs already exist in each analyzer:

| Analyzer   | Struct                   | Keys | Collection key |
|------------|--------------------------|------|----------------|
| complexity | `FunctionMetrics`        | 8    | `"functions"`  |
| halstead   | `FunctionHalsteadMetrics`| 20   | `"functions"`  |
| comments   | `CommentDetail`          | 8    | `"comments"`   |
| comments   | `FunctionInfo`           | 5    | `"functions"`  |
| cohesion   | `Function`               | 7    | `"functions"`  |

### 3. `StampSourceFile` handles `TypedCollection`

Instead of modifying each map in-place, it sets `tc.SourceFile = filePath` on the wrapper.
The converter function stamps `_source_file` on each map when converting.

### 4. `DetailedDataCollector` stores typed data

New internal storage: `typedCollections map[string][]TypedCollection` alongside existing
`collections` for backward compatibility. Conversion to `[]map[string]any` happens in
`AddToResult()` using each TypedCollection's `ToMaps` function.

### 5. `SpillableDataCollector` converts on collection

SpillableDataCollector needs `map[string]any` for identifier extraction and gob serialization.
It calls `TypedCollection.ToMaps()` in `CollectFromReport()` to convert at ingestion time.
This is acceptable because SpillableDataCollector only stores deduplicated items (much fewer
than the full set), and the main memory win is in DetailedDataCollector.

### 6. Backward compatibility

Both collectors fall through to existing `[]map[string]any` handling when the report value
is not a `TypedCollection`. No existing callers break.

## Contract

- `TypedCollection` is a value type in `analyze` package.
- `Items` field holds a concrete typed slice (`[]FunctionMetrics`, etc.).
- `ToMaps` converter receives the items and source file path; returns `[]map[string]any`.
- When `SourceFile` is empty, the converter omits `_source_file` from output maps.
- `DetailedDataCollector` stores `TypedCollection` references until `AddToResult()`.
- `SpillableDataCollector` converts `TypedCollection` to maps at `CollectFromReport()` time.
- JSON output is byte-identical to pre-optimization (same keys, same values, same order).
- `StampSourceFile` handles both `TypedCollection` and legacy `[]map[string]any`.

## Acceptance Criteria

- [x] `analyze.TypedCollection` type defined
- [x] `StampSourceFile` handles `TypedCollection`
- [x] `DetailedDataCollector` stores typed data, converts in `AddToResult()`
- [x] `SpillableDataCollector` handles `TypedCollection` in `CollectFromReport()`
- [x] Complexity analyzer returns `TypedCollection` in per-file reports
- [x] Halstead analyzer returns `TypedCollection` in per-file reports
- [x] Comments analyzer returns `TypedCollection` in per-file reports
- [x] Cohesion analyzer returns `TypedCollection` in per-file reports
- [x] `BenchmarkTypedVsMapAccumulation` shows >2x alloc reduction (2.2x allocs, 2.6x heap)
- [x] `go test ./internal/analyzers/{complexity,halstead,comments,cohesion}/...` passes
- [x] `go test ./internal/analyzers/common/...` passes
- [x] `go test ./internal/analyzers/analyze/...` passes
- [x] `make lint` passes

## Benchmark Results

`BenchmarkTypedVsMapAccumulation` (5000 files × 10 functions = 50K items):

| Metric | map[string]any | TypedCollection | Improvement |
|--------|---------------|-----------------|-------------|
| Heap   | 21.0 MiB      | 8.2 MiB         | 2.6x        |
| Allocs | 267K          | 122K            | 2.2x        |

## Implementation

Files created:
- `internal/analyzers/analyze/typed_collection.go` — `TypedCollection`, `ItemConverter`, `SourceFileKey`

Files modified:
- `internal/analyzers/analyze/static.go` — `StampSourceFile` type switch for TypedCollection
- `internal/analyzers/analyze/analyzer.go` — `ReportFunctionList` TypedCollection support
- `internal/analyzers/analyze/static_test.go` — `TestStampSourceFile_TypedCollection`
- `internal/analyzers/common/detailed_data_collector.go` — typed collection storage
- `internal/analyzers/common/reportutil/reportutil.go` — `mapSlicer` duck-typing interface
- `internal/analyzers/common/aggregator_bench_test.go` — benchmark
- `internal/analyzers/complexity/complexity.go` — `FunctionReportItem`, converter
- `internal/analyzers/complexity/complexity_test.go` — updated assertions
- `internal/analyzers/halstead/halstead.go` — `FunctionReportItem`, converter
- `internal/analyzers/halstead/halstead_test.go` — updated assertions
- `internal/analyzers/halstead/visitor_test.go` — updated assertions
- `internal/analyzers/halstead/stabilization_test.go` — updated assertions
- `internal/analyzers/halstead/cms_test.go` — updated assertions
- `internal/analyzers/comments/comments.go` — `CommentReportItem`, `FunctionReportItem`, converters
- `internal/analyzers/comments/comments_test.go` — updated assertions
- `internal/analyzers/cohesion/cohesion.go` — `FunctionReportItem`, converter
- `internal/analyzers/cohesion/cohesion_test.go` — updated assertions
- `internal/analyzers/cohesion/visitor_test.go` — updated assertions
