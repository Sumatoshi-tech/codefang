# Free-List Stack (Replace gaps map)

## Overview

The RBTree `Allocator` previously used `gaps map[uint32]bool` to track freed
node indices. This was replaced with a `[]uint32` slice-based stack. Map
operations have pointer-chasing overhead and GC pressure from bucket
allocations, while a slice with `append`/reslice provides O(1) amortized
push/pop with minimal GC pressure and better cache locality.

## Changes

### `Allocator.gaps` field

- **Before**: `gaps map[uint32]bool`
- **After**: `gaps []uint32`

### `malloc()`

- **Before**: iterate map to pick arbitrary key, then `delete`
- **After**: pop last element from slice — `gaps[len(gaps)-1]`, reslice

### `free(idx)`

- **Before**: `gaps[idx] = true` (with `doAssert(!exists)` check)
- **After**: `gaps = append(gaps, idx)` (double-free check removed — it was
  a debug aid, not a correctness requirement)

### `Clone()`

- **Before**: `maps.Copy(newAllocator.gaps, allocator.gaps)`
- **After**: `slices.Clone(allocator.gaps)`

### `Hibernate()`

- **Before**: iterate map to build `[]uint32`, then compress
- **After**: compress `gaps` slice directly (no conversion needed)

### `Boot()`

- **Before**: decompress `[]uint32`, iterate to rebuild map
- **After**: decompress directly into `allocator.gaps` slice

## Performance

- `BenchmarkMallocFree_Stack`: 100K malloc/free cycles in ~2.7ms
- Only 56 allocations (slice grows) vs hundreds of map bucket allocations
- Zero per-element GC pressure

## Files

- `internal/rbtree/rbtree.go` — all changes
- `internal/rbtree/rbtree_test.go` — stress test + benchmark added
