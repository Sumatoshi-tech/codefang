# MinHash Package

The `pkg/alg/minhash/` package provides MinHash signature generation for
estimating Jaccard similarity between sets. It is the foundation for
the clone detection analyzer (Milestone 4).

## Overview

MinHash compresses a set of tokens (or shingles) into a compact
fixed-size signature of `k` hash values. The Jaccard similarity between
two sets can then be estimated in O(k) time by comparing the fraction
of matching minimums in their signatures.

## API

```go
// Create a signature with 128 hash functions.
sig, err := minhash.New(128)

// Add tokens to the signature.
sig.Add([]byte("func"))
sig.Add([]byte("main"))

// Compare two signatures.
sim, err := sigA.Similarity(sigB)  // returns float64 in [0, 1]

// Merge signatures for union estimation.
err = sigA.Merge(sigB)  // element-wise min

// Serialize / deserialize.
data := sig.Bytes()
restored, err := minhash.FromBytes(data)

// Utility methods.
sig.Reset()           // clear to initial state
clone := sig.Clone()  // independent copy
empty := sig.IsEmpty() // true if no tokens added
n := sig.Len()         // number of hash functions
```

## Hashing Strategy

Each hash function is derived from a single FNV-1a base hash of the
token bytes, combined with a unique per-hash seed via XOR + splitmix64
finalizer. This produces k independent hash values from a single base
hash computation, amortizing the per-token hashing cost.

Seeds are deterministically generated from a constant base seed using
the splitmix64 sequence, ensuring reproducible signatures across runs.

## Jaccard Similarity Estimation

The estimated Jaccard index between two sets A and B is:

```
J(A, B) ≈ |{i : min_A[i] == min_B[i]}| / k
```

where `k` is the number of hash functions. With k=128 hash functions,
the standard error is approximately `1/sqrt(k)` ≈ 8.8%.

## Merge (Union Estimation)

Merging two signatures produces a signature representing the union of
the underlying sets:

```
merged[i] = min(A[i], B[i])
```

This is useful for combining signatures computed on different shards
or for estimating similarity between unions of sets.

## Serialization

Binary format: `[numHashes as uint32 big-endian (4 bytes)]` followed by
`[mins as []uint64 big-endian (8 * numHashes bytes)]`. Seeds are
deterministically regenerated from the base seed and are not serialized.

For 128 hash functions, the serialized size is 1028 bytes (4 + 128*8).

## Thread Safety

All mutating operations (`Add`, `Merge`, `Reset`) acquire a mutex.
Read operations (`Similarity`, `Bytes`, `IsEmpty`, `Clone`) also
acquire the mutex for consistency with concurrent writes. The typical
use case is single-goroutine signature building per function; the mutex
is a safety net for concurrent access.

## Performance

Benchmarks on AMD Ryzen AI 9 HX 370 with 128 hash functions:

| Benchmark | ns/op | B/op | allocs/op |
|---|---|---|---|
| `Add` (single token) | ~120 | 0 | 0 |
| `Similarity` | ~56 | 0 | 0 |
| `Signature_1KTokens` | ~124,000 | 2112 | 3 |
| `Merge` | ~393 | 2112 | 3 |
| `Bytes` | ~207 | 1152 | 1 |
| `FromBytes` | ~752 | 2112 | 3 |

The `Add` cost (~120 ns) is dominated by the FNV-1a hash computation
and the 128-iteration minimum update loop. The 0-allocation hot path
makes it suitable for high-throughput token processing.

## Accuracy

With 128 hash functions, the accuracy depends on set sizes:

| Overlap | Expected Jaccard | Typical Estimate | Error |
|---|---|---|---|
| Identical | 1.000 | 1.000 | 0.000 |
| 90% shared | 0.818 | ~0.82 | <0.02 |
| 50% shared | 0.333 | ~0.33 | <0.05 |
| Disjoint | 0.000 | ~0.01 | <0.03 |

For clone detection, the typical thresholds are:
- Type-1 (exact): similarity = 1.0
- Type-2 (renamed): similarity > 0.8
- Type-3 (near-miss): similarity > 0.5

The 128-hash signature provides sufficient precision to distinguish
these clone types.

## Design Decisions

1. **FNV-1a + splitmix64**: Single base hash + per-seed mixing avoids
   k independent hash function implementations while maintaining
   independence. Well-studied technique from the MinHash literature.

2. **128 hash functions**: Standard choice for code similarity.
   Provides ~8.8% standard error, sufficient for Type-1/2/3 clone
   classification.

3. **Big-endian serialization**: Consistent byte order for
   cross-platform compatibility.

4. **Error returns**: `Similarity` and `Merge` return errors for
   size mismatches and nil arguments rather than panicking.

5. **No weighted MinHash**: The standard unweighted variant is
   sufficient for token-set similarity in code analysis.
