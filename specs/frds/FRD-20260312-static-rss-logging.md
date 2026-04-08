# FRD: Static Phase RSS Logging (Roadmap perf30/5.2)

**ID**: FRD-20260312-static-rss-logging
**Roadmap**: [specs/perf30/ROADMAP.md](../perf30/ROADMAP.md) — Step 5.2
**Date**: 2026-03-12

## Problem

When running static analysis on large codebases (e.g., ~/sources/kubernetes with ~25K files),
there is no visibility into memory consumption during the analysis pipeline. The existing
memory watchdog (`cmd/codefang/main.go:startMemoryWatchdog`) logs global RSS every 2 seconds,
but provides no correlation with pipeline milestones — file count, aggregator buffer sizes,
or spill activity.

Without milestone-aware logging, diagnosing OOM or excessive RSS requires manual cross-referencing
of watchdog timestamps with analysis duration, which is error-prone and impractical.

## Decision

### 1. `EstimatedStateSize() int64` on `common.Aggregator`

Add a method that estimates the in-memory state size across MetricsProcessor and
SpillableDataCollector. This gives callers a quick byte-estimate without heap profiling.

**SpillableDataCollector** gets `EstimatedBufferBytes() int64`:
```
estimatedItemBytes = 512  (same as budget.StaticAvgItemBytes)
result = len(buffer) * estimatedItemBytes
```

**MetricsProcessor** gets `EstimatedStateBytes() int64`:
```
metricsEntryBytes = 16  (string key pointer + float64 value)
countsEntryBytes  = 16  (string key pointer + int value)
result = len(metrics) * metricsEntryBytes + len(counts) * countsEntryBytes
```

**Aggregator.EstimatedStateSize()** sums both.

### 2. `StateSizer` interface in `analyze` package

```go
type StateSizer interface {
    EstimatedStateSize() int64
}
```

This allows `StaticService` to query aggregator sizes without importing `common`.

### 3. Progress callback on `StaticService`

```go
type StaticProgressEvent struct {
    FilesProcessed int64
    RSSMiB         int64
    AggregatorSize int64  // estimated bytes across all aggregators
    Phase          string // "processing" or "complete"
}

type StaticProgressFunc func(event StaticProgressEvent)
```

`StaticService.ProgressFunc` field — when non-nil, called:
- Every `ProgressInterval` files (default 1000) during `analyzeFilesParallel`
- After `buildFinalResults` returns in `AnalyzeFolder`

### 4. RSS reading

Extract `ReadRSSBytes() int64` to `pkg/meminfo/rss.go` — reads `/proc/self/statm`
and returns RSS in bytes. Reusable by both the existing watchdog and the progress callback.

Falls back to 0 on non-Linux platforms (build tag gated).

### 5. Wiring in `run.go`

`runStaticAnalyzers` sets `svc.ProgressFunc` to a closure that calls `log.Printf` with
the same `MEM` prefix format used by the watchdog, adding `files=` and `agg=` fields.

## Contract

- `EstimatedStateSize()` returns 0 when aggregator has no data.
- `EstimatedStateSize()` grows linearly with item count (within 2x of actual).
- `ProgressFunc == nil` means no progress logging (default; zero-value safe).
- `ProgressInterval == 0` defaults to `DefaultProgressInterval` (1000).
- Progress events do not block the worker pool (called under existing mutex).
- Non-Linux `ReadRSSBytes()` returns 0 (graceful degradation).

## Acceptance Criteria

- [x] `common.Aggregator.EstimatedStateSize()` returns byte estimate
- [x] `common.SpillableDataCollector.EstimatedBufferBytes()` returns byte estimate
- [x] `common.MetricsProcessor.EstimatedStateBytes()` returns byte estimate
- [x] `analyze.StateSizer` interface defined
- [x] `StaticService.ProgressFunc` and `ProgressInterval` fields added
- [x] Progress callback invoked every N files and after completion
- [x] `pkg/meminfo/rss.go` with `ReadRSSBytes()` (Linux) and stub (non-Linux)
- [x] `runStaticAnalyzers` wires progress logging
- [x] `BenchmarkAggregatorEstimatedSize` passes and shows accurate estimation
- [x] `go test ./internal/analyzers/common/...` passes
- [x] `go test ./internal/analyzers/analyze/...` passes
- [x] `go test ./pkg/meminfo/...` passes
- [x] `go test ./cmd/codefang/commands/...` passes
- [x] `make lint` passes

## Implementation

Files created:
- `pkg/meminfo/rss_linux.go` — `ReadRSSBytes()` reading `/proc/self/statm`
- `pkg/meminfo/rss_other.go` — stub returning 0 on non-Linux
- `pkg/meminfo/rss_test.go` — 2 tests

Files modified:
- `internal/analyzers/common/spillable_data_collector.go` — `estimatedItemBytes` constant, `EstimatedBufferBytes()` method
- `internal/analyzers/common/spillable_data_collector_test.go` — 3 tests for `EstimatedBufferBytes`
- `internal/analyzers/common/metrics_processor.go` — `metricsEntryBytes` constant, `EstimatedStateBytes()` method
- `internal/analyzers/common/metrics_processor_test.go` — 2 tests for `EstimatedStateBytes`
- `internal/analyzers/common/aggregator.go` — `EstimatedStateSize()` method, compile-time `StateSizer` check
- `internal/analyzers/common/aggregation_mode_test.go` — 3 tests for `EstimatedStateSize` and `StateSizer`
- `internal/analyzers/common/aggregator_bench_test.go` — `BenchmarkAggregatorEstimatedSize` (estimated 48.83 MiB vs actual 48.60 MiB)
- `internal/analyzers/analyze/analyzer.go` — `StateSizer` interface
- `internal/analyzers/analyze/static.go` — `StaticProgressEvent`, `StaticProgressFunc`, `ProgressFunc`/`ProgressInterval` fields, `emitProgress`, `resolveProgressInterval`, pipeline hooks
- `internal/analyzers/analyze/static_test.go` — 2 tests for `ProgressFunc`
- `cmd/codefang/commands/run.go` — `applyStaticProgressLogging`
