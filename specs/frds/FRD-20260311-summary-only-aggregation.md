# FRD: Summary-only aggregation mode (Roadmap perf30/2.1)

**ID**: FRD-20260311-summary-only-aggregation
**Roadmap**: [specs/perf30/ROADMAP.md](../perf30/ROADMAP.md) — Step 2.1
**Date**: 2026-03-11

## Problem

`DataCollector` and `DetailedDataCollector` in `common.Aggregator` accumulate ALL per-item
data (every function, every comment) from every analyzed file. For text and compact output
formats, only summary metrics (averages, totals, distributions) are displayed — the per-item
data is never rendered but still consumes O(functions) memory.

On kubernetes (~25K files, ~150K functions), this means ~150K `map[string]any` items stored
in memory purely for aggregation, even though text output only shows summary numbers. This is
the single largest memory consumer in the static analysis pipeline.

## Decision

Add an `AggregationMode` type to `common` package: `AggregationModeFull` (default, zero value)
and `AggregationModeSummaryOnly`. When mode is `SummaryOnly`:

- `DataCollector.CollectFromReport` becomes a no-op (skips per-item storage).
- `DetailedDataCollector.CollectFromReports` becomes a no-op.
- `MetricsProcessor.ProcessReport` continues normally (running sums, fixed memory).

### Key design decisions

- **Zero value = Full**: `AggregationModeFull = 0` ensures backward compatibility. Existing
  code that creates aggregators without setting a mode gets full data collection.
- **Mode on DataCollector and DetailedDataCollector**: The mode is set directly on the
  collectors. `Aggregator.SetAggregationMode()` propagates to both its collectors.
- **AggregationModeAware interface**: Type assertion-based interface allows setting mode on
  any aggregator that supports it, without changing the `ResultAggregator` interface.
- **Format→mode mapping in RunAndFormat**: `text` and `compact` → `SummaryOnly`;
  `json`, `yaml`, `plot`, `binary` → `Full`.
- **No change to AnalyzeFolder**: `AnalyzeFolder` always uses the mode set on `StaticService`.
  `RunAndFormat` sets the mode before calling `AnalyzeFolder`.

## Contract

- `AggregationModeFull` (0) = current behavior, all per-item data collected.
- `AggregationModeSummaryOnly` (1) = per-item data collection disabled.
- `MetricsProcessor` always runs regardless of mode.
- `GetSortedData()` returns empty slice in `SummaryOnly` mode.
- `GetResult()` returns report with empty collection in `SummaryOnly` mode.
- All existing tests pass unchanged.
- Text/compact output shows identical summary numbers.

## Acceptance Criteria

- [x] `AggregationMode` type added (`AggregationModeFull`, `AggregationModeSummaryOnly`)
- [x] `AggregationModeAware` interface added with `SetAggregationMode(AggregationMode)`
- [x] `DataCollector.CollectFromReport` is no-op in `SummaryOnly`
- [x] `DetailedDataCollector.CollectFromReports` is no-op in `SummaryOnly`
- [x] `common.Aggregator.SetAggregationMode` propagates to collectors
- [x] `StaticService.RunAndFormat` sets mode based on format
- [x] `BenchmarkAggregatorSummaryMode` shows >50x heap reduction in SummaryOnly
- [x] `go test ./internal/analyzers/common/...` passes
- [x] `go test ./internal/analyzers/analyze/...` passes
- [x] `make lint` passes

## Benchmark Results

```
BenchmarkAggregatorSummaryMode/before-full-mode        274.4 heap-MiB
BenchmarkAggregatorSummaryMode/after-summary-only        8.3 heap-MiB
```

97% heap reduction (33x) for 50,000 reports x 10 functions = 500K items.

## Implementation

Files created:
- `internal/analyzers/analyze/aggregation_mode.go` — `AggregationMode` type, `AggregationModeAware` interface, `ResolveAggregationMode`
- `internal/analyzers/common/aggregation_mode_test.go` — unit tests for mode behavior
- `internal/analyzers/common/aggregator_bench_test.go` — before/after heap benchmark

Files modified:
- `internal/analyzers/analyze/static.go` — `AggregationMode` field, wiring in `RunAndFormat` and `initAggregators`
- `internal/analyzers/analyze/static_test.go` — `ResolveAggregationMode` and integration tests
- `internal/analyzers/common/data_collector.go` — `mode` field, `SetAggregationMode`, no-op guard
- `internal/analyzers/common/detailed_data_collector.go` — `mode` field, `SetAggregationMode`, no-op guard
- `internal/analyzers/common/aggregator.go` — `SetAggregationMode` propagation to `DataCollector`
- `internal/analyzers/complexity/aggregator.go` — `SetAggregationMode` override for `DetailedDataCollector`
- `internal/analyzers/halstead/aggregator.go` — `SetAggregationMode` override for `DetailedDataCollector`
- `internal/analyzers/comments/aggregator.go` — `SetAggregationMode` override for `DetailedDataCollector`
