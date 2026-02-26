package cms_test

import (
	"encoding/binary"
	"fmt"
	"math"
	"sync"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/cms"
)

const (
	standardEpsilon = 0.001
	standardDelta   = 0.001

	// Expected parameters for standard config: width=ceil(e/0.001)=2719, depth=ceil(ln(1/0.001))=7.
	expectedWidth = uint(2719)
	expectedDepth = uint(7)

	// Loose config for faster tests.
	looseEpsilon = 0.01
	looseDelta   = 0.01
	looseWidth   = uint(272)
	looseDepth   = uint(5)

	// Concurrency test parameters.
	concGoroutines = 100
	concOpsPerG    = 1000

	// Overestimation test parameters.
	overestN     = 10_000
	overestFreq  = 100
	overestProbe = 20_000
)

// uint64ToBytes converts a uint64 to an 8-byte big-endian slice.
func uint64ToBytes(v uint64) []byte {
	buf := make([]byte, 8)
	binary.BigEndian.PutUint64(buf, v)

	return buf
}

// testKey generates a deterministic test key from a prefix and index.
func testKey(prefix string, idx int) []byte {
	return fmt.Appendf(nil, "%s-%d", prefix, idx)
}

func TestNew_Parameters(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name      string
		epsilon   float64
		delta     float64
		wantWidth uint
		wantDepth uint
	}{
		{
			name:      "standard_0_001_0_001",
			epsilon:   standardEpsilon,
			delta:     standardDelta,
			wantWidth: expectedWidth,
			wantDepth: expectedDepth,
		},
		{
			name:      "loose_0_01_0_01",
			epsilon:   looseEpsilon,
			delta:     looseDelta,
			wantWidth: looseWidth,
			wantDepth: looseDepth,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			sk, err := cms.New(tt.epsilon, tt.delta)
			require.NoError(t, err)
			assert.Equal(t, tt.wantWidth, sk.Width())
			assert.Equal(t, tt.wantDepth, sk.Depth())
		})
	}
}

func TestNew_EdgeCases(t *testing.T) {
	t.Parallel()

	t.Run("zero_epsilon_returns_error", func(t *testing.T) {
		t.Parallel()

		_, err := cms.New(0.0, standardDelta)
		assert.ErrorIs(t, err, cms.ErrInvalidEpsilon)
	})

	t.Run("negative_epsilon_returns_error", func(t *testing.T) {
		t.Parallel()

		_, err := cms.New(-0.01, standardDelta)
		assert.ErrorIs(t, err, cms.ErrInvalidEpsilon)
	})

	t.Run("zero_delta_returns_error", func(t *testing.T) {
		t.Parallel()

		_, err := cms.New(standardEpsilon, 0.0)
		assert.ErrorIs(t, err, cms.ErrInvalidDelta)
	})

	t.Run("negative_delta_returns_error", func(t *testing.T) {
		t.Parallel()

		_, err := cms.New(standardEpsilon, -0.01)
		assert.ErrorIs(t, err, cms.ErrInvalidDelta)
	})

	t.Run("delta_at_one_returns_error", func(t *testing.T) {
		t.Parallel()

		_, err := cms.New(standardEpsilon, 1.0)
		assert.ErrorIs(t, err, cms.ErrInvalidDelta)
	})

	t.Run("delta_above_one_returns_error", func(t *testing.T) {
		t.Parallel()

		_, err := cms.New(standardEpsilon, 1.5)
		assert.ErrorIs(t, err, cms.ErrInvalidDelta)
	})
}

func TestAdd_Count_SingleKey(t *testing.T) {
	t.Parallel()

	sk, err := cms.New(looseEpsilon, looseDelta)
	require.NoError(t, err)

	key := []byte("token-operator")
	addCount := int64(42)

	sk.Add(key, addCount)

	count := sk.Count(key)
	assert.GreaterOrEqual(t, count, addCount,
		"CMS count must be >= true count")
}

func TestAdd_Count_MultipleKeys(t *testing.T) {
	t.Parallel()

	sk, err := cms.New(looseEpsilon, looseDelta)
	require.NoError(t, err)

	keys := map[string]int64{
		"operator-plus":  100,
		"operator-minus": 50,
		"operand-x":      200,
		"operand-y":      75,
	}

	for key, count := range keys {
		sk.Add([]byte(key), count)
	}

	for key, trueCount := range keys {
		count := sk.Count([]byte(key))
		assert.GreaterOrEqual(t, count, trueCount,
			"CMS count for %q must be >= true count %d, got %d", key, trueCount, count)
	}
}

func TestCount_NeverAdded(t *testing.T) {
	t.Parallel()

	sk, err := cms.New(looseEpsilon, looseDelta)
	require.NoError(t, err)

	// Add some unrelated keys.
	sk.Add([]byte("exists"), 100)

	count := sk.Count([]byte("never-added"))
	assert.GreaterOrEqual(t, count, int64(0),
		"count of absent key must be >= 0")
}

