# LSH Index Package

The `pkg/alg/lsh/` package provides a Locality-Sensitive Hashing (LSH)
index for fast approximate nearest-neighbor retrieval of MinHash
signatures.

## Overview

LSH groups similar MinHash signatures into the same buckets by hashing
bands of consecutive hash values. This enables O(N) indexing and
sublinear query time, replacing O(N^2) pairwise comparison.

For a codebase with 10,000 functions:
- Naive pairwise: 50 million comparisons
- LSH: 10,000 insertions + sublinear queries per function

## API

```go
// Create index: 16 bands x 8 rows = 128 hash functions.
idx, err := lsh.New(16, 8)

// Insert a signature with an identifier.
err = idx.Insert("pkg/foo.go:funcA", sigA)

// Query for candidates (any band match).
candidates, err := idx.Query(querySig)

// Query with similarity threshold (exact similarity post-filter).
results, err := idx.QueryThreshold(querySig, 0.8)

// Utility methods.
n := idx.Size()      // number of indexed signatures
idx.Clear()          // remove all entries
b := idx.NumBands()  // 16
r := idx.NumRows()   // 8
```

## How LSH Works

### Band Hashing

Each MinHash signature of size `k = b * r` is divided into `b` bands
of `r` consecutive hash values. Each band is hashed into a bucket using
FNV-1a:

```
band[i] = FNV-1a(i || mins[i*r : (i+1)*r])
```

Two signatures sharing identical values in any band will land in the
same bucket and become candidates.

### Candidate Probability

The probability that two signatures with Jaccard similarity `s` become
candidates is:

```
P(candidate) = 1 - (1 - s^r)^b
```

For b=16 bands, r=8 rows (128 total hashes):

| Jaccard Similarity | P(candidate) |
|---|---|
| 0.3 | ~0.01% |
| 0.5 | ~2.7% |
| 0.7 | ~47% |
| 0.8 | ~83% |
| 0.9 | ~99.5% |
| 1.0 | 100% |

This creates a natural threshold around s=0.7-0.8, making it ideal
for Type-2 and Type-3 clone detection.

### QueryThreshold

`QueryThreshold` first retrieves all candidates via `Query`, then
computes exact MinHash similarity for each candidate, filtering to
only those above the threshold. This two-phase approach combines the
speed of LSH with the precision of exact comparison.

## Thread Safety

All operations are protected by `sync.RWMutex`:
- `Insert`, `Clear`: write lock
- `Query`, `QueryThreshold`, `Size`: read lock

## Duplicate Handling

Inserting the same ID twice removes the old entry from all buckets
before inserting the new one. This ensures each ID appears at most
once in query results.

## Performance

Benchmarks on AMD Ryzen AI 9 HX 370 with b=16, r=8 (128 hashes):

| Benchmark | ns/op | B/op | allocs/op |
|---|---|---|---|
| `LSHInsert1K` (1000 sigs) | ~4.4M (~4.4 ms) | 6.7M | 36K |
| `LSHQuery1K` | ~1,960 (~2 μs) | 1296 | 3 |
| `LSHQueryThreshold1K` | ~2,100 (~2.1 μs) | 1312 | 4 |

Insert throughput: ~227K sigs/sec. Query throughput: ~500K queries/sec.

## Design Decisions

1. **FNV-1a band hashing**: Each band's consecutive uint64 values are
   serialized and hashed via FNV-1a with a band index prefix for domain
   separation.

2. **Map-of-maps buckets**: `[]map[uint64]map[string]bool` provides
   O(1) bucket lookup and O(1) deduplication within buckets.

3. **Stored signatures**: The index stores references to inserted
   signatures for `QueryThreshold`'s exact similarity computation.

4. **Error returns**: All operations that accept signatures return
   errors for nil, size mismatch, and invalid parameters.

5. **No Remove method**: Duplicate handling via re-insert is
   sufficient for the clone detection use case. A standalone `Remove`
   can be added if needed.
