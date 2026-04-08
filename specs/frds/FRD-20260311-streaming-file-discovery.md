# FRD: Streaming File Discovery with Backpressure (Roadmap perf30/2.3)

**ID**: FRD-20260311-streaming-file-discovery
**Roadmap**: [specs/perf30/ROADMAP.md](../perf30/ROADMAP.md) — Step 2.3
**Date**: 2026-03-11

## Problem

`collectFiles()` in `static.go` walks the entire directory tree and returns `[]string`
before any analysis begins. On kubernetes (~25K files), this allocates a path slice upfront
while workers sit idle. More importantly, the `WorkerPool.Run` method also requires `[]T`,
meaning all paths must be buffered in memory before processing starts.

While the path slice itself is only a few MB, the batch-first design prevents future
memory-aware throttling and wastes time — workers could begin analysis while discovery
is still in progress.

## Decision

### 1. Add `RunChan` to `WorkerPool`

Add a `RunChan(ctx, ch <-chan T) error` method to `pipeline.WorkerPool` that consumes
items from a channel instead of a slice. This preserves the existing `Run(ctx, []T)`
method for backwards compatibility. The implementation mirrors `Run` but reads from
the channel instead of iterating a slice.

### 2. Stream file discovery

Change `collectFiles` to `streamFiles(ctx, rootPath, ch chan<- string) error` — a
function that walks the directory tree and sends paths to a channel. The caller creates
the channel, spawns the walker in a goroutine, and passes the channel to
`analyzeFilesParallel` which now calls `pool.RunChan(ctx, ch)`.

### Key design decisions

- **Channel-based, not `RunPC`**: `RunPC` is a producer-consumer skeleton with separate
  Produce/Consume functions. Here we already have `WorkerPool` with its bounded concurrency
  and error handling. Adding `RunChan` is simpler and more focused.
- **Walker errors via separate channel**: The walker goroutine sends its error on a
  `chan error` (buffered 1). `AnalyzeFolder` checks it after `RunChan` returns.
- **Backpressure via channel capacity**: The file path channel is buffered (100).
  When workers are busy, the walker blocks naturally on channel send.
- **Context cancellation**: If a worker error cancels the context, the walker observes
  it via `ctx.Done()` and stops walking.

## Contract

- `WorkerPool[T].RunChan(ctx, <-chan T) error` — same semantics as `Run` but reads from
  channel. Returns first non-nil error. Cancels context on error.
- `streamFiles` sends each supported file path on the channel, then closes it. Errors are
  returned from the function (caller wraps in goroutine).
- `AnalyzeFolder` behavior is identical: same results, same error handling, same analyzer
  output. Only the internal plumbing changes.
- `collectFiles` method is removed (dead code after streaming switch).

## Acceptance Criteria

- [x] `WorkerPool[T].RunChan(ctx, <-chan T) error` added to `pkg/pipeline/workerpool.go`
- [x] `streamFiles` replaces `collectFiles` in `static.go`
- [x] `analyzeFilesParallel` consumes from channel via `RunChan`
- [x] Walker errors propagated to `AnalyzeFolder` caller
- [x] All existing static tests pass unchanged
- [x] `go test ./pkg/pipeline/...` passes
- [x] `go test ./internal/analyzers/analyze/...` passes
- [x] `make lint` passes

## Implementation

**Created:**
- `specs/frds/FRD-20260311-streaming-file-discovery.md` — this FRD

**Modified:**
- `pkg/pipeline/workerpool.go` — added `RunChan(ctx, <-chan T) error` method, updated `resolveWorkers` to handle unknown item count
- `pkg/pipeline/workerpool_test.go` — 5 new tests for `RunChan` (empty channel, all items processed, first error, context cancellation, nil channel)
- `internal/analyzers/analyze/static.go` — replaced `collectFiles` with `streamFiles`, `AnalyzeFolder` uses goroutine + error channel pattern, `analyzeFilesParallel` accepts `<-chan string` and calls `pool.RunChan`
