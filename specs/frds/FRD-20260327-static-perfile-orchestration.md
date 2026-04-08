# FRD-20260327: StaticService Per-File Orchestration

**Date:** 2026-03-27
**Author:** Agent
**Status:** Complete
**Roadmap:** specs/filestats/ROADMAP.md — Step 1.4
**Spec:** specs/filestats/SPEC.md — Feature 1

## Problem

Steps 1.1-1.3 built the foundation: stats utility, JSON types, and per-file retention in aggregators. Now `StaticService` must wire it all together: enable per-file mode on aggregators, extract per-file results after analysis, build per-file `ReportSection`s, and compute `summary_stats`.

## Solution

### 1. `PerFile bool` field on `StaticService`

New exported field controlling per-file behavior. When true:
- `initAggregators()` calls `SetPerFileMode(true)` on aggregators that support it.
- After `AnalyzeFolder()`, per-file reports are retrievable.

### 2. `PerFileModeEnabled` interface in `analyze` package

```go
type PerFileModeEnabled interface {
    SetPerFileMode(enabled bool)
    PerFileResults() map[string]Report
}
```

Used for type-asserting aggregators in `initAggregators()` and `buildPerFileResults()`.

### 3. `BuildPerFileSections()` method

```go
func (svc *StaticService) BuildPerFileSections(
    perFileResults map[string]map[string]Report,
) map[string][]ReportSection
```

For each analyzer, iterates its per-file reports and calls `CreateReportSection()` on each.

### 4. `ComputeSummaryStats()` method

```go
func (svc *StaticService) ComputeSummaryStats(
    perFileSections map[string][]ReportSection,
) map[string]map[string]stats.Summary
```

For each analyzer, collects per-file metric values by label and calls `stats.ComputeSummary()`.

### 5. `buildPerFileResults()` helper

```go
func buildPerFileResults(
    aggregators map[string]ResultAggregator,
) map[string]map[string]Report
```

Extracts per-file results from aggregators that implement `PerFileModeEnabled`.

## Test Plan

- `initAggregators()` with `PerFile=true`: aggregator must have per-file mode enabled.
- `ComputeSummaryStats()`: given known per-file sections, verify stats are correct.
- `BuildPerFileSections()`: given per-file reports, verify section count and titles.
- Integration: `AnalyzeFolder()` with `PerFile=true` on 3-file fixture, verify per-file results.

## Implementation

**Status:** Complete

**Files created:**
- `internal/analyzers/analyze/perfile.go` — `PerFileModeEnabled` interface, `PerFileResults()`, `extractPerFileResults()`, `BuildPerFileSections()`, `ComputeSummaryStats()`, `collectMetricValues()`

**Files modified:**
- `internal/analyzers/analyze/static.go` — `PerFile bool` field, `perFileResults` internal field, wired `initAggregators()` and `AnalyzeFolder()`
- `internal/analyzers/analyze/static_test.go` — 5 new tests

**Coverage:** 81-100% across all functions in `perfile.go`.
**Race detector:** Clean.
**Lint:** Clean.
