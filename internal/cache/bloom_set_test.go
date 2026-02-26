package cache_test

import (
	"sync"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/cache"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

const (
	// bloomSetTestElements is the expected element count for most BloomHashSet tests.
	bloomSetTestElements = 1000

	// bloomSetTestFPRate is the false-positive rate for BloomHashSet tests.
	bloomSetTestFPRate = 0.01

	// bloomSetFPTestElements is the element count for the statistical FP rate test.
	bloomSetFPTestElements = 100_000

	// bloomSetFPProbeCount is the number of absent elements to probe in the FP rate test.
	bloomSetFPProbeCount = 100_000

	// bloomSetFPMaxRate is the maximum acceptable FP rate (2x configured rate).
	bloomSetFPMaxRate = 0.02

	// bloomSetConcurrentGoroutines is the number of goroutines for concurrency tests.
	bloomSetConcurrentGoroutines = 100

	// bloomSetConcurrentOps is the number of operations per goroutine in concurrency tests.
	bloomSetConcurrentOps = 1000
)

func TestBloomHashSet_AddContains(t *testing.T) {
	t.Parallel()

	bs, err := cache.NewBloomHashSet(bloomSetTestElements, bloomSetTestFPRate)
	require.NoError(t, err)

	hash := gitlib.Hash{0x01, 0x02, 0x03}

	// Initially not contained.
	assert.False(t, bs.Contains(hash))

	// Add returns true for new hash.
	assert.True(t, bs.Add(hash))

	// Now contained.
	assert.True(t, bs.Contains(hash))
}

func TestBloomHashSet_AddReturnsNewness(t *testing.T) {
	t.Parallel()

	bs, err := cache.NewBloomHashSet(bloomSetTestElements, bloomSetTestFPRate)
	require.NoError(t, err)

	hash := gitlib.Hash{0xAA, 0xBB, 0xCC}

	// First add: definitely new.
	isNew := bs.Add(hash)
	assert.True(t, isNew, "first Add should return true (definitely new)")

	// Second add: possibly present (Bloom says maybe).
	isNew = bs.Add(hash)
	assert.False(t, isNew, "second Add should return false (possibly present)")
}

func TestBloomHashSet_NoFalseNegatives(t *testing.T) {
	t.Parallel()

	bs, err := cache.NewBloomHashSet(bloomSetTestElements, bloomSetTestFPRate)
	require.NoError(t, err)

	// Insert elements.
	for i := range uint16(bloomSetTestElements) {
		hash := makeTestHashU16(i)
		bs.Add(hash)
	}

	// Every inserted element must be found (no false negatives).
	for i := range uint16(bloomSetTestElements) {
		hash := makeTestHashU16(i)
		assert.True(t, bs.Contains(hash), "inserted hash %d must be found", i)
	}
}

func TestBloomHashSet_FalsePositiveRate(t *testing.T) {
	t.Parallel()

	bs, err := cache.NewBloomHashSet(bloomSetFPTestElements, bloomSetTestFPRate)
	require.NoError(t, err)

	// Insert elements using hashes 0..N-1.
	for i := range uint32(bloomSetFPTestElements) {
		var hash gitlib.Hash

		hash[0] = byte(i >> 24)
		hash[1] = byte(i >> 16)
		hash[2] = byte(i >> 8)
		hash[3] = byte(i)

		bs.Add(hash)
	}

	// Probe absent elements using hashes N..2N-1.
	falsePositives := 0

	for i := uint32(bloomSetFPTestElements); i < uint32(bloomSetFPTestElements+bloomSetFPProbeCount); i++ {
		var hash gitlib.Hash

		hash[0] = byte(i >> 24)
		hash[1] = byte(i >> 16)
		hash[2] = byte(i >> 8)
		hash[3] = byte(i)

		if bs.Contains(hash) {
			falsePositives++
		}
	}

	fpRate := float64(falsePositives) / float64(bloomSetFPProbeCount)
	assert.LessOrEqual(t, fpRate, bloomSetFPMaxRate,
		"FP rate %.4f exceeds max %.4f (got %d false positives out of %d probes)",
		fpRate, bloomSetFPMaxRate, falsePositives, bloomSetFPProbeCount)
}

func TestBloomHashSet_Len(t *testing.T) {
	t.Parallel()

	bs, err := cache.NewBloomHashSet(bloomSetTestElements, bloomSetTestFPRate)
	require.NoError(t, err)

	assert.Equal(t, uint(0), bs.Len())

	bs.Add(gitlib.Hash{0x01})
	assert.Equal(t, uint(1), bs.Len())

	bs.Add(gitlib.Hash{0x02})
	assert.Equal(t, uint(2), bs.Len())
}

func TestBloomHashSet_Clear(t *testing.T) {
	t.Parallel()

	bs, err := cache.NewBloomHashSet(bloomSetTestElements, bloomSetTestFPRate)
	require.NoError(t, err)

	hash := gitlib.Hash{0x01}

	bs.Add(hash)
	assert.True(t, bs.Contains(hash))

	bs.Clear()

	// After clear, nothing should be contained.
	assert.False(t, bs.Contains(hash))
	assert.Equal(t, uint(0), bs.Len())
}

func TestBloomHashSet_ClearResetsFPRate(t *testing.T) {
	t.Parallel()

	bs, err := cache.NewBloomHashSet(bloomSetTestElements, bloomSetTestFPRate)
	require.NoError(t, err)

	// Fill the set to increase fill ratio.
	for i := range uint16(bloomSetTestElements) {
		bs.Add(makeTestHashU16(i))
	}

	fillBefore := bs.FillRatio()
	assert.Positive(t, fillBefore, "fill ratio should be positive after insertions")

	bs.Clear()

	fillAfter := bs.FillRatio()
	assert.InDelta(t, 0.0, fillAfter, 0.001, "fill ratio should be ~0 after clear")
}

func TestBloomHashSet_EmptyContains(t *testing.T) {
	t.Parallel()

	bs, err := cache.NewBloomHashSet(bloomSetTestElements, bloomSetTestFPRate)
	require.NoError(t, err)

	// Empty set should return false for all queries.
	for i := range 100 {
		hash := makeTestHash(byte(i))
		assert.False(t, bs.Contains(hash), "empty set should not contain hash %d", i)
	}
}

func TestBloomHashSet_Concurrent(t *testing.T) {
	t.Parallel()

	bs, err := cache.NewBloomHashSet(bloomSetConcurrentGoroutines*bloomSetConcurrentOps, bloomSetTestFPRate)
	require.NoError(t, err)

	var wg sync.WaitGroup

	// Concurrent adds and contains.
	for g := range bloomSetConcurrentGoroutines {
		wg.Add(1)

		go func(goroutineID int) {
			defer wg.Done()

			for i := range bloomSetConcurrentOps {
				var hash gitlib.Hash

				hash[0] = byte(goroutineID)
				hash[1] = byte(i >> 8)
				hash[2] = byte(i)

				bs.Add(hash)
				bs.Contains(hash)
			}
		}(g)
	}

	wg.Wait()

	// Verify no panics and positive count.
	assert.Positive(t, bs.Len(), "set should have elements after concurrent adds")
}

func TestBloomHashSet_FillRatio(t *testing.T) {
	t.Parallel()

	bs, err := cache.NewBloomHashSet(bloomSetTestElements, bloomSetTestFPRate)
	require.NoError(t, err)

	// Initially zero.
	assert.InDelta(t, 0.0, bs.FillRatio(), 0.001)

	// Add elements and verify fill ratio increases.
	for i := range uint16(bloomSetTestElements) {
		bs.Add(makeTestHashU16(i))
	}

	assert.Positive(t, bs.FillRatio(), "fill ratio should be positive after insertions")
	assert.LessOrEqual(t, bs.FillRatio(), 1.0, "fill ratio should not exceed 1.0")
}

func TestBloomHashSet_ConstructorErrors(t *testing.T) {
	t.Parallel()

	// Zero elements should error.
	_, err := cache.NewBloomHashSet(0, bloomSetTestFPRate)
	require.Error(t, err)

	// FP rate out of bounds.
	_, err = cache.NewBloomHashSet(bloomSetTestElements, 0.0)
	require.Error(t, err)

	_, err = cache.NewBloomHashSet(bloomSetTestElements, 1.0)
	require.Error(t, err)

	_, err = cache.NewBloomHashSet(bloomSetTestElements, -0.1)
	require.Error(t, err)
}

func TestBloomHashSet_ZeroHash(t *testing.T) {
	t.Parallel()

	bs, err := cache.NewBloomHashSet(bloomSetTestElements, bloomSetTestFPRate)
	require.NoError(t, err)

	zeroHash := gitlib.ZeroHash()

	assert.False(t, bs.Contains(zeroHash))

	assert.True(t, bs.Add(zeroHash))
	assert.True(t, bs.Contains(zeroHash))
}
