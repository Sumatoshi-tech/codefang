# FRD: Create generic record reader for ReportReader (Roadmap F0.10)

**ID**: FRD-20260302-record-reader
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F0.10
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster: Store Utilities

## Problem

12 store reader functions across 3 analyzers repeat the same "check kind → iterate → decode → append" pattern against `analyze.ReportReader`:

| Analyzer | Function Count | Multi-record | Single-record |
|----------|---------------|--------------|---------------|
| `anomaly/store_reader.go` | 5 | 4 (`ReadTimeSeriesIfPresent`, `ReadAnomaliesIfPresent`, `ReadExternalAnomaliesIfPresent`, `ReadExternalSummariesIfPresent`) | 1 (`ReadAggregateIfPresent`) |
| `shotness/store_reader.go` | 1 | 1 (`readNodeDataIfPresent`) | 0 |
| `devs/store_reader.go` | 6 | 5 (`readDevelopersIfPresent`, `readLanguagesIfPresent`, `readBusFactorIfPresent`, `readActivityIfPresent`, `readChurnIfPresent`) | 1 (`readAggregateIfPresent`) |

**Multi-record pattern** (10 functions):
```go
func readXIfPresent(reader analyze.ReportReader, kinds []string) ([]T, error) {
    if !slices.Contains(kinds, KindX) { return nil, nil }
    var result []T
    err := reader.Iter(KindX, func(raw []byte) error {
        var record T
        if err := analyze.GobDecode(raw, &record); err != nil { return err }
        result = append(result, record)
        return nil
    })
    return result, err
}
```

**Single-record pattern** (2 functions):
```go
func readAggregateIfPresent(reader analyze.ReportReader, kinds []string) (T, error) {
    var result T
    if !slices.Contains(kinds, KindX) { return result, nil }
    err := reader.Iter(KindX, func(raw []byte) error {
        return analyze.GobDecode(raw, &result)
    })
    return result, err
}
```

## Feature

Create two generic functions in `internal/analyzers/analyze/record_reader.go`.

**Note:** The roadmap originally suggested `internal/analyzers/common/spillstore/reader.go`, but the actual pattern operates on `analyze.ReportReader` + `analyze.GobDecode`, not `SpillStore`. Placing in `analyze/` avoids an unnecessary cross-package dependency.

### record_reader.go — Generic Record Readers

| Export | Signature | Behavior |
|--------|-----------|----------|
| `ReadRecordsIfPresent[T any]` | `func(reader ReportReader, kinds []string, kind string) ([]T, error)` | If `kind` is in `kinds`, iterates and gob-decodes all records into `[]T`. Returns `(nil, nil)` if kind absent. |
| `ReadRecordIfPresent[T any]` | `func(reader ReportReader, kinds []string, kind string) (T, error)` | If `kind` is in `kinds`, reads the last record of that kind into `T`. Returns `(zero, nil)` if kind absent. |

### Design Decisions

- **Placed in `analyze` package**: Operates on `ReportReader` and uses `GobDecode`, both defined in `analyze`. No new imports needed.
- **`kinds` parameter**: All callers already have `kinds := reader.Kinds()` and pass it through. Accepting the pre-fetched slice avoids repeated `reader.Kinds()` calls.
- **No decode function parameter**: All 12 callers use `analyze.GobDecode`. The generic internalizes this, reducing API surface.
- **`ReadRecordIfPresent` returns last**: For single-record kinds (aggregates), `Iter` yields exactly one record. If multiple exist, last wins — matching existing behavior.

## Acceptance Criteria

- [x] `internal/analyzers/analyze/record_reader.go` exports: `ReadRecordsIfPresent[T any]`, `ReadRecordIfPresent[T any]`
- [x] `internal/analyzers/analyze/record_reader_test.go` covers: kind absent, empty records, single record, multiple records, decode error, single-record variant, last-record-wins (8 tests)
- [x] All tests pass, 100% statement coverage
- [x] `go vet` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files created:**
- `internal/analyzers/analyze/record_reader.go` — `ReadRecordsIfPresent[T]`, `ReadRecordIfPresent[T]`
- `internal/analyzers/analyze/record_reader_test.go` — 8 tests

**Files modified (F1.10 wiring):**
- `internal/analyzers/anomaly/store_reader.go` — 5 reader functions now delegate to generics; removed `slices` import
- `internal/analyzers/shotness/store_reader.go` — 1 reader function now delegates to generic; removed `slices` import
- `internal/analyzers/devs/store_reader.go` — 6 reader functions now delegate to generics; removed `slices` import
