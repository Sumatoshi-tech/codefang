# FRD-20260327: Per-File Report Retention in Aggregators

**Date:** 2026-03-27
**Author:** Agent
**Status:** Complete
**Roadmap:** specs/filestats/ROADMAP.md — Step 1.3
**Spec:** specs/filestats/SPEC.md — Feature 1

## Problem

The `--per-file` output mode (Feature 1) requires each analyzer section to include per-file breakdowns. Currently, aggregators merge per-file reports into a single aggregated report, discarding per-file identity. We need to retain the per-file report snapshots before they are merged.

## Solution

Create a `PerFileRetainer` embeddable struct in `internal/analyzers/common/` that stores per-file report clones keyed by source file path. Each of the 5 static analyzer aggregators embeds it and calls `Retain()` in their `Aggregate()` method.

### Key Design Decisions

1. **Embeddable struct, not decorator** — follows Go composition idiom, keeps the `ResultAggregator` interface unchanged.
2. **Extract file path from report data** — uses the `_source_file` key already stamped by `StampSourceFile()` on `TypedCollection.SourceFile` or collection items.
3. **Shallow clone of report** — `maps.Clone()` produces a new map with the same values. This is sufficient since values are scalars or immutable slices from single-file analysis.
4. **No-op when disabled** — `Retain()` returns immediately when per-file mode is off. Zero memory overhead.

## Type Definition

```go
// PerFileRetainer stores per-file report snapshots during aggregation.
type PerFileRetainer struct {
    enabled bool
    reports map[string]analyze.Report
}
```

## Public API

```go
// SetPerFileMode enables or disables per-file report retention.
func (r *PerFileRetainer) SetPerFileMode(enabled bool)

// Retain extracts the source file path from the report and stores a clone.
// No-op when per-file mode is disabled.
func (r *PerFileRetainer) Retain(report analyze.Report)

// PerFileResults returns the retained per-file reports, keyed by file path.
// Returns nil when per-file mode is disabled or no files were retained.
func (r *PerFileRetainer) PerFileResults() map[string]analyze.Report
```

## File Path Extraction

The file path is extracted from the report by scanning values for:
1. `analyze.TypedCollection` with non-empty `SourceFile`
2. `[]map[string]any` items containing `_source_file` key

This reuses the stamping already done by `StampSourceFile()` in `static.go`.

## Integration Per Aggregator

Each aggregator's `Aggregate()` method adds one line:
```go
func (a *MyAggregator) Aggregate(results map[string]analyze.Report) {
    for _, report := range results {
        a.PerFileRetainer.Retain(report)  // NEW
    }
    // ... existing aggregation logic
}
```

For aggregators that embed `*common.Aggregator` and don't override `Aggregate()` (cohesion), a new override is needed.

## Test Plan

- Unit test for `PerFileRetainer` in isolation: retain 3 files, verify 3 entries.
- Disabled mode: retain calls are no-ops, `PerFileResults()` returns nil.
- Empty report / no source file key: gracefully skipped.
- Per-aggregator integration test: aggregate 3 files, verify per-file results count and keys.

## Implementation

**Status:** Complete

**Files created:**
- `internal/analyzers/common/perfile_retainer.go` — `PerFileRetainer` struct with `SetPerFileMode`, `Retain`, `PerFileResults`, `extractSourceFile`, `cloneReport`
- `internal/analyzers/common/perfile_retainer_test.go` — 6 test cases, 100% coverage

**Files modified:**
- `internal/analyzers/complexity/aggregator.go` — embedded `PerFileRetainer`, calls `Retain` in `Aggregate`
- `internal/analyzers/comments/aggregator.go` — embedded `PerFileRetainer`, calls `Retain` in `Aggregate`
- `internal/analyzers/halstead/aggregator.go` — embedded `PerFileRetainer`, calls `Retain` in `Aggregate`
- `internal/analyzers/cohesion/aggregator.go` — embedded `PerFileRetainer`, added `Aggregate` override with `Retain`
- `internal/analyzers/imports/aggregator.go` — embedded `PerFileRetainer`, calls `Retain` in `Aggregate`

**Design decisions:**
- Embeddable struct (not interface) — Go composition idiom, promoted methods work transparently.
- `maps.Clone` for shallow report clone — per `modernize` linter.
- File path extracted from `TypedCollection.SourceFile` or legacy `_source_file` items — reuses existing `StampSourceFile` mechanism.
- Zero value of `PerFileRetainer` is disabled — backward compatible with no memory overhead.

**Coverage:** 100% on `perfile_retainer.go`.
**Race detector:** Clean.
**Lint:** Clean.
