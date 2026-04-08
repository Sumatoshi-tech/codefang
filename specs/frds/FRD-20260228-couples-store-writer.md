# FRD: Couples WriteToStore + Bounded Renderer (Phase 6)

**ID**: FRD-20260228-couples-store-writer
**Roadmap**: [specs/perf3/ROADMAP.md](../perf3/ROADMAP.md) — Steps 6.1, 6.2, 6.3
**Spec**: [specs/perf3/SPEC.md](../perf3/SPEC.md) — Step 6
**Depends on**: [FRD-20260228-runner-integration](FRD-20260228-runner-integration.md) (Phase 2)

## Problem

The couples analyzer produces an O(N²) file coupling matrix during `ticksToReport` →
`buildReport` → `computeFilesMatrix`. For kubernetes (50K files), this dense matrix
consumes gigabytes. Additionally, `FlushAllTicks` → `FlushTick` → `copyFilesMap` deep-copies
the entire sparse coupling map, doubling peak memory.

The store path (Phase 2) already calls `FlushAllTicks` before passing ticks to `WriteToStore`.
To eliminate the deep copy, we need a new interface — `DirectStoreWriter` — that receives
the aggregator directly after `Collect()`, skipping `FlushAllTicks` entirely.

## Feature

### 6.1 DirectStoreWriter Interface + Couples WriteToStore

1. **`DirectStoreWriter` interface** — New optional interface in `analyze` package. Analyzers
   implementing it write directly from aggregator state, bypassing `FlushAllTicks` deep copy.
2. **Runner integration** — `finalizeAnalyzerToStore` checks for `DirectStoreWriter` first,
   then `StoreWriter`, then legacy. For `DirectStoreWriter`, calls `Collect()` but skips
   `FlushAllTicks`.
3. **Couples `WriteToStoreFromAggregator`** — Accesses `Aggregator.files.Current()` and
   `people` directly. Streams pre-computed bounded data:
   - `"file_coupling"` kind: Top-K `FileCouplingData` records (computed from sparse map, no dense matrix)
   - `"dev_matrix"` kind: Single record with bounded top-N dev coupling matrix + names
   - `"ownership"` kind: `FileOwnershipData` records per file (contributor counts via HLL)
   - `"aggregate"` kind: Single `AggregateData` record

### 6.2 Couples Store-Based Section Renderer

1. **`StoreSectionRendererFunc` type** — New function type in `analyze` package:
   `func(reader ReportReader) ([]plotpage.Section, error)`.
2. **`RegisterStorePlotSections` / `StorePlotSectionsFor`** — Parallel registration for
   store-aware renderers, checked before legacy renderers.
3. **`renderOneAnalyzer` update** — Tries store section renderer first; falls back to legacy.
4. **Couples store renderer** — Reads specific kinds, builds charts without materializing
   full Report.

### 6.3 Couples Store Tests

1. **Round-trip test** — Write via `DirectStoreWriter` → read via store renderer → verify sections.
2. **Equivalence test** — Compare top-K file coupling and dev matrix from store path vs. legacy path.
3. **Memory bound test** — Store path uses O(K) memory, not O(N²).

## Interface Definitions

### DirectStoreWriter

```go
// DirectStoreWriter is optionally implemented by HistoryAnalyzers that can write
// directly from their aggregator state to a ReportWriter.
// Unlike StoreWriter, this interface receives the aggregator after Collect()
// without FlushAllTicks, avoiding the deep copy overhead.
type DirectStoreWriter interface {
    WriteToStoreFromAggregator(ctx context.Context, agg Aggregator, w ReportWriter) error
}
```

### StoreSectionRendererFunc

```go
// StoreSectionRendererFunc renders plot sections from a ReportReader.
type StoreSectionRendererFunc func(reader ReportReader) ([]plotpage.Section, error)
```

## Store Record Types

### Kind: `"file_coupling"`

Multiple records, each a gob-encoded `FileCouplingData`:
```go
type FileCouplingData struct {
    File1     string  `json:"file1"`
    File2     string  `json:"file2"`
    CoChanges int64   `json:"co_changes"`
    Strength  float64 `json:"coupling_strength"`
}
```

### Kind: `"dev_matrix"`

Single record, gob-encoded:
```go
type StoreDevMatrix struct {
    Names  []string        `json:"names"`
    Matrix []map[int]int64 `json:"matrix"`
}
```

### Kind: `"ownership"`

Multiple records, each a gob-encoded `FileOwnershipData`:
```go
type FileOwnershipData struct {
    File         string `json:"file"`
    Lines        int    `json:"lines"`
    Contributors int    `json:"contributors"`
}
```

