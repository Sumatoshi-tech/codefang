# FRD: Promote signalOnDrain to pkg/pipeline (Roadmap 3.1)

**ID**: FRD-20260310-signal-on-drain
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 3.1
**Date**: 2026-03-10

## Problem

`internal/framework/coordinator.go` contains `signalOnDrain[T any]`, a pure
channel combinator with zero internal dependencies. It forwards items from a
source channel to a new output channel and closes a signal channel once the
source is exhausted.

This utility belongs in `pkg/pipeline` alongside `RunPC`, `Phase`, and other
composable pipeline primitives. Promoting it:

1. Makes it available to future packages without import cycles.
2. Co-locates it with related pipeline building blocks.
3. Follows the project pattern of domain-free utilities in `pkg/`.

## Decision

Create `pkg/pipeline/drain.go` with an exported generic function:

```go
// SignalOnDrain forwards items from src to the returned forwarded channel
// and closes the returned drained channel once src is exhausted.
func SignalOnDrain[T any](src <-chan T) (forwarded <-chan T, drained <-chan struct{})
```

Update `coordinator.go` to call `pipeline.SignalOnDrain` instead of the
local `signalOnDrain`. Delete the local function.

## Contract

- `forwarded` receives every item from `src` in order.
- `forwarded` is closed after `src` is closed and all items have been sent.
- `drained` is closed after `forwarded` is closed (signals source exhaustion).
- Blocking on `forwarded` read does not block `drained` close — `drained`
  closes only after all items exit via `forwarded`.
- `src` being nil causes the goroutine to close both channels immediately.

## Scope

### Files created

| File | Description |
|------|-------------|
| `pkg/pipeline/drain.go` | `SignalOnDrain[T]` implementation |
| `pkg/pipeline/drain_test.go` | Unit tests |

### Files modified

| File | Change |
|------|--------|
| `internal/framework/coordinator.go` | Replace `signalOnDrain` calls with `pipeline.SignalOnDrain`; delete local function |

### Out of scope

- Changing pipeline behavior or channel buffering
- Adding context cancellation to `SignalOnDrain` (source channel closure is the signal)

## Acceptance Criteria

- [x] `pkg/pipeline/drain.go` created with `SignalOnDrain[T]`
- [x] `pkg/pipeline/drain_test.go` with tests (forwarding, ordering, drain signal, nil/empty source)
- [x] `coordinator.go` updated: 3 call sites use `pipeline.SignalOnDrain`
- [x] Local `signalOnDrain` deleted from `coordinator.go`
- [x] `go test ./pkg/pipeline/...` passes
- [x] `go test ./internal/framework/...` passes
- [x] `make lint` passes — 0 issues, no dead code

## Implementation

### Files Created

| File | Change |
|------|--------|
| `pkg/pipeline/drain.go` | `SignalOnDrain[T]` implementation |
| `pkg/pipeline/drain_test.go` | 4 unit tests: forwarding, empty source, drain-after-forward, nil source |

### Files Modified

| File | Change |
|------|--------|
| `internal/framework/coordinator.go` | Import `pkg/pipeline`; replace 3 `signalOnDrain(` calls with `pipeline.SignalOnDrain(`; delete local `signalOnDrain[T]` function |
| `specs/ref/ROADMAP.md` | Mark 3.1 done |
