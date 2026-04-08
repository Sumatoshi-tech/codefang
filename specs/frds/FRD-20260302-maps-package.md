# FRD: Create pkg/alg/mapx with Generic Map and Slice Operations (Roadmap F0.3)

**ID**: FRD-20260302-maps-package
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F0.3
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster: Clone/Merge/Sorted-Keys

## Problem

Map clone, merge, and sorted-key extraction are duplicated across 5+ locations:

1. **`burndown/checkpoint.go:159-224`** — `cloneSparseHistory`, `clonePeopleHistories`, `cloneStringMap`, `cloneRenamesReverse`, `clonePathIDMap` (5 clone functions, 66 lines)
2. **`couples/aggregator.go:842-873`** — `copyFilesMap`, `copyPeopleSlice`, `copyIntSlice` (3 copy functions, 32 lines)
3. **`burndown/shard_spill.go:36-83`** — `mergeSparseHistory`, `mergeMatrixInto`, `mergePeopleHistories` (3 merge functions, 48 lines)
4. **`couples/aggregator.go:310-332,568-580`** — `mergeChunkIntoResult`, `mergeTickFiles` (2 merge functions, 36 lines)
5. **`anomaly/metrics.go:363-373`** — `sortedTickKeys` (10 lines)
6. **`quality/metrics.go:315-324`** — `sortedTickKeys` (identical, 10 lines)
7. **`shotness/report.go:69-79`** — inline sorted-keys pattern (10 lines)

~212 lines of duplicated clone/merge/sorted-key boilerplate.

## Feature

Create `pkg/alg/mapx` as the canonical package for generic map and slice operations.

> **Note:** Package renamed from `maps` to `mapx` to avoid conflict with Go stdlib `maps` package (Go 1.21+), which would trigger `revive` var-naming lint errors.

### maps.go — Generic Map Operations

| Function | Signature | Behavior |
|----------|-----------|----------|
| `Clone` | `Clone[K comparable, V any](m map[K]V) map[K]V` | Shallow clone; returns nil for nil input |
| `CloneFunc` | `CloneFunc[K comparable, V any](m map[K]V, cloneV func(V) V) map[K]V` | Deep clone with custom value cloner |
| `CloneNested` | `CloneNested[K1, K2 comparable, V any](m map[K1]map[K2]V) map[K1]map[K2]V` | Deep clone of two-level nested map |
| `MergeAdditive` | `MergeAdditive[K comparable, V Numeric](dst, src map[K]V)` | Additive merge: `dst[k] += src[k]` |
| `SortedKeys` | `SortedKeys[K cmp.Ordered, V any](m map[K]V) []K` | Extract keys, return sorted |

> **YAGNI:** `MergeFunc` was designed but removed — no production caller exists. Domain-specific merge functions (e.g., `mergeChunkIntoResult`, `mergeTickFiles`) have filtering/nesting semantics that don't reduce to a simple `func(V, V) V`.

### slices.go — Generic Slice Operations

| Function | Signature | Behavior |
|----------|-----------|----------|
| `CloneSlice` | `CloneSlice[T any](s []T) []T` | Shallow clone; returns nil for nil input |
| `Unique` | `Unique[T comparable](s []T) []T` | Deduplicate preserving order |

### Type Constraint

```go
type Numeric interface {
    ~int | ~int8 | ~int16 | ~int32 | ~int64 |
    ~uint | ~uint8 | ~uint16 | ~uint32 | ~uint64 |
    ~float32 | ~float64
}
```

## Acceptance Criteria

- [x] `pkg/alg/mapx/maps.go` exports all map functions above
- [x] `pkg/alg/mapx/slices.go` exports all slice functions above
- [x] `pkg/alg/mapx/maps_test.go` covers: nil maps, empty maps, nested clone independence, additive merge correctness, sorted output
- [x] `pkg/alg/mapx/slices_test.go` covers: nil slices, empty slices, clone independence, dedup preserves order
- [x] 100% statement coverage
- [x] `go vet` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Design Decisions

- **Package name `mapx`**: avoids conflict with Go stdlib `maps` package (Go 1.21+). The `revive` linter flags stdlib name shadows.
- **Shallow `Clone` uses `maps.Copy`**: matches Go stdlib semantics. Use `CloneFunc` or `CloneNested` for deep copies.
- **`MergeAdditive` uses `Numeric` constraint**: covers all integer and float types used across burndown (int64) and couples (int).
- **`CloneNested` is a two-level specialization**: covers the dominant pattern (map[K1]map[K2]V) without over-generalizing.
- **`SortedKeys` returns a new slice**: callers own the result and can iterate freely.
- **`Unique` preserves insertion order**: important for deterministic output in reports.

## Risk

Low. New package, pure functions, no side effects. All operations are well-understood generic patterns.

## Implementation

**Files created:**
- `pkg/alg/mapx/maps.go` — `Clone`, `CloneFunc`, `CloneNested`, `MergeAdditive`, `SortedKeys`, `Numeric` type constraint
- `pkg/alg/mapx/slices.go` — `CloneSlice`, `Unique`
- `pkg/alg/mapx/maps_test.go` — 24 tests, 100% coverage
- `pkg/alg/mapx/slices_test.go` — 10 tests, 100% coverage

**Files modified (callers wired, F1.3 done in same pass):**
- `internal/analyzers/burndown/checkpoint.go` — 5 clone functions replaced with `mapx.Clone`, `mapx.CloneNested`
- `internal/analyzers/burndown/aggregator.go` — `cloneSparseHistory` → `mapx.CloneNested`, `cloneFileHistories` → `mapx.CloneFunc`, `cloneFileOwnership` → `mapx.CloneNested`
- `internal/analyzers/couples/aggregator.go` — `copyFilesMap` → `mapx.CloneNested`, `copyIntSlice` → `mapx.CloneSlice`
- `internal/analyzers/anomaly/metrics.go` — `sortedTickKeys` deleted, uses `mapx.SortedKeys`, additive merge uses `mapx.MergeAdditive`
- `internal/analyzers/anomaly/analyzer.go` — uses `mapx.SortedKeys`
- `internal/analyzers/anomaly/store_reader.go` — uses `mapx.SortedKeys`
- `internal/analyzers/quality/metrics.go` — `sortedTickKeys` deleted, uses `mapx.SortedKeys`
- `internal/analyzers/cohesion/calculations.go` — `uniqueStrings` deleted, uses `mapx.Unique`

**Dead code eliminated:** ~120 lines of helper functions across 5 packages.
