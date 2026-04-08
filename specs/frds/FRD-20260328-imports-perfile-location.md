# FRD-20260328: IMPORTS Per-File Issue Location

**Date:** 2026-03-28
**Author:** Agent
**Status:** Complete
**Roadmap:** specs/filestats/ROADMAP.md — Step 1.7
**Spec:** specs/filestats/SPEC.md — FR-1.5

## Problem

IMPORTS per-file entries have issues with empty `location` field. The spec requires `location` to be set to the source `file_path` for info-only analyzers.

## Solution

In `NewReportSection`, extract `_source_file` from the report. Pass it to `importIssues` which sets `Location` on each issue.

## Implementation

**Status:** Complete

**Files modified:**
- `internal/analyzers/imports/report_section.go` — `importIssues` extracts `_source_file` via `reportutil.GetString`, passes as `location` to `buildIssuesFromCounts` and `buildIssuesFromList`
- `internal/analyzers/imports/report_section_test.go` — 2 new tests

**Lint:** Clean. **Race:** Clean.
