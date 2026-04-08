# FRD: Migrate 7 store_reader.go files to generic readers (Roadmap F1.2)

**ID**: FRD-20260303-store-reader-migration
**Roadmap**: [specs/dedup/ROADMAP.md](../dedup/ROADMAP.md) — Item F1.2
**Spec**: [specs/dedup/SPEC.md](../dedup/SPEC.md) — Cluster 5: Persistence & Serialization

## Problem

7 out of 10 store_reader.go files inline identical GOB-decoding boilerplate:

```go
func readXxxIfPresent(reader analyze.ReportReader, kinds []string) ([]T, error) {
    if !slices.Contains(kinds, KindXxx) {
        return nil, nil
    }
    var result []T
    iterErr := reader.Iter(KindXxx, func(raw []byte) error {
        var record T
        decErr := analyze.GobDecode(raw, &record)
        if decErr != nil { return decErr }
        result = append(result, record)
        return nil
    })
    return result, iterErr
}
```

The `analyze` package already provides `ReadRecordsIfPresent[T]` (multi-record) and
`ReadRecordIfPresent[T]` (single-record) generics that encapsulate this exact pattern.
Three analyzers (devs, anomaly, shotness) already use them successfully. The remaining
7 analyzers still inline the boilerplate.

## Feature

Replace all inline GOB-reading boilerplate in the 7 unmigrated store_reader.go files
with one-liner delegations to the existing generic readers.

### Migration Categories

**Category A — Clean replacements (5 files):**
These files have reader functions that return value types matching the generic signatures
exactly. Each becomes a single `return analyze.ReadRecordsIfPresent[T](...)` or
`return analyze.ReadRecordIfPresent[T](...)` call.

**Category B — Pointer-return adjustments (2 files):**
`burndown` and `couples` return `*T` from reader functions, but the actual semantics
never use nil — absent kinds return `&T{}` (empty struct pointer). The generic readers
return value types (zero `T` when absent), which is semantically equivalent. These
require changing `buildStoreSections` signatures from pointer to value parameters.

### Design Decisions

- **No new code in `analyze` package**: Both `ReadRecordsIfPresent[T]` and
  `ReadRecordIfPresent[T]` already exist and are tested.
- **Keep wrapper functions**: Each analyzer keeps its named `readXxxIfPresent`
  wrapper (one-liner delegation) for readability at the call site, consistent with
  the devs/anomaly/shotness pattern.
- **Pointer-to-value migration**: For burndown and couples, `buildStoreSections`
  changes from pointer parameters to value parameters. Internal functions that take
  pointers (`buildStoreSummarySection`, `buildChartFromStoreData`) receive `&value`
  at the call site.
- **Dead helpers deleted**: `couples/store_reader.go` has 4 unnecessary inner
  functions (`hasKind`, `readFileCoupling`, `readDevMatrix`, `readOwnership`) that
  are deleted.
- **`slices` import removed**: All 7 files used `slices` only for `slices.Contains`
  in the reader functions. The generic readers handle this internally.

### Migration Scope

| Analyzer | Functions migrated | Category | Notes |
|----------|-------------------|----------|-------|
| file_history | `readFileChurnIfPresent` → `ReadRecordsIfPresent[FileChurnData]` | A | Direct replacement |
| quality | `readTimeSeriesIfPresent`, `readAggregateIfPresent` | A | Multi-record + single-record |
| sentiment | `readTimeSeriesIfPresent`, `readTrendIfPresent`, `readAggregateIfPresent` | A | 1 multi-record + 2 single-record |
| imports | `readImportUsageIfPresent` → `ReadRecordsIfPresent[ImportUsageRecord]` | A | Direct replacement |
| typos | `readFileTyposIfPresent` → `ReadRecordsIfPresent[FileTypoData]` | A | Direct replacement |
| burndown | `readChartDataIfPresent`, `readMetricsIfPresent` | B | Returns change from `*T` to `T` |
| couples | `readFileCouplingIfPresent`, `readDevMatrixIfPresent`, `readOwnershipIfPresent` | B | 4 inner functions deleted |

**Already migrated** (not in scope):
- devs/store_reader.go — 6 reader functions already use generics
- anomaly/store_reader.go — 5 reader functions already use generics
- shotness/store_reader.go — 1 reader function already uses generics

## Acceptance Criteria

- [x] `file_history/store_reader.go` uses `analyze.ReadRecordsIfPresent` (1 function migrated)
- [x] `quality/store_reader.go` uses generic readers (2 functions migrated)
- [x] `sentiment/store_reader.go` uses generic readers (3 functions migrated)
- [x] `imports/store_reader.go` uses `analyze.ReadRecordsIfPresent` (1 function migrated)
- [x] `typos/store_reader.go` uses `analyze.ReadRecordsIfPresent` (1 function migrated)
- [x] `burndown/store_reader.go` uses generic readers (2 functions migrated, pointer→value)
- [x] `couples/store_reader.go` uses generic readers (3 functions migrated, 4 helpers deleted)
- [x] `slices` import removed from all 7 files
- [x] All existing tests pass unchanged
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files modified:**
- `internal/analyzers/file_history/store_reader.go` — `readFileChurnIfPresent` → one-liner, `slices` import removed
- `internal/analyzers/quality/store_reader.go` — `readTimeSeriesIfPresent` + `readAggregateIfPresent` → one-liners, `slices` import removed
- `internal/analyzers/sentiment/store_reader.go` — 3 reader functions → one-liners, `slices` import removed
- `internal/analyzers/imports/store_reader.go` — `readImportUsageIfPresent` → one-liner, `slices` import removed
- `internal/analyzers/typos/store_reader.go` — `readFileTyposIfPresent` → one-liner, `slices` import removed
- `internal/analyzers/burndown/store_reader.go` — 2 reader functions → one-liners (pointer→value), `buildStoreSections` takes value types, `slices` import removed
- `internal/analyzers/couples/store_reader.go` — 3 reader functions → one-liners, `hasKind` + `readFileCoupling` + `readDevMatrix` + `readOwnership` deleted, `buildStoreSections` takes value `StoreDevMatrix`, `slices` import removed
- `internal/analyzers/couples/store_writer_test.go` — updated 3 call sites from deleted inner functions to use `readXxxIfPresent` wrappers
