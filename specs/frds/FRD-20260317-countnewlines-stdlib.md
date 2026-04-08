# FRD: Replace countNewlines with bytes.Count (Phase 1.1)

**ID**: FRD-20260317-countnewlines-stdlib
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Phase 1.1
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Section 1 Stdlib Replacements

## Problem

`countNewlines` in internal/analyzers/couples duplicates stdlib functionality. `bytes.Count(p, []byte{'\n'})` provides identical semantics with better performance (optimized implementation).

## Goal

Remove `countNewlines` and use `bytes.Count` at the single call site in `countFileLinesAt` (aggregator.go).

## In Scope

- Replace `countNewlines(buf[:n])` with `bytes.Count(buf[:n], []byte{'\n'})` in aggregator.go
- Remove `countNewlines` from history.go
- Add `bytes` import to aggregator.go

## Out of Scope

- Other stdlib replacements (joinTypes, stats.Min/Max, etc.)
- Changing countFileLinesAt logic

## Acceptance Criteria

- [ ] countNewlines removed from history.go
- [ ] countFileLinesAt uses bytes.Count
- [ ] `go test ./internal/analyzers/couples/...` passes
- [ ] `make lint` passes
- [ ] Line counts in couples report unchanged (behavioral equivalence)

## Implementation

- Modified: internal/analyzers/couples/aggregator.go — added bytes import, replaced countNewlines with bytes.Count
- Modified: internal/analyzers/couples/history.go — removed countNewlines function
