# FRD: Generic LRU Cache with Bloom Pre-filter (Roadmap 4.1)

**ID**: FRD-20260302-generic-lru-cache
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 4.1
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster 2, LIST #15

## Problem

Two independent LRU cache implementations share identical linked-list operations, Bloom pre-filter integration, stats tracking, and Clear patterns:

| Feature | `internal/cache/lru.go` (LRUBlobCache) | `internal/framework/diff_cache.go` (DiffCache) |
|---------|----------------------------------------|------------------------------------------------|
| **Key type** | `gitlib.Hash` | `DiffKey` (OldHash + NewHash) |
| **Value type** | `*gitlib.CachedBlob` | `plumbing.FileDiffData` |
| **Capacity** | Size-based (bytes, default 256 MB) | Count-based (entries, default 10,000) |
| **Eviction** | Cost-aware sampling (5 candidates, accessCount/sizeKB) | Simple LRU tail removal |
| **Bloom** | `hash[:]` as key bytes, 1% FP rate | `[oldHash || newHash]` as key bytes, 1% FP rate |
| **Batch ops** | `GetMulti`, `PutMulti` | None |
| **Clone on insert** | Yes (`blob.Clone()`) | No |
| **Reject oversized** | Yes (skip items > maxSize) | No |

### Identical methods (code duplication)

| Method | LRUBlobCache lines | DiffCache lines |
|--------|-------------------|----------------|
| `moveToFront` | 329-336 | 183-190 |
| `addToFront` | 339-352 | 193-206 |
| `removeFromList` | 355-367 | 209-221 |
| `Clear` | 317-326 | 133-141 |
| `CacheHits`/`CacheMisses` | 291-294 | 144-147 |
| `Stats` (struct + HitRate) | 276-314 | 149-180 |

Total duplicated: ~150 lines of identical linked-list and stats code, plus ~100 lines of structurally identical Bloom integration and eviction logic.

## Solution

Create a generic `Cache[K comparable, V any]` in `pkg/alg/lru/` using the functional options pattern. The two existing caches become thin wrappers (~30 lines each) that delegate to the generic implementation while preserving their current public APIs.

### Placement

`pkg/alg/lru/` — fits alongside `pkg/alg/bloom/`, `pkg/alg/cms/`, `pkg/alg/hll/`, etc. as a reusable algorithm package.

### API

```go
package lru

// Cache is a thread-safe generic LRU cache with optional Bloom pre-filtering.
type Cache[K comparable, V any] struct { /* ... */ }

// New creates a new LRU cache. At least one capacity limit
// (WithMaxEntries or WithMaxBytes) must be provided.
func New[K comparable, V any](opts ...Option[K, V]) *Cache[K, V]

// Option configures a Cache.
type Option[K comparable, V any] func(*Cache[K, V])

// WithMaxEntries sets the maximum number of entries (count-based eviction).
func WithMaxEntries[K comparable, V any](n int) Option[K, V]

// WithMaxBytes sets the maximum total size in bytes.
// sizeFunc returns the byte size of a value.
func WithMaxBytes[K comparable, V any](maxBytes int64, sizeFunc func(V) int64) Option[K, V]

// WithBloomFilter enables Bloom pre-filtering for Get/GetMulti.
// keyToBytes converts a key to its byte representation for the Bloom filter.
// expectedN is the expected number of elements for Bloom filter sizing.
func WithBloomFilter[K comparable, V any](keyToBytes func(K) []byte, expectedN uint) Option[K, V]

// WithCostEviction enables sampling-based eviction with a cost function.
// Higher cost = less desirable to evict. sampleSize entries are sampled
// from the LRU tail; the one with lowest cost is evicted.
func WithCostEviction[K comparable, V any](sampleSize int, costFunc func(accessCount, sizeBytes int64) float64) Option[K, V]

// WithCloneFunc sets a function to clone values before insertion.
// Useful to detach values from shared memory arenas.
func WithCloneFunc[K comparable, V any](clone func(V) V) Option[K, V]

// Core operations.
func (c *Cache[K, V]) Get(key K) (V, bool)
func (c *Cache[K, V]) Put(key K, value V)
func (c *Cache[K, V]) GetMulti(keys []K) (found map[K]V, missing []K)
func (c *Cache[K, V]) PutMulti(items map[K]V)

// Stats and lifecycle.
func (c *Cache[K, V]) Stats() Stats
func (c *Cache[K, V]) CacheHits() int64
func (c *Cache[K, V]) CacheMisses() int64
func (c *Cache[K, V]) Clear()
func (c *Cache[K, V]) Len() int

// Stats holds cache performance metrics.
type Stats struct {
    Hits          int64
    Misses        int64
    BloomFiltered int64
    Entries       int
    CurrentSize   int64  // 0 when size tracking is not enabled.
    MaxEntries    int    // 0 when count-based limit is not set.
    MaxSize       int64  // 0 when size-based limit is not set.
}

func (s Stats) HitRate() float64
```

### Migration

**LRUBlobCache** becomes a thin wrapper (~30 lines):

