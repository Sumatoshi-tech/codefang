# FRD: Migrate cmd/uast parallel processing to WorkerPool (Roadmap 4.4)

**ID**: FRD-20260310-cmd-uast-workerpool
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 4.4
**Date**: 2026-03-10

## Problem

`cmd/uast/parse.go` and `cmd/uast/analyze.go` each implement their own
semaphore+WaitGroup+first-error fan-out pattern. These are the remaining two
occurrences of the pattern that `WorkerPool[T]` was designed to eliminate.

### parse.go

`parallelState` struct with `atomic.Value` (firstErr) + `atomic.Int64` (completed)
+ `worker` method. Each worker creates its own `uast.Parser` to avoid contention.
Progress tracking via atomic counter.

### analyze.go

`runAnalyzeParallel` with `sync.WaitGroup.Go` + `sync.Once` + `firstErr`.
Shares a single parser (thread-safe). Writes to pre-allocated `[]analysisResult`
by index.

## Decision

### parse.go

- Replace `parallelState` struct and `worker` method with `WorkerPool[string]`.
- Use `sync.Pool` for per-goroutine parser reuse (preserving the current
  "one parser per worker" optimization for the high-throughput parse-only path).
- Keep progress tracking via `atomic.Int64` in the Work closure.

### analyze.go

- Replace manual WaitGroup/Once with `WorkerPool[indexedFile]`.
- Pre-allocate results slice and index-remap items before `pool.Run`.
- Remove `sync` and `runtime` imports (handled by WorkerPool).

## Scope

### Files modified

| File | Change |
|------|--------|
| `cmd/uast/parse.go` | Rewrite `runParseParallel`, delete `parallelState` struct + `worker` method, add `getOrCreateParseParser` |
| `cmd/uast/analyze.go` | Rewrite `runAnalyzeParallel`, remove `sync`/`runtime` imports |

### Out of scope

- Sequential parsing paths (unchanged)
- `parseOnly`, `analyzeFile`, `analyzeNode` (unchanged)

## Acceptance Criteria

- [x] `parallelState` struct removed from parse.go
- [x] `runParseParallel` uses `WorkerPool[string]`
- [x] `runAnalyzeParallel` uses `WorkerPool[indexedFile]`
- [x] `go build ./cmd/uast/...` passes
- [x] `make lint` passes — 0 issues, no dead code

## Implementation

### Files Modified

| File | Change |
|------|--------|
| `cmd/uast/parse.go` | Rewrote `runParseParallel` with `WorkerPool[string]` + `sync.Pool`; deleted `parallelState` struct + `worker` method; added `getOrCreateParseParser`; updated `parseOnly` to accept `context.Context` |
| `cmd/uast/analyze.go` | Rewrote `runAnalyzeParallel` with `WorkerPool[indexedFile]`; removed `sync`/`runtime` imports; updated `analyzeFile` to accept `context.Context` |
| `specs/ref/ROADMAP.md` | Mark 4.4 done |