### Kind: `"aggregate"`

Single record, gob-encoded `AggregateData`:
```go
type AggregateData struct {
    TotalFiles          int     `json:"total_files"`
    TotalDevelopers     int     `json:"total_developers"`
    TotalCoChanges      int64   `json:"total_co_changes"`
    AvgCouplingStrength float64 `json:"avg_coupling_strength"`
    HighlyCoupledPairs  int     `json:"highly_coupled_pairs"`
}
```

## Configuration

- **`TopKPerFile`** — Maximum file coupling pairs to emit (default: 100). Set on `HistoryAnalyzer`.
- **`MinEdgeWeight`** — Minimum co-change count to include an edge (default: 2). Set on `HistoryAnalyzer`.

These are configured via `Configure(facts map[string]any)` with keys
`"CouplesTopKPerFile"` and `"CouplesMinWeight"`.

## Behavior

### WriteToStoreFromAggregator Flow

1. Cast `agg` to `*couples.Aggregator`.
2. **File coupling** — Iterate `agg.files.Current()`. For each file pair (i < j), compute
   coupling strength from self-change counts. Collect pairs with count ≥ `MinEdgeWeight`.
   Sort by co-changes descending. Emit top-K as `"file_coupling"` records.
3. **Developer matrix** — Build bounded people matrix from `agg.people` (top-N devs by activity).
   Emit as single `"dev_matrix"` record.
4. **Ownership** — Count unique contributors per file using HLL sketches from `agg.people`.
   Emit as `"ownership"` records.
5. **Aggregate** — Compute summary stats from file coupling data. Emit as `"aggregate"` record.

### Runner Changes

```go
func (runner *Runner) finalizeAnalyzerToStore(...) error {
    collectErr := agg.Collect()
    // ...
    meta := analyze.ReportMeta{AnalyzerID: a.Flag()}
    w, beginErr := store.Begin(a.Flag(), meta)
    // ...
    var writeErr error
    if dsw, ok := a.(analyze.DirectStoreWriter); ok {
        // Skip FlushAllTicks — no deep copy.
        writeErr = dsw.WriteToStoreFromAggregator(ctx, agg, w)
    } else {
        ticks, flushErr := agg.FlushAllTicks()
        // ...existing StoreWriter or legacy path...
    }
    // ...
}
```

### Store-Based Rendering

1. `renderOneAnalyzer` calls `analyze.StorePlotSectionsFor(id)`.
2. If non-nil: opens `store.Open(id)` → passes reader to store section renderer → returns sections.
3. If nil: falls back to `readLegacyReport` → `sectionFn(report)` (existing path).

## Acceptance Criteria

1. `DirectStoreWriter` interface added to `internal/analyzers/analyze/history.go`.
2. `finalizeAnalyzerToStore` handles `DirectStoreWriter` — no `FlushAllTicks` call.
3. Couples `WriteToStoreFromAggregator` streams four kinds: `file_coupling`, `dev_matrix`, `ownership`, `aggregate`.
4. Couples store section renderer reads store and builds all three chart sections.
5. `renderOneAnalyzer` tries store renderer first, falls back to legacy.
6. Round-trip test: write → read → verify all sections generated.
7. Equivalence test: top-K from store matches top-K from legacy path.
8. `go test ./internal/analyzers/couples/...` passes.
9. `go test ./internal/framework/...` passes.
10. `go test ./cmd/codefang/commands/...` passes.
11. `make lint` clean.

## Non-Goals

- No CLI flags for TopK/MinWeight in this phase — use configurable defaults via Configure().
- No changes to text/JSON/YAML serialization paths.
- No changes to other analyzers' store paths.

## Implementation

### Files Created
- `internal/analyzers/couples/store_writer.go` — `WriteToStoreFromAggregator`, record types, bounded computation
- `internal/analyzers/couples/store_reader.go` — Store-based section renderer
- `internal/analyzers/couples/store_writer_test.go` — Round-trip + equivalence tests

### Files Modified
- `internal/analyzers/analyze/history.go` — `DirectStoreWriter` interface
- `internal/analyzers/analyze/conversion.go` — `StoreSectionRendererFunc`, `RegisterStorePlotSections`, `StorePlotSectionsFor`
- `internal/framework/runner.go` — `finalizeAnalyzerToStore` DirectStoreWriter branch
- `internal/framework/runner_test.go` — DirectStoreWriter test
- `cmd/codefang/commands/render.go` — `renderOneAnalyzer` store renderer fallback
- `internal/analyzers/couples/plot.go` — `RegisterPlotSections` registers store renderer
- `.golangci.yml` — `DirectStoreWriter` iface exclusion
