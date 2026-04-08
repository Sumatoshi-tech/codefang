# FRD: Unify mergePeopleHistories with mergeKeyedDeltas (Phase 3.1)

**ID**: FRD-20260317-merge-people-histories
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Phase 3.1
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Section 3 mergeKeyedDeltas

## Problem

`mergePeopleHistories` in shard_spill.go duplicates logic of `mergeKeyedDeltas[K comparable]` in history_deltas.go. Both merge per-key sparse histories additively. For K=int (people/author IDs), they are equivalent.

## Goal

Remove `mergePeopleHistories` and use `mergeKeyedDeltas[int]` at call sites.

## In Scope

- Replace mergePeopleHistories(dst, src) with dst = mergeKeyedDeltas(src, dst)
- Remove mergePeopleHistories from shard_spill.go
- Call sites: aggregator.go (Add, Collect)

## Out of Scope

- mergeMatrixInto, collectFileDeltas, etc.

## Semantics

- mergePeopleHistories(dst, src): merges src into dst in-place
- mergeKeyedDeltas(source, result): merges source into result, returns result (may allocate if result was nil)
- Equivalent: dst = mergeKeyedDeltas(src, dst)

## Acceptance Criteria

- [x] mergePeopleHistories removed from shard_spill.go
- [x] Both call sites use mergeKeyedDeltas[int]
- [x] `go test ./internal/analyzers/burndown/...` passes
- [x] `make lint` passes
- [x] Burndown analysis produces identical results

## Implementation

- Modified: internal/analyzers/burndown/aggregator.go (Add, Collect use mergeKeyedDeltas)
- Modified: internal/analyzers/burndown/shard_spill.go (removed mergePeopleHistories, removed unused mapx import)
