# FRD: Replace manual sorted-key patterns with mapx.SortedKeys (Roadmap F2.1)

**ID**: FRD-20260303-sorted-keys
**Roadmap**: [specs/dedup/ROADMAP.md](../dedup/ROADMAP.md) — Item F2.1
**Spec**: [specs/dedup/SPEC.md](../dedup/SPEC.md) — Cluster 2: Collection Utilities

## Problem

5 files manually build a sorted key slice from a map using the pattern:
```go
keys := make([]T, 0, len(m))
for k := range m { keys = append(keys, k) }
sort.Strings(keys) // or sort.Ints(keys)
```

`mapx.SortedKeys[K cmp.Ordered, V any](m map[K]V) []K` already exists and is tested
in `pkg/alg/mapx/maps.go:83`. It is already used by `anomaly` and `quality` analyzers.

## Feature

Replace all 5 manual sorted-key patterns with `mapx.SortedKeys()`. Delete the
`sortedKeys()` helper function in `devs/metrics.go` that wraps the same pattern.

### Design Decisions

- **`mapx.SortedKeys` works for all key types**: `string` and `int` both satisfy
  `cmp.Ordered`. `map[string]bool` satisfies `map[K]V` where `V=bool` (implements `any`).
- **`sort` import removal**: Two files (`shotness/report.go`, `analyze/generic_aggregator.go`)
  use `sort` only for the key extraction. After migration, `sort` import can be removed.
  Other files keep `sort` for other uses (`sort.Slice`, `sort.Strings` on non-map data).
- **`devs/metrics.go` helper deletion**: The `sortedKeys()` function is replaced entirely
  by `mapx.SortedKeys()`. All 3 call sites updated.

### Migration Scope

| File | Lines | Map type | Sort call | Remove `sort`? |
|------|-------|----------|-----------|----------------|
| common/reporter.go | 269-274 | `map[string]bool` | `sort.Strings` | No (3 other uses) |
| common/formatter.go | 298-303 | `map[string]bool` | `sort.Strings` | No (3 other uses) |
| devs/metrics.go | 1037-1047 | `map[int]map[int]*DevTick` | `sort.Ints` | No (5 other uses) |
| shotness/report.go | 69-74 | `map[string]*nodeShotnessData` | `sort.Strings` | Yes |
| analyze/generic_aggregator.go | 89-94 | `map[int]S` | `sort.Ints` | Yes |

### Not migrated (from original roadmap)

- `common/data_collector.go` — uses `sort.Slice()` with custom comparator, not key extraction
- `imports/plot.go`, `burndown/plot.go`, `complexity/plot.go`, `halstead/plot.go` — no sorted-key pattern found
- `analyze/report_store_file.go` — iterates directory entries, not map keys

## Acceptance Criteria

- [x] `common/reporter.go` uses `mapx.SortedKeys(keySet)`
- [x] `common/formatter.go` uses `mapx.SortedKeys(keySet)`
- [x] `devs/metrics.go` uses `mapx.SortedKeys()` at 3 call sites, `sortedKeys()` helper deleted
- [x] `shotness/report.go` uses `mapx.SortedKeys(merged)`, `sort` import removed
- [x] `analyze/generic_aggregator.go` uses `mapx.SortedKeys(a.ByTick)`, `sort` import removed
- [x] All existing tests pass
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files modified:**
- `internal/analyzers/common/reporter.go` — added `mapx` import, replaced 6-line pattern with `mapx.SortedKeys(keySet)`
- `internal/analyzers/common/formatter.go` — added `mapx` import, replaced 6-line pattern with `mapx.SortedKeys(keySet)`
- `internal/analyzers/devs/metrics.go` — added `mapx` import, 3 call sites migrated, `sortedKeys()` helper deleted (11 lines)
- `internal/analyzers/shotness/report.go` — added `mapx` import, removed `sort` import, replaced 5-line pattern
- `internal/analyzers/analyze/generic_aggregator.go` — added `mapx` import, removed `sort` import, replaced 5-line pattern
