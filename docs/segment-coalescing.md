# Segment Coalescing — Treap Node Compaction

`MergeAdjacentSameValue()` compacts the implicit treap timeline by merging
neighboring nodes that share the same `TimeKey` value. This reduces node
count, memory usage, and improves `Replace` throughput.

## When to Use

Call `MergeAdjacentSameValue()` periodically during burndown processing —
for example, after every N `Replace` operations or when `Nodes()` exceeds
a threshold. Coalescing is not required for correctness; it is a pure
performance optimization.

```go
tl := burndown.NewTreapTimeline(0, 10000)

for i, edit := range edits {
    tl.Replace(edit.Pos, edit.Del, edit.Ins, edit.Time)

    // Coalesce every 500 edits to keep the tree compact.
    if (i+1) % 500 == 0 {
        tl.MergeAdjacentSameValue()
    }
}
```

## How It Works

1. **Collect** segments via in-order traversal (`Segments()`).
2. **Merge** adjacent segments with identical `Value` into a single segment
   with combined `Length`.
3. **Rebuild** a balanced tree from the coalesced segment list
   (`ReconstructFromSegments()`).

If no adjacent segments share the same value, the tree is already optimal
and no rebuild occurs (early exit).

## Why Fragmentation Happens

`Replace` splits nodes when the edit falls inside a segment. For example,
deleting lines 50-55 from a 1000-line segment splits it into two segments
(0-49 and 56-999), both with the same `TimeKey`. After many such edits,
the tree accumulates redundant nodes.

## Performance

Benchmarks on AMD Ryzen AI 9 HX 370 (50K-line file, 10K edits):

| Scenario | Time/op | Notes |
|---|---|---|
| Without coalescing | ~51 ms | Tree grows unbounded, each Replace slower |
| With periodic coalescing (every 500 edits) | ~14 ms | Tree stays compact, each Replace faster |
| **Speedup** | **3.6x** | Periodic compaction amortizes rebuild cost |

## Invariants Preserved

- `Len()` unchanged (total line count).
- `Iterate()` output identical (same logical line-to-time mapping).
- `Validate()` passes (TreeEnd sentinel preserved).
- `Replace()` works correctly after coalescing.
- Idempotent: calling twice produces no additional change.

## API

```go
// On treapTimeline:
tl.MergeAdjacentSameValue()

// On File:
file.MergeAdjacentSameValue()
```

Both are O(N) where N is the current number of segments.

## Limitations

- Not thread-safe by itself — callers must not call concurrently with `Replace`.
  This is acceptable since burndown processing is single-file sequential.
- Rebuilds the entire tree; there is no partial/subtree coalescing.
- The rebuild reassigns node priorities, changing the tree shape (but not semantics).
