# FRD: Composable Pipeline Patterns (Spike Evolutionary Path)

**ID**: FRD-20260302-composable-pipeline-patterns
**Source**: [specs/ref/SPIKE-generic-pipeline.md](../ref/SPIKE-generic-pipeline.md) — Evolutionary Path

## Problem

The architecture spike (F4.1) concluded that a monolithic `BatchProcessor[In,Out,Job]`
is NOT FEASIBLE (~30% shared structure, ~70% divergent). However, the spike identified
5 composable patterns that capture overlap **one axis at a time** without forcing a
false common model.

Both `BlobPipeline` and `DiffPipeline` share identical goroutine topology (~20 lines):
create channels, start producer/consumer goroutines, `defer close` channels, propagate
context. This is the most bug-prone shared code (channel closing order, goroutine
lifecycle). Additionally, both have batching policies (pass-through vs threshold) and
multi-phase processing.

## Feature

Create 5 composable building blocks in `pkg/pipeline`:

1. **`RunPC[In, Out, Job]`** — Producer-consumer micro-skeleton. Owns goroutine
   topology, channel creation, and orderly shutdown. Each pipeline delegates its
   `Process()` method to `RunPC.Run()`.

2. **`Phase[S]` + `RunPhases[S]`** — Chain-of-responsibility phase runner.
   Represent phases as first-class values, execute sequentially, stop on first error.

3. **`Batcher[In, Batch]`** — Batching strategy interface with two implementations:
   `ThresholdBatcher[T]` (accumulates until count) and `PassthroughBatcher[T]`
   (each item is its own batch).

4. **`DispatchFunc[Req]`** — Dispatch strategy as a function type. Captures worker
   channel in closure, decoupled from request semantics.

5. **`Fetcher[Req, Resp]`** — Fetch-with-context interface for cache decorator pattern.

Wire `RunPC` into both pipelines:
- `BlobPipeline.Process()` delegates to `RunPC[<-chan CommitBatch, BlobData, blobJob].Run()`
- `DiffPipeline.Process()` delegates to `RunPC[<-chan BlobData, CommitData, diffJob].Run()`
- `runProducer`/`runConsumer` lose their `defer close` (RunPC manages channel lifecycle)

## Acceptance Criteria

- [x] `pkg/pipeline/runpc.go` exports `RunPC[In, Out, Job any]` with `Run(ctx, in) <-chan Out`
- [x] `pkg/pipeline/phase.go` exports `Phase[S]` interface, `PhaseFunc[S]` adapter, `RunPhases[S]`
- [x] `pkg/pipeline/batcher.go` exports `Batcher[In, Batch]` interface, `ThresholdBatcher[T]`, `PassthroughBatcher[T]`
- [x] `pkg/pipeline/dispatch.go` exports `DispatchFunc[Req any]`
- [x] `pkg/pipeline/fetcher.go` exports `Fetcher[Req, Resp]` interface, `FetcherFunc[Req, Resp]` adapter
- [x] `pkg/pipeline/runpc_test.go` covers: basic flow, context cancellation, empty producer, ordering, buffer behavior
- [x] `pkg/pipeline/phase_test.go` covers: empty phases, single phase, multi-phase, error stops chain, context propagation
- [x] `pkg/pipeline/batcher_test.go` covers: threshold flush, partial flush, passthrough, empty flush
- [x] `pkg/pipeline/dispatch_test.go` covers: successful dispatch, context cancellation
- [x] `pkg/pipeline/fetcher_test.go` covers: successful fetch, error propagation, FetcherFunc adapter
- [x] `BlobPipeline.Process()` uses `RunPC.Run()`; `runProducer`/`runConsumer` no longer close channels
- [x] `DiffPipeline.Process()` uses `RunPC.Run()`; `runDiffProducer`/`runDiffConsumer` no longer close channels
- [x] All existing pipeline tests pass
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Risk

**Low-Medium.** The building blocks are small independent utilities (~30 lines each).
Wiring `RunPC` into existing pipelines is a targeted refactoring that removes `defer close`
from producer/consumer and replaces the `Process()` body with `RunPC.Run()`. Existing
tests cover the concurrent behavior and will catch regressions.

## Non-Goals

- Replacing the entire pipeline architecture with a framework.
- Wiring Phase, Batcher, Dispatcher, or Fetcher into existing pipelines (building blocks only).
- Adding error channels to RunPC (errors flow through the output data stream).
- Changing pipeline semantics or behavior.

## Implementation

### Files Created

- `pkg/pipeline/runpc.go` — `RunPC[In, Out, Job]` type, `Run` method
- `pkg/pipeline/runpc_test.go` — tests
- `pkg/pipeline/phase.go` — `Phase[S]` interface, `PhaseFunc[S]`, `RunPhases[S]`
- `pkg/pipeline/phase_test.go` — tests
- `pkg/pipeline/batcher.go` — `Batcher[In, Batch]`, `ThresholdBatcher[T]`, `PassthroughBatcher[T]`
- `pkg/pipeline/batcher_test.go` — tests
- `pkg/pipeline/dispatch.go` — `DispatchFunc[Req]`
- `pkg/pipeline/dispatch_test.go` — tests
- `pkg/pipeline/fetcher.go` — `Fetcher[Req, Resp]`, `FetcherFunc[Req, Resp]`
- `pkg/pipeline/fetcher_test.go` — tests

### Files Modified

- `pkg/pipeline/options.go` — updated package doc comment
- `internal/framework/blob_pipeline.go` — `Process()` delegates to `RunPC.Run()`; `runProducer` loses `defer close(jobs)`; `runConsumer` loses `defer close(out)`
- `internal/framework/diff_pipeline.go` — `Process()` delegates to `RunPC.Run()`; `runDiffProducer` loses `defer close(jobs)`; `runDiffConsumer` loses `defer close(out)`

### Verification

- `go vet ./...` — clean
- `go test ./pkg/pipeline/... ./internal/framework/...` — all pass
- `make lint` — 0 issues, 0 dead code
