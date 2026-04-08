# FRD: WorkerPool[T] generic bounded fan-out (Roadmap 4.1)

**ID**: FRD-20260310-worker-pool
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 4.1
**Date**: 2026-03-10

## Problem

Three independent locations in the codebase implement the same
semaphore+WaitGroup+first-error fan-out pattern:

1. `cmd/uast/parse.go` — `parallelState` with `atomic.Value` + `atomic.Int64`
2. `cmd/uast/analyze.go` — `sync.Once` + `sync.WaitGroup`
3. `internal/analyzers/analyze/static.go` — `workerState` with `sync.Mutex`

All three share the same shape:
- Create N worker goroutines (typically `runtime.NumCPU()`)
- Feed items via a buffered channel
- Capture the first non-nil error
- Wait for all workers to complete
- Respect context cancellation

`pkg/pipeline.RunPC` handles producer-consumer topology. `WorkerPool[T]` fills
the missing abstraction for bounded slice fan-out.

## Decision

Add a generic struct and method to `pkg/pipeline/workerpool.go`:

```go
// WorkerPool runs Work on each item with at most MaxParallel goroutines.
// Returns the first non-nil error encountered, or nil.
type WorkerPool[T any] struct {
    MaxParallel int
    Work        func(ctx context.Context, item T) error
}

// Run processes all items. If any Work call returns an error, the context
// is cancelled and Run returns the first error after all goroutines finish.
func (p WorkerPool[T]) Run(ctx context.Context, items []T) error
```

### Key design decisions

- **MaxParallel == 0** defaults to `runtime.NumCPU()`.
- **First-error semantics**: consistent with all three existing patterns. The
  context is cancelled on first error so remaining workers can exit early.
- **Orderly shutdown**: all goroutines are awaited before returning, preventing
  goroutine leaks.
- **No result collection**: callers that need results use closure capture or
  write to a pre-allocated slice (matching existing patterns).
- **Channel-based work distribution**: items are sent via a buffered channel
  rather than index-based partitioning, enabling dynamic load balancing.

## Contract

- `Run` spawns `min(MaxParallel, len(items))` goroutines.
- Each item is processed exactly once.
- If `Work` returns a non-nil error, `Run` cancels the derived context and
  returns that error after all goroutines complete.
- If multiple `Work` calls error, only the first error is returned.
- If the input `ctx` is already cancelled, `Run` returns `ctx.Err()`.
- `Run(ctx, nil)` and `Run(ctx, []T{})` return nil immediately.
- `Work` must not be nil (caller responsibility; panic is acceptable).

## Scope

### Files created

| File | Description |
|------|-------------|
| `pkg/pipeline/workerpool.go` | `WorkerPool[T]` implementation |
| `pkg/pipeline/workerpool_test.go` | Unit tests |

### Out of scope

- Result collection (handled by closures or pre-allocated slices)
- Ordered output (use `RunPC` for that)
- Retry logic
- Migration of callers (steps 4.2–4.4)

## Acceptance Criteria

- [x] `WorkerPool[T]` struct and `Run` method implemented
- [x] Tests: serial, parallel, first-error, context cancellation, empty items, default MaxParallel, capped workers, all-items-processed, error-cancels-context
- [x] `go test -race ./pkg/pipeline/...` passes
- [x] `make lint` passes — 0 issues, no dead code (whitelisted pending callers)

## Implementation

### Files Created

| File | Description |
|------|-------------|
| `pkg/pipeline/workerpool.go` | `WorkerPool[T]` struct + `Run` method + `resolveWorkers` helper |
| `pkg/pipeline/workerpool_test.go` | 8 unit tests covering all contracts |

### Files Modified

| File | Change |
|------|--------|
| `specs/ref/ROADMAP.md` | Mark 4.1 done |
| `internal/analyzers/analyze/static.go` | First caller (Step 4.2): `analyzeFilesParallel` uses `WorkerPool[string]` |
