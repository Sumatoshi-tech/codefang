# FRD: Unify UASTTraverser.FindNodes API (Roadmap 7.1)

**ID**: FRD-20260310-find-nodes-predicate
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 7.1
**Date**: 2026-03-10

## Problem

`internal/analyzers/common/uast_traversal.go` has four nearly identical find methods:

- `FindNodesByType(root, nodeTypes []string)` — 8 production callers
- `FindNodesByRoles(root, roles []string)` — 8 production callers
- `FindNodesByFilter(root, filter NodeFilter)` — test-only usage
- `FindNodesByFilters(root, filters []NodeFilter)` — test-only usage

All four methods share identical structure: nil-check root, traverse, collect
matching nodes. The only difference is the predicate applied to each node.

## Decision

Add a single predicate-based method:

```go
// FindNodes returns all nodes for which predicate returns true.
func (ut *UASTTraverser) FindNodes(root *node.Node, predicate func(*node.Node) bool) []*node.Node
```

Refactor all four existing methods to delegate to `FindNodes` internally.
This eliminates code duplication while preserving the existing public API
so no callers break.

## Contract

- `FindNodes` traverses the tree depth-first, respecting `MaxDepth` config.
- `predicate` is called for every visited node; nodes where it returns `true` are collected.
- `nil` root returns `nil`.
- Existing methods (`FindNodesByType`, `FindNodesByRoles`, `FindNodesByFilter`,
  `FindNodesByFilters`) remain unchanged in signature and behavior.
- All 16 production call sites continue to work without modification.

## Scope

### Files modified

| File | Change |
|------|-------------|
| `internal/analyzers/common/uast_traversal.go` | Add `FindNodes`; refactor 4 methods to delegate |
| `internal/analyzers/common/uast_traversal_test.go` | Add tests for `FindNodes` |

### Out of scope

- Changing any analyzer call sites.
- Deprecating or removing existing methods (they remain as convenience wrappers).
- Modifying `traverse`, `CountLines`, or `GetNodePosition`.

## Acceptance Criteria

- [x] `FindNodes` implemented with predicate-based API
- [x] `FindNodesByType`, `FindNodesByRoles`, `FindNodesByFilter`, `FindNodesByFilters` delegate to `FindNodes`
- [x] New test for `FindNodes` with custom predicate
- [x] All existing tests pass unchanged
- [x] `go test ./internal/analyzers/common/...` passes
- [x] `make lint` passes — 0 issues, no dead code

## Implementation

### Files Modified

| File | Change |
|------|--------|
| `internal/analyzers/common/uast_traversal.go` | Added `FindNodes(root, predicate)` method; refactored `FindNodesByType`, `FindNodesByRoles`, `FindNodesByFilter`, `FindNodesByFilters` to delegate to `FindNodes` |
| `internal/analyzers/common/uast_traversal_test.go` | Added `TestUASTTraverser_FindNodes` (custom predicate, match-nothing, match-all, nil root) and `TestUASTTraverser_FindNodes_RespectsMaxDepth`; extracted node type constants to satisfy goconst |
| `specs/ref/ROADMAP.md` | Marked 7.1 done, added FRD link |
