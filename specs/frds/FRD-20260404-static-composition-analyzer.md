# FRD-20260404: Static Composition Analyzer

**Date:** 2026-04-04
**Author:** Agent
**Status:** In Progress

## Problem

File composition analysis (source/vendor/generated/docs/config/binary/image classification via enry) is only available through the `history/file-history` analyzer, which requires full Git commit traversal. Users need a quick static snapshot of file composition without scanning history. Additionally, the current `StaticAnalyzer` interface requires a UAST node, but composition analysis needs raw file paths and content for enry classification, not parsed ASTs.

## Solution

### 1. `ContentAnalyzer` Interface

Add a new `ContentAnalyzer` interface in `internal/analyzers/analyze/analyzer.go` for analyzers that operate on raw file content instead of UAST nodes. This interface mirrors `StaticAnalyzer` but replaces `Analyze(*node.Node)` with `AnalyzeContent(path string, content []byte)`.

### 2. `StaticService` Extension

Extend `StaticService` with:
- `ContentAnalyzers []ContentAnalyzer` field.
- `streamAllFiles()` method that walks ALL files (not just UAST-supported ones), skipping only `.git` directories.
- `analyzeContentParallel()` method for concurrent content analysis.
- Content read limited to first 8KB (enry only needs a prefix for binary detection).
- Concurrent execution: UAST walk and content walk run in parallel.
- Results merged into a single output map.

### 3. `static/composition` Analyzer

New analyzer at `internal/analyzers/composition/` implementing `ContentAnalyzer`. Reuses `filehistory.Classifier`, `filehistory.Category`, `filehistory.AllCategories`, and `filehistory.CategoryCounts` directly.

**Report metrics:** Total Files, Source Files, Source %.
**Distribution:** One item per category with percent and count.
**Score:** Info-only (-1), composition is informational.
**Issues:** Non-source files grouped by category.

### 4. Registration

Register via `defaultContentAnalyzers()` in `cmd/codefang/commands/run.go`. Include in registry for `--list-analyzers`.

## Test Plan

### Composition Analyzer Tests
- `TestAnalyzer_Name` - name is "composition".
- `TestAnalyzer_Flag` - flag is "composition".
- `TestAnalyzer_AnalyzeContent_GoFile` - classifies `.go` file as source.
- `TestAnalyzer_AnalyzeContent_VendorPath` - classifies vendor path correctly.
- `TestAnalyzer_AnalyzeContent_BinaryContent` - classifies binary content.
- `TestAnalyzer_AnalyzeContent_Markdown` - classifies `.md` as documentation.
- `TestAnalyzer_AnalyzeContent_ConfigFile` - classifies config files.

### Aggregator Tests
- `TestAggregator_SingleFile` - single file aggregation.
- `TestAggregator_MultipleFiles` - multi-file breakdown and percentages.
- `TestAggregator_EmptyResult` - no files produces empty report.

### Report Section Tests
- `TestCompositionSection_Title` - title is "COMPOSITION".
- `TestCompositionSection_Score_InfoOnly` - score is -1.
- `TestCompositionSection_KeyMetrics` - 3 metrics present.
- `TestCompositionSection_Distribution` - category distribution items.
- `TestCompositionSection_Issues` - non-source files listed.
- `TestCompositionSection_ImplementsInterface` - interface compliance.

### StaticService Integration Tests
- `TestStaticService_ContentAnalyzers_Registered` - content analyzers field works.
- `TestStaticService_StreamAllFiles_IncludesNonSource` - walks all files.
- `TestStaticService_AnalyzeFolder_MergesContentResults` - content results in output.

## Implementation

**Status:** Complete

**Files created:**
- `internal/analyzers/composition/analyzer.go` - ContentAnalyzer implementation.
- `internal/analyzers/composition/aggregator.go` - Category count aggregation.
- `internal/analyzers/composition/report_section.go` - Report section with metrics/distribution.
- `internal/analyzers/composition/analyzer_test.go` - Analyzer and aggregator tests.
- `internal/analyzers/composition/report_section_test.go` - Report section tests.

**Files modified:**
- `internal/analyzers/analyze/analyzer.go` - `ContentAnalyzer` interface.
- `internal/analyzers/analyze/static.go` - `ContentAnalyzers` field, `streamAllFiles`, content pipeline.
- `cmd/codefang/commands/run.go` - `defaultContentAnalyzers()`, registry integration.
- `AGENTS.md` - Document new package.
