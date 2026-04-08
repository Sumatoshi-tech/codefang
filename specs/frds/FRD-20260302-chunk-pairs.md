# FRD: Extract BuildChunks + ForEachPair to pkg/alg (Roadmap F4.2)

**ID**: FRD-20260302-chunk-pairs
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F4.2

## Problem

Two generic algorithms are currently embedded in domain-specific packages:

1. **Range chunking** (`buildChunks` in `internal/streaming/planner.go:514-528`): Splits a
   total count into [start, end) ranges of a given size. The same logic is also inlined in
   `Planner.Plan()` (lines 76-81). Pure range math with zero domain coupling. See LIST.md #14.

2. **Pairwise iteration** (inner loop in `internal/analyzers/shotness/aggregator.go:111-121`):
   Iterates all C(n,2) unique pairs (i,j where i < j). Generic combinatorial utility. See
   LIST.md #39.

## Feature

Create two small generic algorithm functions in `pkg/alg`:

- `Chunk(total, size int) []Range` — splits a range [0, total) into chunks of the given size
- `ForEachPair(n int, visit func(i, j int))` — calls visit for all unique pairs (i,j) where 0 <= i < j < n

Wire existing callers:
- `streaming.ChunkBounds` becomes a type alias for `alg.Range`
- `Planner.Plan()` and `buildChunks` delegate to `alg.Chunk`; `buildChunks` removed as dead code
- `shotness/aggregator.go` uses `alg.ForEachPair` for the pairwise iteration

## Acceptance Criteria

- [x] `pkg/alg/chunk.go` exports `type Range struct { Start, End int }`, `Chunk(total, size int) []Range`
- [x] `pkg/alg/pairs.go` exports `ForEachPair(n int, visit func(i, j int))`
- [x] `pkg/alg/chunk_test.go` covers: total=0, size=0, size>total, exact division, remainder, single element
- [x] `pkg/alg/pairs_test.go` covers: n=0, n=1, n=2, n=3, n=5 (verifies C(n,2) count and all pairs)
- [x] `streaming.ChunkBounds` aliased to `alg.Range`; `buildChunks` deleted
- [x] `Planner.Plan()` uses `alg.Chunk`
- [x] `shotness/aggregator.go` uses `alg.ForEachPair`
- [x] All existing tests pass
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Risk

**Low.** Pure extraction of existing logic. `ChunkBounds` becomes a type alias, so all
existing code compiles without changes. `ForEachPair` replaces an inline loop with identical
semantics.

## Non-Goals

- Changing chunk sizing logic in the planner.
- Changing coupling computation logic in shotness.
- Adding new algorithms beyond Chunk and ForEachPair.

## Implementation

### Files Created

- `pkg/alg/chunk.go` — `Range` type, `Chunk` function
- `pkg/alg/chunk_test.go` — tests for Chunk
- `pkg/alg/pairs.go` — `ForEachPair` function
- `pkg/alg/pairs_test.go` — tests for ForEachPair

### Files Modified

- `internal/streaming/planner.go` — `ChunkBounds` aliased to `alg.Range`; `buildChunks` removed; `Plan()` delegates to `alg.Chunk`
- `internal/analyzers/shotness/aggregator.go` — pairwise loop uses `alg.ForEachPair`

### Verification

- `go vet ./...` — clean
- `go test ./pkg/alg/... ./internal/streaming/... ./internal/analyzers/shotness/...` — all pass
- `make lint` — 0 issues, 0 dead code
