package bloom_test

import (
	"encoding/binary"
	"fmt"
	"sync"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/bloom"
)

const (
	standardN      = uint(10_000_000)
	standardFP     = 0.01
	smallN         = uint(1000)
	tightN         = uint(100)
	tightFP        = 0.001
	fpTestN        = uint(100_000)
	fpTestFP       = 0.01
	fpTestProbeN   = 200_000
	fpMargin       = 1.5 // Allow 50 percent above configured FP.
	concGoroutines = 100
	concOpsPerG    = 1000

	// Expected parameter values derived from formulas.
	expectedM10M1pct   = uint(95_850_584) // m = ceil(-10M * ln(0.01) / ln(2)^2).
	expectedK10M1pct   = uint(7)          // k = round(m/n * ln(2)).
	expectedM1K1pct    = uint(9586)       // m for n=1000, fp=0.01.
	expectedK1K1pct    = uint(7)
	expectedM100_01pct = uint(1438) // m for n=100, fp=0.001.
	expectedK100_01pct = uint(10)   // k = round(1438/100 * ln(2)).
)

// uint64ToBytes converts a uint64 to an 8-byte big-endian slice.
func uint64ToBytes(v uint64) []byte {
	buf := make([]byte, 8)
	binary.BigEndian.PutUint64(buf, v)

	return buf
}

func TestNewWithEstimates_Parameters(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		n     uint
		fp    float64
		wantM uint
		wantK uint
	}{
		{
			name:  "standard_10M_1pct",
			n:     standardN,
			fp:    standardFP,
			wantM: expectedM10M1pct,
			wantK: expectedK10M1pct,
		},
		{
			name:  "small_1000_1pct",
			n:     smallN,
			fp:    standardFP,
			wantM: expectedM1K1pct,
			wantK: expectedK1K1pct,
		},
		{
			name:  "tight_100_0_1pct",
			n:     tightN,
			fp:    tightFP,
			wantM: expectedM100_01pct,
			wantK: expectedK100_01pct,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			f, err := bloom.NewWithEstimates(tt.n, tt.fp)
			require.NoError(t, err)
			assert.Equal(t, tt.wantM, f.BitCount())
			assert.Equal(t, tt.wantK, f.HashCount())
		})
	}
}

func TestNewWithEstimates_EdgeCases(t *testing.T) {
	t.Parallel()

	t.Run("zero_n_returns_error", func(t *testing.T) {
		t.Parallel()

		_, err := bloom.NewWithEstimates(0, standardFP)
		assert.Error(t, err)
	})

	t.Run("zero_fp_returns_error", func(t *testing.T) {
		t.Parallel()

		_, err := bloom.NewWithEstimates(smallN, 0.0)
		assert.Error(t, err)
	})

	t.Run("fp_at_one_returns_error", func(t *testing.T) {
		t.Parallel()

		_, err := bloom.NewWithEstimates(smallN, 1.0)
		assert.Error(t, err)
	})

	t.Run("fp_above_one_returns_error", func(t *testing.T) {
		t.Parallel()

		_, err := bloom.NewWithEstimates(smallN, 1.5)
		assert.Error(t, err)
	})

	t.Run("negative_fp_returns_error", func(t *testing.T) {
		t.Parallel()

		_, err := bloom.NewWithEstimates(smallN, -0.01)
		assert.Error(t, err)
	})
}

func TestAdd_Test_NoFalseNegatives(t *testing.T) {
	t.Parallel()

	f, err := bloom.NewWithEstimates(smallN, standardFP)
	require.NoError(t, err)

	// Insert N elements.
	for i := range uint64(smallN) {
		f.Add(uint64ToBytes(i))
	}

	// Every inserted element must test positive.
	for i := range uint64(smallN) {
		assert.True(t, f.Test(uint64ToBytes(i)), "false negative for element %d", i)
	}
}

func TestTest_DefiniteAbsence(t *testing.T) {
	t.Parallel()

	f, err := bloom.NewWithEstimates(smallN, standardFP)
	require.NoError(t, err)

	// Empty filter must return false for any query.
	assert.False(t, f.Test([]byte("never-added")))
	assert.False(t, f.Test(uint64ToBytes(42)))
}

func TestTestAndAdd_FirstAndSecondCall(t *testing.T) {
	t.Parallel()

	f, err := bloom.NewWithEstimates(smallN, standardFP)
	require.NoError(t, err)

	data := []byte("unique-element")

	// First call: element not present.
	wasPresent := f.TestAndAdd(data)
	assert.False(t, wasPresent)

	// Second call: element now present.
	wasPresent = f.TestAndAdd(data)
	assert.True(t, wasPresent)
}

func TestAddBulk_TestBulk(t *testing.T) {
	t.Parallel()

	f, err := bloom.NewWithEstimates(smallN, standardFP)
	require.NoError(t, err)

	const bulkSize = 500

	items := make([][]byte, bulkSize)
	for i := range items {
		items[i] = uint64ToBytes(uint64(i))
	}

	f.AddBulk(items)

	results := f.TestBulk(items)
	require.Len(t, results, bulkSize)

	for i, present := range results {
		assert.True(t, present, "false negative in bulk test for element %d", i)
	}
}

func TestAddBulk_EmptySlice(t *testing.T) {
	t.Parallel()

	f, err := bloom.NewWithEstimates(smallN, standardFP)
	require.NoError(t, err)

	// Must not panic.
	f.AddBulk(nil)
	f.AddBulk([][]byte{})
	assert.Equal(t, uint(0), f.EstimatedCount())
}

