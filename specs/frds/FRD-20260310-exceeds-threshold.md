# FRD: Promote exceedsThreshold to pkg/alg/stats (Roadmap 3.2)

**ID**: FRD-20260310-exceeds-threshold
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 3.2
**Date**: 2026-03-10

## Problem

`internal/streaming/planner.go` contains `exceedsThreshold(observed, predicted, threshold float64) bool`,
a pure numerical helper with zero domain dependencies. It computes whether
the absolute relative divergence `|observed − predicted| / predicted` exceeds
a given threshold fraction.

This utility belongs in `pkg/alg/stats` alongside `Clamp`, `Mean`, and other
composable statistics primitives. Promoting it:

1. Makes it available to future packages without import cycles.
2. Co-locates it with related numerical helpers.
3. Follows the project pattern of domain-free utilities in `pkg/`.

## Decision

Add an exported function to `pkg/alg/stats/stats.go`:

```go
// ExceedsThreshold reports whether observed diverges from predicted
// by more than threshold (as a fraction, e.g. 0.1 = 10%).
// Returns false when predicted <= 0 (no meaningful baseline).
func ExceedsThreshold(observed, predicted, threshold float64) bool
```

Update `internal/streaming/planner.go` to call `stats.ExceedsThreshold`
instead of the local `exceedsThreshold`. Delete the local function.

## Contract

- Returns `false` when `predicted <= 0` (degenerate baseline).
- Computes absolute relative divergence: `|observed − predicted| / predicted`.
- Returns `true` when divergence strictly exceeds threshold.
- Pure function, no side effects.

## Scope

### Files modified

| File | Change |
|------|--------|
| `pkg/alg/stats/stats.go` | Add `ExceedsThreshold` |
| `pkg/alg/stats/stats_test.go` | Add tests for `ExceedsThreshold` |
| `internal/streaming/planner.go` | Replace `exceedsThreshold` calls with `stats.ExceedsThreshold`; delete local function |

### Out of scope

- Changing adaptive planner behavior or thresholds
- Adding variants (e.g., signed divergence, percentage-based API)

## Acceptance Criteria

- [x] `ExceedsThreshold` added to `pkg/alg/stats/stats.go`
- [x] Unit tests cover: exact threshold, above/below, zero predicted, negative predicted, negative observed
- [x] `internal/streaming/planner.go` updated: 3 call sites use `stats.ExceedsThreshold`
- [x] Local `exceedsThreshold` deleted from `planner.go`
- [x] `go test ./pkg/alg/stats/...` passes
- [x] `go test ./internal/streaming/...` passes
- [x] `make lint` passes — 0 issues, no dead code

## Implementation

### Files Modified

| File | Change |
|------|--------|
| `pkg/alg/stats/stats.go` | Add `ExceedsThreshold` function |
| `pkg/alg/stats/stats_test.go` | 11 table-driven tests for `ExceedsThreshold` |
| `internal/streaming/planner.go` | Replace 3 `exceedsThreshold(` calls with `stats.ExceedsThreshold(`; delete local function |
| `specs/ref/ROADMAP.md` | Mark 3.2 done |