```go
type LRUBlobCache struct {
    cache *lru.Cache[gitlib.Hash, *gitlib.CachedBlob]
}

func NewLRUBlobCache(maxSize int64) *LRUBlobCache {
    if maxSize <= 0 {
        maxSize = DefaultLRUCacheSize
    }
    expectedN := max(uint(maxSize/averageBlobSizeEstimate), minBloomElements)
    return &LRUBlobCache{
        cache: lru.New(
            lru.WithMaxBytes[gitlib.Hash, *gitlib.CachedBlob](maxSize, blobSize),
            lru.WithBloomFilter[gitlib.Hash, *gitlib.CachedBlob](hashToBytes, expectedN),
            lru.WithCostEviction[gitlib.Hash, *gitlib.CachedBlob](evictionSampleSize, evictionCost),
            lru.WithCloneFunc[gitlib.Hash, *gitlib.CachedBlob](cloneBlob),
        ),
    }
}
```

**DiffCache** becomes a thin wrapper (~30 lines):

```go
type DiffCache struct {
    cache *lru.Cache[DiffKey, plumbing.FileDiffData]
}

func NewDiffCache(maxEntries int) *DiffCache {
    if maxEntries <= 0 {
        maxEntries = DefaultDiffCacheSize
    }
    return &DiffCache{
        cache: lru.New(
            lru.WithMaxEntries[DiffKey, plumbing.FileDiffData](maxEntries),
            lru.WithBloomFilter[DiffKey, plumbing.FileDiffData](diffKeyToBytes, uint(maxEntries)),
        ),
    }
}
```

### Key design decisions

1. **No maxEntries in constructor signature**: Both capacity modes are optional. At least one must be provided, validated at construction time via panic (same pattern as `bloom.NewWithEstimates`).

2. **Bloom FP rate is a constant**: Both existing caches use 1% (`0.01`). Rather than adding another parameter, the generic cache uses a package-level constant. This matches the Bloom filter's own `NewWithEstimates(n, fp)` design.

3. **accessCount always tracked**: Even without cost-based eviction, tracking access count is a single `int64` increment per Get — negligible overhead and useful for diagnostics.

4. **Value semantics for Get**: Returns `(V, bool)` instead of a pointer. For pointer value types (like `*CachedBlob`), the zero value is `nil`, which naturally signals "not found". The wrapper's `Get(hash) *CachedBlob` method can simply return the value and ignore the bool.

5. **Put behavior differences**: When `Put` is called for an existing key, the generic cache updates the value (DiffCache behavior) and increments access count (LRUBlobCache behavior). The LRUBlobCache wrapper skips nil values before calling Put.

## Acceptance Criteria

- [x] Generic `Cache[K, V]` implemented in `pkg/alg/lru/`
- [x] Options: `WithMaxEntries`, `WithMaxBytes`, `WithBloomFilter`, `WithCostEviction`, `WithCloneFunc`
- [x] `Get`, `Put`, `GetMulti`, `PutMulti`, `Stats`, `Clear`, `CacheHits`, `CacheMisses`, `Len`
- [x] Unit tests: basic get/put, LRU eviction, size-based eviction, cost-based eviction, Bloom pre-filtering, concurrent access, clear, stats, GetMulti/PutMulti
- [x] Benchmark suite: hit-heavy, miss-heavy, GetMulti, Put throughput
- [x] `internal/cache/lru.go` reduced to thin wrapper delegating to `lru.Cache`
- [x] `internal/framework/diff_cache.go` reduced to thin wrapper delegating to `lru.Cache`
- [x] All existing tests pass: `go test ./pkg/alg/lru/... ./internal/cache/... ./internal/framework/...`
- [x] `go vet` clean
- [x] `make lint` passes — zero issues, zero dead code
- [x] Benchmark regression within 5% of original

## Risk

Medium. The core linked-list and Bloom integration logic is well-tested in both existing implementations. The generic wrapper adds type parameterization but no new algorithms. The thin-wrapper approach preserves existing APIs, avoiding cascading caller changes. Main risk is subtle behavior differences between Put semantics (LRUBlobCache: skip duplicates, DiffCache: update value).

## Implementation

### Files created

| File | Purpose |
|------|---------|
| `pkg/alg/lru/cache.go` | Generic `Cache[K,V]` struct, `New` constructor, functional options (`WithMaxEntries`, `WithMaxBytes`, `WithBloomFilter`, `WithCostEviction`, `WithCloneFunc`), `Len` |
| `pkg/alg/lru/ops.go` | Core operations: `Get`, `Put`, `putLocked`, `GetMulti`, `PutMulti`, `Clear`, eviction (`evictUntilFits`, `evictOne`, `evictTail`, `evictLowestCost`), linked-list management (`moveToFront`, `addToFront`, `removeFromList`), Bloom partitioning |
| `pkg/alg/lru/stats.go` | `Stats` struct, `HitRate`, `Stats()`, `CacheHits`, `CacheMisses` |
| `pkg/alg/lru/cache_test.go` | 22 unit tests covering all acceptance criteria |
| `pkg/alg/lru/benchmark_test.go` | 4 benchmarks: hit-heavy, miss-heavy, GetMulti, Put throughput |

### Files modified

| File | Change |
|------|--------|
| `internal/cache/lru.go` | Rewritten from 414 lines to ~155 lines as thin wrapper delegating to `lru.Cache[gitlib.Hash, *gitlib.CachedBlob]` |
| `internal/framework/diff_cache.go` | Rewritten from 233 lines to ~100 lines as thin wrapper delegating to `lru.Cache[DiffKey, plumbing.FileDiffData]` |
