# Bloom Filter Package

## Overview

`pkg/alg/bloom` provides a space-efficient probabilistic set membership filter.
A Bloom filter answers "definitely not in set" or "possibly in set" with a
tunable false-positive rate.

## When to Use

Use a Bloom filter as a pre-filter to avoid expensive exact lookups (map access,
lock acquisition, disk I/O). The filter is particularly effective when:

- Most lookups result in misses (high miss ratio).
- The exact set is too large to keep in memory.
- A small false-positive rate is acceptable.

## API

```go
// Create a filter for 10 million elements at 1% false-positive rate.
f, err := bloom.NewWithEstimates(10_000_000, 0.01)

// Insert elements.
f.Add([]byte("key1"))

// Test membership.
if f.Test([]byte("key1")) {
    // Possibly present — do the expensive exact lookup.
}

// Atomic test-and-add.
wasPresent := f.TestAndAdd([]byte("key2"))

// Bulk operations.
f.AddBulk(items)
results := f.TestBulk(queries)

// Monitoring.
count := f.EstimatedCount()
ratio := f.FillRatio()

// Clear without reallocation.
f.Reset()
```

## Performance Characteristics

| Operation      | Bloom Filter        | `map[string]bool`    |
|----------------|--------------------|-----------------------|
| Add            | ~31 ns, 0 allocs   | ~344 ns, 1 alloc     |
| Test (hit)     | ~27 ns, 0 allocs   | ~29 ns, 0 allocs     |
| Test (miss)    | ~18 ns, 0 allocs   | ~29 ns, 0 allocs     |
| Memory (10M)   | ~12 MB              | ~800 MB              |
| False positives| ~1% (configurable)  | 0%                   |

## Thread Safety

All operations are thread-safe. `Add`, `TestAndAdd`, `AddBulk`, and `Reset`
acquire a write lock. `Test`, `TestBulk`, `EstimatedCount`, and `FillRatio`
acquire a read lock.

## Algorithm

The implementation uses the double-hashing technique from Kirsch and
Mitzenmacher (2006). Two base hashes are derived from FNV-128a, and k bit
positions are computed as `h(i) = h1 + i*h2 mod m`. The second hash is forced
odd to ensure the step is coprime with the bit-array size.

## Parameters

Parameters are computed automatically from the expected element count `n` and
desired false-positive rate `fp`:

- Bit-array size: `m = ceil(-n * ln(fp) / ln(2)^2)`
- Hash function count: `k = round(m/n * ln(2))`

For 10M elements at 1% FP: m = 95,850,584 bits (~11.4 MB), k = 7.

## Errors

- `bloom.ErrZeroN` — returned when n is zero.
- `bloom.ErrInvalidFP` — returned when fp is not in (0, 1).
