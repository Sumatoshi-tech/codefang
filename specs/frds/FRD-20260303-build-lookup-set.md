# FRD: Add BuildLookupSet[T] to pkg/alg/mapx (Roadmap F5.2)

**ID**: FRD-20260303-build-lookup-set
**Roadmap**: [specs/dedup/ROADMAP.md](../dedup/ROADMAP.md) — Item F5.2
**Spec**: [specs/dedup/SPEC.md](../dedup/SPEC.md) — Cluster 2: Generic Collections & Algorithms

## Problem

The `map[T]struct{}` set-from-slice pattern appears in multiple locations across the codebase
(e.g., `analyze/timeseries.go:116-122`, `analyze/registry.go:173-176`,
`cohesion/calculations.go:46-51`). Each site manually allocates the map, iterates the source,
and inserts `struct{}{}` sentinels. This is a well-known Go idiom that benefits from a
single, tested, generic implementation.

## Feature

Add `BuildLookupSet[T comparable](items []T) map[T]struct{}` to `pkg/alg/mapx/slices.go`.
The function converts a slice into a lookup set (`map[T]struct{}`), handling nil input
and pre-sizing the map for efficiency.

Then migrate `analyze/timeseries.go:assembleCommits` to collect commit hashes into a
slice and use `BuildLookupSet` to build the lookup set.

### Design Decisions

- **Placement in `slices.go`**: The function transforms a slice into a map, consistent with
  `Unique` (which also builds a `map[T]struct{}` internally). Placed alongside other
  slice-to-collection operations.
- **Nil-in → nil-out**: Follows the `CloneSlice`/`Unique`/`SortAndLimit` convention in this
  package. A nil input returns nil, not an empty map.
- **Pre-sized map**: `make(map[T]struct{}, len(items))` avoids rehashing for known-size inputs.
- **Duplicate tolerance**: Duplicate items in the input are silently deduplicated (set semantics).

### Migration Scope

| File | Action |
|------|--------|
| `pkg/alg/mapx/slices.go` | Add `BuildLookupSet[T]` function |
| `pkg/alg/mapx/slices_test.go` | Add `TestBuildLookupSet` (6 cases) |
| `internal/analyzers/analyze/timeseries.go` | Migrate `assembleCommits` to use `BuildLookupSet` |

### Not Migrated (pattern doesn't match `BuildLookupSet`)

- `analyze/registry.go:descriptorIDSet()` — builds set from struct field (`.ID`), not a plain slice
- `cohesion/calculations.go:collectUniqueVariables()` — builds set from nested slice iteration, then converts back to slice
- `anomaly/metrics.go:256-267` — incremental set build within a processing loop
- `file_history/store_writer.go:108-115` — incremental set build within a processing loop

## Acceptance Criteria

- [x] `mapx.BuildLookupSet[T comparable](items []T) map[T]struct{}` exists in `pkg/alg/mapx/slices.go`
- [x] Unit tests in `pkg/alg/mapx/slices_test.go` (6 cases: nil, empty, no duplicates, with duplicates, single element, string type)
- [x] `analyze/timeseries.go:assembleCommits` uses `mapx.BuildLookupSet`
- [x] All existing tests pass
- [x] `go vet` clean, `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files modified:**
- `pkg/alg/mapx/slices.go` — added `BuildLookupSet[T comparable](items []T) map[T]struct{}`
- `pkg/alg/mapx/slices_test.go` — added `TestBuildLookupSet` (6 cases: nil_returns_nil, empty_returns_empty, no_duplicates, with_duplicates, single_element, string_type)
- `internal/analyzers/analyze/timeseries.go` — `assembleCommits` refactored: commit hashes collected into slice, then `mapx.BuildLookupSet` builds the lookup set
