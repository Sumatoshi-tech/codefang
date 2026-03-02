package hll_test

import (
	"encoding/binary"
	"fmt"
	"math"
	"sync"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/hll"
)

const (
	defaultPrecision = uint8(14)
	minPrecision     = uint8(4)
	maxPrecision     = uint8(18)
	belowMinPrec     = uint8(3)
	aboveMaxPrec     = uint8(19)

	// Register counts for known precisions.
	registersP4  = uint(1 << 4)  // 16.
	registersP14 = uint(1 << 14) // 16384.
	registersP18 = uint(1 << 18) // 262144.

	// Accuracy test parameters.
	accuracyMaxError = 0.03 // 3% relative error.

	// Concurrency test parameters.
	concGoroutines = 100
	concOpsPerG    = 1000

	// Duplicate count test parameters.
	duplicateCount     = 1000
	duplicateExpected  = uint64(1)
	duplicateMaxResult = uint64(2) // Allow small HLL noise.

	// Cardinality test sizes.
	cardN100  = 100
	cardN1K   = 1_000
	cardN10K  = 10_000
	cardN100K = 100_000
	cardN1M   = 1_000_000
	cardN10M  = 10_000_000
)

// uint64ToBytes converts a uint64 to an 8-byte big-endian slice.
func uint64ToBytes(v uint64) []byte {
	buf := make([]byte, 8)
	binary.BigEndian.PutUint64(buf, v)

	return buf
}

func TestNew_Parameters(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name       string
		precision  uint8
		wantRegCnt uint
	}{
		{
			name:       "min_precision_4",
			precision:  minPrecision,
			wantRegCnt: registersP4,
		},
		{
			name:       "default_precision_14",
			precision:  defaultPrecision,
			wantRegCnt: registersP14,
		},
		{
			name:       "max_precision_18",
			precision:  maxPrecision,
			wantRegCnt: registersP18,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			sk, err := hll.New(tt.precision)
			require.NoError(t, err)
			assert.Equal(t, tt.precision, sk.Precision())
			assert.Equal(t, tt.wantRegCnt, sk.RegisterCount())
		})
	}
}

func TestNew_EdgeCases(t *testing.T) {
	t.Parallel()

	t.Run("below_min_precision_returns_error", func(t *testing.T) {
		t.Parallel()

		_, err := hll.New(belowMinPrec)
		assert.ErrorIs(t, err, hll.ErrPrecisionOutOfRange)
	})

	t.Run("above_max_precision_returns_error", func(t *testing.T) {
		t.Parallel()

		_, err := hll.New(aboveMaxPrec)
		assert.ErrorIs(t, err, hll.ErrPrecisionOutOfRange)
	})

	t.Run("zero_precision_returns_error", func(t *testing.T) {
		t.Parallel()

		_, err := hll.New(0)
		assert.ErrorIs(t, err, hll.ErrPrecisionOutOfRange)
	})
}

func TestCount_EmptySketch(t *testing.T) {
	t.Parallel()

	sk, err := hll.New(defaultPrecision)
	require.NoError(t, err)

	assert.Equal(t, uint64(0), sk.Count())
}

func TestAdd_Count_SingleElement(t *testing.T) {
	t.Parallel()

	sk, err := hll.New(defaultPrecision)
	require.NoError(t, err)

	sk.Add([]byte("hello"))

	count := sk.Count()
	assert.GreaterOrEqual(t, count, uint64(1))
	assert.LessOrEqual(t, count, uint64(2))
}

func TestAdd_Count_DuplicateElements(t *testing.T) {
	t.Parallel()

	sk, err := hll.New(defaultPrecision)
	require.NoError(t, err)

	data := []byte("same-element")

	for range duplicateCount {
		sk.Add(data)
	}

	count := sk.Count()
	assert.LessOrEqual(t, count, duplicateMaxResult,
		"adding same element %d times should produce count <= %d, got %d",
		duplicateCount, duplicateMaxResult, count)
}

func TestAccuracy_Ranges(t *testing.T) {
	t.Parallel()

	cardinalities := []int{
		cardN100,
		cardN1K,
		cardN10K,
		cardN100K,
		cardN1M,
		cardN10M,
	}

	for _, n := range cardinalities {
		t.Run(fmt.Sprintf("n_%d", n), func(t *testing.T) {
			t.Parallel()

			sk, err := hll.New(defaultPrecision)
			require.NoError(t, err)

			for i := range n {
				sk.Add(uint64ToBytes(uint64(i)))
			}

			count := sk.Count()
			expected := float64(n)
			relativeError := math.Abs(float64(count)-expected) / expected

			t.Logf("n=%d, count=%d, relError=%.4f%%", n, count, relativeError*100)
			assert.LessOrEqual(t, relativeError, accuracyMaxError,
				"relative error %.4f exceeds maximum %.4f for n=%d",
				relativeError, accuracyMaxError, n)
		})
	}
}

func TestDeterminism(t *testing.T) {
	t.Parallel()

	sk1, err := hll.New(defaultPrecision)
	require.NoError(t, err)

	sk2, err := hll.New(defaultPrecision)
	require.NoError(t, err)

	for i := range cardN1K {
		data := uint64ToBytes(uint64(i))
		sk1.Add(data)
		sk2.Add(data)
	}

	assert.Equal(t, sk1.Count(), sk2.Count())
}

