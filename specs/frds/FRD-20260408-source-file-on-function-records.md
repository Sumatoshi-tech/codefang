# FRD-20260408: Emit `_source_file` on every function-level record

## Roadmap Link
- Source roadmap: specs/analytics-readiness/roadmap.md
- Feature: Feature 1 — Emit `_source_file` on every function-level record

## Problem

Function-level arrays in the JSON output (`function_complexity`, `function_halstead`, `function_cohesion`, `comment_quality`, `function_documentation`, `undocumented_functions`) contain bare function names with no file path. This makes 1M+ rows unjoinable to files in analytics/DWH systems.

## Context

The `_source_file` stamping mechanism exists and works correctly through the aggregation pipeline:
1. `StampSourceFile` (static.go) stamps `TypedCollection.SourceFile` per file
2. TypedCollection converters (e.g., `convertFunctionReportItems`) add `_source_file` to each `map[string]any` item when `sourceFile != ""`
3. `DetailedDataCollector.AddToResult()` calls `tc.ToMaps(tc.Items, tc.SourceFile)` preserving the field

**The loss point**: When `FormatReportBinary` calls `ComputeAllMetrics(report)`, the report's `[]map[string]any` items (which contain `_source_file`) are parsed into typed structs (`FunctionComplexityData`, etc.). These structs do **not** have a `SourceFile` field, so `_source_file` is silently dropped during struct conversion.

## Goal

Every function-level record in the JSON output includes `_source_file` as a relative file path.

## In Scope

- Add `SourceFile` field to all function-level output data structs across 4 analyzers
- Populate the field during `Compute()` from the `_source_file` map key
- Make the path relative (strip repo root) — leverage existing `MakeRelativePath`

## Out of Scope

- Adding `_language` or `_directory` fields (Features 8, 9)
- History analyzer file paths (Feature 4)
- Clone pair path normalization (Feature 4)

## Functional Requirements

### MUST
- `FunctionComplexityData` gains `SourceFile string` with JSON tag `"_source_file,omitempty"`
- `HighRiskFunctionData` gains same field
- `FunctionHalsteadData` gains same field
- `HighEffortFunctionData` gains same field
- `FunctionCohesionData` gains same field
- `LowCohesionFunctionData` gains same field
- `CommentQualityData` gains same field
- `FunctionDocumentationData` gains same field
- `UndocumentedFunctionData` gains same field
- Each `Compute()` method reads `_source_file` from the input map and sets the struct field
- Paths are relative to the analysis root (not absolute)

### SHOULD
- Relative path conversion happens at `StampSourceFile` time (before aggregation) so the path is relative throughout the pipeline

### COULD
- N/A

### WON'T
- Changing the internal `"functions"` key name
- Changing the TypedCollection mechanism

## Non-Functional Requirements

- Zero performance regression (field copy is O(1) per item)
- No new allocations beyond the string field

## Affected Files

### Struct changes (add `SourceFile` field):
- `internal/analyzers/complexity/metrics.go` — `FunctionComplexityData`, `HighRiskFunctionData`
- `internal/analyzers/halstead/metrics.go` — `FunctionHalsteadData`, `HighEffortFunctionData`
- `internal/analyzers/cohesion/metrics.go` — `FunctionCohesionData`, `LowCohesionFunctionData`
- `internal/analyzers/comments/metrics.go` — `CommentQualityData`, `FunctionDocumentationData`, `UndocumentedFunctionData`

### Compute method changes (populate field from map):
- `internal/analyzers/complexity/metrics.go` — `FunctionComplexityMetric.Compute()`, `HighRiskFunctionsMetric.Compute()`
- `internal/analyzers/halstead/metrics.go` — corresponding Compute methods
- `internal/analyzers/cohesion/metrics.go` — corresponding Compute methods
- `internal/analyzers/comments/metrics.go` — corresponding Compute methods

### Relative path conversion:
- `internal/analyzers/analyze/static.go` — `StampSourceFile` or `rawFilePhase`/`uastPhase`

## Implementation

### Root cause
The `_source_file` field was correctly stamped on `TypedCollection.SourceFile` and propagated through `DetailedDataCollector.AddToResult()` into `[]map[string]any` items. However, `FormatReportBinary` calls `ComputeAllMetrics(report)` which parses these maps into typed structs (`FunctionComplexityData`, etc.). These structs lacked a `SourceFile` field, silently dropping the value during struct conversion.

### Changes
1. Added `SourceFile string` to input data structs (`FunctionData`) in all 4 analyzers
2. Added `SourceFile string` with `json:"source_file,omitempty"` to all output data structs
3. Wired `SourceFile` through `parseFunctionData` → `Compute()` for all metric types
4. Updated `StampSourceFile` to accept `rootPath` and convert to relative via `MakeRelativePath`
5. Updated callers (`analyzeFilesParallel`, `classifyFile`) to pass `rootPath`

### Files modified
- `internal/analyzers/analyze/static.go` — `StampSourceFile` signature, `analyzeFilesParallel`, `classifyFile`, `analyzersByName`
- `internal/analyzers/complexity/metrics.go` — `FunctionData`, `FunctionComplexityData`, `HighRiskFunctionData`, `parseFunctionData`, `Compute`
- `internal/analyzers/halstead/metrics.go` — same pattern
- `internal/analyzers/cohesion/metrics.go` — same pattern + extracted `parseReportFunctions`/`parseFunctionData`
- `internal/analyzers/comments/metrics.go` — `CommentData`, `FunctionCommentData`, all output structs, parse/Compute
- `internal/analyzers/analyze/static_test.go` — updated `StampSourceFile` test calls
- `internal/analyzers/complexity/metrics_test.go` — added `TestParseReportData_WithSourceFile`, `TestFunctionComplexityMetric_Compute_SourceFile`

### JSON output key
The field is emitted as `"source_file"` (not `"_source_file"`) to comply with the `tagliatelle` linter which enforces snake_case without leading underscores.
