# Interval Tree

## Overview

The `pkg/alg/interval` package provides an augmented interval tree for efficient
range-overlap queries. It supports Insert, Delete, QueryOverlap, and QueryPoint
operations with O(log N) insert/delete and O(log N + k) query time, where k is
the number of overlapping intervals.

## When to Use

Use an interval tree when you need to answer questions like:
- "Which segments intersect lines X-Y?" (blame-like queries)
- "Which intervals contain point P?" (point containment)
- Range-overlap queries where linear scans are too slow

Use the existing `internal/rbtree` when you only need point queries (FindGE/FindLE).

## API

```go
// Create an empty interval tree.
tree := interval.New()

// Insert intervals [low, high] with associated values.
tree.Insert(10, 20, 1)
tree.Insert(15, 25, 2)
tree.Insert(30, 40, 3)

// Query overlapping intervals — returns all intervals intersecting [12, 18].
results := tree.QueryOverlap(12, 18)  // returns [10,20] and [15,25]

// Point query — returns all intervals containing point 35.
results = tree.QueryPoint(35)  // returns [30,40]

// Delete a specific interval.
deleted := tree.Delete(10, 20, 1)  // true

// Utility methods.
count := tree.Len()  // number of stored intervals
tree.Clear()         // remove all intervals
```

## Design

### Data Structure
The tree is a pointer-based red-black tree augmented with a `maxHigh` field at
each node. This field stores the maximum `High` endpoint in the node's entire
subtree, enabling efficient subtree pruning during overlap queries.

### Interval Type
```go
type Interval struct {
    Low   uint32  // inclusive lower bound
    High  uint32  // inclusive upper bound
    Value uint32  // associated data
}
```

### Overlap Semantics
An interval [a, b] overlaps query [qLow, qHigh] when `a <= qHigh AND b >= qLow`.
Both endpoints are inclusive. Zero-width intervals (Low == High) are supported
as point intervals.

### BST Ordering
Intervals are ordered by Low (primary) then High (secondary). Duplicate intervals
with the same [Low, High] but different Values are supported. Duplicate intervals
with the same [Low, High, Value] are also supported — each insert creates a
separate node.

### Rotation Unification
Left and right rotations, insert fixup cases, and delete fixup cases are unified
using a direction parameter to eliminate code duplication and satisfy the `dupl`
linter rule.

## Performance

Benchmarks on AMD Ryzen AI 9 HX 370 (10K intervals, spacing=10, width=5):

| Operation | Time/op | Allocs/op |
|-----------|---------|-----------|
| Insert 10K intervals | ~600 us | 10K (480 KB) |
| QueryOverlap [500,1500] (~101 results) | ~1.1 us | 8 |
| QueryPoint (single result) | ~42 ns | 1 |
| Delete 10K intervals | ~510 us | 0 |

### Complexity
- Insert: O(log N) amortized
- Delete: O(log N) amortized
- QueryOverlap: O(log N + k) where k = number of results
- QueryPoint: O(log N + k) where k = number of containing intervals

## Limitations

- **Pointer-based nodes**: Each interval allocates a heap node. For very large
  trees (millions of intervals), consider an array-based approach like `internal/rbtree`.
- **Not thread-safe**: Callers must synchronize concurrent access.
- **No serialization**: Unlike `internal/rbtree`, there is no hibernation or disk
  persistence support.
- **No merge**: Overlapping intervals are stored independently; callers handle
  merging if needed.

## Files

| File | Description |
|------|-------------|
| `pkg/alg/interval/interval.go` | Tree implementation |
| `pkg/alg/interval/interval_test.go` | 23 correctness tests |
| `pkg/alg/interval/benchmark_test.go` | 4 benchmarks |
| `specs/frds/FRD-20260226-017-interval-tree.md` | Feature requirements |
