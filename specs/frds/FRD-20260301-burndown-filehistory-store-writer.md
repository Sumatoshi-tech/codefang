# FRD: Burndown + FileHistory WriteToStore (Phase 7)

**ID**: FRD-20260301-burndown-filehistory-store-writer
**Roadmap**: [specs/perf3/ROADMAP.md](../perf3/ROADMAP.md) — Steps 7.1, 7.2, 7.3
**Spec**: [specs/perf3/SPEC.md](../perf3/SPEC.md) — Step 7
**Depends on**: [FRD-20260228-couples-store-writer](FRD-20260228-couples-store-writer.md) (Phase 6)

## Problem

### Burndown

The burndown analyzer's `FlushTick` performs 5 deep clones: `cloneSparseHistory`,
`clonePeopleHistories`, `cloneMatrix`, `cloneFileHistories`, `cloneFileOwnership`.
Then `ticksToReport` converts all sparse histories to dense (`groupSparseHistory`),
creating `DenseHistory` matrices for global, per-person, and per-file data.

For kubernetes (50K files), dense file histories are the main memory bottleneck.
However, `computeFileSurvival` only uses `FileOwnership` data — it does NOT need
dense file histories. The `DirectStoreWriter` path avoids both the deep copies and
the unnecessary dense file history materialization.

### FileHistory

The file_history analyzer's `FlushTick` copies the entire files map via `maps.Copy`.
The `DirectStoreWriter` path avoids this copy by accessing `agg.files.Current()`
directly after `Collect()` merges spilled chunks.

## Feature

### 7.1 Burndown DirectStoreWriter

1. **`WriteToStoreFromAggregator`** — Accesses aggregator's sparse histories directly.
   Converts only the global sparse history to dense (bounded: ~numSamples x numBands).
   Pre-computes all metrics without materializing dense file histories.
2. **Store kinds**:
   - `"chart_data"`: Single `BurndownChartData` record (global DenseHistory + metadata).
   - `"metrics"`: Single pre-computed `ComputedMetrics` record.
3. **Memory optimization**: Developer survival computed by converting per-person sparse
   to dense one at a time. File survival computed from ownership map only (no dense).

### 7.2 FileHistory DirectStoreWriter

1. **`WriteToStoreFromAggregator`** — Accesses aggregator's files directly via
   `Current()`. Filters by last commit tree. Streams per-file churn records.
2. **Store kinds**:
   - `"file_churn"`: Per-file `FileChurnData` records.
   - `"summary"`: Single `AggregateData` record.

### 7.3 Store-Based Section Renderers

1. **Burndown store renderer** — Reads `chart_data` and `metrics` kinds, builds
   burndown chart + summary section without materializing full Report.
2. **FileHistory store renderer** — Reads `file_churn` kind, sorts by commit count,
   takes top 20, builds bar chart.
3. Both register via `analyze.RegisterStorePlotSections`.

## Store Record Types

### Burndown Kind: `"chart_data"`

Single record, gob-encoded:
```go
type ChartData struct {
    GlobalHistory DenseHistory
    Sampling      int
    Granularity   int
    TickSize      time.Duration
    EndTime       time.Time
}
```

### Burndown Kind: `"metrics"`

Single record, gob-encoded `ComputedMetrics`:
```go
type ComputedMetrics struct {
    Aggregate         AggregateData
    GlobalSurvival    []SurvivalData
    FileSurvival      []FileSurvivalData
    DeveloperSurvival []DeveloperSurvivalData
    Interaction       []InteractionData
}
```

### FileHistory Kind: `"file_churn"`

Multiple records, each a gob-encoded `FileChurnData`:
```go
type FileChurnData struct {
    Path             string
    CommitCount      int
    ContributorCount int
    TotalAdded       int
    TotalRemoved     int
    TotalChanged     int
    ChurnScore       float64
}
```

### FileHistory Kind: `"summary"`

Single record, gob-encoded `AggregateData`:
```go
type AggregateData struct {
    TotalFiles             int
    TotalCommits           int
    TotalContributors      int
    AvgCommitsPerFile      float64
    AvgContributorsPerFile float64
    HighChurnFiles         int
}
```

## Behavior

### Burndown WriteToStoreFromAggregator Flow

1. Cast `agg` to `*burndown.Aggregator`.
2. Access `globalHistory` (sparse) from aggregator.
3. Convert global sparse to dense via `groupSparseHistory` (bounded).
4. For each person: convert sparse to dense, compute `DeveloperSurvivalData`, discard dense.
5. Compute `InteractionData` from `matrix`.
6. Compute `FileSurvivalData` from `fileOwnership` (no dense conversion).
7. Compute `AggregateData` from global dense.
8. Write `"chart_data"` record.
9. Write `"metrics"` record.

### FileHistory WriteToStoreFromAggregator Flow

1. Cast `agg` to `*file_history.Aggregator`.
2. Access `files.Current()` and `lastCommitHash` from aggregator.
3. Filter files by last commit tree (reuse `filterFilesByLastCommit`).
4. For each file: compute `FileChurnData`. Write as `"file_churn"` record.
5. Compute `AggregateData`. Write as `"summary"` record.

## Acceptance Criteria

1. Burndown `WriteToStoreFromAggregator` writes `chart_data` and `metrics` kinds.
2. FileHistory `WriteToStoreFromAggregator` writes `file_churn` and `summary` kinds.
3. Both implement `analyze.DirectStoreWriter`.
4. Burndown store renderer builds chart + summary sections from store data.
5. FileHistory store renderer builds bar chart from store data.
6. Round-trip tests: write -> read -> verify sections generated.
7. Equivalence tests: store path produces same metrics as legacy path.
8. `go test ./internal/analyzers/burndown/...` passes.
9. `go test ./internal/analyzers/file_history/...` passes.
10. `make lint` clean.

## Non-Goals

- No CLI flags for burndown/file_history store parameters in this phase.
- No changes to text/JSON/YAML serialization paths.
- No changes to the couples store writer.

## Implementation

### Files Created
- `internal/analyzers/burndown/store_writer.go` — `WriteToStoreFromAggregator`, `ChartData`, `buildChartData`, `computeMetricsFromAggregator`, `computeDevSurvivalStreaming`, `computeInteractionFromSparse`, `computeFileSurvivalFromOwnership`
- `internal/analyzers/burndown/store_reader.go` — `GenerateStoreSections`, `readChartDataIfPresent`, `readMetricsIfPresent`, `buildStoreSections`, `buildStoreSummarySection`, `buildChartFromStoreData`
- `internal/analyzers/burndown/store_writer_test.go` — 5 tests (RoundTrip, WrongType, EmptyAggregator, MetricsEquivalence, GenerateStoreSections_RoundTrip)
- `internal/analyzers/file_history/store_writer.go` — `WriteToStoreFromAggregator`, `computeFileChurnFromFiles`, `computeAggregateFromFiles`, `writeFileChurn`
- `internal/analyzers/file_history/store_reader.go` — `GenerateStoreSections`, `readFileChurnIfPresent`, `buildStoreSections`, `buildBarChartFromChurnData`
- `internal/analyzers/file_history/store_writer_test.go` — 5 tests (RoundTrip, WrongType, EmptyAggregator, EquivalenceLegacy, GenerateStoreSections_RoundTrip)

### Files Modified
- `internal/analyzers/burndown/plot.go` — added `RegisterStorePlotSections` call
- `internal/analyzers/file_history/plot.go` — added `RegisterStorePlotSections` call
