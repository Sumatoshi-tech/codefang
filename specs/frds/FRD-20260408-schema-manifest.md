# FRD-20260408: Schema manifest in output

## Roadmap Link
- Source roadmap: specs/analytics-readiness/roadmap.md
- Feature: Feature 11

## Problem
DWH ingestion requires knowing the output schema for ETL. Currently consumers must reverse-engineer it from sample data.

## Design Decision
- **Format**: Custom lightweight schema — `map[string]FieldMeta` per analyzer
- **Location**: New `Schema` field on `AnalyzerResult` (json:"schema,omitempty")
- **Population**: Static registry in `internal/analyzers/analyze/schema_registry.go`
- **FieldMeta**: `{Type string, Grain string, Description string}`
  - Type: "list", "aggregate", "time_series", "risk", "scalar"
  - Grain: "function", "file", "tick", "pair", "developer", "" (for aggregates/scalars)
  - Description: one-line human-readable

## Approach
1. Define `FieldMeta` struct and `AnalyzerSchema` type alias
2. Build static registry covering all analyzers
3. Add `Schema AnalyzerSchema` to `AnalyzerResult`
4. Populate in `DecodeCombinedBinaryReports` or after

## Affected Files
- `internal/analyzers/analyze/conversion.go` — `AnalyzerResult` gains `Schema` field
- `internal/analyzers/analyze/schema_registry.go` — new file with registry
