# FRD: Move System Metrics to Observability (Roadmap F2.2)

**ID**: FRD-20260302-sysmetrics-move
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F2.2

## Problem

`TakeHeapSnapshot` and `readRSSBytes` in `internal/streaming/planner.go` are general-purpose
profiling utilities, not streaming-specific. They belong in `internal/observability` alongside
other system metrics (scheduler, RED, analysis metrics). See LIST.md #16.

## Feature

Move `HeapSnapshot`, `TakeHeapSnapshot`, `readRSSBytes` (renamed `ReadRSSBytes`), and
`statmMinFields` from `internal/streaming/planner.go` to a new file
`internal/observability/sysmetrics.go`. Update all callers in `internal/framework/streaming.go`
to import from `observability`.

## Acceptance Criteria

- [x] `internal/observability/sysmetrics.go` exports `HeapSnapshot`, `TakeHeapSnapshot`, `ReadRSSBytes`
- [x] `internal/streaming/planner.go` no longer contains these definitions
- [x] `internal/framework/streaming.go` uses `observability.TakeHeapSnapshot()` and `observability.HeapSnapshot`
- [x] All existing tests pass
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Risk

**Trivial.** Pure code relocation with no behavior change. `ReadRSSBytes` is promoted from
unexported to exported — no existing external callers are affected since the old function
was unexported.

## Non-Goals

- Adding new system metrics or changing the HeapSnapshot struct.
- Modifying CheckMemoryPressure or MemoryPressureLevel (these remain in streaming/planner.go).
- Changing how HeapSnapshot is used by callers (buildReplanObservation, logChunkMemory, etc.).

## Implementation

### Files Created

- `internal/observability/sysmetrics.go` — `HeapSnapshot` struct, `TakeHeapSnapshot()`, `ReadRSSBytes()`, `statmMinFields` constant
- `internal/observability/sysmetrics_test.go` — 4 tests covering snapshot values, sys/heap relationship, timestamp validity, RSS on Linux

### Files Modified

- `internal/streaming/planner.go` — removed `HeapSnapshot`, `TakeHeapSnapshot`, `readRSSBytes`, `statmMinFields`; removed unused imports (`os`, `runtime`, `strconv`, `strings`, `time`)
- `internal/streaming/planner_test.go` — removed `TestHeapSnapshot_ReturnsPositiveValues` (moved to observability)
- `internal/framework/streaming.go` — replaced `streaming.TakeHeapSnapshot()` with `observability.TakeHeapSnapshot()` (6 sites); replaced `streaming.HeapSnapshot` with `observability.HeapSnapshot` (5 sites)

### Verification

- `go vet ./...` — clean
- `go test ./internal/observability/... ./internal/streaming/... ./internal/framework/...` — all pass
- `make lint` — 0 issues, 0 dead code
