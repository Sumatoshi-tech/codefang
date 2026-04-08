# FRD: Merge ExtractFunctionName / ExtractVariableName (Roadmap 1.3)

**ID**: FRD-20260302-extract-entity-name
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 1.3
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster 7

## Problem

`ExtractFunctionName` and `ExtractVariableName` in `internal/analyzers/common/data_extraction.go` have identical logic: try props["name"], then token, then first child. Only the parameter name differs. This duplication adds confusion and maintenance burden.

## Feature

### 1.3 Merge into ExtractEntityName

- Create `ExtractEntityName` function with the shared logic
- Delete `ExtractFunctionName` and `ExtractVariableName`
- Update all call sites (15 occurrences across 10 files)
- Consolidate two redundant test functions into one `TestExtractEntityName`

## Acceptance Criteria

- [x] `ExtractEntityName` replaces both functions
- [x] All 15 call sites updated across 10 files
- [x] Redundant tests consolidated
- [x] `go vet` clean on all affected packages
- [x] `go test` passes on all affected packages
- [x] `make lint` passes (zero issues, zero dead code)

## Risk

Trivial. The two functions have identical bodies. All call sites are mechanical renames.

## Implementation

### Files Modified

- `internal/analyzers/common/data_extraction.go` — Replaced `ExtractFunctionName` + `ExtractVariableName` (~20 lines each) with single `ExtractEntityName` (~12 lines)
- `internal/analyzers/common/data_extraction_test.go` — Consolidated `TestExtractFunctionName` + `TestExtractVariableName` into `TestExtractEntityName`
- `internal/analyzers/clones/analyzer.go` — `ExtractFunctionName` → `ExtractEntityName`
- `internal/analyzers/halstead/visitor.go` — `ExtractFunctionName` → `ExtractEntityName`
- `internal/analyzers/halstead/halstead.go` — `ExtractFunctionName` → `ExtractEntityName` (2 occurrences)
- `internal/analyzers/complexity/complexity.go` — `ExtractFunctionName` → `ExtractEntityName` (3 occurrences)
- `internal/analyzers/complexity/cognitive_complexity.go` — `ExtractFunctionName` → `ExtractEntityName`
- `internal/analyzers/cohesion/visitor.go` — `ExtractFunctionName` + `ExtractVariableName` → `ExtractEntityName`
- `internal/analyzers/cohesion/types.go` — `ExtractFunctionName` + `ExtractVariableName` → `ExtractEntityName`
- `internal/analyzers/cohesion/cohesion.go` — `ExtractFunctionName` + `ExtractVariableName` → `ExtractEntityName`
- `internal/analyzers/comments/visitor.go` — `ExtractFunctionName` → `ExtractEntityName`
- `internal/analyzers/comments/types.go` — `ExtractFunctionName` → `ExtractEntityName`
- `internal/analyzers/comments/comments.go` — `ExtractFunctionName` → `ExtractEntityName`

### Lines Eliminated

~28 lines of duplicate function body + ~18 lines of duplicate tests removed.

### Verification

- `go vet` — clean
- `go test` — all pass
- `make lint` — zero issues, zero dead code
