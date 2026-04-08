# FRD: Spillable DataCollector (Roadmap perf30/2.2)

**ID**: FRD-20260311-spillable-data-collector
**Roadmap**: [specs/perf30/ROADMAP.md](../perf30/ROADMAP.md) — Step 2.2
**Date**: 2026-03-11

## Problem

In `Full` aggregation mode (json/yaml/plot), `DataCollector.collectedData` grows without
bound. On kubernetes (~25K files, ~150K functions), this accumulates ~150K `map[string]any`
items in a single `map[string]any`. Each item has ~7 keys averaging ~500 bytes, totaling
~75-100 MiB of heap for the data collector alone across all analyzers.

Step 2.1 eliminated this for text/compact output, but json/yaml/plot still need per-item
data and currently hold everything in memory.

## Decision

Create `SpillableDataCollector` in `internal/analyzers/common/` that replaces `DataCollector`
in `common.Aggregator`. It provides the same public API but adds transparent spill-to-disk
when the in-memory item count exceeds a configurable threshold.

### Key design decisions

- **Replaces DataCollector in Aggregator**: `Aggregator.dataCollector` changes from
  `*DataCollector` to `*SpillableDataCollector`. The original `DataCollector` was deleted
  as dead code after the switch.
- **Gob encoding for spill files**: Spill files use `encoding/gob` with registered types
  (`map[string]any`, `[]map[string]any`, `[]any`). Gob preserves exact Go types through
  serialization (unlike JSON which converts `int` to `float64`). Types are registered
  lazily via `sync.Once` in the constructor to avoid `init()` functions.
- **Threshold-based spilling**: When `len(buffer) >= spillThreshold`, the current buffer is
  serialized to a temp gob file and cleared. Default threshold: 10,000 items.
- **Merge-sort on read**: `GetSortedData()` reads all spill files plus the in-memory buffer,
  deduplicates by identifier key (last-write-wins, same as DataCollector), sorts, and returns.
  Spill files are cleaned up after read.
- **Zero threshold disables spilling**: When `spillThreshold == 0`, the collector behaves
  identically to the old DataCollector (no temp files, no overhead).
- **SummaryOnly mode**: In `AggregationModeSummaryOnly`, `CollectFromReport` remains a no-op
  (inherited from Step 2.1). No spill files are ever created.
- **Graceful spill failure**: If a spill write fails, the threshold is disabled (set to 0)
  to prevent repeated failure attempts. Data stays in memory.

## Contract

- `SpillableDataCollector` presents the same public API as the old `DataCollector`:
  `CollectFromReport`, `GetSortedData`, `GetDataCount`, `GetCollectionKey`,
  `GetIdentifierKey`, `SetAggregationMode`.
- `GetSortedData()` returns identical results to the old `DataCollector.GetSortedData()` for
  the same input sequence (sorted by identifier key, last-write-wins dedup, exact type fidelity).
- Temp files are created under `os.TempDir()` with prefix `codefang-spill-dc-`.
- Temp files are cleaned up after `GetSortedData()` completes or when `Cleanup()` is called.
- `Aggregator.SetSpillThreshold(n int)` configures the threshold; default is 10,000.

## Acceptance Criteria

- [x] `SpillableDataCollector` created in `internal/analyzers/common/`
- [x] `spillThreshold` configurable (default 10K items)
- [x] Temp files cleaned up on `GetSortedData()` completion
- [x] `common.Aggregator` uses `SpillableDataCollector`
- [x] `BenchmarkSpillableCollector` shows >4x peak heap reduction vs plain DataCollector
- [x] `GetSortedData()` returns identical sorted output (correctness)
- [x] `go test ./internal/analyzers/common/...` passes
- [x] `go test ./internal/analyzers/analyze/...` passes
- [x] `make lint` passes

## Benchmark Results

```
BenchmarkSpillableCollector/no_spill (baseline)   52.0 MiB peak heap
BenchmarkSpillableCollector/spillable (threshold=5000)   7.0 MiB peak heap
Reduction: 7.4x (>4x target met)
```

## Implementation

**Created:**
- `internal/analyzers/common/spillable_data_collector.go` — `SpillableDataCollector` type with gob-based spill-to-disk
- `internal/analyzers/common/spillable_data_collector_test.go` — 11 unit tests (FRD traceability)
- `internal/analyzers/common/spillable_bench_test.go` — before/after heap benchmark

**Modified:**
- `internal/analyzers/common/aggregator.go` — `dataCollector` field changed to `*SpillableDataCollector`, added `SetSpillThreshold`
- `internal/analyzers/common/aggregation_mode_test.go` — updated tests for SpillableDataCollector
- `internal/analyzers/common/detailed_data_collector.go` — godoc reference update
- `internal/analyzers/analyze/aggregation_mode.go` — godoc reference update

**Deleted:**
- `internal/analyzers/common/data_collector.go` — dead code after Aggregator switched
- `internal/analyzers/common/data_collector_test.go` — tests for deleted type
