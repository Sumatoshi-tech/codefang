# FRD: MergeNestedAdditive in pkg/alg/mapx (Roadmap 2.3)

**ID**: FRD-20260306-merge-nested-additive
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 2.3
**Date**: 2026-03-06

## Problem

Two-level nested map additive merge (`map[K1]map[K2]V += src`) is duplicated in
multiple places:

| File | Function | Type |
|------|----------|------|
| `burndown/shard_spill.go` | `mergeSparseHistory` | `map[int]map[int]int64` |
| `couples/aggregator.go` | `mergeTickFiles` | `map[string]map[string]int` |

Pattern:
```go
for k1, inner := range src {
    if dst[k1] == nil { dst[k1] = map[K2]V{} }
    for k2, v := range inner { dst[k1][k2] += v }
}
```

`pkg/alg/mapx` already has `MergeAdditive` for single-level maps. The nested
variant is missing.

**DoR findings:**

- `mergeSparseHistory` signature: `func mergeSparseHistory(dst, src sparseHistory)`
  where `type sparseHistory = map[int]map[int]int64` ✓
- `mergeTickFiles` signature: `func mergeTickFiles(dst, src map[string]map[string]int)` ✓
- `quality/metrics.go` — no `map[K1]map[K2]V` additive merge found; out of scope

**Call sites:**

`mergeSparseHistory` (burndown) — 10 direct calls:
- `shard_spill.go:81` (inside `mergePeopleHistories`)
- `aggregator.go:90`, `:149`, `:349`, `:358`, `:554`, `:571`, `:623`
- `history_deltas.go:34`, `:63`

`mergeTickFiles` (couples) — 1 direct call:
- `aggregator.go:545`

## Decision

Add to `pkg/alg/mapx/maps.go`:

```go
// MergeNestedAdditive merges src into dst for two-level maps.
// For each key k1 in src with a non-empty inner map, the inner map is merged
// additively into dst[k1]. If dst[k1] is nil it is initialized.
// Empty inner maps in src are skipped. If dst is nil this is a no-op.
func MergeNestedAdditive[K1, K2 comparable, V Numeric](dst, src map[K1]map[K2]V)
```

Delete `mergeSparseHistory` and `mergeTickFiles`; replace all call sites.

## Contract

- `MergeNestedAdditive(nil, src)` → no-op (consistent with `MergeAdditive`)
- `MergeNestedAdditive(dst, nil)` → no-op
- `MergeNestedAdditive(dst, src)` where `src[k1]` is empty → k1 skipped (no alloc)
- `MergeNestedAdditive(dst, src)` where `dst[k1]` is nil → initialized before merge
- `dst[k1][k2] += src[k1][k2]` for all non-empty inner maps

## Scope

### Files modified

| File | Change |
|------|--------|
| `pkg/alg/mapx/maps.go` | Add `MergeNestedAdditive[K1, K2, V]` |
| `pkg/alg/mapx/maps_test.go` | Tests for `MergeNestedAdditive` |
| `burndown/shard_spill.go` | Delete `mergeSparseHistory`; update `mergePeopleHistories`; add mapx import |
| `burndown/aggregator.go` | Replace 6 `mergeSparseHistory` calls; simplify 2 inline loops |
| `burndown/history_deltas.go` | Replace 2 `mergeSparseHistory` calls; add mapx import |
| `couples/aggregator.go` | Delete `mergeTickFiles`; replace 1 call site |

### Out of scope

- `quality` — no nested map additive merge found
- `mergeMatrixInto` — operates on `*[]map[int]int64` (slice pointer, different shape)
- `mergePeopleHistories`, `mergeTickPeopleHistories` — 3-level structure; only inner call updated

## Acceptance Criteria

- [x] `MergeNestedAdditive` in `maps.go` with tests (nil dst, nil src, empty inner, additive)
- [x] `mergeSparseHistory` deleted from burndown; all 10 call sites replaced
- [x] `mergeTickFiles` deleted from couples; 1 call site replaced
- [x] `go test ./pkg/alg/mapx/...` passes
- [x] `go test ./internal/analyzers/{burndown,couples}/...` passes
- [x] `make lint` — 0 issues, no dead code

## Implementation

### Files Modified

| File | Change |
|------|--------|
| `pkg/alg/mapx/maps.go` | Add `MergeNestedAdditive` |
| `pkg/alg/mapx/maps_test.go` | Tests |
| `internal/analyzers/burndown/shard_spill.go` | Delete `mergeSparseHistory`; update callers |
| `internal/analyzers/burndown/aggregator.go` | Replace all `mergeSparseHistory` calls |
| `internal/analyzers/burndown/history_deltas.go` | Replace `mergeSparseHistory` calls |
| `internal/analyzers/couples/aggregator.go` | Delete `mergeTickFiles`; replace caller |
| `specs/ref/ROADMAP.md` | Mark 2.3 done |
| `AGENTS.md` | Update mapx entry |
