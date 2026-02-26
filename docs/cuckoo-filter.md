# Cuckoo Filter

## Overview

The `pkg/alg/cuckoo` package provides a space-efficient probabilistic set membership
filter that supports **deletion**, unlike Bloom filters. A Cuckoo filter stores
compact fingerprints in a hash table with two candidate buckets per entry,
supporting Insert, Lookup, and Delete operations with O(1) amortized time.

## When to Use

Use a Cuckoo filter instead of a Bloom filter when:
- Elements need to be **removed** from the set (e.g., incremental analysis with
  file renames/removals)
- You need comparable false-positive rates with similar memory usage
- You want a simpler API with delete support

Use a Bloom filter when:
- Deletion is not needed
- You want slightly simpler implementation with no eviction edge cases

## API

```go
// Create a filter sized for the expected number of elements.
f, err := cuckoo.New(100000)

// Insert an element. Returns false if the filter is full.
ok := f.Insert([]byte("hello"))

// Check membership. May return false positives, never false negatives.
found := f.Lookup([]byte("hello")) // true

// Remove an element. Returns false if not found.
deleted := f.Delete([]byte("hello"))
found = f.Lookup([]byte("hello")) // false

// Utility methods.
count := f.Count()          // number of stored elements
lf := f.LoadFactor()        // occupancy ratio 0.0–1.0
cap := f.Capacity()         // total fingerprint slots
f.Reset()                   // clear without reallocation
```

## Design

### Parameters
- **Fingerprint size**: 16 bits — provides < 0.1% false-positive rate
- **Bucket size**: 4 entries per bucket — balances space efficiency and lookup speed
- **Capacity headroom**: 2x overprovisioning — keeps load factor below ~50% for
  reliable inserts
- **Max kicks**: 500 eviction attempts — standard limit from the literature

### Hashing
- Primary hash: FNV-1a 64-bit
- Fingerprint: upper 16 bits of the hash, guaranteed non-zero
- Alternate index: `i1 XOR hash(fingerprint)` — ensures symmetry so
  `altIndex(altIndex(i, fp), fp) == i`

### PRNG
Uses a splitmix64 generator (non-cryptographic) for random bucket selection
during cuckoo eviction. This avoids `math/rand` which triggers gosec G404.

## Performance

Benchmarks on AMD Ryzen AI 9 HX 370 (10K items, 100K capacity):

| Operation | Time/op | Allocs/op |
|-----------|---------|-----------|
| Insert (10K items) | ~63 µs | 0 |
| Lookup (10K items) | ~55 µs | 0 |
| Delete (10K items) | ~83 µs | 0 |
| Memory (100K items) | ~1.6 ms | 1 (524 KB) |

Per-element: ~6.3 ns insert, ~5.5 ns lookup, ~8.3 ns delete.

## False-Positive Rate

With 16-bit fingerprints and 100K elements inserted, the measured false-positive
rate is well below the 3% threshold required by the specification. The theoretical
upper bound for 16-bit fingerprints is approximately `2 * bucketSize / 2^16 ≈ 0.012%`.

## Limitations

- **No counting**: Deleting an element that was inserted multiple times removes
  only one copy. Double-delete may remove a different element's fingerprint.
- **No dynamic resizing**: The filter must be sized at creation time.
- **Not thread-safe**: Callers must synchronize concurrent access.
- **Filter full**: When the filter cannot insert after 500 kick attempts, Insert
  returns false. Overprovisioning (2x headroom) mitigates this in practice.

## Files

| File | Description |
|------|-------------|
| `pkg/alg/cuckoo/cuckoo.go` | Filter implementation |
| `pkg/alg/cuckoo/cuckoo_test.go` | 18 correctness tests |
| `pkg/alg/cuckoo/benchmark_test.go` | 4 benchmarks |
| `specs/frds/FRD-20260226-016-cuckoo-filter-package.md` | Feature requirements |
