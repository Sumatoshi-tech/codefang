package cuckoo

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// Test constants.
const (
	testCapSmall        = 100
	testCapMedium       = 1000
	testCapLarge        = 100000
	testFPRateThreshold = 0.03
	testFPTestCount     = 100000
	testLoadFactor95    = 0.95
	testFloatDelta      = 0.001
)

// TestNew_ValidCapacity verifies filter creation.
func TestNew_ValidCapacity(t *testing.T) {
	t.Parallel()

	f, err := New(testCapSmall)
	require.NoError(t, err)
	assert.NotNil(t, f)
	assert.Equal(t, uint(0), f.Count())
}

// TestNew_ZeroCapacity verifies zero capacity error.
func TestNew_ZeroCapacity(t *testing.T) {
	t.Parallel()

	f, err := New(0)
	require.ErrorIs(t, err, ErrZeroCapacity)
	assert.Nil(t, f)
}

// TestInsert_Lookup_Basic verifies basic insert and lookup.
func TestInsert_Lookup_Basic(t *testing.T) {
	t.Parallel()

	f, err := New(testCapSmall)
	require.NoError(t, err)

	data := []byte("hello")
	ok := f.Insert(data)
	assert.True(t, ok)
	assert.True(t, f.Lookup(data))
	assert.Equal(t, uint(1), f.Count())
}

// TestLookup_NotInserted verifies lookup of non-existent element.
func TestLookup_NotInserted(t *testing.T) {
	t.Parallel()

	f, err := New(testCapSmall)
	require.NoError(t, err)

	assert.False(t, f.Lookup([]byte("absent")))
}

// TestInsert_Delete_Lookup verifies insert-delete-lookup cycle.
func TestInsert_Delete_Lookup(t *testing.T) {
	t.Parallel()

	f, err := New(testCapSmall)
	require.NoError(t, err)

	data := []byte("deleteme")
	ok := f.Insert(data)
	assert.True(t, ok)
	assert.True(t, f.Lookup(data))

	deleted := f.Delete(data)
	assert.True(t, deleted)
	assert.False(t, f.Lookup(data))
	assert.Equal(t, uint(0), f.Count())
}

// TestDelete_NotInserted verifies deleting non-existent element.
func TestDelete_NotInserted(t *testing.T) {
	t.Parallel()

	f, err := New(testCapSmall)
	require.NoError(t, err)

	assert.False(t, f.Delete([]byte("ghost")))
}

// TestInsert_NoFalseNegatives verifies no false negatives.
func TestInsert_NoFalseNegatives(t *testing.T) {
	t.Parallel()

	f, err := New(testCapMedium)
	require.NoError(t, err)

	// Insert all elements.
	for i := range testCapMedium {
		data := intToBytes(i)
		ok := f.Insert(data)
		require.True(t, ok, "insert failed at element %d", i)
	}

	// Verify all present (no false negatives).
	for i := range testCapMedium {
		data := intToBytes(i)
		assert.True(t, f.Lookup(data), "false negative at element %d", i)
	}
}

// TestInsert_FilterFull verifies Insert returns false when full.
func TestInsert_FilterFull(t *testing.T) {
	t.Parallel()

	// Very small filter — will fill up quickly.
	f, err := New(bucketSize)
	require.NoError(t, err)

	failedInserts := 0
	// Try to insert more than the filter can hold.
	insertAttempts := int(f.Capacity()) * 4

	for i := range insertAttempts {
		data := intToBytes(i)
		if !f.Insert(data) {
			failedInserts++
		}
	}

	// At least some inserts should fail.
	assert.Positive(t, failedInserts)
}

// TestCount_AfterOperations verifies count after insert and delete.
func TestCount_AfterOperations(t *testing.T) {
	t.Parallel()

	f, err := New(testCapSmall)
	require.NoError(t, err)

	assert.Equal(t, uint(0), f.Count())

	f.Insert([]byte("a"))
	f.Insert([]byte("b"))
	f.Insert([]byte("c"))
	assert.Equal(t, uint(3), f.Count())

	f.Delete([]byte("b"))
	assert.Equal(t, uint(2), f.Count())
}

// TestReset verifies reset clears all elements.
func TestReset(t *testing.T) {
	t.Parallel()

	f, err := New(testCapSmall)
	require.NoError(t, err)

	f.Insert([]byte("x"))
	f.Insert([]byte("y"))
	assert.Equal(t, uint(2), f.Count())

	f.Reset()
	assert.Equal(t, uint(0), f.Count())
	assert.False(t, f.Lookup([]byte("x")))
	assert.False(t, f.Lookup([]byte("y")))
}

