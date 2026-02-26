package rbtree_test

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/rbtree"
)

// Delta encoding test constants.
const (
	deltaTestSize      = 1000
	deltaTestConstVal  = 7
	deltaBenchSize     = 100000
	deltaBenchSortStep = 3
)

func TestCompressDecompressUInt32Slice(t *testing.T) {
	t.Parallel()

	data := make([]uint32, 1000)
	for idx := range data {
		data[idx] = 7
	}

	packed := rbtree.CompressUInt32Slice(data)

	// Check that compression actually reduced the size (or at least didn't fail).
	assert.NotNil(t, packed)
	assert.NotEmpty(t, packed, "Compression should produce some output")

	// Clear the data and decompress.
	for idx := range data {
		data[idx] = 0
	}

	rbtree.DecompressUInt32Slice(packed, data)

	// Verify that all values were restored correctly.
	for idx := range data {
		assert.Equal(t, uint32(7), data[idx], "Value at index %d should be 7", idx)
	}
}

// TestDeltaEncode_SortedAscending verifies round-trip on sorted ascending data.
func TestDeltaEncode_SortedAscending(t *testing.T) {
	t.Parallel()

	original := make([]uint32, deltaTestSize)
	for i := range original {
		original[i] = uint32(i * deltaBenchSortStep)
	}

	data := make([]uint32, len(original))
	copy(data, original)

	rbtree.DeltaEncodeUInt32Slice(data)

	// After encoding, first element unchanged, rest should be deltaBenchSortStep.
	assert.Equal(t, original[0], data[0])

	for i := 1; i < len(data); i++ {
		assert.Equal(t, uint32(deltaBenchSortStep), data[i], "delta at index %d", i)
	}

	rbtree.DeltaDecodeUInt32Slice(data)
	assert.Equal(t, original, data)
}

// TestDeltaEncode_SortedDescending verifies round-trip on descending data.
func TestDeltaEncode_SortedDescending(t *testing.T) {
	t.Parallel()

	original := make([]uint32, deltaTestSize)
	for i := range original {
		original[i] = uint32((deltaTestSize - i) * deltaBenchSortStep)
	}

	data := make([]uint32, len(original))
	copy(data, original)

	rbtree.DeltaEncodeUInt32Slice(data)
	rbtree.DeltaDecodeUInt32Slice(data)

	assert.Equal(t, original, data)
}

// TestDeltaEncode_AllSame verifies round-trip on identical values.
func TestDeltaEncode_AllSame(t *testing.T) {
	t.Parallel()

	original := make([]uint32, deltaTestSize)
	for i := range original {
		original[i] = deltaTestConstVal
	}

	data := make([]uint32, len(original))
	copy(data, original)

	rbtree.DeltaEncodeUInt32Slice(data)

	// After encoding, first element unchanged, rest should be 0.
	assert.Equal(t, uint32(deltaTestConstVal), data[0])

	for i := 1; i < len(data); i++ {
		assert.Zero(t, data[i], "delta at index %d should be 0", i)
	}

	rbtree.DeltaDecodeUInt32Slice(data)
	assert.Equal(t, original, data)
}

// TestDeltaEncode_Empty verifies no-op on empty slice.
func TestDeltaEncode_Empty(t *testing.T) {
	t.Parallel()

	var data []uint32

	rbtree.DeltaEncodeUInt32Slice(data)
	rbtree.DeltaDecodeUInt32Slice(data)

	assert.Nil(t, data)
}

// TestDeltaEncode_SingleElement verifies single-element slice is unchanged.
func TestDeltaEncode_SingleElement(t *testing.T) {
	t.Parallel()

	data := []uint32{42}

	rbtree.DeltaEncodeUInt32Slice(data)
	assert.Equal(t, uint32(42), data[0])

	rbtree.DeltaDecodeUInt32Slice(data)
	assert.Equal(t, uint32(42), data[0])
}

// TestDeltaEncode_Random verifies round-trip on unsorted data.
func TestDeltaEncode_Random(t *testing.T) {
	t.Parallel()

	rng := newDeltaRNG(42)

	original := make([]uint32, deltaTestSize)
	for i := range original {
		original[i] = uint32(rng.next())
	}

	data := make([]uint32, len(original))
	copy(data, original)

	rbtree.DeltaEncodeUInt32Slice(data)
	rbtree.DeltaDecodeUInt32Slice(data)

	assert.Equal(t, original, data)
}

