# FRD: EstimateMapSize in pkg/alg/mapx (Roadmap 8.1)

**ID**: FRD-20260310-estimate-map-size
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 8.1
**Date**: 2026-03-10

## Problem

Five analyzer `EstimatedStateSize()` methods (and `sizeState` functions) repeat the
same `int64(len(m)) * entryBytes` expression for maps. The pattern is simple but
error-prone: the `int64` cast is easy to forget, and the expression does not
communicate intent.

Call sites:
- `internal/analyzers/burndown/aggregator.go` — `estimateSparseHistorySize` (inner maps), matrix rows
- `internal/analyzers/couples/aggregator.go` — 2 map-based size estimates (files lanes, people files)
- `internal/analyzers/file_history/aggregator.go` — People map iteration
- `internal/analyzers/quality/analyzer.go` — commitQuality outer map
- `internal/analyzers/devs/analyzer.go` — Languages map per commit

## Decision

Add a single generic function to `pkg/alg/mapx`:

```go
// EstimateMapSize estimates memory usage of m assuming entryBytes per entry.
func EstimateMapSize[K comparable, V any](m map[K]V, entryBytes int) int64
```

Replace `int64(len(m)) * constant` with `mapx.EstimateMapSize(m, constant)` at
all applicable map-based call sites. Slice-based expressions (e.g., quality's
per-field slice sizes) remain unchanged — they operate on slices, not maps.

## Contract

- Returns `int64(len(m)) * int64(entryBytes)`.
- Nil map returns 0 (len of nil map is 0).
- Zero entryBytes returns 0.
- No allocation.

## Scope

### Files modified

| File | Change |
|------|--------|
| `pkg/alg/mapx/maps.go` | Add `EstimateMapSize` |
| `pkg/alg/mapx/maps_test.go` | Add tests |
| `internal/analyzers/burndown/aggregator.go` | `estimateSparseHistorySize` uses `mapx.EstimateMapSize` |
| `internal/analyzers/couples/aggregator.go` | 2 map-based lines use `mapx.EstimateMapSize` |
| `internal/analyzers/file_history/aggregator.go` | People map line uses `mapx.EstimateMapSize` |
| `internal/analyzers/quality/analyzer.go` | commitQuality outer map uses `mapx.EstimateMapSize` |
| `internal/analyzers/devs/analyzer.go` | Languages map uses `mapx.EstimateMapSize` |

### Out of scope

- Slice-based size estimates (quality's per-field slices, file_history's Hashes slice).
- Changing entryBytes constants (they remain per-package).
- Adding `EstimateSliceSize` — can be added later if needed.

## Acceptance Criteria

- [x] `EstimateMapSize` added to `maps.go` with tests
- [x] All 5 `EstimatedStateSize`/`sizeState` bodies use it for map-based estimates
- [x] `go test ./pkg/alg/mapx/...` passes
- [x] `go test ./internal/analyzers/{burndown,couples,file_history,quality,devs}/...` passes
- [x] `make lint` passes — 0 issues, no dead code

## Implementation

### Files Created

None — all changes to existing files.

### Files Modified

| File | Change |
|------|--------|
| `pkg/alg/mapx/maps.go` | Added `EstimateMapSize[K comparable, V any]` generic function |
| `pkg/alg/mapx/maps_test.go` | Added 5 table-driven tests for `EstimateMapSize` |
| `internal/analyzers/burndown/aggregator.go` | `estimateSparseHistorySize` inner loop + matrix row use `mapx.EstimateMapSize` |
| `internal/analyzers/couples/aggregator.go` | 2 map-based lines (file lanes, people files) use `mapx.EstimateMapSize` |
| `internal/analyzers/file_history/aggregator.go` | People map line uses `mapx.EstimateMapSize` |
| `internal/analyzers/quality/analyzer.go` | `commitQuality` outer map uses `mapx.EstimateMapSize` |
| `internal/analyzers/devs/analyzer.go` | Languages map uses `mapx.EstimateMapSize` |
| `specs/ref/ROADMAP.md` | Marked 8.1 done, added FRD link |
