# FRD: Migrate runParallel to WorkerPool (Roadmap 4.3)

**ID**: FRD-20260310-runparallel-workerpool
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 4.3
**Date**: 2026-03-10

## Problem

`internal/analyzers/analyze/analyzer.go` implements a semaphore+WaitGroup+error-collect
fan-out pattern with `parallelState`, `runIndependentParallel`, and `runVisitorsParallel`.
This duplicates the bounded fan-out abstraction that `WorkerPool[T]` already provides.

The independent analyzers are a classic slice fan-out: each analyzer name is an item,
each produces a report or error. `WorkerPool[string]` fits exactly.

`runVisitorsParallel` has different semantics (single task, not item fan-out) and is
inlined as a simple goroutine.

## Decision

Rewrite `runParallel` to:

1. Use `pipeline.WorkerPool[string]` for independent analyzers.
2. Run visitors in a plain goroutine (no semaphore — single task).
3. Remove `parallelState` struct entirely.
4. Remove `runIndependentParallel` and `runVisitorsParallel` methods.

### Behavior change

Current code collects ALL errors in a `[]string` slice. With `WorkerPool`, the first
error cancels remaining work and is returned. This is intentional — consistent with
`WorkerPool`'s first-error contract and avoids wasting CPU on analyzers that will be
discarded anyway.

## Contract

- Independent analyzers are processed via `WorkerPool[string]` with `MaxParallel = f.maxParallel`.
- Visitors run concurrently with the pool in a separate goroutine.
- First analyzer error cancels the pool context and is returned wrapped with `ErrAnalysisFailed`.
- All existing tests pass unchanged.

## Scope

### Files modified

| File | Change |
|------|--------|
| `internal/analyzers/analyze/analyzer.go` | Rewrite `runParallel`, delete `parallelState`, `runIndependentParallel`, `runVisitorsParallel` |

### Out of scope

- `runVisitors` (tree traversal logic — unchanged)
- `runSequentially` (unchanged)
- `categorizeAnalyzers` (unchanged)

## Acceptance Criteria

- [x] `parallelState` struct removed
- [x] `runIndependentParallel` method removed
- [x] `runVisitorsParallel` method removed
- [x] `runParallel` uses `WorkerPool[string]`
- [x] `go test ./internal/analyzers/analyze/...` passes
- [x] `make lint` passes — 0 issues, no dead code

## Implementation

### Files Modified

| File | Change |
|------|--------|
| `internal/analyzers/analyze/analyzer.go` | Rewrote `runParallel` with `WorkerPool[string]` + `wg.Go`; deleted `parallelState`, `runIndependentParallel`, `runVisitorsParallel`; removed `strings` import |
| `specs/ref/ROADMAP.md` | Mark 4.3 done |