// TestDeltaEncode_MaxValues verifies overflow wraps correctly.
func TestDeltaEncode_MaxValues(t *testing.T) {
	t.Parallel()

	original := []uint32{0, 1, ^uint32(0), ^uint32(0) - 1, 0}

	data := make([]uint32, len(original))
	copy(data, original)

	rbtree.DeltaEncodeUInt32Slice(data)
	rbtree.DeltaDecodeUInt32Slice(data)

	assert.Equal(t, original, data)
}

// TestDeltaEncode_CompressionImprovement verifies delta encoding improves
// LZ4 compression ratio for sorted data.
func TestDeltaEncode_CompressionImprovement(t *testing.T) {
	t.Parallel()

	// Create sorted key data simulating RBTree keys.
	data := make([]uint32, deltaBenchSize)
	for i := range data {
		data[i] = uint32(i)
	}

	// Compress without delta encoding.
	plainCompressed := rbtree.CompressUInt32Slice(data)
	require.NotNil(t, plainCompressed)

	// Compress with delta encoding.
	deltaData := make([]uint32, len(data))
	copy(deltaData, data)

	rbtree.DeltaEncodeUInt32Slice(deltaData)

	deltaCompressed := rbtree.CompressUInt32Slice(deltaData)
	require.NotNil(t, deltaCompressed)

	// Delta-encoded version should compress significantly better.
	assert.Less(t, len(deltaCompressed), len(plainCompressed),
		"delta-encoded data should compress better than plain for sorted keys")
}

// deltaRNG is a simple splitmix64 PRNG for deterministic delta encoding tests.
type deltaRNG struct {
	state uint64
}

// deltaRNG mixing constants.
const (
	deltaRNGInc    = 0x9e3779b97f4a7c15
	deltaRNGMix1   = 0xbf58476d1ce4e5b9
	deltaRNGMix2   = 0x94d049bb133111eb
	deltaRNGShift1 = 30
	deltaRNGShift2 = 27
	deltaRNGShift3 = 31
)

func newDeltaRNG(seed uint64) *deltaRNG {
	return &deltaRNG{state: seed}
}

func (r *deltaRNG) next() uint64 {
	r.state += deltaRNGInc

	z := r.state
	z = (z ^ (z >> deltaRNGShift1)) * deltaRNGMix1
	z = (z ^ (z >> deltaRNGShift2)) * deltaRNGMix2

	return z ^ (z >> deltaRNGShift3)
}

// BenchmarkCompress_Plain benchmarks LZ4 compression without delta encoding.
func BenchmarkCompress_Plain(b *testing.B) {
	data := make([]uint32, deltaBenchSize)
	for i := range data {
		data[i] = uint32(i * deltaBenchSortStep)
	}

	b.ResetTimer()

	for range b.N {
		buf := make([]uint32, len(data))
		copy(buf, data)

		rbtree.CompressUInt32Slice(buf)
	}
}

// BenchmarkCompress_DeltaEncoded benchmarks delta encoding + LZ4 compression.
func BenchmarkCompress_DeltaEncoded(b *testing.B) {
	data := make([]uint32, deltaBenchSize)
	for i := range data {
		data[i] = uint32(i * deltaBenchSortStep)
	}

	b.ResetTimer()

	for range b.N {
		buf := make([]uint32, len(data))
		copy(buf, data)

		rbtree.DeltaEncodeUInt32Slice(buf)
		rbtree.CompressUInt32Slice(buf)
	}
}

// BenchmarkDeltaEncode benchmarks delta encoding alone.
func BenchmarkDeltaEncode(b *testing.B) {
	data := make([]uint32, deltaBenchSize)
	for i := range data {
		data[i] = uint32(i * deltaBenchSortStep)
	}

	b.ResetTimer()

	for range b.N {
		buf := make([]uint32, len(data))
		copy(buf, data)

		rbtree.DeltaEncodeUInt32Slice(buf)
	}
}

// BenchmarkDeltaDecode benchmarks delta decoding alone.
func BenchmarkDeltaDecode(b *testing.B) {
	data := make([]uint32, deltaBenchSize)
	for i := range data {
		data[i] = deltaBenchSortStep
	}

	data[0] = 0

	b.ResetTimer()

	for range b.N {
		buf := make([]uint32, len(data))
		copy(buf, data)

		rbtree.DeltaDecodeUInt32Slice(buf)
	}
}
