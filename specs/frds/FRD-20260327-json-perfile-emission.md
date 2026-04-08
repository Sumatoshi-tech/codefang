# FRD-20260327: JSON Per-File Emission

**Date:** 2026-03-27
**Author:** Agent
**Status:** Complete
**Roadmap:** specs/filestats/ROADMAP.md — Step 1.5
**Spec:** specs/filestats/SPEC.md — Feature 1

## Problem

Steps 1.1-1.4 built all foundation: stats, JSON types, per-file retention, orchestration. Now `FormatJSON()` must actually populate `JSONSection.Files` and `JSONSection.SummaryStats` when `PerFile` is true.

## Solution

### Approach: Post-process in `FormatJSON`

`FormatJSON()` already has access to `svc.PerFile`, `svc.PerFileResults()`, `svc.BuildPerFileSections()`, and `svc.ComputeSummaryStats()`. After the existing `SectionsToJSON()` call produces the `JSONReport`, inject per-file data into each section.

### New function in renderer: `SectionToJSONFileEntry`

Convert a `ReportSection` + `filePath` into a `JSONFileEntry`. Reuses the same metric/distribution/issue conversion as `SectionToJSON`.

### New function in renderer: `InjectPerFileData`

Takes a `JSONReport`, per-file sections by analyzer, and summary stats. For each `JSONSection`, looks up the matching per-file sections and stats, converts them to `JSONFileEntry` and `stats.Summary`, and injects.

### File path handling

Per-file sections carry absolute paths from `StampSourceFile`. The `file_path` in JSON must be relative to the analysis root. `FormatJSON` doesn't know the root path, but `AnalyzeFolder` does. We need to store the root path on `StaticService` during analysis.

## Test Plan

- Unit test: `SectionToJSONFileEntry` produces correct shape.
- Unit test: `InjectPerFileData` populates `Files` and `SummaryStats` on sections.
- Integration test: `FormatJSON` with `PerFile=true` produces JSON with `files` and `summary_stats`.
- E2E tests should go green.

## Implementation

**Status:** Complete

**Files modified:**
- `internal/analyzers/analyze/static.go` — `analysisRootPath`, `FormatJSON` enrichment, `StampSourceFile` top-level stamp
- `internal/analyzers/analyze/perfile.go` — `enrichWithPerFileData`, `PerFileEnricher`, `MakeRelativePath`, `parseNumericMetricValue`
- `internal/analyzers/common/renderer/json.go` — `EnrichWithPerFileData`, `SectionToJSONFileEntry`
- `internal/analyzers/common/renderer/static_renderer.go` — pointer return for enrichment
- `internal/analyzers/common/perfile_retainer.go` — top-level key extraction
- `tests/e2e/helpers_test.go` — `newPerFileStaticService()`

**Key design decisions:**
- `PerFileEnricher` interface avoids import cycle between analyze↔renderer
- `StampSourceFile` stamps `_source_file` at report top level (not just in collections) — enables retention for all analyzers including imports
- `parseNumericMetricValue` strips `%` suffix for percentage metrics

**E2E scorecard:** 10 PASS / 2 FAIL (EmptyDir → step 1.6, ImportsInfoOnly → step 1.7)
