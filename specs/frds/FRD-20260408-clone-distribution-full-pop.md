# FRD-20260408: Clone pair distribution from full population

## Roadmap Link
- Source roadmap: specs/analytics-readiness/roadmap.md
- Feature: Feature 7 — Clone pair distribution from full population

## Problem

Clone pairs are capped at 1000 (`DefaultMaxClonePairs`) but distribution metrics (Type-1/2/3 breakdown) are computed from the capped sample in `Distribution()`. For 22M total pairs, only 1000 are counted, skewing percentages.

## Goal

Track clone type distribution during pair discovery (before capping) and emit accurate counts.

## Functional Requirements

### MUST
- Add `typeDistribution cloneTypeCounts` to `clonePairResult`
- Increment per-type counters in `matchCandidates` when a valid pair is found
- Add `clone_type_distribution` key to the report with `{"Type-1": N, "Type-2": N, "Type-3": N}`
- `ReportSection.Distribution()` uses the full-population counters, not the capped array

## Implementation

### Changes
- `clonePairResult` gained `typeDistribution cloneTypeCounts` field (visitor.go)
- `matchCandidates` calls `result.typeDistribution.increment(pair.CloneType)` for every valid pair (before cap check)
- `cloneTypeCounts` gained `increment(*cloneTypeCounts)` method and `cloneTypeDistMap()` standalone function
- `keyCloneTypeDistribution` report key added (report.go)
- Both `Aggregator.GetResult()` and `Analyzer.buildReport()` emit the distribution map
- `ReportSection.Distribution()` reads `clone_type_distribution` from report when available, falls back to capped array

### Files modified
- `internal/analyzers/clones/visitor.go` — `clonePairResult`, `matchCandidates`
- `internal/analyzers/clones/aggregator.go` — `GetResult`
- `internal/analyzers/clones/analyzer.go` — `buildReport`
- `internal/analyzers/clones/report_section.go` — `Distribution()`, `extractDistribution`, `increment`, `cloneTypeDistMap`
- `internal/analyzers/clones/report.go` — `keyCloneTypeDistribution`

## Affected Files
- `internal/analyzers/clones/visitor.go` — `clonePairResult`, `matchCandidates`
- `internal/analyzers/clones/aggregator.go` — `GetResult` report building
- `internal/analyzers/clones/analyzer.go` — `buildReport`
- `internal/analyzers/clones/report_section.go` — `Distribution()`
- `internal/analyzers/clones/report.go` — constants for new key
