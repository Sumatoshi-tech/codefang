# FRD: Extract SignalCleanupGuard to pkg/sigutil (Roadmap F4.3)

**ID**: FRD-20260302-signal-cleanup-guard
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F4.3

## Problem

`SpillCleanupGuard` in `internal/streaming/hibernatable.go:20-70` is a generic
signal-driven cleanup pattern (SIGINT/SIGTERM + `sync.Once` idempotent cleanup +
goroutine listener + deregistration on `Close`). The mechanism has zero coupling
to spill files or streaming—it accepts cleaners and calls them once. This is
reusable infrastructure that any long-running pipeline could leverage. See
LIST.md #17.

## Feature

Create a generic `SignalCleanupGuard` in `pkg/sigutil`:

- `NewSignalCleanupGuard(cleanup func(), logger *slog.Logger) *SignalCleanupGuard`
  — registers SIGINT/SIGTERM handlers that call `cleanup` exactly once
- `Close()` — performs cleanup (if not already done), deregisters signal handler

Wire streaming:
- `SpillCleanupGuard` embeds `*sigutil.SignalCleanupGuard`, delegates all behavior
- `NewSpillCleanupGuard` constructs the embedded guard with a closure over cleaners
- `Close()` is promoted from the embedded type
- Existing tests and callers compile unchanged

## Acceptance Criteria

- [x] `pkg/sigutil/guard.go` exports `SignalCleanupGuard` type, `NewSignalCleanupGuard` constructor
- [x] `pkg/sigutil/guard_test.go` covers: cleanup called on Close, idempotent Close, nil cleanup, multiple cleaners via closure
- [x] `internal/streaming/hibernatable.go` `SpillCleanupGuard` embeds `*sigutil.SignalCleanupGuard`
- [x] `NewSpillCleanupGuard` delegates to `sigutil.NewSignalCleanupGuard`
- [x] Existing `hibernatable_test.go` tests pass unchanged
- [x] All existing tests pass
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Risk

**Low.** Pure extraction. `SpillCleanupGuard` embeds the generic guard and
promotes `Close()`, so all existing callers compile without changes. Signal
registration semantics are identical.

## Non-Goals

- Adding new signal types beyond SIGINT/SIGTERM.
- Changing cleanup ordering or error handling.
- Making the guard configurable (custom signals, timeouts).

## Implementation

### Files Created

- `pkg/sigutil/guard.go` — `SignalCleanupGuard` type, `NewSignalCleanupGuard` function
- `pkg/sigutil/guard_test.go` — tests for `SignalCleanupGuard`

### Files Modified

- `internal/streaming/hibernatable.go` — `SpillCleanupGuard` embeds `*sigutil.SignalCleanupGuard`; body of `NewSpillCleanupGuard` delegates to generic constructor

### Verification

- `go vet ./...` — clean
- `go test ./pkg/sigutil/... ./internal/streaming/...` — all pass
- `make lint` — 0 issues, 0 dead code
