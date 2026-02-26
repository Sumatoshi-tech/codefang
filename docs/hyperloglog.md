# HyperLogLog Package

## Overview

`pkg/alg/hll` provides a space-efficient probabilistic cardinality estimator.
A HyperLogLog sketch answers "approximately how many distinct elements have
been observed?" with ~2% standard error using only 2^p bytes of memory
(e.g., 16 KB for precision 14).

## When to Use

Use a HyperLogLog sketch when you need to count unique items without
maintaining a full set. The sketch is particularly effective when:

- The number of distinct elements is large (thousands to billions).
- Exact counts are not required — ~2% error is acceptable.
- Memory efficiency is critical (16 KB vs hundreds of MB for a map).
- Sketches need to be merged for parallel/distributed analysis.

## API

```go
// Create a sketch with precision 14 (16384 registers, ~0.8% error).
sk, err := hll.New(14)

// Insert elements.
sk.Add([]byte("user-123"))

// Estimate cardinality.
count := sk.Count()

// Merge two sketches (must have same precision).
err = sk.Merge(otherSketch)

// Clone for independent modification.
clone := sk.Clone()

// Clear without reallocation.
sk.Reset()

// Inspect configuration.
p := sk.Precision()      // 14
regs := sk.RegisterCount() // 16384
```

## Performance Characteristics

| Operation      | HyperLogLog         | `map[string]struct{}` |
|----------------|---------------------|-----------------------|
| Add            | ~19 ns, 0 allocs    | ~358 ns, 1 alloc      |
| Count          | ~132 us, 0 allocs   | O(1) via `len()`      |
| Merge          | ~6.7 us, 0 allocs   | O(N) set union        |
| Memory (any N) | 16 KB (p=14)        | ~80 bytes per element |
| Accuracy       | ~2% error           | Exact                 |

## Thread Safety

All operations are thread-safe. `Add`, `Merge`, and `Reset` acquire a write
lock. `Count`, `Precision`, `RegisterCount`, and `Clone` acquire a read lock.

## Algorithm

The implementation uses the standard HyperLogLog algorithm with LogLog-Beta
bias correction from Qin et al. (2016). Each element is hashed using FNV-1a
with a splitmix64 finalizer for full-avalanche mixing. The upper `p` bits of
the hash determine the register index, and the remaining `64-p` bits are used
to count leading zeros (rho function).

The cardinality estimate uses the LogLog-Beta formula:

```
E = alpha_m * m * (m - V(0)) / (beta(V(0)) + sum(2^{-M[j]}))
```

where `V(0)` is the number of zero registers and `beta` is a polynomial
correction term that handles bias across all cardinality ranges.

## Precision and Error

The precision parameter `p` (range [4, 18]) controls the trade-off between
memory and accuracy:

| Precision | Registers | Memory   | Typical Error |
|-----------|-----------|----------|---------------|
| 4         | 16        | 16 B     | ~26%          |
| 10        | 1024      | 1 KB     | ~3.25%        |
| 14        | 16384     | 16 KB    | ~0.8%         |
| 18        | 262144    | 256 KB   | ~0.2%         |

The theoretical standard error is `1.04 / sqrt(2^p)`.

## Errors

- `hll.ErrPrecisionOutOfRange` — returned when precision is not in [4, 18].
- `hll.ErrPrecisionMismatch` — returned when merging sketches with different precisions.
