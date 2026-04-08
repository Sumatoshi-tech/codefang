# FRD: Create pkg/textutil with binary detection + line counting (Roadmap F0.5)

**ID**: FRD-20260302-textutil-package
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F0.5
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster: Text Utilities

## Problem

Binary detection (`IsBinary`) and line counting (`computeLineCount`) logic is embedded as methods on `pkg/gitlib.CachedBlob`. These are general-purpose text operations useful beyond git blob handling (e.g., UAST source analysis, report formatting, file inspection), but the current location couples them to the git abstraction. Similarly, `io.NopCloser(bytes.NewReader(data))` is repeated in 3 places across gitlib as a trivial `[]byte → io.ReadCloser` adapter.

## Feature

Create `pkg/textutil` as the canonical package for byte-level text utilities. Extract the core algorithms from `pkg/gitlib/cached_blob.go` into standalone functions.

### textutil.go — Text Utilities

| Export | Signature | Behavior |
|--------|-----------|----------|
| `BinarySniffLength` | `const = 8000` | Maximum bytes to scan for binary detection |
| `IsBinary` | `func(data []byte) bool` | Returns true if data contains null bytes within the sniff window |
| `CountLines` | `func(data []byte) int` | Returns the number of lines (newline-delimited); 0 for empty data |
| `BytesReader` | `func(data []byte) io.ReadCloser` | Returns `io.NopCloser(bytes.NewReader(data))` |

### Algorithm Details

**IsBinary**: Scans up to `BinarySniffLength` bytes for a null byte (`\x00`). Empty data returns false.

**CountLines**: Counts `\n` occurrences in full data. If data is non-empty and doesn't end with `\n`, adds 1 (last line without trailing newline). Returns 0 for empty data. Does NOT check for binary — caller decides whether to check `IsBinary` first.

**BytesReader**: Trivial adapter wrapping `bytes.NewReader` with `io.NopCloser`.

### Design Decisions

- **CountLines does not check binary**: Separating concerns. The caller (e.g., `CachedBlob.CountLines`) handles the binary check and caching. The standalone function is a pure line counter.
- **BinarySniffLength exported**: Callers may want to use the same constant for their own sniffing logic.
- **No new dependencies**: Pure stdlib (`bytes`, `io`).

## Acceptance Criteria

- [x] `pkg/textutil/textutil.go` exports: `BinarySniffLength`, `IsBinary`, `CountLines`, `BytesReader`
- [x] `pkg/textutil/textutil_test.go` covers: empty data, pure text, binary with null bytes, sniff boundary, line counting edge cases (no trailing newline, empty lines, single newline), BytesReader round-trip
- [x] All tests pass, ≥95% statement coverage
- [x] `go vet` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Implementation

**Created:**
- `pkg/textutil/textutil.go` — `BinarySniffLength`, `IsBinary`, `CountLines`, `BytesReader`
- `pkg/textutil/textutil_test.go` — 19 tests (binary detection, line counting, BytesReader, sniff boundary)

**Modified (F1.5 wiring):**
- `pkg/gitlib/cached_blob.go` — `IsBinary()` delegates to `textutil.IsBinary`, `computeLineCount()` delegates to `textutil.IsBinary` + `textutil.CountLines`, `Reader()` delegates to `textutil.BytesReader`; removed `binarySniffLength` constant
- `pkg/gitlib/blob.go` — `Blob.Reader()` delegates to `textutil.BytesReader`
- `pkg/gitlib/changes.go` — `File.Reader()` delegates to `textutil.BytesReader`

**Coverage:** 100% statement coverage on `pkg/textutil/`.

## Risk

Low. Pure extraction with identical algorithms. All existing gitlib tests pass unchanged.