func TestMerge_DisjointSets(t *testing.T) {
	t.Parallel()

	sk1, err := hll.New(defaultPrecision)
	require.NoError(t, err)

	sk2, err := hll.New(defaultPrecision)
	require.NoError(t, err)

	half := cardN10K / 2

	// First half goes to sk1.
	for i := range half {
		sk1.Add(uint64ToBytes(uint64(i)))
	}

	// Second half goes to sk2.
	for i := half; i < cardN10K; i++ {
		sk2.Add(uint64ToBytes(uint64(i)))
	}

	err = sk1.Merge(sk2)
	require.NoError(t, err)

	count := sk1.Count()
	expected := float64(cardN10K)
	relativeError := math.Abs(float64(count)-expected) / expected

	t.Logf("merged count=%d, expected=%d, relError=%.4f%%", count, cardN10K, relativeError*100)
	assert.LessOrEqual(t, relativeError, accuracyMaxError,
		"merge error %.4f exceeds maximum %.4f", relativeError, accuracyMaxError)
}

func TestMerge_OverlappingSets(t *testing.T) {
	t.Parallel()

	sk1, err := hll.New(defaultPrecision)
	require.NoError(t, err)

	sk2, err := hll.New(defaultPrecision)
	require.NoError(t, err)

	// sk1 has [0, 1000), sk2 has [500, 1500) — overlap of 500 elements.
	for i := range cardN1K {
		sk1.Add(uint64ToBytes(uint64(i)))
	}

	overlap := cardN1K / 2

	for i := overlap; i < cardN1K+overlap; i++ {
		sk2.Add(uint64ToBytes(uint64(i)))
	}

	err = sk1.Merge(sk2)
	require.NoError(t, err)

	count := sk1.Count()
	// Union is [0, 1500) = 1500 elements.
	expected := float64(cardN1K + overlap)
	relativeError := math.Abs(float64(count)-expected) / expected

	t.Logf("overlapping merge count=%d, expected=%d, relError=%.4f%%",
		count, int(expected), relativeError*100)
	assert.LessOrEqual(t, relativeError, accuracyMaxError)
}

func TestMerge_PrecisionMismatch(t *testing.T) {
	t.Parallel()

	sk1, err := hll.New(defaultPrecision)
	require.NoError(t, err)

	sk2, err := hll.New(minPrecision)
	require.NoError(t, err)

	err = sk1.Merge(sk2)
	assert.ErrorIs(t, err, hll.ErrPrecisionMismatch)
}

func TestMerge_EmptySketch(t *testing.T) {
	t.Parallel()

	sk1, err := hll.New(defaultPrecision)
	require.NoError(t, err)

	sk2, err := hll.New(defaultPrecision)
	require.NoError(t, err)

	for i := range cardN1K {
		sk1.Add(uint64ToBytes(uint64(i)))
	}

	countBefore := sk1.Count()

	err = sk1.Merge(sk2)
	require.NoError(t, err)

	assert.Equal(t, countBefore, sk1.Count())
}

func TestNilData(t *testing.T) {
	t.Parallel()

	sk, err := hll.New(defaultPrecision)
	require.NoError(t, err)

	// Must not panic on nil data.
	sk.Add(nil)

	count := sk.Count()
	assert.GreaterOrEqual(t, count, uint64(1))
}

func TestReset(t *testing.T) {
	t.Parallel()

	sk, err := hll.New(defaultPrecision)
	require.NoError(t, err)

	for i := range cardN1K {
		sk.Add(uint64ToBytes(uint64(i)))
	}

	assert.Positive(t, sk.Count())

	sk.Reset()

	assert.Equal(t, uint64(0), sk.Count())
	assert.Equal(t, defaultPrecision, sk.Precision())
}

func TestClone(t *testing.T) {
	t.Parallel()

	sk, err := hll.New(defaultPrecision)
	require.NoError(t, err)

	for i := range cardN1K {
		sk.Add(uint64ToBytes(uint64(i)))
	}

	clone := sk.Clone()

	// Clone must have same count.
	assert.Equal(t, sk.Count(), clone.Count())
	assert.Equal(t, sk.Precision(), clone.Precision())

	// Modifying clone must not affect original.
	for i := cardN1K; i < cardN10K; i++ {
		clone.Add(uint64ToBytes(uint64(i)))
	}

	// Original count should be unchanged.
	originalCount := sk.Count()
	cloneCount := clone.Count()

	assert.Greater(t, cloneCount, originalCount,
		"clone should have more elements after additional adds")
}

func TestConcurrent_AddCount(t *testing.T) {
	t.Parallel()

	sk, err := hll.New(defaultPrecision)
	require.NoError(t, err)

	var wg sync.WaitGroup

	wg.Add(concGoroutines)

	for g := range concGoroutines {
		go func(goroutineID int) {
			defer wg.Done()

			base := uint64(goroutineID) * uint64(concOpsPerG)

			for i := range uint64(concOpsPerG) {
				sk.Add(uint64ToBytes(base + i))
			}

			// Read while others are writing.
			_ = sk.Count()
		}(g)
	}

	wg.Wait()

	count := sk.Count()
	expected := float64(concGoroutines * concOpsPerG)
	relativeError := math.Abs(float64(count)-expected) / expected

	t.Logf("concurrent count=%d, expected=%d, relError=%.4f%%",
		count, int(expected), relativeError*100)
	assert.LessOrEqual(t, relativeError, accuracyMaxError)
}

func TestMemoryUsage_P14(t *testing.T) {
	t.Parallel()

	sk, err := hll.New(defaultPrecision)
	require.NoError(t, err)

	// Registers should be exactly 2^14 = 16384 bytes.
	assert.Equal(t, registersP14, sk.RegisterCount())

	t.Logf("register count: %d bytes", sk.RegisterCount())
}

func TestEmptySliceData(t *testing.T) {
	t.Parallel()

	sk, err := hll.New(defaultPrecision)
	require.NoError(t, err)

	// Empty slice should behave identically to nil.
	sk.Add([]byte{})

	count := sk.Count()
	assert.GreaterOrEqual(t, count, uint64(1))
}
