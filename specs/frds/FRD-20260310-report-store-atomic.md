# FRD: Refactor report_store_file.go to use WriteAtomic (Roadmap 6.2)

**ID**: FRD-20260310-report-store-atomic
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 6.2
**Date**: 2026-03-10

## Problem

`internal/analyzers/analyze/report_store_file.go` contains two functions that
manually implement the write-tmp+rename atomic pattern:

1. `writeManifest` — marshals JSON, writes to `.tmp`, renames.
2. `flushKind` — creates `.tmp`, copies gob data, syncs, closes, renames.

Step 6.1 introduced `storage.WriteAtomic` which encapsulates this pattern.
Both functions should delegate to `WriteAtomic` to eliminate duplication and
ensure consistent fsync + cleanup behavior.

## Decision

Replace the manual tmp+rename sequences in both `writeManifest` and `flushKind`
with calls to `storage.WriteAtomic`. The callback passed to `WriteAtomic`
handles only the encoding/copying logic.

Key behavior changes:
- `writeManifest` gains fsync (previously skipped) — uniformly safe now.
- `flushKind` gains automatic tmp cleanup on error (previously leaked on copy/sync failure).
- Error messages change slightly (prefixed with "atomic" from `WriteAtomic`), but
  error wrapping preserves the original cause so `errors.Is` still works.

## Contract

- All existing tests must pass unchanged — behavior is preserved.
- `tmpExtension` constant is no longer used by these two functions (still used
  by `Open` for torn-write detection).
- Import `internal/storage` added; `os` import may be trimmed if no longer needed.

## Scope

### Files modified

| File | Change |
|------|--------|
| `internal/analyzers/analyze/report_store_file.go` | `writeManifest` and `flushKind` rewritten to use `storage.WriteAtomic` |

### Out of scope

- Changing the `Open` torn-write detection logic (still scans for `.tmp` files).
- Modifying `Begin` or any other function.

## Acceptance Criteria

- [x] `writeManifest` uses `storage.WriteAtomic`
- [x] `flushKind` uses `storage.WriteAtomic`
- [x] All 8 existing tests pass unchanged
- [x] `go test ./internal/analyzers/analyze/...` passes
- [x] `make lint` passes — 0 issues, no dead code

## Implementation

### Files Modified

| File | Change |
|------|--------|
| `internal/analyzers/analyze/report_store_file.go` | Import `internal/storage`; `writeManifest` delegates JSON write to `storage.WriteAtomic`; `flushKind` delegates `io.Copy` to `storage.WriteAtomic` — removed manual tmp/sync/rename/cleanup |
| `.deadcode-whitelist` | Removed `WriteAtomic` entry (now reachable via `report_store_file.go`) |
| `specs/ref/ROADMAP.md` | Marked 6.2 done, added FRD link |
