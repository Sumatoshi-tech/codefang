# FRD: Eager UAST Tree Release After Analysis (Roadmap perf30/3.1)

**ID**: FRD-20260311-eager-tree-release
**Roadmap**: [specs/perf30/ROADMAP.md](../perf30/ROADMAP.md) — Step 3.1
**Date**: 2026-03-11

## Problem

In `analyzeFile()`, the `*node.Node` tree returned by `parser.Parse()` is passed to
`runAnalyzers()`. After analyzers produce their reports, the tree is no longer needed.
However, it lingers on the Go heap until the next GC cycle collects unreachable nodes.

With `MaxWorkers=8` concurrent parsers, up to 8 UAST trees can be alive simultaneously.
Each tree for a moderately complex file can consume 100KB–1MB of Go heap. On kubernetes
(~25K files), this contributes to sustained high heap pressure.

## Decision

After `runAnalyzers()` returns in `analyzeFile`, call `node.ReleaseTree(uastNode)` to
immediately return all Go-side nodes and positions to `sync.Pool` for reuse by
subsequent parses.

### Key findings from investigation

- **Tree-sitter native tree already released**: `DSLParser.Parse()` calls
  `defer tree.Close()` after converting to Go nodes. The C-side `ts_tree_delete` is
  already invoked within the `Parse()` call.
- **`node.ReleaseTree(root)` already exists**: Iteratively walks the Go-side node tree
  and returns each `*Node` and `*Positions` to the global `sync.Pool` (node.go:317).
- **`Allocator.ReleaseTree(root)` also exists**: Returns nodes to per-worker free lists
  (allocator.go:87). However, the allocator is inside the pooled `parseContext` which
  is already returned by the time `analyzeFile` runs analyzers.
- **No new `parser.ReleaseTree()` method needed**: The existing `node.ReleaseTree` is
  sufficient. The `node` package is already imported in `static.go`.

## Contract

- After `runAnalyzers()` returns in `analyzeFile`, `node.ReleaseTree(uastNode)` is called.
- The UAST tree must not be referenced after `ReleaseTree` — it is invalidated.
- Analyzer reports (which contain extracted metric data, not node references) remain valid.
- All existing tests and output continue to work unchanged.

## Benchmark Results

`BenchmarkParserTreeRelease` — 80 synthetic Go files (50 functions × 20 lines each):

| Variant | heap-delta-MiB | Reduction |
|---------|---------------:|-----------|
| before-no-release | 96.2 | — |
| after-with-release | 21.3 | **78% (4.5x)** |

## Acceptance Criteria

- [x] `node.ReleaseTree(uastNode)` called in `analyzeFile` after `runAnalyzers`
- [x] `BenchmarkParserTreeRelease` shows heap reduction from eager release (96.2 → 21.3 MiB, 4.5x)
- [x] `go test ./pkg/uast/...` passes
- [x] `go test ./internal/analyzers/analyze/...` passes
- [x] `make lint` passes

## Implementation

**Files modified:**
- `internal/analyzers/analyze/static.go` — added `node.ReleaseTree(uastNode)` call after `runAnalyzers()` in `analyzeFile`

**Files created:**
- `pkg/uast/parser_bench_test.go` — `BenchmarkParserTreeRelease` benchmark (before-no-release vs after-with-release)
- `specs/frds/FRD-20260311-eager-tree-release.md` — this FRD

**Traceability:**
- `internal/analyzers/analyze/static_bench_test.go` — FRD link comment
- `pkg/uast/parser_bench_test.go` — FRD link comment