func TestTestBulk_EmptySlice(t *testing.T) {
	t.Parallel()

	f, err := bloom.NewWithEstimates(smallN, standardFP)
	require.NoError(t, err)

	assert.Nil(t, f.TestBulk(nil))
	assert.Nil(t, f.TestBulk([][]byte{}))
}

func TestEstimatedCount(t *testing.T) {
	t.Parallel()

	f, err := bloom.NewWithEstimates(smallN, standardFP)
	require.NoError(t, err)

	assert.Equal(t, uint(0), f.EstimatedCount())

	const insertCount = 42

	for i := range uint64(insertCount) {
		f.Add(uint64ToBytes(i))
	}

	assert.Equal(t, uint(insertCount), f.EstimatedCount())
}

func TestReset(t *testing.T) {
	t.Parallel()

	f, err := bloom.NewWithEstimates(smallN, standardFP)
	require.NoError(t, err)

	data := []byte("to-be-reset")
	f.Add(data)
	assert.True(t, f.Test(data))
	assert.Equal(t, uint(1), f.EstimatedCount())

	f.Reset()

	assert.False(t, f.Test(data))
	assert.Equal(t, uint(0), f.EstimatedCount())
	assert.InDelta(t, 0.0, f.FillRatio(), 0.0001)
}

func TestFillRatio(t *testing.T) {
	t.Parallel()

	f, err := bloom.NewWithEstimates(smallN, standardFP)
	require.NoError(t, err)

	// Empty filter has zero fill ratio.
	assert.InDelta(t, 0.0, f.FillRatio(), 0.0001)

	// After insertions, fill ratio is positive.
	for i := range uint64(smallN) {
		f.Add(uint64ToBytes(i))
	}

	ratio := f.FillRatio()
	assert.Greater(t, ratio, 0.0)
	assert.LessOrEqual(t, ratio, 1.0)
}

func TestNilData(t *testing.T) {
	t.Parallel()

	f, err := bloom.NewWithEstimates(smallN, standardFP)
	require.NoError(t, err)

	// Must not panic on nil data.
	f.Add(nil)
	assert.True(t, f.Test(nil))

	// Empty slice behaves identically to nil.
	f2, err := bloom.NewWithEstimates(smallN, standardFP)
	require.NoError(t, err)
	f2.Add([]byte{})
	assert.True(t, f2.Test([]byte{}))
}

func TestFalsePositiveRate(t *testing.T) {
	t.Parallel()

	f, err := bloom.NewWithEstimates(fpTestN, fpTestFP)
	require.NoError(t, err)

	// Insert fpTestN elements with keys starting from zero.
	for i := range uint64(fpTestN) {
		f.Add(uint64ToBytes(i))
	}

	// Probe fpTestProbeN non-members using keys above the inserted range.
	falsePositives := 0

	for i := uint64(fpTestN); i < uint64(fpTestN)+uint64(fpTestProbeN); i++ {
		if f.Test(uint64ToBytes(i)) {
			falsePositives++
		}
	}

	observedRate := float64(falsePositives) / float64(fpTestProbeN)
	maxAllowed := fpTestFP * fpMargin

	t.Logf("false positive rate: %.4f%% (max allowed: %.4f%%)",
		observedRate*100, maxAllowed*100)
	assert.LessOrEqual(t, observedRate, maxAllowed,
		"FP rate %.4f exceeds maximum %.4f", observedRate, maxAllowed)
}

func TestConcurrent_AddTest(t *testing.T) {
	t.Parallel()

	f, err := bloom.NewWithEstimates(uint(concGoroutines*concOpsPerG), standardFP)
	require.NoError(t, err)

	var wg sync.WaitGroup

	wg.Add(concGoroutines)

	for g := range concGoroutines {
		go func(goroutineID int) {
			defer wg.Done()

			base := uint64(goroutineID) * uint64(concOpsPerG)

			for i := range uint64(concOpsPerG) {
				data := uint64ToBytes(base + i)
				f.Add(data)
			}

			// Verify our own insertions.
			for i := range uint64(concOpsPerG) {
				data := uint64ToBytes(base + i)
				assert.True(t, f.Test(data))
			}
		}(g)
	}

	wg.Wait()

	expectedCount := uint(concGoroutines * concOpsPerG)
	assert.Equal(t, expectedCount, f.EstimatedCount())
}

func TestMemoryUsage_10M_1pct(t *testing.T) {
	t.Parallel()

	f, err := bloom.NewWithEstimates(standardN, standardFP)
	require.NoError(t, err)

	// Bit array should stay well under 15 MB for 10M elements at 1% FP.
	const maxBytes = 15 * 1024 * 1024

	actualBytes := f.BitCount() / 8

	assert.LessOrEqual(t, actualBytes, uint(maxBytes),
		"filter uses %d bytes, exceeding %d byte limit", actualBytes, maxBytes)

	t.Logf("bit array: %d bits = %.2f MB.", f.BitCount(), float64(f.BitCount())/(8*1024*1024))
}

// testKey generates a deterministic test key from a prefix and index.
func testKey(prefix string, idx int) []byte {
	return fmt.Appendf(nil, "%s-%d", prefix, idx)
}

func TestTestBulk_MixedPresence(t *testing.T) {
	t.Parallel()

	f, err := bloom.NewWithEstimates(smallN, standardFP)
	require.NoError(t, err)

	const half = 50

	// Insert first half.
	for i := range half {
		f.Add(testKey("member", i))
	}

	// Build query with both members and non-members.
	queries := make([][]byte, half*2)

	for i := range half {
		queries[i] = testKey("member", i)
		queries[half+i] = testKey("nonmember", i)
	}

	results := f.TestBulk(queries)
	require.Len(t, results, half*2)

	// Members must all be true.
	for i := range half {
		assert.True(t, results[i], "member %d should be present", i)
	}
}
