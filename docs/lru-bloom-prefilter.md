# LRU Blob Cache — Bloom Pre-Filter

## Overview

`LRUBlobCache` in `internal/cache/lru.go` uses a Bloom filter as a first-level
guard to short-circuit `Get` and `GetMulti` lookups for hashes that are
definitely not in the cache. This avoids acquiring the mutex and probing the
map for the majority of cache misses.

## How It Works

```
Get(hash):
  1. bloom.Test(hash[:])  — read-only, no cache lock needed
  2. If false → return nil immediately (definite miss)
  3. If true  → acquire lock, probe map (existing path)

GetMulti(hashes):
  1. For each hash: bloom.Test(hash[:])
  2. Partition into candidates (pass) and filtered (fail)
  3. Only lock and probe map for candidates
  4. Filtered hashes go directly to the missing set

Put(hash, blob):
  1. Existing path: nil check, size check, lock, insert, eviction
  2. After insertion: bloom.Add(hash[:])

Clear():
  1. Existing path: clear map, reset pointers
  2. bloom.Reset()  — clears the filter alongside the cache
```

## Bloom Filter Sizing

The filter is sized automatically based on the cache's `maxSize`:

```go
expectedElements = max(maxSize / 4096, 64)  // 4 KB average blob estimate
fpRate = 0.01                                // 1% false-positive rate
```

| Cache Size | Expected Elements | Bloom Memory |
|------------|-------------------|--------------|
| 64 KB      | 64 (minimum)      | ~77 B        |
| 1 MB       | 256               | ~307 B       |
| 256 MB     | 65,536            | ~78 KB       |
| 1 GB       | 262,144           | ~314 KB      |

## Eviction and False Positives

Bloom filters do not support deletion. When an entry is evicted from the LRU
cache, its hash remains in the Bloom filter as a "phantom positive." This is
harmless:

- A phantom positive causes one unnecessary lock+map lookup (the map returns
  nil).
- The worst case is that all Bloom tests return "maybe" — the cache behaves
  identically to the pre-Bloom implementation.
- `Clear()` resets the Bloom filter, removing all phantoms.

## Statistics

`LRUStats` includes a `BloomFiltered` counter that tracks how many lookups
were short-circuited by the Bloom pre-filter.

```go
stats := cache.Stats()
fmt.Printf("Bloom-filtered: %d\n", stats.BloomFiltered)
fmt.Printf("Misses:         %d\n", stats.Misses)
fmt.Printf("Hits:           %d\n", stats.Hits)
```

Note: `BloomFiltered` is a subset of `Misses` — every Bloom-filtered lookup
is also counted as a miss.

## Performance

Benchmarks with 10,000 preloaded entries:

| Benchmark                     | ns/op  | B/op | allocs/op |
|-------------------------------|--------|------|-----------|
| `BenchmarkLRUGet_MissHeavy`   | ~74    | 0    | 0         |
| `BenchmarkLRUGet_HitHeavy`    | ~76    | 0    | 0         |
| `BenchmarkLRUGetMulti_MissHeavy` | ~12,449 | 9,424 | 16 |
| `BenchmarkLRUPut`             | ~35    | 0    | 0         |

The miss-heavy benchmark shows zero allocations on the fast path — the Bloom
filter test uses a stack-allocated slice header from `hash[:]` and the FNV-128a
hash computation is allocation-free.

## Thread Safety

The Bloom filter has its own internal `sync.RWMutex`. `Test` acquires a read
lock, allowing concurrent lookups. `Add` acquires a write lock but is called
inside the cache's write lock (which is already held for map insertion), so
there is no additional contention.
