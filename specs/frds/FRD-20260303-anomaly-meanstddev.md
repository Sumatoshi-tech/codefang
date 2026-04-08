# FRD: Replace anomaly's inline MeanStdDev with stats.MeanStdDev (Roadmap F1.6)

**ID**: FRD-20260303-anomaly-meanstddev
**Roadmap**: [specs/dedup/ROADMAP.md](../dedup/ROADMAP.md) — Item F1.6
**Spec**: [specs/dedup/SPEC.md](../dedup/SPEC.md) — Cluster 1: Shared Constants

## Problem

`anomaly/zscore.go:60-62` exports a `MeanStdDev()` function that is a pure delegation
wrapper around `stats.MeanStdDev()`. The wrapper adds no value — it just forwards the
call. Internal callers (`metrics.go` × 4 sites, `enrich.go` × 1 site) should call
`stats.MeanStdDev()` directly.

Three test functions in `zscore_test.go` test the wrapper and duplicate coverage already
present in `pkg/alg/stats/stats_test.go`.

## Feature

Delete the wrapper `MeanStdDev()` from `zscore.go` and replace all 5 internal call sites
with direct `stats.MeanStdDev()` calls. Delete the 3 redundant test functions.

### Design Decisions

- **Delete wrapper rather than keep**: The wrapper has no callers outside the anomaly
  package (verified via `anomaly.MeanStdDev` grep). It adds an unnecessary indirection.
- **Delete redundant tests**: The same cases (basic, single value, empty) are already
  covered in `pkg/alg/stats/stats_test.go`.
- **`metrics.go` already imports `stats`**: Added in F1.5, so no new import needed there.
- **`enrich.go` needs `stats` import**: Only file requiring a new import.

### Migration Scope

| File | Change |
|------|--------|
| anomaly/zscore.go | Delete `MeanStdDev` wrapper function (lines 58-62) |
| anomaly/metrics.go | 4 calls → `stats.MeanStdDev()` |
| anomaly/enrich.go | 1 call → `stats.MeanStdDev()`, add `stats` import |
| anomaly/zscore_test.go | Delete 3 redundant `TestMeanStdDev_*` functions |

## Acceptance Criteria

- [x] `MeanStdDev` wrapper deleted from `anomaly/zscore.go`
- [x] All 5 call sites use `stats.MeanStdDev()` directly
- [x] 3 redundant test functions deleted from `zscore_test.go`
- [x] All existing tests pass (results numerically identical)
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files modified:**
- `internal/analyzers/anomaly/zscore.go` — deleted `MeanStdDev` wrapper (4 lines)
- `internal/analyzers/anomaly/metrics.go` — 4 calls migrated to `stats.MeanStdDev()`
- `internal/analyzers/anomaly/enrich.go` — added `stats` import, 1 call migrated
- `internal/analyzers/anomaly/zscore_test.go` — deleted 3 redundant test functions, fixed trailing blank line
