# Count-Min Sketch Package

## Overview

`pkg/alg/cms` provides a space-efficient probabilistic frequency estimator.
A Count-Min Sketch answers "how many times has this element been seen?" with
a bounded overestimation guarantee: the estimated count is always >= the true
count, and the overestimation is bounded by `epsilon * totalCount` with
probability >= `1 - delta`.

## When to Use

Use a Count-Min Sketch when you need to count element frequencies in a
data stream without maintaining a full frequency map. The sketch is
particularly effective when:

- The number of distinct elements is large (millions of tokens).
- Exact counts are not required — bounded overestimation is acceptable.
- Memory must remain constant regardless of the number of distinct elements.
- Streaming processing requires O(1) per-element updates.

## API

```go
// Create a sketch with epsilon=0.001 (error bound) and delta=0.001 (confidence).
// Width = ceil(e/epsilon) = 2719, Depth = ceil(ln(1/delta)) = 7.
sk, err := cms.New(0.001, 0.001)

// Increment counter for a key.
sk.Add([]byte("operator-plus"), 1)

// Add multiple at once.
sk.Add([]byte("operand-x"), 42)

// Query estimated frequency (always >= true count).
count := sk.Count([]byte("operator-plus"))

// Get total of all additions.
total := sk.TotalCount()

// Clear without reallocation.
sk.Reset()

// Inspect configuration.
w := sk.Width() // 2719
d := sk.Depth() // 7
```

## Performance Characteristics

| Operation      | Count-Min Sketch    | `map[string]int64`   |
|----------------|--------------------|-----------------------|
| Add            | ~77 ns, 0 allocs   | ~373 ns, 1 alloc     |
| Count          | ~74 ns, 0 allocs   | ~31 ns, 0 allocs     |
| Memory         | ~152 KB (constant)  | ~80 bytes per key    |
| Accuracy       | Bounded overestimate| Exact                |

## Thread Safety

All operations are thread-safe. `Add` and `Reset` acquire a write lock.
`Count`, `TotalCount`, `Width`, and `Depth` acquire a read lock.

## Algorithm

The implementation uses a 2D array of `depth` rows and `width` columns.
Each row uses an independent hash function (FNV-1a with a unique per-row
seed). On `Add`, all `depth` counters for the key are incremented. On
`Count`, the minimum across all `depth` counters is returned.

### Parameters

Parameters are computed automatically from the desired error bounds:

- Width: `w = ceil(e / epsilon)` — controls overestimation magnitude.
- Depth: `d = ceil(ln(1 / delta))` — controls confidence level.

| epsilon | delta  | Width | Depth | Memory   |
|---------|--------|-------|-------|----------|
| 0.01    | 0.01   | 272   | 5     | ~11 KB   |
| 0.001   | 0.001  | 2719  | 7     | ~152 KB  |
| 0.0001  | 0.0001 | 27183 | 10    | ~2.2 MB  |

### Guarantees

For positive-only additions:
- `Count(key) >= trueCount(key)` — never underestimates.
- `Count(key) - trueCount(key) < epsilon * totalCount` with probability >= `1 - delta`.

## Errors

- `cms.ErrInvalidEpsilon` — returned when epsilon is not positive.
- `cms.ErrInvalidDelta` — returned when delta is not in the open interval (0, 1).
