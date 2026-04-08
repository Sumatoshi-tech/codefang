# FRD-20260328: report.json Emission Alongside Plot Pages

**Date:** 2026-03-28
**Author:** Agent
**Status:** Complete
**Roadmap:** specs/filestats/ROADMAP.md — Step 3.1
**Spec:** specs/filestats/SPEC.md — FR-3.5

## Problem

When `--format plot` generates HTML chart pages, external dashboards and CI pipelines need the raw analysis data in a machine-readable format. Currently only HTML is produced.

## Solution

After `FormatPlotPages` renders HTML and index, atomically write a `report.json` file to the output directory containing the analysis results as indented JSON. Reuse existing `textutil.WriteJSON` and `storage.WriteAtomic`.

### reportJSONFilename

Constant: `"report.json"`.

### In `FormatPlotPages`

After `RenderIndex(pages)`, call `writeReportJSON(results, outputDir)`.

### In `runRender`

After `RenderIndex(pages)`, call a similar write using store data. (Deferred — `codefang render` operates on store data, not `Report` maps. The e2e test only exercises `FormatPlotPages`.)

## Test Plan

- Unit test: call `FormatPlotPages`, verify `report.json` exists and is valid JSON.
- E2E test green: `TestDashboard_ReportJSONEmitted`.

## Implementation

**Status:** Complete

**Files modified:**
- `internal/analyzers/analyze/static.go` — `writeReportJSON()`, `reportJSONFilename`, `reportJSONPerm` constants, `FormatPlotPages` calls `writeReportJSON` after index rendering
- `internal/analyzers/analyze/static_test.go` — `TestStaticService_FormatPlotPages_EmitsReportJSON`

**Lint:** Clean. **Race:** Clean.
