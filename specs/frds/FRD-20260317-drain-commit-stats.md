# FRD: Extract generic DrainCommitStats helper (Phase 5.1)

**ID**: FRD-20260317-drain-commit-stats
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Phase 5.1
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Section 5 Cross-Analyzer Consolidation

## Problem

burndown, couples, and file_history each implement DrainCommitStats with nearly identical structure: convert map[string]*CommitSummary to map[string]any, return with commitsByTick, clear source maps. The only difference is the CommitSummary→map conversion (different fields per analyzer).

## Goal

Create a generic helper in internal/analyzers/analyze that encapsulates the common pattern. Each aggregator provides a toMap converter and a clear callback.

## In Scope

- Add DrainCommitStatsHelper[T] in analyze package
- burndown, couples, file_history use the helper
- devs does not implement CommitStatsDrainer (verified)

## Out of Scope

- Changing CommitStatsDrainer interface
- Changing timeseries output format

## Acceptance Criteria

- [x] Generic DrainCommitStats helper in analyze package
- [x] burndown, couples, file_history use helper
- [x] `go test ./internal/analyzers/...` passes
- [x] Timeseries output unchanged

## Implementation

- Created: internal/analyzers/analyze/drain_commit_stats.go (DrainCommitStatsHelper)
- Created: internal/analyzers/analyze/drain_commit_stats_test.go
- Modified: internal/analyzers/burndown/aggregator.go
- Modified: internal/analyzers/couples/aggregator.go
- Modified: internal/analyzers/file_history/aggregator.go
