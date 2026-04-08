# FRD: allocateProportionally in internal/budget (Roadmap 2.5)

**ID**: FRD-20260310-allocate-proportionally
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 2.5
**Date**: 2026-03-10

## Problem

`internal/budget/solver.go` repeats the percentage-to-bytes pattern
`total * percent / percentDivisor` across two functions:

1. `SolveForBudget` — 3 sites (cache, worker, buffer allocations)
2. `deriveKnobs` — 2 sites (blob cache ratio, diff cache ratio)

Total: 5 instances of identical integer-percentage math. Additionally,
`NativeLimitsForBudget` in `model.go` has 2 more instances of the same pattern.

## Decision

Add a private helper to `solver.go`:

```go
// allocateProportionally distributes total bytes across named buckets by weight.
// Weights must be in [0,1] and should sum to <= 1.0.
// Returns a map from bucket name to allocated bytes (truncated to int64).
func allocateProportionally(total int64, weights map[string]float64) map[string]int64
```

### Design notes

- **Private**: no external consumers; purely an internal DRY extraction.
- **Float64 weights**: clearer than integer percentages; `0.60` reads better than
  `60 / 100`. Existing integer constants remain for documentation, but the helper
  accepts pre-computed float64 values.
- **Truncation**: `int64(float64(total) * weight)` truncates toward zero, matching
  the existing `total * pct / 100` behavior for positive values.
- **No validation**: private function; callers are trusted. Negative weights or
  sums > 1.0 are programmer errors caught by tests, not runtime checks.
- **Scope**: refactor `SolveForBudget` and `deriveKnobs` in `solver.go`.
  `NativeLimitsForBudget` in `model.go` could also benefit but is out of
  scope for this step (only 2 sites, different file).

## Contract

- `allocateProportionally(total, nil)` returns empty map.
- `allocateProportionally(total, {"a": 0.5})` returns `{"a": total/2}` (truncated).
- `allocateProportionally(0, {"a": 0.5})` returns `{"a": 0}`.
- Result values are non-negative when total >= 0 and weights are in [0,1].

## Scope

### Files modified

| File | Change |
|------|--------|
| `internal/budget/solver.go` | Add `allocateProportionally`; refactor `SolveForBudget` and `deriveKnobs` |
| `internal/budget/solver_test.go` | Add tests for `allocateProportionally` |

### Out of scope

- `model.go` / `NativeLimitsForBudget` — separate step if desired
- Changing budget constants or solver behavior

## Acceptance Criteria

- [x] `allocateProportionally` added to `solver.go`
- [x] `SolveForBudget` uses `allocateProportionally` for cache/worker/buffer split
- [x] `deriveKnobs` uses `allocateProportionally` for blob/diff cache split
- [x] All existing tests pass unchanged (behavior-preserving refactor)
- [x] New tests cover `allocateProportionally` directly
- [x] `go test ./internal/budget/...` passes
- [x] `make lint` passes — 0 issues, no dead code

## Implementation

### Files Modified

| File | Change |
|------|--------|
| `internal/budget/solver.go` | Add `allocateProportionally`, weight constants, bucket constants; refactor `SolveForBudget` and `deriveKnobs` |
| `internal/budget/solver_test.go` | 5 new tests for `allocateProportionally` |
| `specs/ref/ROADMAP.md` | Mark 2.5 done |
