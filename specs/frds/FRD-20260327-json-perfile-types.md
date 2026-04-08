# FRD-20260327: JSON Per-File Output Types

**Date:** 2026-03-27
**Author:** Agent
**Status:** Complete
**Roadmap:** specs/filestats/ROADMAP.md — Step 1.2
**Spec:** specs/filestats/SPEC.md — Feature 1

## Problem

The `JSONSection` struct in `internal/analyzers/common/renderer/json.go` has no fields for per-file breakdowns or summary statistics. Feature 1 requires each section to optionally contain a `files` array and a `summary_stats` map when `--per-file` is active. These types must be added to the renderer package without changing the default output shape.

## Solution

Add two new types and two new optional fields to `JSONSection`:

1. **`JSONFileEntry`** — represents one file's analysis results within a section.
2. **Reuse `stats.Summary`** from `internal/analyzers/common/stats/` — already has the correct JSON tags (`min`, `p25`, `p50`, `p75`, `p95`, `max`, `avg`). No duplicate type needed.
3. **`JSONSection.Files`** — `[]JSONFileEntry` with `json:"files,omitempty"`.
4. **`JSONSection.SummaryStats`** — `map[string]stats.Summary` with `json:"summary_stats,omitempty"`.

Both use `omitempty` so they are absent from JSON when nil/empty — preserving backward compatibility.

## Type Definitions

```go
// JSONFileEntry represents one file's analysis results within a section.
type JSONFileEntry struct {
    FilePath     string             `json:"file_path"`
    ScoreLabel   string             `json:"score_label"`
    Status       string             `json:"status"`
    Metrics      []JSONMetric       `json:"metrics"`
    Distribution []JSONDistribution `json:"distribution,omitempty"`
    Issues       []JSONIssue        `json:"issues"`
    Score        float64            `json:"score"`
}
```

## Backward Compatibility

- `omitempty` on both new fields ensures zero-value (nil slice, nil map) produces no JSON keys.
- Existing `SectionsToJSON()` and `SectionToJSON()` do not populate these fields — output unchanged.
- All existing tests must continue to pass without modification.

## Test Plan

- **Backward compat:** Marshal a `JSONSection` with no `Files`/`SummaryStats` set, verify JSON has no `files` or `summary_stats` keys.
- **With files:** Marshal a `JSONSection` with populated `Files`, verify `files` array in JSON with correct shape.
- **With summary_stats:** Marshal a `JSONSection` with populated `SummaryStats`, verify `summary_stats` map in JSON.
- **Round-trip:** Unmarshal JSON with `files` and `summary_stats` back into `JSONSection`, verify fields.

## Implementation

**Status:** Complete

**Files modified:**
- `internal/analyzers/common/renderer/json.go` — added `JSONFileEntry` type, added `Files` and `SummaryStats` fields to `JSONSection` with `omitempty`
- `internal/analyzers/common/renderer/json_test.go` — 4 new tests: omission, files inclusion, summary_stats inclusion, round-trip

**Design decision:** Reused `stats.Summary` from `internal/analyzers/common/stats/` instead of creating a duplicate `StatsSummary` type. This avoids duplication and ensures the JSON output shape matches the computation layer.

**Coverage:** 85.5% (package), 100% (new code paths).
**Race detector:** Clean.
**Lint:** Clean.
