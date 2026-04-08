# FRD: Extract SharedResponse[T] in Framework (Roadmap F2.3)

**ID**: FRD-20260302-shared-response
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F2.3

## Problem

`batchBlobState` in `internal/framework/blob_pipeline.go` and `sharedDiffResponse` in
`internal/framework/diff_pipeline.go` both implement the same pattern: a `sync.Once`-guarded
channel read that caches a result for shared access across multiple goroutines. The duplicated
pattern is: struct with `once sync.Once`, result fields, and a method/inline func that does
`once.Do(func() { read channels })`. See LIST.md #19.

## Feature

Extract a generic `SharedResponse[T]` type parameterized on result type `T`. The constructor
`NewSharedResponse` takes a `compute func(context.Context) (T, error)` closure; `Get(ctx)`
evaluates it exactly once via `sync.Once` and returns the cached result.

Replace:
- `sharedDiffResponse` → `SharedResponse[[]gitlib.DiffResult]` (single channel read)
- `batchBlobState` → `SharedResponse[map[gitlib.Hash]*gitlib.CachedBlob]` (multi-channel merge)

## Acceptance Criteria

- [x] `internal/framework/shared_response.go` exports `SharedResponse[T]`, `NewSharedResponse[T]`
- [x] `internal/framework/shared_response_test.go` has ≥90% coverage
- [x] `internal/framework/diff_pipeline.go` uses `SharedResponse[[]gitlib.DiffResult]`, `sharedDiffResponse` type removed
- [x] `internal/framework/blob_pipeline.go` uses `SharedResponse[map[gitlib.Hash]*gitlib.CachedBlob]`, `batchBlobState` type removed
- [x] All existing framework tests pass
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Risk

**Trivial.** Pure structural refactoring with no behavior change. The `sync.Once` semantics are
preserved exactly. Channel reads, error propagation, and global cache side effects remain identical.

## Non-Goals

- Changing pipeline batch sizing, sharding, or buffer strategies.
- Adding new functionality to `SharedResponse[T]` (e.g., Reset, timeout).
- Modifying `DiffPipeline.Process` or `BlobPipeline.Process` signatures.

## Implementation

### Files Created

- `internal/framework/shared_response.go` — `SharedResponse[T any]` struct, `NewSharedResponse[T]` factory, `Get(context.Context) (T, error)` method
- `internal/framework/shared_response_test.go` — tests: success path, error path, concurrent access, context cancellation

### Files Modified

- `internal/framework/diff_pipeline.go` — removed `sharedDiffResponse` struct and `wait` method; `diffJob.batchResp` field type changed to `*SharedResponse[[]gitlib.DiffResult]`; `flushBatch` creates `NewSharedResponse` with channel-read closure; `runDiffConsumer` uses `batchResp.Get(ctx)` instead of `wait(ctx)` + field access
- `internal/framework/blob_pipeline.go` — removed `batchBlobState` struct; `blobJob.batchState` field type changed to `*SharedResponse[map[gitlib.Hash]*gitlib.CachedBlob]`; `processBatch` builds `respChans` locally then creates `NewSharedResponse` with multi-channel-merge closure; `collectBlobResponse` uses `batchState.Get(ctx)` instead of inline `once.Do`

### Verification

- `go vet ./...` — clean
- `go test ./internal/framework/...` — all pass
- `make lint` — 0 issues, 0 dead code