// TestLoadFactor verifies load factor computation.
func TestLoadFactor(t *testing.T) {
	t.Parallel()

	f, err := New(testCapSmall)
	require.NoError(t, err)

	assert.InDelta(t, 0.0, f.LoadFactor(), testFloatDelta)

	f.Insert([]byte("a"))
	assert.Positive(t, f.LoadFactor())
}

// TestCapacity verifies capacity is a multiple of bucket size.
func TestCapacity(t *testing.T) {
	t.Parallel()

	f, err := New(testCapSmall)
	require.NoError(t, err)

	assert.Equal(t, uint(0), f.Capacity()%bucketSize)
	assert.GreaterOrEqual(t, f.Capacity(), uint(testCapSmall))
}

// TestFalsePositiveRate verifies false-positive rate at high load.
func TestFalsePositiveRate(t *testing.T) {
	t.Parallel()

	f, err := New(testCapLarge)
	require.NoError(t, err)

	// Insert elements.
	inserted := 0

	for i := range testCapLarge {
		data := intToBytes(i)
		if f.Insert(data) {
			inserted++
		}
	}

	require.Positive(t, inserted)

	// Test false positives with non-inserted elements.
	falsePositives := 0

	for i := testCapLarge; i < testCapLarge+testFPTestCount; i++ {
		data := intToBytes(i)
		if f.Lookup(data) {
			falsePositives++
		}
	}

	fpRate := float64(falsePositives) / float64(testFPTestCount)
	assert.Less(t, fpRate, testFPRateThreshold, "false positive rate %.4f exceeds threshold %.4f", fpRate, testFPRateThreshold)
}

// TestDelete_AfterReinsert verifies delete works after re-insertion.
func TestDelete_AfterReinsert(t *testing.T) {
	t.Parallel()

	f, err := New(testCapSmall)
	require.NoError(t, err)

	data := []byte("reinsert")
	f.Insert(data)
	f.Delete(data)
	f.Insert(data)

	assert.True(t, f.Lookup(data))
	assert.Equal(t, uint(1), f.Count())
}

// TestMultipleInserts verifies multiple distinct elements.
func TestMultipleInserts(t *testing.T) {
	t.Parallel()

	f, err := New(testCapMedium)
	require.NoError(t, err)

	items := []string{"alpha", "beta", "gamma", "delta", "epsilon"}
	for _, item := range items {
		ok := f.Insert([]byte(item))
		assert.True(t, ok)
	}

	assert.Equal(t, uint(len(items)), f.Count())

	for _, item := range items {
		assert.True(t, f.Lookup([]byte(item)))
	}
}

// TestNextPowerOfTwo verifies power of two calculation.
func TestNextPowerOfTwo(t *testing.T) {
	t.Parallel()

	assert.Equal(t, uint(1), nextPowerOfTwo(0))
	assert.Equal(t, uint(1), nextPowerOfTwo(1))
	assert.Equal(t, uint(2), nextPowerOfTwo(2))
	assert.Equal(t, uint(4), nextPowerOfTwo(3))
	assert.Equal(t, uint(4), nextPowerOfTwo(4))
	assert.Equal(t, uint(8), nextPowerOfTwo(5))
	assert.Equal(t, uint(1024), nextPowerOfTwo(1000))
}

// TestDeriveFingerprint_NeverZero verifies fingerprint is always non-zero.
func TestDeriveFingerprint_NeverZero(t *testing.T) {
	t.Parallel()

	for i := range testCapMedium {
		data := intToBytes(i)
		h := fnvHash64(data)
		fp := deriveFingerprint(h)
		assert.NotEqual(t, fingerprint(0), fp, "fingerprint must never be zero for input %d", i)
	}
}

// TestAltIndex_Symmetry verifies alt index is symmetric.
func TestAltIndex_Symmetry(t *testing.T) {
	t.Parallel()

	f, err := New(testCapMedium)
	require.NoError(t, err)

	for i := range testCapSmall {
		data := intToBytes(i)
		fp, i1 := f.fingerprintAndIndex(data)
		i2 := f.altIndex(i1, fp)
		i1Again := f.altIndex(i2, fp)
		assert.Equal(t, i1, i1Again, "alt index must be symmetric for input %d", i)
	}
}
