# FRD-20260328: FilterTicksSince — Post-Analysis TICK Filter

**Date:** 2026-03-28
**Author:** Agent
**Status:** Complete
**Roadmap:** specs/filestats/ROADMAP.md — Step 2.4
**Spec:** specs/filestats/SPEC.md — FR-2.4

## Problem

`--since` currently truncates the commit walk, which breaks line attribution in burndown analysis. FR-2.4 mandates repurposing it as a post-analysis output filter that only affects which TICKs appear in the final report, not which commits are processed.

## Solution

### 1. `FilterTicksSince` function

Package-level function in `internal/analyzers/analyze/tc.go`:

```go
func FilterTicksSince(ticks []TICK, since time.Time) []TICK
```

Returns TICKs whose `EndTime` is at or after `since`. Preserves order. Returns nil for empty input.

### 2. E2e test update

The existing e2e test uses a type-assertion approach that doesn't match the package-level function design. Update it to call `FilterTicksSince` directly.

## Test Plan

- Unit test: 4 TICKs, since in the middle → 2 returned.
- Edge: empty input → nil.
- Edge: since before all TICKs → all returned.
- Edge: since after all TICKs → nil.
- E2E test green: `TestCache_SinceIsOutputFilter`.

## Implementation

**Status:** Complete

**Files modified:**
- `internal/analyzers/analyze/tc.go` — `FilterTicksSince` function
- `internal/analyzers/analyze/tc_test.go` — 5 test cases (table-driven + empty input)
- `tests/e2e/filestats_cache_test.go` — updated to call `FilterTicksSince` directly

**Lint:** Clean. **Race:** Clean.
