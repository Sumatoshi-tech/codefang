# FRD: Extract data extraction guard helper (Roadmap F4.3)

**ID**: FRD-20260303-data-extraction-guard
**Roadmap**: [specs/dedup/ROADMAP.md](../dedup/ROADMAP.md) — Item F4.3
**Spec**: [specs/dedup/SPEC.md](../dedup/SPEC.md) — Cluster 8: Analyzer Report & Visualization

## Problem

4 analyzers (complexity, halstead, cohesion, comments) extract function lists
from reports using a two-key fallback pattern. Each extraction site:

1. Tries `analyze.ReportFunctionList(report, "functions")`
2. If not found, tries a fallback key (e.g., `"function_complexity"`)
3. If still not found, returns an error or empty chart

This 6-8 line guard pattern is repeated 10 times across 4 plot.go files,
totaling ~70 lines of duplicated boilerplate.

`analyze.ReportFunctionList` already exists but only tries a single key.

## Feature

Add `ReportFunctionListWithFallback(report Report, primaryKey, fallbackKey string) ([]map[string]any, bool)`
to `internal/analyzers/analyze/analyzer.go` alongside the existing `ReportFunctionList`.
Then migrate all 10 call sites across 4 analyzers.

### Design Decisions

- **Same package, same file**: Placed adjacent to `ReportFunctionList` in `analyzer.go`
  since it's a direct extension of the same function.
- **Same return signature**: Returns `([]map[string]any, bool)` to match `ReportFunctionList`.
  The caller still handles error/empty-chart responses, keeping the helper reusable.
- **No chart coupling**: The helper does not return chart types. Empty chart creation
  remains analyzer-specific (different chart types: Bar, Scatter, Pie, BoxPlot).
- **quality/plot.go excluded**: Quality uses store-reader pattern (`ReadRecordsIfPresent`),
  not `ReportFunctionList`. Not applicable.

### Migration Scope

| File | Call Sites | Status |
|------|-----------|--------|
| complexity/plot.go | 3 (bar, scatter, pie) | Migrate |
| halstead/plot.go | 3 (bar, scatter, pie) | Migrate |
| cohesion/plot.go | 3 (histogram, pie, boxplot) | Migrate |
| comments/plot.go | 1 (bar) | Migrate |
| quality/plot.go | 0 | Not applicable (store-reader pattern) |

## Acceptance Criteria

- [x] `ReportFunctionListWithFallback` exists in `analyze/analyzer.go`
- [x] Unit tests in `analyze/analyzer_test.go` (4 cases)
- [x] 10 call sites across 4 analyzers migrated
- [x] All existing tests pass
- [x] `go vet` clean, `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files modified:**
- `internal/analyzers/analyze/analyzer.go` — added `ReportFunctionListWithFallback` (7 lines)
- `internal/analyzers/analyze/analyzer_test.go` — added 4 tests: PrimaryKeyFound, NeitherKeyExists, JSONDecodedFallback, FallbackKeyUsed
- `internal/analyzers/complexity/plot.go` — 3 call sites: `generateComplexityBarChart`, `generateComplexityScatterChart`, `generateComplexityPieChart`
- `internal/analyzers/halstead/plot.go` — 3 call sites: `generateEffortBarChart`, `generateVolumeVsDifficultyChart`, `generateVolumePieChart`
- `internal/analyzers/cohesion/plot.go` — 3 call sites: `generateHistogram`, `generatePieChart`, `generateBoxPlot`
- `internal/analyzers/comments/plot.go` — 1 call site: `generateFunctionCoverageChart`
