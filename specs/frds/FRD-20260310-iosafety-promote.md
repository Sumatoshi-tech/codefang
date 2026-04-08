# FRD: Promote cmd/uast I/O helpers to pkg/iosafety (Roadmap 3.3)

**ID**: FRD-20260310-iosafety-promote
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 3.3
**Date**: 2026-03-10

## Problem

`cmd/uast/io_safety.go` contains defensive file-reading and terminal-output
utilities used by 5 sub-commands (parse, query, explore, diff, analyze). These
functions have zero cmd-specific coupling — they are pure I/O safety wrappers:

- `safeReadFile(path) ([]byte, string, error)` — resolve + validate + read
- `resolveUserFilePath(path) (string, error)` — clean, abs, stat, reject dirs
- `sanitizeForTerminal(input) string` — HTML-escape + strip control chars
- `writeTerminalLine(args ...any)` — trivial `fmt.Fprintln(os.Stdout, ...)`

Promoting the first three to `pkg/iosafety` makes them reusable across future
CLI tools and packages. `writeTerminalLine` is a trivial one-liner and should
be inlined at call sites rather than exported.

## Decision

Create `pkg/iosafety/iosafety.go` with exported functions:

```go
package iosafety

// ReadFile resolves, validates, and reads a user-supplied file path.
// Returns content, the resolved absolute path, and any error.
func ReadFile(path string) (content []byte, resolvedPath string, err error)

// ResolvePath normalises and validates a user-supplied file path.
// Returns the absolute path after cleaning, resolving, and stat-checking.
// Returns an error for empty paths, NUL bytes, directories, or stat failures.
func ResolvePath(path string) (string, error)

// SanitizeForTerminal strips control characters and HTML-escapes the input.
// Newlines, carriage returns, and tabs are replaced with spaces.
func SanitizeForTerminal(input string) string
```

Exported sentinel errors:

```go
var (
    ErrDirectoryPath  = errors.New("path points to a directory")
    ErrEmptyPath      = errors.New("path is empty")
    ErrPathContainsNUL = errors.New("path contains NUL byte")
)
```

Update all `cmd/uast/` callers to import `pkg/iosafety`. Delete
`cmd/uast/io_safety.go`. Inline `writeTerminalLine` at its call sites.

## Contract

- `ResolvePath("")` returns `ErrEmptyPath`.
- `ResolvePath` rejects NUL bytes with `ErrPathContainsNUL`.
- `ResolvePath` rejects directories with `ErrDirectoryPath`.
- `ResolvePath` returns absolute, cleaned path on success.
- `ReadFile` wraps `ResolvePath` + `os.ReadFile` with wrapped errors.
- `SanitizeForTerminal` replaces `\n`, `\r`, `\t` with space, drops other
  control chars, HTML-escapes the rest.
- All errors are wrapped with `fmt.Errorf` for context.

## Scope

### Files created

| File | Description |
|------|-------------|
| `pkg/iosafety/iosafety.go` | Exported functions + sentinel errors |
| `pkg/iosafety/iosafety_test.go` | Unit tests |

### Files modified

| File | Change |
|------|--------|
| `cmd/uast/parse.go` | Replace `safeReadFile` with `iosafety.ReadFile` |
| `cmd/uast/query.go` | Replace `safeReadFile`, `sanitizeForTerminal`, `writeTerminalLine` |
| `cmd/uast/explore.go` | Replace `safeReadFile`, `sanitizeForTerminal`, `writeTerminalLine` |

### Files deleted

| File | Reason |
|------|--------|
| `cmd/uast/io_safety.go` | All functions promoted or inlined |

### Out of scope

- Adding context cancellation or timeout to `ReadFile`
- Changing file-reading behavior (symlink resolution, max-size limits)
- `writeTerminalLine` promotion (inline at call sites instead)

## Acceptance Criteria

- [x] `pkg/iosafety/iosafety.go` created with `ReadFile`, `ResolvePath`, `SanitizeForTerminal`
- [x] Sentinel errors exported: `ErrDirectoryPath`, `ErrEmptyPath`, `ErrPathContainsNUL`
- [x] `pkg/iosafety/iosafety_test.go` with 13 tests covering all contracts
- [x] `cmd/uast/io_safety.go` deleted
- [x] All cmd/uast callers updated to import `pkg/iosafety`
- [x] `writeTerminalLine` inlined at 5 call sites as `fmt.Fprintln(os.Stdout, ...)`
- [x] `go test ./pkg/iosafety/...` passes
- [x] `go build ./cmd/uast/...` passes
- [x] `make lint` passes — 0 issues, no dead code

## Implementation

### Files Created

| File | Description |
|------|-------------|
| `pkg/iosafety/iosafety.go` | `ReadFile`, `ResolvePath`, `SanitizeForTerminal` + sentinel errors |
| `pkg/iosafety/iosafety_test.go` | 13 unit tests covering all contracts |

### Files Modified

| File | Change |
|------|--------|
| `cmd/uast/parse.go` | Import `pkg/iosafety`; replace 2 `safeReadFile` calls with `iosafety.ReadFile` |
| `cmd/uast/query.go` | Import `pkg/iosafety`; replace `safeReadFile`, `sanitizeForTerminal`, inline `writeTerminalLine` |
| `cmd/uast/explore.go` | Import `pkg/iosafety`; replace `safeReadFile`, `sanitizeForTerminal`, inline `writeTerminalLine` |
| `specs/ref/ROADMAP.md` | Mark 3.3 done |

### Files Deleted

| File | Reason |
|------|--------|
| `cmd/uast/io_safety.go` | All functions promoted to `pkg/iosafety` or inlined |