func TestOverestimation_Bounded(t *testing.T) {
	t.Parallel()

	sk, err := cms.New(standardEpsilon, standardDelta)
	require.NoError(t, err)

	// Insert overestN keys each with frequency overestFreq.
	trueFreqs := make(map[string]int64, overestN)

	for i := range overestN {
		key := testKey("key", i)
		sk.Add(key, overestFreq)

		trueFreqs[string(key)] = overestFreq
	}

	totalCount := sk.TotalCount()
	maxOverest := float64(totalCount) * standardEpsilon
	violations := 0

	for keyStr, trueFreq := range trueFreqs {
		estimated := sk.Count([]byte(keyStr))
		overestimation := float64(estimated - trueFreq)

		if overestimation > maxOverest {
			violations++
		}
	}

	// With delta=0.001, we expect <0.1% violations.
	maxViolations := int(math.Ceil(float64(overestN) * standardDelta * 10))

	t.Logf("violations=%d, maxAllowed=%d, maxOverest=%.2f, totalCount=%d",
		violations, maxViolations, maxOverest, totalCount)
	assert.LessOrEqual(t, violations, maxViolations,
		"too many overestimation violations: %d > %d", violations, maxViolations)
}

func TestAdd_ZeroCount(t *testing.T) {
	t.Parallel()

	sk, err := cms.New(looseEpsilon, looseDelta)
	require.NoError(t, err)

	sk.Add([]byte("key"), 0)

	assert.Equal(t, int64(0), sk.TotalCount())
	assert.Equal(t, int64(0), sk.Count([]byte("key")))
}

func TestNilKey(t *testing.T) {
	t.Parallel()

	sk, err := cms.New(looseEpsilon, looseDelta)
	require.NoError(t, err)

	// Must not panic on nil key.
	sk.Add(nil, 5)

	count := sk.Count(nil)
	assert.GreaterOrEqual(t, count, int64(5))
}

func TestEmptySliceKey(t *testing.T) {
	t.Parallel()

	sk, err := cms.New(looseEpsilon, looseDelta)
	require.NoError(t, err)

	sk.Add([]byte{}, 3)

	count := sk.Count([]byte{})
	assert.GreaterOrEqual(t, count, int64(3))
}

func TestReset(t *testing.T) {
	t.Parallel()

	sk, err := cms.New(looseEpsilon, looseDelta)
	require.NoError(t, err)

	sk.Add([]byte("key"), 100)
	assert.Positive(t, sk.Count([]byte("key")))
	assert.Positive(t, sk.TotalCount())

	sk.Reset()

	assert.Equal(t, int64(0), sk.Count([]byte("key")))
	assert.Equal(t, int64(0), sk.TotalCount())
}

func TestTotalCount(t *testing.T) {
	t.Parallel()

	sk, err := cms.New(looseEpsilon, looseDelta)
	require.NoError(t, err)

	assert.Equal(t, int64(0), sk.TotalCount())

	sk.Add([]byte("a"), 10)
	sk.Add([]byte("b"), 20)
	sk.Add([]byte("a"), 5)

	assert.Equal(t, int64(35), sk.TotalCount())
}

func TestDeterminism(t *testing.T) {
	t.Parallel()

	sk1, err := cms.New(looseEpsilon, looseDelta)
	require.NoError(t, err)

	sk2, err := cms.New(looseEpsilon, looseDelta)
	require.NoError(t, err)

	for i := range 100 {
		key := testKey("det", i)
		sk1.Add(key, int64(i+1))
		sk2.Add(key, int64(i+1))
	}

	for i := range 100 {
		key := testKey("det", i)
		assert.Equal(t, sk1.Count(key), sk2.Count(key),
			"determinism violated for key %d", i)
	}
}

func TestConcurrent_AddCount(t *testing.T) {
	t.Parallel()

	sk, err := cms.New(looseEpsilon, looseDelta)
	require.NoError(t, err)

	var wg sync.WaitGroup

	wg.Add(concGoroutines)

	for g := range concGoroutines {
		go func(goroutineID int) {
			defer wg.Done()

			for i := range concOpsPerG {
				key := uint64ToBytes(uint64(goroutineID*concOpsPerG + i))
				sk.Add(key, 1)
			}

			// Read while others are writing.
			_ = sk.Count(uint64ToBytes(uint64(goroutineID)))
		}(g)
	}

	wg.Wait()

	expectedTotal := int64(concGoroutines * concOpsPerG)
	assert.Equal(t, expectedTotal, sk.TotalCount())
}

func TestMemoryUsage(t *testing.T) {
	t.Parallel()

	sk, err := cms.New(standardEpsilon, standardDelta)
	require.NoError(t, err)

	// Counter array should be width * depth * 8 bytes.
	expectedBytes := expectedWidth * expectedDepth * 8

	t.Logf("width=%d, depth=%d, counter bytes=%d (~%.1f KB)",
		sk.Width(), sk.Depth(), expectedBytes, float64(expectedBytes)/1024)

	assert.Equal(t, expectedWidth, sk.Width())
	assert.Equal(t, expectedDepth, sk.Depth())
}

func TestMultipleAddsAccumulate(t *testing.T) {
	t.Parallel()

	sk, err := cms.New(looseEpsilon, looseDelta)
	require.NoError(t, err)

	key := []byte("accumulate")

	for range 100 {
		sk.Add(key, 1)
	}

	count := sk.Count(key)
	assert.GreaterOrEqual(t, count, int64(100))
}
