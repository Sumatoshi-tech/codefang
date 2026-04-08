# FRD: Common Checkpoint Helper (Roadmap F3.2)

**ID**: FRD-20260302-checkpoint-helper
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F3.2

## Problem

Three analyzers (burndown, couples, file_history) each implement an identical checkpoint
persistence pattern: a `newPersister()` factory that creates a `persist.Persister[T]`, plus
`SaveCheckpoint(dir)` and `LoadCheckpoint(dir)` methods that delegate to it. The only
differences are the basename string, codec choice (JSON vs Gob), and state type `T`. This is
~20 lines of duplicated boilerplate per analyzer (~60 lines total). See LIST.md #38.

## Feature

Create a generic `CheckpointHelper[T]` struct in `internal/analyzers/common` that wraps a
`persist.Persister[T]` with pre-bound build and restore callbacks. The helper exposes
`SaveCheckpoint(dir string) error` and `LoadCheckpoint(dir string) error` methods, which
can be promoted into an analyzer struct via embedding, automatically satisfying the
`checkpoint.Checkpointable` interface (except `CheckpointSize`, which remains
analyzer-specific).

Migrate the file_history analyzer as proof that the pattern works end-to-end.

## Acceptance Criteria

- [x] `internal/analyzers/common/checkpoint_helper.go` exports `CheckpointHelper[T]`, `NewCheckpointHelper[T]`
- [x] `internal/analyzers/common/checkpoint_helper_test.go` has ≥90% coverage
- [x] file_history analyzer migrated: embeds `*common.CheckpointHelper[checkpointState]`, removes `newPersister`, `SaveCheckpoint`, `LoadCheckpoint`
- [x] All existing tests pass (file_history, common)
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Risk

**Low.** The helper is a thin wrapper around `persist.Persister[T]` with no behavior change.
The file_history migration replaces 3 functions (`newPersister`, `SaveCheckpoint`,
`LoadCheckpoint`) with an embedded field and helper initialization. Checkpoint round-trip
semantics are preserved exactly.

## Non-Goals

- Migrating all 3 analyzers in this FRD (only file_history as proof).
- Changing checkpoint state types or build/restore logic.
- Adding `CheckpointSize` to the helper (remains analyzer-specific).
- Modifying `pkg/persist` or `internal/checkpoint` packages.

## Implementation

### Files Created

- `internal/analyzers/common/checkpoint_helper.go` — `CheckpointHelper[T any]` struct with `SaveCheckpoint`, `LoadCheckpoint` methods; `NewCheckpointHelper[T]` factory accepting basename, codec, build, restore
- `internal/analyzers/common/checkpoint_helper_test.go` — tests for save/load round-trip, error propagation, nil-safe construction

### Files Modified

- `internal/analyzers/file_history/checkpoint.go` — `newPersister` removed, `SaveCheckpoint`/`LoadCheckpoint` methods removed, replaced by embedded `*common.CheckpointHelper[checkpointState]`
- `internal/analyzers/file_history/analyzer.go` — analyzer struct embeds `*common.CheckpointHelper[checkpointState]`; helper initialized in `NewAnalyzer()` and `Fork()`

### Verification

- `go vet ./...` — clean
- `go test ./internal/analyzers/common/... ./internal/analyzers/file_history/...` — all pass
- `make lint` — 0 issues, 0 dead code
