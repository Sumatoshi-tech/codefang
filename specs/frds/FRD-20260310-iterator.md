# FRD: Iterator[T] and CollectN[T] (Roadmap 5.1)

**ID**: FRD-20260310-iterator
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 5.1
**Date**: 2026-03-10

## Problem

`CommitIter`, `FileIter`, and `RevWalk` in `pkg/gitlib` all follow the same
pull-based iteration pattern: `Next() (T, error)` + `Close()`. There is no shared
interface, so generic collection utilities cannot be written.

## Decision

Add `Iterator[T]` interface and `CollectN[T]` helper to `pkg/alg/iter.go`.

```go
// Iterator is a pull-based sequence of T values.
// Next returns io.EOF to signal normal end-of-sequence.
type Iterator[T any] interface {
    Next() (T, error)
    Close()
}

// CollectN drains up to limit items from iter.
// A limit of 0 means unlimited.
func CollectN[T any](iter Iterator[T], limit int) ([]T, error)
```

### Key design decisions

- **`io.EOF` signals end**: consistent with `bufio.Scanner`, `sql.Rows`, and Go conventions.
- **`Close()` not `Free()`**: Go convention. `RevWalk` will alias `Free()` in Step 5.2.
- **`limit == 0` means unlimited**: matches common Go patterns (e.g., `regexp.FindAll`).
- **No result on EOF**: `CollectN` does not return a partial error — EOF is swallowed
  as normal termination.

## Contract

- `Iterator.Next()` returns `(zero, io.EOF)` when exhausted.
- `CollectN` collects items until EOF or limit is reached.
- `CollectN` returns `(nil, err)` for non-EOF errors.
- `CollectN(iter, 0)` collects all items (unlimited).
- `CollectN(iter, n)` collects at most n items.
- `CollectN` on an already-exhausted iterator returns `(nil, nil)`.

## Scope

### Files created

| File | Description |
|------|-------------|
| `pkg/alg/iter.go` | `Iterator[T]` interface + `CollectN[T]` function |
| `pkg/alg/iter_test.go` | Unit tests with slice-backed stub iterator |

### Out of scope

- Migrating gitlib iterators (Step 5.2)
- Replacing `collectCommits` (Step 5.3)

## Acceptance Criteria

- [x] `Iterator[T]` interface defined
- [x] `CollectN[T]` implemented
- [x] Tests: empty iterator, collect all, collect with limit, error propagation, limit zero, exhausted, limit one, limit exceeds items
- [x] `go test ./pkg/alg/...` passes
- [x] `make lint` passes — 0 issues, no dead code (whitelisted pending callers)

## Implementation

### Files Created

| File | Description |
|------|-------------|
| `pkg/alg/iter.go` | `Iterator[T]` interface + `CollectN[T]` function |
| `pkg/alg/iter_test.go` | 8 unit tests with slice-backed stub iterator |

### Files Modified

| File | Change |
|------|--------|
| `.deadcode-whitelist` | Added `CollectN` (pending callers in Step 5.3) |
| `specs/ref/ROADMAP.md` | Mark 5.1 done |
