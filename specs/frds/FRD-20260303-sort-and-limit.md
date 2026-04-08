# FRD: Add SortAndLimit[T] utility to pkg/alg/mapx (Roadmap F2.3)

**ID**: FRD-20260303-sort-and-limit
**Roadmap**: [specs/dedup/ROADMAP.md](../dedup/ROADMAP.md) — Item F2.3
**Spec**: [specs/dedup/SPEC.md](../dedup/SPEC.md) — Cluster 2: Collection Utilities

## Problem

3 plot files implement identical sort-then-truncate patterns:
```go
sorted := sortByXxx(items)
if len(sorted) > limit { sorted = sorted[:limit] }
```

Each has its own sort helper function (copy + sort.Slice + return), adding ~12 lines per file.

## Feature

Add `SortAndLimit[T any](items []T, less func(a, b T) bool, limit int) []T` to
`pkg/alg/mapx/slices.go`. This function copies the input slice, sorts by the provided
comparator, and truncates to `min(len, limit)`.

Then migrate all 3 call sites to use `mapx.SortAndLimit()` directly, inlining the
comparator. Delete the 3 now-unused sort helper functions.

### Design Decisions

- **Copy semantics**: `SortAndLimit` copies the input to avoid modifying the caller's data.
  This matches the existing sort helpers which also copy before sorting.
- **Inline comparator**: Each sort helper's comparator is inlined at the call site. The
  value-extraction helpers (`getCyclomaticValue`, `getEffortValue`, `getLinesValue`) remain.
- **Delete sort helpers**: After migration, `sortByComplexity`, `sortByEffort`, and
  `sortByLines` have no callers. Tests that referenced them are updated to use
  `mapx.SortAndLimit` directly.
- **`pkg/alg/mapx/slices.go`**: The file already contains `CloneSlice` and `Unique`.
  `SortAndLimit` fits naturally.

### Migration Scope

| File | Before | After |
|------|--------|-------|
| complexity/plot.go | `sortByComplexity()` + truncate (15 lines) | `mapx.SortAndLimit()` (3 lines) |
| halstead/plot.go | `sortByEffort()` + truncate (15 lines) | `mapx.SortAndLimit()` (3 lines) |
| comments/plot.go | `sortByLines()` + truncate (15 lines) | `mapx.SortAndLimit()` (3 lines) |

## Acceptance Criteria

- [x] `SortAndLimit[T]` exists in `pkg/alg/mapx/slices.go`
- [x] Unit tests: empty input, limit > len, limit < len, preserves original
- [x] complexity/plot.go uses `mapx.SortAndLimit()`, `sortByComplexity` deleted
- [x] halstead/plot.go uses `mapx.SortAndLimit()`, `sortByEffort` deleted
- [x] comments/plot.go uses `mapx.SortAndLimit()`, `sortByLines` deleted
- [x] Test files updated (no references to deleted functions)
- [x] All existing tests pass
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files created:**
- `pkg/alg/mapx/slices.go` — added `SortAndLimit[T]` function
- `pkg/alg/mapx/slices_test.go` — added `TestSortAndLimit` (6 cases)

**Files modified:**
- `internal/analyzers/complexity/plot.go` — deleted `sortByComplexity`, migrated call site
- `internal/analyzers/complexity/plot_test.go` — updated to use `mapx.SortAndLimit`
- `internal/analyzers/halstead/plot.go` — deleted `sortByEffort`, migrated call site
- `internal/analyzers/halstead/plot_test.go` — updated to use `mapx.SortAndLimit`
- `internal/analyzers/comments/plot.go` — deleted `sortByLines`, migrated call site
