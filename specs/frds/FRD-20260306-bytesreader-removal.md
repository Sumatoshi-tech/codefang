# FRD: Remove textutil.BytesReader (Roadmap 1.4)

**ID**: FRD-20260306-bytesreader-removal
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 1.4
**Date**: 2026-03-06

## Problem

`textutil.BytesReader(data)` is a one-liner wrapper around `io.NopCloser(bytes.NewReader(data))`.
It adds no semantic value and teaches callers a fake vocabulary instead of stdlib idioms.
Three callers in `pkg/gitlib` import `textutil` solely for this function.

## Decision

Delete `BytesReader` entirely. No external code (outside the monorepo) uses it.
Keeping it as a deprecated wrapper would leave dead code that `make deadcode` would flag.

## Scope

### Callers to inline

| File | Site |
|------|------|
| `pkg/gitlib/blob.go:33` | `Blob.Reader()` |
| `pkg/gitlib/cached_blob.go:78` | `CachedBlob.Reader()` |
| `pkg/gitlib/changes.go:214` | `File.Reader()` |

### Functions to delete

- `pkg/textutil/textutil.go` — `BytesReader` function + `io` import (no longer needed)
- `pkg/textutil/textutil_test.go` — `TestBytesReader_EmptyData`, `TestBytesReader_RoundTrip`,
  `TestBytesReader_CloseIsIdempotent`

### Import adjustments

| File | Remove | Add |
|------|--------|-----|
| `pkg/gitlib/blob.go` | `textutil` | `bytes` |
| `pkg/gitlib/cached_blob.go` | — | `bytes` |
| `pkg/gitlib/changes.go` | `textutil` | `bytes` |
| `pkg/textutil/textutil.go` | `io` | — |

`cached_blob.go` retains `textutil` for `IsBinary` and `CountLines`.

## Acceptance Criteria

- [ ] `BytesReader` deleted from `textutil.go`
- [ ] `io` import removed from `textutil.go`
- [ ] All 3 call sites use `io.NopCloser(bytes.NewReader(data))` directly
- [ ] Tests for `BytesReader` deleted from `textutil_test.go`
- [ ] `go test ./pkg/gitlib/...` passes
- [ ] `go test ./pkg/textutil/...` passes
- [ ] `make lint` passes

## Risk

None. `io.NopCloser(bytes.NewReader(data))` is the exact implementation of `BytesReader`.
Behaviorally identical.

## Implementation

### Files Modified

- `pkg/textutil/textutil.go` — deleted `BytesReader`, removed `io` import
- `pkg/textutil/textutil_test.go` — deleted 3 `BytesReader` test functions
- `pkg/gitlib/blob.go` — inlined stdlib, removed `textutil` import, added `bytes`
- `pkg/gitlib/cached_blob.go` — inlined stdlib, added `bytes` import
- `pkg/gitlib/changes.go` — inlined stdlib, removed `textutil` import, added `bytes`
- `AGENTS.md` — removed `BytesReader` from `pkg/textutil` description
- `site/architecture/overview.md` — updated table entry for `pkg/textutil`
- `specs/ref/ROADMAP.md` — marked step 1.4 done
