# FRD: Atomic File Write Helper (Roadmap 6.1)

**ID**: FRD-20260310-atomic-file-write
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 6.1
**Date**: 2026-03-10

## Problem

`internal/analyzers/analyze/report_store_file.go` contains two functions that
implement the write-tmp+fsync+rename atomic file write pattern:

1. `flushKind` — writes gob data via `os.Create(tmp)` + `io.Copy` + `fd.Sync()` + `fd.Close()` + `os.Rename(tmp, final)`.
2. `writeManifest` — writes JSON via `os.WriteFile(tmp)` + `os.Rename(tmp, final)` (no fsync).

The pattern is identical in structure: write to a `.tmp` sibling, optionally
sync, then atomically rename over the target path. Any future file writers
must re-implement the same sequence, risking omission of fsync or cleanup.

## Decision

Create `internal/storage/atomicfile.go` with a single exported function:

```go
// WriteAtomic writes to path atomically: creates a .tmp sibling, calls write
// with the temporary file, syncs the file to disk, then renames over path.
// If write returns an error or any step fails, the .tmp file is removed.
func WriteAtomic(path string, perm os.FileMode, write func(w io.Writer) error) error
```

Key design choices:
- `perm` parameter allows callers to specify file permissions (existing code uses `0o600`).
- `write func(w io.Writer) error` — caller writes via `io.Writer`, keeping the helper encoding-agnostic.
- fsync is always performed — `writeManifest` currently skips it, but fsync is cheap for small files and makes the contract uniform.
- On any failure after creating the tmp file, cleanup removes the tmp file (best-effort).
- Tmp file name: `path + ".tmp"` — matches existing convention in `report_store_file.go`.

## Contract

- The `.tmp` file is created in the same directory as `path` (same filesystem for atomic rename).
- `write` is called with an `io.Writer` backed by the `.tmp` file.
- After `write` returns nil, the file is fsynced and then renamed atomically over `path`.
- If `write` returns an error, the `.tmp` file is removed and the error is returned.
- If fsync or rename fails, the `.tmp` file is removed (best-effort) and the error is returned.
- The function is not safe for concurrent calls with the same `path` — callers must serialize.

## Scope

### Files created

| File | Description |
|------|-------------|
| `internal/storage/atomicfile.go` | `WriteAtomic` implementation |
| `internal/storage/atomicfile_test.go` | Unit tests |

### Out of scope

- Refactoring `report_store_file.go` to use `WriteAtomic` (Step 6.2).
- Directory-level fsync (overkill for this use case).
- Cross-filesystem atomic writes (rename requires same filesystem).

## Acceptance Criteria

- [x] `internal/storage/atomicfile.go` created with `WriteAtomic`
- [x] Tests cover: success path, write callback error, create error, overwrite, empty write
- [x] `go test ./internal/storage/...` passes
- [x] `make lint` passes — 0 issues, no dead code (whitelisted pending caller)

## Implementation

### Files Created

| File | Change |
|------|--------|
| `internal/storage/atomicfile.go` | `WriteAtomic(path, perm, write)` — atomic write with tmp+sync+rename |
| `internal/storage/atomicfile_test.go` | 5 unit tests: success, overwrite, callback error cleanup, create error, empty write |

### Files Modified

| File | Change |
|------|--------|
| `.deadcode-whitelist` | Added `WriteAtomic` (pending caller in Step 6.2) |
| `specs/ref/ROADMAP.md` | Marked 6.1 done, added FRD link |
| `AGENTS.md` | Added `internal/storage` package documentation |
