# FRD: Add ToPercent() helper and consolidate percentMultiplier (Roadmap F1.5)

**ID**: FRD-20260303-to-percent
**Roadmap**: [specs/dedup/ROADMAP.md](../dedup/ROADMAP.md) — Item F1.5
**Spec**: [specs/dedup/SPEC.md](../dedup/SPEC.md) — Cluster 1: Shared Constants

## Problem

4 analyzers define identical `const percentMultiplier = 100` independently:
- `burndown/metrics.go:15`
- `devs/metrics.go:564`
- `anomaly/metrics.go:110`
- `sentiment/metrics.go:201`

All 9 usage sites across 7 files perform the same operation: `value * percentMultiplier`
(converting a ratio to a percentage). This is a DRY violation and obscures intent.

## Feature

Add `ToPercent(ratio float64) float64` to `pkg/alg/stats` and an exported constant
`PercentMultiplier = 100`. Then migrate all 9 usage sites to `stats.ToPercent(value)`
and remove all 4 local `percentMultiplier` constant definitions.

### Design Decisions

- **`ToPercent()` over raw constant**: All 9 usage sites multiply by the constant.
  A function call `stats.ToPercent(ratio)` is more readable than `ratio * stats.PercentMultiplier`.
  The exported constant is still provided for edge cases needing the raw value.
- **`pkg/alg/stats` is the right home**: The package already provides statistical utilities
  (Mean, MeanStdDev, Percentile, Clamp). Percentage conversion fits naturally.
- **importShadow mitigation**: 3 files use local variables named `stats` that will
  shadow the imported package. These are renamed proactively:
  - `burndown/plot.go:101` — `stats` → `statCards`
  - `burndown/store_reader.go:80` — `stats` → `statCards`
  - `devs/metrics.go:88,454,498` — `stats` → `langSt` (loop variable in range)

### Migration Scope

| File | Usage sites | Action |
|------|------------|--------|
| burndown/metrics.go | 1 (line 262) | Remove const, add import, replace |
| burndown/text.go | 2 (lines 80, 199) | Add import, replace |
| burndown/plot.go | 1 (line 98) | Add import, replace, rename `stats` var |
| burndown/store_reader.go | 1 (line 77) | Add import, replace, rename `stats` var |
| devs/metrics.go | 2 (lines 608, 614) | Remove const, add import, replace, rename `stats` loop vars |
| anomaly/metrics.go | 1 (line 120) | Remove const, add import, replace |
| sentiment/metrics.go | 1 (line 284) | Remove const, add import, replace |

## Acceptance Criteria

- [x] `ToPercent(ratio float64) float64` exists in `pkg/alg/stats/stats.go`
- [x] `const PercentMultiplier = 100` exists in `pkg/alg/stats/stats.go`
- [x] Unit tests for `ToPercent` cover positive ratio, zero, and negative ratio
- [x] All 4 local `percentMultiplier` const definitions removed
- [x] All 9 usage sites migrated to `stats.ToPercent()`
- [x] No `importShadow` lint errors (local `stats` variables renamed)
- [x] All existing tests pass
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files created/modified:**
- `pkg/alg/stats/stats.go` — added `PercentMultiplier` constant and `ToPercent()` function
- `pkg/alg/stats/stats_test.go` — added `TestToPercent` (5 cases) and `TestPercentMultiplierConstant`
- `internal/analyzers/burndown/metrics.go` — removed const from block, added `stats` import, 1 usage migrated
- `internal/analyzers/burndown/text.go` — added `stats` import, 2 usages migrated
- `internal/analyzers/burndown/plot.go` — added `stats` import, 1 usage migrated, renamed `stats` → `statCards`
- `internal/analyzers/burndown/store_reader.go` — added `stats` import, 1 usage migrated, renamed `stats` → `statCards`
- `internal/analyzers/devs/metrics.go` — removed const, added `stats` import, 2 usages migrated, renamed `stats` → `langSt` (3 loop vars)
- `internal/analyzers/anomaly/metrics.go` — removed const, added `stats` import, 1 usage migrated
- `internal/analyzers/sentiment/metrics.go` — removed const from block, added `stats` import, 1 usage migrated
