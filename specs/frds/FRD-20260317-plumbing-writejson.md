# FRD: Replace plumbing Serialize with textutil.WriteJSON (Phase 2.1)

**ID**: FRD-20260317-plumbing-writejson
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Phase 2.1
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Section 2 Plumbing JSON Serialization

## Problem

All 8 plumbing analyzers use `json.NewEncoder(writer).Encode(report)` in Serialize. This duplicates logic that textutil.WriteJSON provides with consistent formatting (pretty-print support).

## Goal

Replace inline JSON encoding with `textutil.WriteJSON(writer, report, true)` for consistent, pretty-printed output.

## In Scope

- blob_cache.go, file_diff.go, identity.go, line_stats.go, languages.go, ticks.go, uast.go, tree_diff.go
- Replace json.NewEncoder(writer).Encode(report) with textutil.WriteJSON(writer, report, true)
- Remove unused encoding/json imports where applicable

## Out of Scope

- Changing format handling (still check format == analyze.FormatJSON)
- Other analyzer Serialize methods (non-plumbing)

## Acceptance Criteria

- [x] All 8 plumbing Serialize methods use textutil.WriteJSON
- [x] `go test ./internal/analyzers/plumbing/...` passes
- [x] `make lint` passes
- [x] Serialized output semantically equivalent (valid JSON; pretty-printed)

## Implementation

- Modified: internal/analyzers/plumbing/blob_cache.go, file_diff.go, identity.go, line_stats.go, languages.go, ticks.go, uast.go, tree_diff.go
