# FRD: Data Extraction Cleanup (Roadmap 2.3)

**ID**: FRD-20260302-data-extraction-cleanup
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 2.3
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Clusters 7, 14

## Problem

`internal/analyzers/common/data_extraction.go` has two forms of duplication:

1. **Method/function duplication**: `ExtractNameFromProps`, `ExtractNameFromToken`, `ExtractNameFromChildren` exist as both DataExtractor methods (lines 58-99) and standalone package-level functions (lines 184-228). The bodies are identical — neither form uses DataExtractor state.

2. **Merge function duplication**: `mergeNameExtractors` and `mergeValueExtractors` (lines 230-256) have identical structure, differing only in map value type (`NameExtractor` vs `ValueExtractor`).

## Feature

### 2.3.a Make DataExtractor Methods Delegate to Standalone Functions

The DataExtractor methods don't use any receiver state — they only operate on `*node.Node`. Make the methods delegate to the standalone functions, eliminating ~25 lines of duplicate logic while preserving both APIs.

### 2.3.b+c Create Generic mergeExtractors and Replace Both Typed Versions

Create `func mergeExtractors[V any](custom, defaults map[string]V) map[string]V` and replace both `mergeNameExtractors` and `mergeValueExtractors`.

## Acceptance Criteria

- [x] DataExtractor methods delegate to standalone functions (no duplicate bodies)
- [x] Generic `mergeExtractors[V any]` created
- [x] `mergeNameExtractors` and `mergeValueExtractors` deleted
- [x] All call sites updated
- [x] Tests updated (merge tests use generic function)
- [x] `go vet` clean
- [x] `go test ./internal/analyzers/common/...` passes
- [x] `make lint` passes (zero issues, zero dead code)

## Risk

Trivial. Methods are pure functions that don't use receiver state. Generic merge is a direct type parameterization of identical code.

## Implementation

### Files Modified

- `internal/analyzers/common/data_extraction.go` — Method delegation + generic merge
- `internal/analyzers/common/data_extraction_test.go` — Updated merge tests

### Lines Eliminated

~40 lines of duplicate method bodies + merge functions removed.

### Verification

- `go vet` — clean
- `go test` — all pass
- `make lint` — zero issues, zero dead code
