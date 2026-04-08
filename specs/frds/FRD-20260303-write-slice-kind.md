# FRD: Extract shared WriteSliceKind[T] to analyze package (Roadmap F1.1)

**ID**: FRD-20260303-write-slice-kind
**Roadmap**: [specs/dedup/ROADMAP.md](../dedup/ROADMAP.md) — Item F1.1
**Spec**: [specs/dedup/SPEC.md](../dedup/SPEC.md) — Cluster 5: Persistence & Serialization

## Problem

8 out of 10 store_writer.go files inline an identical write-loop pattern:

```go
for i := range records {
    writeErr := w.Write(kindConstant, records[i])
    if writeErr != nil {
        return fmt.Errorf("write %s: %w", kindConstant, writeErr)
    }
}
```

Only `devs/store_writer.go` has extracted this into a package-private generic
`writeSliceKind[T any]`. The function cannot be reused by other analyzers because
it is unexported and lives in the `devs` package.

## Feature

Move the generic function to `internal/analyzers/analyze/record_writer.go` as an
exported `WriteSliceKind[T any]` — the write-side counterpart to the existing
`ReadRecordsIfPresent[T]` / `ReadRecordIfPresent[T]` in `record_reader.go`.

### API

```go
// WriteSliceKind writes each element of a typed slice as a separate record
// under the given kind. Returns nil for empty or nil slices.
func WriteSliceKind[T any](w ReportWriter, kind string, records []T) error
```

### Design Decisions

- **Placed in `analyze` package**: Operates on `ReportWriter` defined in the same
  package. Symmetric with `ReadRecordsIfPresent[T]` in `record_reader.go`.
- **Error wrapping inside the function**: Matches the devs implementation — wraps
  with `fmt.Errorf("write %s: %w", kind, writeErr)` so callers don't need to.
- **Nil/empty slice returns nil**: No-op for zero-length input — safe to call
  unconditionally.

### Migration Scope

| Analyzer | Loops replaced | Notes |
|----------|---------------|-------|
| devs | 5 (remove local `writeSliceKind`) | Direct replacement |
| anomaly | 4 (TimeSeries, Anomalies, ExternalAnomaly, ExternalSummary) | Direct replacement |
| quality | 1 (TimeSeries) | Direct replacement |
| sentiment | 1 (TimeSeries) | Direct replacement |
| typos | 1 (FileTypos) | Direct replacement |
| file_history | 1 (FileChurn via `writeFileChurn` helper) | Replace helper with `WriteSliceKind`, remove helper |
| couples | 1 (writeFileCoupling — truncated slice) | Can use after `pairs[:limit]` slicing |

**Not migrated** (construct records inline from parallel arrays):
- imports/store_writer.go — builds `ImportUsageRecord` from `labels[i]` + `data[i]`
- shotness/store_writer.go — builds `NodeStoreRecord` from `nodes[i]` + `counters[i]`
- couples/writeOwnership — builds `FileOwnershipData` from multiple sources
- burndown/store_writer.go — no slice loops (only single-record writes)

## Acceptance Criteria

- [x] `WriteSliceKind[T any](w ReportWriter, kind string, records []T) error` exists in `analyze/record_writer.go`
- [x] Unit tests in `analyze/record_writer_test.go` cover: nil slice, empty slice, single record, multiple records, write error propagation
- [x] `devs/store_writer.go` uses `analyze.WriteSliceKind`, local `writeSliceKind` removed
- [x] `anomaly/store_writer.go` uses `analyze.WriteSliceKind` (4 loops replaced)
- [x] `quality/store_writer.go` uses `analyze.WriteSliceKind` (1 loop replaced)
- [x] `sentiment/store_writer.go` uses `analyze.WriteSliceKind` (1 loop replaced)
- [x] `typos/store_writer.go` uses `analyze.WriteSliceKind` (1 loop replaced)
- [x] `file_history/store_writer.go` uses `analyze.WriteSliceKind`, `writeFileChurn` removed
- [x] `couples/store_writer.go` writeFileCoupling uses `analyze.WriteSliceKind` after slicing
- [x] All existing tests pass unchanged (24 analyzer packages pass)
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files created:**
- `internal/analyzers/analyze/record_writer.go` — `WriteSliceKind[T]`
- `internal/analyzers/analyze/record_writer_test.go` — 5 tests

**Files modified:**
- `internal/analyzers/devs/store_writer.go` — removed local `writeSliceKind[T]`, 5 calls replaced
- `internal/analyzers/anomaly/store_writer.go` — 4 inline loops replaced
- `internal/analyzers/quality/store_writer.go` — 1 inline loop replaced
- `internal/analyzers/sentiment/store_writer.go` — 1 inline loop replaced
- `internal/analyzers/typos/store_writer.go` — 1 inline loop replaced
- `internal/analyzers/file_history/store_writer.go` — `writeFileChurn` helper removed, 1 call replaced
- `internal/analyzers/couples/store_writer.go` — `writeFileCoupling` loop replaced with `WriteSliceKind(w, kind, pairs[:limit])`
