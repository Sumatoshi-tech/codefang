# FRD: Create generic FilterByInterface utility (Roadmap F0.9)

**ID**: FRD-20260302-filter-by-interface
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F0.9
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster: Common Utilities

## Problem

The framework layer has 3 identical functions that filter a slice of analyzers by interface satisfaction:

| Function | File | Target Interface | Return Type |
|----------|------|------------------|-------------|
| `collectHibernatables` | `framework/streaming.go:638` | `streaming.Hibernatable` | `[]streaming.Hibernatable` |
| `collectSpillCleaners` | `framework/streaming.go:650` | `streaming.SpillCleaner` | `[]streaming.SpillCleaner` |
| `collectCheckpointables` | `framework/streaming.go:662` | `checkpoint.Checkpointable` | `[]checkpoint.Checkpointable` |

Each repeats the same pattern:
```go
var result []TargetInterface
for _, a := range analyzers {
    if t, ok := a.(TargetInterface); ok {
        result = append(result, t)
    }
}
return result
```

Note: `collectSnapshotters` (`runner.go:1121`) is a strict assertion (all items MUST match, returns error on failure) — different semantics, not a filter pattern.

## Feature

Create a generic `FilterByInterface` in `internal/analyzers/common/filter.go`.

### filter.go — Generic Interface Filter

| Export | Signature | Behavior |
|--------|-----------|----------|
| `FilterByInterface[T any, U any]` | `func(items []T, cast func(T) (U, bool)) []U` | Returns a new slice containing only items where `cast` returns `(value, true)`. Preserves input order. |

### Design Decisions

- **Function-based cast**: Go generics cannot express interface type assertions directly in generic code. A `cast func(T) (U, bool)` parameter lets callers use the standard Go type assertion `a.(SomeInterface)` in the closure, keeping the generic fully type-safe.
- **No error variant**: The soft-filter pattern intentionally skips non-matching items. The strict "all must match" pattern (like `collectSnapshotters`) has different semantics and is better left as a dedicated function.
- **Preserves order**: Matching items appear in the same relative order as in the input slice.
- **Nil/empty input returns nil**: `FilterByInterface(nil, cast)` returns `nil`, matching Go convention for uninitialized slices.
- **No new dependencies**: Pure Go, no imports needed.

## Acceptance Criteria

- [x] `internal/analyzers/common/filter.go` exports: `FilterByInterface[T any, U any](items []T, cast func(T) (U, bool)) []U`
- [x] `internal/analyzers/common/filter_test.go` covers: empty slice, nil slice, no matches, all match, partial match, preserves order, concrete type, single element (8 tests)
- [x] All tests pass, 100% statement coverage
- [x] `go vet` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files created:**
- `internal/analyzers/common/filter.go` — `FilterByInterface[T, U]` generic soft-filter function
- `internal/analyzers/common/filter_test.go` — 8 tests

**Files modified (F1.9 wiring):**
- `internal/framework/streaming.go` — `collectHibernatables`, `collectSpillCleaners`, `collectCheckpointables` now delegate to `common.FilterByInterface`
