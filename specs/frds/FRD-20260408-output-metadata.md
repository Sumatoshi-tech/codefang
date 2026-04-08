# FRD-20260408: Top-level metadata section in JSON output

## Roadmap Link
- Source roadmap: specs/analytics-readiness/roadmap.md
- Feature: Feature 6 — Top-level metadata section

## Problem

The JSON output has no provenance. A DWH ingesting reports from multiple repos cannot distinguish them. No repo name, analysis timestamp, or codefang version is present.

## Goal

Add a `metadata` section to the `UnifiedModel` JSON envelope with repo path, repo name, analysis timestamp, and codefang version.

## Functional Requirements

### MUST
- `AnalysisMetadata` struct with: `RepoPath`, `RepoName`, `AnalyzedAt` (RFC 3339), `CodefangVersion`
- `UnifiedModel.Metadata *AnalysisMetadata` field (json:"metadata,omitempty")
- `NewAnalysisMetadata(repoPath string)` constructor that populates all fields
- Injected after `DecodeCombinedBinaryReports` in the combined render path

### SHOULD
- `RepoName` derived as `filepath.Base(repoPath)`

## Implementation

### Files created
- `internal/analyzers/analyze/metadata.go` — `AnalysisMetadata` struct, `NewAnalysisMetadata` constructor
- `internal/analyzers/analyze/metadata_test.go` — 5 test cases

### Files modified
- `internal/analyzers/analyze/conversion.go` — added `Metadata *AnalysisMetadata` to `UnifiedModel`
- `cmd/codefang/commands/run.go` — `model.Metadata = analyze.NewAnalysisMetadata(path)` after decode

## Affected Files
- `internal/analyzers/analyze/conversion.go` — `UnifiedModel`, `AnalysisMetadata`
- `cmd/codefang/commands/run.go` — inject metadata after decode
