# FRD: TraverseTree[T] in pkg/alg (Roadmap 9.1)

**ID**: FRD-20260310-traverse-tree
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 9.1
**Date**: 2026-03-10

## Problem

`UASTTraverser.traverse` in `internal/analyzers/common/uast_traversal.go` performs
a recursive pre-order DFS over `*node.Node`. The algorithm itself is not
UAST-specific — it only accesses `.Children` and calls a visitor with depth.
Making it generic enables reuse for any tree-shaped data (config trees, AST
variants, file system trees) without importing UAST types.

## Decision

Add a single generic function to `pkg/alg`:

```go
// TraverseTree performs an iterative pre-order DFS over a tree.
// children returns the children of a node; visit is called for each node
// with its depth. An empty children slice terminates the branch.
func TraverseTree[T any](root T, children func(T) []T, visit func(node T, depth int))
```

Implementation uses an explicit stack (not recursion) to avoid stack overflow
on deep trees.

Rewrite `UASTTraverser.FindNodes` to delegate to `TraverseTree`, inlining the
old `traverse` method's logic. The `traverse` private method is removed.

`MultiAnalyzerTraverser` is reviewed but NOT rewritten — it uses pre+post-order
callbacks (`OnEnter`/`OnExit`), which is a fundamentally different pattern that
`TraverseTree` (pre-order only) cannot serve.

## Contract

- Iterative pre-order DFS using explicit stack.
- `visit` is called for every reachable node, root first, children left-to-right.
- `children` returning nil or empty terminates the branch.
- Depth of root is 0.
- Zero allocation for leaf nodes (no children pushed).
- No goroutines.

## Scope

### Files created

| File | Content |
|------|---------|
| `pkg/alg/tree.go` | `TraverseTree[T any]` |
| `pkg/alg/tree_test.go` | Table-driven tests |

### Files modified

| File | Change |
|------|--------|
| `internal/analyzers/common/uast_traversal.go` | `FindNodes` calls `TraverseTree`; `traverse` method removed |

### Out of scope

- Post-order traversal (needed by `MultiAnalyzerTraverser` — different pattern).
- Changing `Node.VisitPreOrder`, `Node.Find`, or any `pkg/uast/pkg/node` methods.
- Adding `TraverseTreePostOrder` — can be added later if needed.

## Acceptance Criteria

- [x] `TraverseTree` added to `pkg/alg/tree.go` with tests
- [x] `UASTTraverser.traverse` removed; `FindNodes` uses `TraverseTree`
- [x] `MultiAnalyzerTraverser` reviewed (no rewrite needed — pre+post order)
- [x] `go test ./pkg/alg/...` passes
- [x] `go test ./internal/analyzers/common/...` passes
- [x] `make lint` passes — 0 issues, no dead code

## Implementation

### Files Created

| File | Content |
|------|---------|
| `pkg/alg/tree.go` | `TraverseTree[T any]` — iterative pre-order DFS with explicit stack |
| `pkg/alg/tree_test.go` | 8 tests: single node, pre-order, depth tracking, nil/empty children, depth control, value types, wide tree |

### Files Modified

| File | Change |
|------|--------|
| `internal/analyzers/common/uast_traversal.go` | `FindNodes` calls `alg.TraverseTree`; `traverse` method removed |
| `internal/analyzers/common/uast_traversal_test.go` | Removed `TestUASTTraverser_traverse_StopVisiting` (tested removed method) |
| `specs/ref/ROADMAP.md` | Marked 9.1 done, added FRD link |
