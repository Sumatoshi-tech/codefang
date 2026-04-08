# FRD: Remove appendUniqueIDs (Roadmap 1.5)

**ID**: FRD-20260306-append-unique-ids-removal
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 1.5
**Date**: 2026-03-06

## Problem

`appendUniqueIDs` in `internal/analyzers/analyze/registry.go` is a manual dedup loop
that maintains an external seen-set for cross-call deduplication.
`mapx.Unique[T comparable]` already exists and provides the same invariant: first
occurrence wins, insertion order preserved.

```go
// current: stateful cross-call helper
func appendUniqueIDs(target *[]string, targetSet map[string]struct{}, ids []string)
```

## Decision

**Collect-then-deduplicate**: accumulate IDs from all pattern resolutions into a flat
slice, then call `mapx.Unique(selected)` once before returning. This:

- Deletes `appendUniqueIDs` (8 lines) and its companion `selectedSet` variable
- Preserves identical semantics: first occurrence wins across multiple patterns
- Uses the existing, tested `mapx.Unique` implementation

**Why not per-call `Unique`?** That would require re-reading the already-seen slice each
time — equivalent cost, worse clarity.

## Scope

### Changed

| File | Change |
|------|--------|
| `internal/analyzers/analyze/registry.go` | Inline `ExpandPatterns` using `mapx.Unique`; delete `appendUniqueIDs` |

### Not changed

`mapx.Unique` — already correct; no modification needed.

## Acceptance Criteria

- [ ] `appendUniqueIDs` deleted from `registry.go`
- [ ] `selectedSet` variable removed from `ExpandPatterns`
- [ ] `ExpandPatterns` returns `mapx.Unique(selected), nil`
- [ ] New tests cover: exact match, glob match, wildcard `*`, overlapping patterns (dedup),
      unknown pattern error, empty pattern error
- [ ] `go test ./internal/analyzers/analyze/...` passes with ≥90% coverage on registry.go
- [ ] `make lint` passes

## Risk

Low. Behavior is identical: `mapx.Unique` preserves insertion order and first-occurrence
semantics, matching what `appendUniqueIDs` + `selectedSet` produced.

## Implementation

### Files Modified

- `internal/analyzers/analyze/registry.go` — refactored `ExpandPatterns`, deleted `appendUniqueIDs`
- `internal/analyzers/analyze/registry_test.go` — added `TestRegistry_ExpandPatterns*` tests
- `specs/ref/ROADMAP.md` — marked step 1.5 done
