# BloomHashSet — Probabilistic Hash Set

`BloomHashSet` is a memory-efficient probabilistic alternative to `HashSet` for
blob deduplication. It wraps `pkg/alg/bloom` with a `gitlib.Hash`-typed API and
provides constant memory regardless of element count.

## When to Use

Use `BloomHashSet` instead of `HashSet` when:

- You need to track a large number of seen hashes (100K+).
- A small false-positive rate is acceptable (e.g., dedup where re-processing is idempotent).
- Memory efficiency is more important than exact membership.

Use `HashSet` when:

- You need exact membership (zero false positives).
- The number of elements is small (< 10K).
- You need to iterate over stored elements.

## API

```go
import "github.com/Sumatoshi-tech/codefang/internal/cache"

// Create a set sized for 1M elements at 1% false-positive rate.
bs, err := cache.NewBloomHashSet(1_000_000, 0.01)

// Add a hash. Returns true if definitely new, false if possibly present.
isNew := bs.Add(hash)

// Test membership. False = definitely absent. True = possibly present.
exists := bs.Contains(hash)

// Approximate element count.
count := bs.Len()

// Monitor filter saturation (0.0 to 1.0).
fill := bs.FillRatio()

// Reset without reallocation.
bs.Clear()
```

## Thread Safety

All operations are safe for concurrent use. Thread-safety is inherited from
the underlying `bloom.Filter` which uses `sync.RWMutex` internally:

- `Contains` uses read lock (concurrent readers allowed).
- `Add` uses write lock (exclusive).
- `Clear` uses write lock (exclusive).

## Memory Comparison

| Elements | `HashSet` (exact) | `BloomHashSet` (1% FP) | Reduction |
|---|---|---|---|
| 10K | ~560 KB | ~12 KB | 98% |
| 100K | ~5.6 MB | ~123 KB | 98% |
| 1M | ~56 MB | ~1.2 MB | 98% |
| 10M | ~560 MB | ~12 MB | 98% |

## Performance

Benchmarks on AMD Ryzen AI 9 HX 370 (100K preloaded elements):

| Operation | `BloomHashSet` | `HashSet` | Notes |
|---|---|---|---|
| `Add` | ~58 ns/op, 0 B/op | ~266 ns/op, ~72 B/op | 4.6x faster, zero allocs |
| `Contains` | ~51 ns/op, 0 B/op | ~33 ns/op, 0 B/op | Map lookup is faster for exact |

Key takeaway: `BloomHashSet.Add` is significantly faster than `HashSet.Add` because
it avoids map growth and allocation. `Contains` is slightly slower due to hash function
computation, but the memory savings at scale far outweigh the per-lookup cost.

## False-Positive Behavior

A `Contains` call may return `true` for an element that was never added (false positive).
The rate is bounded by the `fpRate` parameter passed to the constructor:

- At `fpRate = 0.01`: ~1% of queries for absent elements return `true`.
- False positives are harmless in dedup scenarios — they cause one unnecessary skip.
- There are **zero false negatives**: if `Contains` returns `false`, the element was definitely never added.

`Add` returns `true` when the element is **definitely new** (Bloom says "not in set").
When `Add` returns `false`, the element was **possibly already present** — this may be
a false positive of the underlying Bloom test.

## Sizing

The constructor `NewBloomHashSet(expectedElements, fpRate)` automatically computes
optimal bit array size and hash function count:

- `expectedElements`: estimated number of unique hashes to store.
- `fpRate`: target false-positive rate, in range (0, 1). Use `0.01` for 1%.

If actual element count exceeds `expectedElements`, the FP rate degrades gracefully
(approaches 100%). Monitor with `FillRatio()` — values above 0.5 indicate saturation.

## Limitations

- **No deletion**: Bloom filters do not support element removal. Use Cuckoo filter
  (milestone 5) if deletion is required.
- **Approximate `Len()`**: Returns `bloom.EstimatedCount()` which may over-count if
  the same hash is added multiple times.
- **No iteration**: Cannot enumerate stored elements (Bloom is lossy).
- **No serialization**: Filter state cannot be persisted to disk.
