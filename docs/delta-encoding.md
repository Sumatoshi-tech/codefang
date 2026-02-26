# Delta Encoding for Hibernation

## Overview

Delta encoding enhances the `Allocator.Hibernate()` compression pipeline by
preprocessing the Key array before LZ4 compression. Instead of storing
absolute key values, each element stores the difference from its predecessor.
For sorted or near-sorted data (typical of RBTree keys), this transforms
large, unique values into small, repetitive deltas that LZ4 compresses
dramatically better.

## API

### `DeltaEncodeUInt32Slice(data []uint32)`

Replaces each element with the difference from its predecessor, in place.
The first element is left unchanged. Uses unsigned subtraction which wraps
naturally for uint32, so the operation is lossless for any input.

### `DeltaDecodeUInt32Slice(data []uint32)`

Performs a prefix-sum to restore original values from deltas. The inverse of
`DeltaEncodeUInt32Slice`.

## Integration

The delta encoding is applied in the `Hibernate()`/`Boot()` pipeline:

1. **Hibernate**: deinterleave fields -> delta-encode Key array -> LZ4
   compress each buffer
2. **Boot**: LZ4 decompress each buffer -> delta-decode Key array ->
   reinterleave into nodes

Only the Key array (buffer index 0) is delta-encoded. The Value, Left,
Parent, Right, and Color arrays have no guaranteed ordering and would not
benefit from delta encoding.

## Performance

- Delta encode/decode: ~60-80 us for 100K elements (negligible vs LZ4 time)
- Delta-encoded sorted keys compress significantly better than plain LZ4
- No additional memory allocations (in-place operation)

## Files

- `internal/rbtree/lz4.go` — `DeltaEncodeUInt32Slice`, `DeltaDecodeUInt32Slice`
- `internal/rbtree/rbtree.go` — integrated into `Hibernate()` and `Boot()`
- `internal/rbtree/lz4_test.go` — 8 unit tests + 4 benchmarks
