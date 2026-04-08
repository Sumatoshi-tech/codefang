# FRD: Stdlib Replacements (Roadmap 1.1)

**ID**: FRD-20260302-stdlib-replacements
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 1.1
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster 6

## Problem

Three hand-rolled utility functions duplicate functionality already provided by the Go standard library:

1. **`ReverseCommits`** (`pkg/gitlib/helpers.go:69`) — manual slice reversal for `[]*Commit`. Go 1.21+ provides `slices.Reverse`.
2. **`blobReader`** (`pkg/gitlib/blob.go:47`) — custom `io.Reader` over `[]byte` with manual position tracking. `bytes.NewReader` does this with more features (Seek, ReadAt).
3. **`stringSlicesEqual`** (`internal/checkpoint/manager.go:221`) — manual string slice comparison. Go 1.21+ provides `slices.Equal`.

## Feature

### 1.1.a Replace ReverseCommits with slices.Reverse

- Delete `ReverseCommits` function from `pkg/gitlib/helpers.go`
- Replace the single call site (`helpers.go:121`) with `slices.Reverse(commits)`
- Add `slices` import

### 1.1.b Replace blobReader with bytes.NewReader

- Delete `blobReader` struct and its `Read` method from `pkg/gitlib/blob.go`
- Replace `&blobReader{data: ...}` in `blob.go:31` with `bytes.NewReader(...)`
- Replace `&blobReader{data: contents}` in `changes.go:212` with `bytes.NewReader(contents)`
- Add `bytes` import where needed

### 1.1.c Replace stringSlicesEqual with slices.Equal

- Delete `stringSlicesEqual` function from `internal/checkpoint/manager.go`
- Replace the single call site (`manager.go:214`) with `slices.Equal(meta.Analyzers, analyzerNames)`
- Add `slices` import

## Acceptance Criteria

- [ ] All three custom functions/types are deleted from source
- [ ] All call sites use stdlib equivalents
- [ ] `go test ./pkg/gitlib/... ./internal/checkpoint/...` passes
- [ ] No new tests needed — existing tests validate behavior
- [ ] `go vet ./pkg/gitlib/... ./internal/checkpoint/...` clean
- [ ] `make lint` passes

## Risk

Trivial. All three stdlib functions are exact behavioral matches:
- `slices.Reverse` — in-place reversal, same semantics
- `bytes.NewReader` — superset of `blobReader` (adds Seek, ReadAt, Len)
- `slices.Equal` — element-wise comparison, same semantics

## Implementation

### Files Modified

- `pkg/gitlib/helpers.go` — Deleted `ReverseCommits` function (~6 lines), replaced call site with `slices.Reverse(commits)`
- `pkg/gitlib/blob.go` — Deleted `blobReader` struct and `Read` method (~16 lines), replaced with `bytes.NewReader`
- `pkg/gitlib/changes.go` — Updated `File.Reader()` to use `bytes.NewReader` instead of `blobReader`
- `internal/checkpoint/manager.go` — Deleted `stringSlicesEqual` function (~13 lines), replaced call site with `slices.Equal`
- `internal/analyzers/analyze/base_history.go` — Refactored `Serialize` to reduce cyclomatic complexity (pre-existing lint violation fixed)

### Lines Eliminated

~35 lines of custom code replaced by stdlib calls.

### Verification

- `go vet ./pkg/gitlib/... ./internal/checkpoint/...` — clean
- `go test ./pkg/gitlib/... ./internal/checkpoint/... ./internal/analyzers/analyze/...` — all pass
- `make lint` — zero issues
