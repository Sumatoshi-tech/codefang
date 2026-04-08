# FRD: Replace stats.Min/Max with slices.Min/Max (Phase 1.4)

**ID**: FRD-20260317-stats-minmax-stdlib
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Phase 1.4
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Section 1 Stdlib Replacements

## Problem

`stats.Min` and `stats.Max` in pkg/alg/stats duplicate stdlib `slices.Min` and `slices.Max` (Go 1.21+). The only difference: stats returns zero for empty slice; slices panics. Callers must handle empty slices explicitly.

## Goal

Remove `stats.Min` and `stats.Max`; use `slices.Min` and `slices.Max` at call sites with empty-slice guards.

## In Scope

- Remove stats.Min and stats.Max from pkg/alg/stats
- Update internal/analyzers/quality/metrics.go to use slices.Min/Max with empty guards
- Update internal/analyzers/quality/analyzer_test.go (remove or adapt stats.Min/Max tests)

## Out of Scope

- stats.Mean, stats.Median, stats.Sum, stats.Percentile, etc. (unchanged)

## Acceptance Criteria

- [x] stats.Min and stats.Max removed from pkg/alg/stats
- [x] quality/metrics.go uses slices.Min/Max with empty-slice guards
- [x] `go test ./...` passes
- [x] `make lint` passes
- [x] No panic on empty slices in production paths

## Implementation

- Modified: pkg/alg/stats/stats.go (removed Min, Max)
- Modified: pkg/alg/stats/stats_test.go (removed TestMin, TestMax, TestMinInt, TestMaxInt)
- Modified: internal/analyzers/quality/metrics.go (minFloat64, maxFloat64, maxInt helpers using slices.Min/Max; empty guards)
- Modified: internal/analyzers/quality/analyzer_test.go (TestMinMaxFloat tests minFloat64/maxFloat64; TestSumIntMaxInt uses slices.Max)
