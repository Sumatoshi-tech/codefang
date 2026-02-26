# Treap Node Pool — Free-List Allocator

The implicit treap timeline uses a `nodePool` free-list to recycle
`treapNode` objects instead of allocating them on the heap. This reduces
GC pressure and improves cache locality.

## How It Works

The pool maintains a `[]*treapNode` free-list (LIFO stack):

- **Acquire**: pop from free-list if available; otherwise allocate a new
  `&treapNode{}` on the heap.
- **Release**: zero all fields (`*node = treapNode{}`), push to free-list.
- **ReleaseSubtree**: post-order traversal releasing all nodes.

After warmup (a few hundred Replace operations), the pool accumulates
enough nodes to serve most subsequent allocations from the free-list,
eliminating per-Replace heap allocations.

## Integration Points

| Operation | Pool Interaction |
|---|---|
| `newNode(length, value)` | Acquires from pool |
| `splitByLines` (mid-segment split) | Releases original root after detaching children |
| `Replace` (deleted middle) | Releases deleted subtree via `releaseSubtree` |
| `Erase` | Releases entire tree via `releaseSubtree` |
| `CloneDeep` | Acquires from clone's own pool |
| `Reconstruct` | Releases old tree, rebuilds via pool |
| `ReconstructFromSegments` | Releases old tree, rebuilds via pool |

## Performance

Benchmarks on AMD Ryzen AI 9 HX 370:

| Metric | With Pool | Notes |
|---|---|---|
| `Replace` latency | ~642 ns/op | After warmup |
| `Replace` allocs | 2 allocs/op | Node allocs from pool (0 heap); remaining 2 from `DeltaReport` slice |

## Design Decisions

1. **Free-list of pointers** (not slab): A `[]treapNode` slab would
   invalidate all pointers on `append`-triggered reallocation. Using
   `[]*treapNode` avoids this — each node is a stable heap object.

2. **Zero on release**: `*node = treapNode{}` clears all fields including
   `left`/`right` pointers, preventing dangling references and making
   use-after-release detectable.

3. **No `sync.Pool`**: Treap operations are single-threaded per file.
   A simple slice-based stack is sufficient and has zero synchronization
   overhead.

4. **Post-order release**: `releaseSubtree` recurses left, right, then
   releases current. This ensures children are released before parent.

## Limitations

- Pool only grows; released nodes are retained indefinitely.
- No cross-treap sharing; each timeline owns its pool.
- `CloneDeep` allocates from the clone's pool (clone starts with an
  empty pool and grows as needed).
