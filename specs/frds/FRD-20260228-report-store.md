# FRD: ReportStore Foundation (Phase 1)

**ID**: FRD-20260228-report-store
**Roadmap**: [specs/perf3/ROADMAP.md](../perf3/ROADMAP.md) — Steps 1.1, 1.2, 1.3
**Spec**: [specs/perf3/SPEC.md](../perf3/SPEC.md) — Step 1

## Problem

`FinalizeWithAggregators` materializes ALL analyzer reports simultaneously in memory.
On kubernetes (56K commits, 50K files), this causes OOM (10-20 GB peak).
The fix requires a chunked, spillable boundary between compute and presentation.

## Feature

Introduce `ReportStore` — an abstraction that writes and reads per-analyzer report
artifacts as streams of typed records. Reports are never monolithic blobs; they are
sequences of gob-encoded records grouped by kind.

## Interfaces

### ReportStore

Manages the lifecycle of a report store directory.

```go
type ReportStore interface {
    Begin(analyzerID string, meta ReportMeta) (ReportWriter, error)
    Open(analyzerID string) (ReportReader, error)
    AnalyzerIDs() []string
    Close() error
}
```

### ReportWriter

Appends typed records for one analyzer. Atomic: data is visible only after `Close()`.

```go
type ReportWriter interface {
    Write(kind string, record any) error
    Close() error
}
```

### ReportReader

Streams records for one analyzer, one kind at a time. Memory = one decoded record.

```go
type ReportReader interface {
    Meta() ReportMeta
    Kinds() []string
    Iter(kind string, fn func(raw []byte) error) error
    Close() error
}
```

### ReportMeta

```go
type ReportMeta struct {
    AnalyzerID string `json:"analyzer_id"`
    Version    string `json:"version"`
    SchemaHash string `json:"schema_hash"`
}
```

## File-backed Implementation (FileReportStore)

### Directory Layout

```
<store-dir>/
  manifest.json
  <analyzer-id>/
    meta.json
    <kind>.gob
```

### Behavior

- **Encoding**: `encoding/gob`. Each record gob-encoded and appended to the kind file.
- **Atomic writes**: Writer writes to `<kind>.tmp`, on `Close()` calls `fsync` then renames to `<kind>.gob`. Manifest updated last.
- **No caching**: Each `Iter` opens, reads sequentially, closes. Memory = one decoded record.
- **Manifest**: JSON file listing ordered analyzer IDs. Updated atomically on each `Begin/Close` cycle.

### Error Handling

- `Open()` on non-existent analyzer returns error.
- `Open()` on torn write (`.tmp` exists but no `.gob`) returns error.
- Double `Close()` on writer is safe (idempotent).

## Acceptance Criteria

1. Round-trip: write 3 kinds x 100 records per kind, read all back, assert byte-level equality.
2. Multiple analyzers: write for 3 analyzer IDs, `AnalyzerIDs()` returns all 3 in order.
3. Torn write detection: write without `Close()`, `Open()` returns clean error.
4. Memory regression: iterate 10K records under tight GOMEMLIMIT, no OOM.
5. `go build ./internal/analyzers/analyze/...` compiles.
6. `go test ./internal/analyzers/analyze/...` passes.
7. `make lint` clean.

## Non-Goals

- Compression (can be added later).
- Concurrent writers (one analyzer at a time by design).
- Network-backed store (file-only for now).

## Implementation

Files created/modified:
- `internal/analyzers/analyze/report_store.go` — interfaces + ReportMeta
- `internal/analyzers/analyze/report_store_file.go` — FileReportStore implementation
- `internal/analyzers/analyze/report_store_test.go` — tests
