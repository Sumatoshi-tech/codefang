# FRD-20260408: Tick-to-date mapping in JSON output

## Roadmap Link
- Source roadmap: specs/analytics-readiness/roadmap.md
- Feature: Feature 2 — Tick-to-date mapping in JSON output

## Problem

All 6 history time-series analyzers emit `tick: <int>` with no calendar date. The TICK struct already carries StartTime/EndTime (populated by sentiment and anomaly analyzers, but NOT by quality, devs, file-history). Without timestamps, time-series charts have unlabeled X-axes.

## Context

- `TICK` struct (analyze/tc.go) has `StartTime time.Time` and `EndTime time.Time` fields
- Sentiment and anomaly analyzers populate these via `tickAccumulator.startTime`/`endTime`
- Quality, devs, file-history do NOT populate them — their `buildTick()` leaves them zero
- The `ticksToReport` functions in each analyzer build `analyze.Report` maps but don't include tick timestamps
- `ComputeAllMetrics` parses these reports into typed structs that also lack timestamp fields
- Final JSON output (via `ComputedMetrics` → binary envelope → `UnifiedModel`) has no timestamp per tick

## Goal

Every time-series tick entry in the JSON output includes `start_time` and `end_time` as RFC 3339 strings.

## In Scope

- Add `TickBounds` type and `BuildTickBounds` helper to `analyze` package
- Add `start_time`/`end_time` to output structs: sentiment `TimeSeriesData`, anomaly time series, quality time series, devs `ActivityData`/`ChurnData`, file-history `CompositionTSData`
- Populate timestamps from TICK.StartTime/EndTime during metrics computation
- Add `startTime`/`endTime` tracking to quality, devs, file-history tick accumulators

## Out of Scope

- Adding `tick_size` to aggregate (deferred)
- Changing the time-series granularity
- Changing NDJSON/timeseries format

## Functional Requirements

### MUST
- `TickBounds` struct in `analyze` package: `{StartTime, EndTime time.Time}`
- `BuildTickBounds(ticks []TICK) map[int]TickBounds` extracts tick boundaries
- Each analyzer's `ticksToReport` passes tick bounds in the Report under key `"tick_bounds"`
- Each analyzer's `ParseReportData` reads tick bounds from Report
- Time-series output structs gain `StartTime string json:"start_time,omitempty"` and `EndTime string json:"end_time,omitempty"`
- Timestamps formatted as RFC 3339

### SHOULD
- Quality, devs, file-history `buildTick()` functions populate `TICK.StartTime`/`EndTime` from commit timestamps

## Implementation

### Shared infrastructure
- `internal/analyzers/analyze/tick_bounds.go` — new file: `TickBounds` type + `BuildTickBounds` helper
- `internal/analyzers/analyze/tick_bounds_test.go` — tests

### Per-analyzer changes (same pattern applied to all 5):
1. Added `StartTime`/`EndTime` string fields to time-series output structs
2. Added `TickBounds map[int]analyze.TickBounds` to `ReportData`/`TickData` input structs
3. Parsed `tick_bounds` from Report in `ParseReportData`/`ParseTickDataWithPrecision`
4. Set `start_time`/`end_time` from `TickBounds` during Compute
5. Added `tick_bounds: analyze.BuildTickBounds(ticks)` to each `ticksToReport`

### Files modified
- `internal/analyzers/sentiment/metrics.go` — `TimeSeriesData`, `ReportData`, `computeTimeSeriesWithOpts`
- `internal/analyzers/sentiment/analyzer.go` — `ticksToReport`
- `internal/analyzers/anomaly/metrics.go` — `TimeSeriesEntry`, `ReportData`, `computeTimeSeries`, `ParseReportData`
- `internal/analyzers/anomaly/analyzer.go` — `ticksToReport`
- `internal/analyzers/quality/metrics.go` — `TimeSeriesEntry`, `ReportData`, `ComputeAllMetrics` (+ extracted `computeAggregate`)
- `internal/analyzers/quality/analyzer.go` — `ticksToReport`
- `internal/analyzers/devs/metrics.go` — `ActivityData`, `ChurnData`, `TickData`, `ParseTickDataWithPrecision`, Compute methods
- `internal/analyzers/devs/analyzer.go` — `ticksToReport`
- `internal/analyzers/file_history/metrics.go` — `CompositionTimeSeriesEntry`, `computeComposition` signature
- `internal/analyzers/file_history/aggregator.go` — `TicksToReport`
- `internal/analyzers/file_history/store_writer.go` — updated `computeComposition` call

## Affected Files

- `internal/analyzers/analyze/tc.go` — add `TickBounds` type
- `internal/analyzers/analyze/tick_bounds.go` — new file: `BuildTickBounds` helper
- `internal/analyzers/sentiment/metrics.go` — `TimeSeriesData` struct, `computeTimeSeriesWithOpts`
- `internal/analyzers/sentiment/analyzer.go` — `ticksToReport` adds tick_bounds
- `internal/analyzers/anomaly/metrics.go` — time series struct, compute function
- `internal/analyzers/anomaly/analyzer.go` — `ticksToReport` adds tick_bounds
- `internal/analyzers/quality/metrics.go` — time series struct, compute function
- `internal/analyzers/quality/analyzer.go` — `ticksToReport` adds tick_bounds, `buildTick` adds timestamps
- `internal/analyzers/devs/metrics.go` — activity/churn structs, compute functions
- `internal/analyzers/devs/analyzer.go` — `ticksToReport`, `buildTick`
- `internal/analyzers/file_history/aggregator.go` — `FlushTick`, composition_ts
