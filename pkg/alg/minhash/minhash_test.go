package minhash

import (
	"encoding/binary"
	"fmt"
	"math"
	"sync"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func (s *Signature) Merge(other *Signature) error {
	if other == nil {
		return ErrNilSignature
	}

	if s == other {
		return nil
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	other.mu.Lock()
	defer other.mu.Unlock()

	if len(s.mins) != len(other.mins) {
		return ErrSizeMismatch
	}

	for i := range s.mins {
		if other.mins[i] < s.mins[i] {
			s.mins[i] = other.mins[i]
		}
	}

	return nil
}

func FromBytes(data []byte) (*Signature, error) {
	if len(data) < HeaderSize {
		return nil, ErrInvalidData
	}

	numHashes := int(binary.BigEndian.Uint32(data[:HeaderSize]))
	if numHashes <= 0 {
		return nil, ErrZeroNumHashes
	}

	expectedLen := HeaderSize + numHashes*BytesPerHash
	if len(data) != expectedLen {
		return nil, ErrInvalidData
	}

	mins := make([]uint64, numHashes)

	for i := range numHashes {
		offset := HeaderSize + i*BytesPerHash
		mins[i] = binary.BigEndian.Uint64(data[offset : offset+BytesPerHash])
	}

	return &Signature{
		mins:  mins,
		seeds: generateSeeds(numHashes),
	}, nil
}

func (s *Signature) Reset() {
	s.mu.Lock()
	defer s.mu.Unlock()

	for i := range s.mins {
		s.mins[i] = math.MaxUint64
	}
}

func (s *Signature) Clone() *Signature {
	s.mu.Lock()
	defer s.mu.Unlock()

	mins := make([]uint64, len(s.mins))
	copy(mins, s.mins)

	seeds := make([]uint64, len(s.seeds))
	copy(seeds, s.seeds)

	return &Signature{
		mins:  mins,
		seeds: seeds,
	}
}

func (s *Signature) IsEmpty() bool {
	s.mu.Lock()
	defer s.mu.Unlock()

	for _, v := range s.mins {
		if v != math.MaxUint64 {
			return false
		}
	}

	return true
}

// Test constants for MinHash tests.
const (
	// testNumHashes is the default number of hash functions used in tests.
	testNumHashes = 128

	// testSmallNumHashes is a small number of hash functions for focused tests.
	testSmallNumHashes = 16

	// testOverlapSetSize is the number of tokens per set in overlap tests.
	testOverlapSetSize = 1000

	// testOverlapTolerance is the allowed deviation from expected Jaccard similarity.
	testOverlapTolerance = 0.1

	// testDisjointThreshold is the maximum expected similarity for disjoint sets.
	testDisjointThreshold = 0.1

	// testConcurrentGoroutines is the number of goroutines for concurrency tests.
	testConcurrentGoroutines = 100

	// testConcurrentTokensPerGoroutine is the number of tokens each goroutine adds.
	testConcurrentTokensPerGoroutine = 100
)

// --- Constructor Tests ---.

func TestNew_ValidNumHashes(t *testing.T) {
	t.Parallel()

	sig, err := New(testNumHashes)

	require.NoError(t, err)
	require.NotNil(t, sig)
	assert.Equal(t, testNumHashes, sig.Len())
}

func TestNew_SmallNumHashes(t *testing.T) {
	t.Parallel()

	sig, err := New(1)

	require.NoError(t, err)
	require.NotNil(t, sig)
	assert.Equal(t, 1, sig.Len())
}

func TestNew_ZeroNumHashes(t *testing.T) {
	t.Parallel()

	sig, err := New(0)

	require.Error(t, err)
	assert.Nil(t, sig)
	assert.ErrorIs(t, err, ErrZeroNumHashes)
}

// --- Add Tests ---.

func TestAdd_SingleToken(t *testing.T) {
	t.Parallel()

	sig, err := New(testSmallNumHashes)
	require.NoError(t, err)

	sig.Add([]byte("hello"))

	// After adding a token, at least some minimums should change from MaxUint64.
	assert.False(t, sig.IsEmpty(), "signature should not be empty after Add")
}

func TestAdd_NilToken(t *testing.T) {
	t.Parallel()

	sig, err := New(testSmallNumHashes)
	require.NoError(t, err)

	// Adding nil should not panic.
	sig.Add(nil)
}

func TestAdd_EmptyToken(t *testing.T) {
	t.Parallel()

	sig, err := New(testSmallNumHashes)
	require.NoError(t, err)

	sig.Add([]byte{})

	// Should be valid — empty byte slice is a valid token.
	assert.False(t, sig.IsEmpty())
}

// --- Similarity Tests ---.

func TestSimilarity_Identical(t *testing.T) {
	t.Parallel()

	sigA, err := New(testNumHashes)
	require.NoError(t, err)

	sigB, err := New(testNumHashes)
	require.NoError(t, err)

	tokens := []string{"func", "main", "return", "if", "else"}
	for _, tok := range tokens {
		sigA.Add([]byte(tok))
		sigB.Add([]byte(tok))
	}

	sim, err := sigA.Similarity(sigB)

	require.NoError(t, err)
	assert.InDelta(t, 1.0, sim, 0.001, "identical sets should have similarity 1.0")
}

func TestSimilarity_Disjoint(t *testing.T) {
	t.Parallel()

	sigA, err := New(testNumHashes)
	require.NoError(t, err)

	sigB, err := New(testNumHashes)
	require.NoError(t, err)

	for i := range testOverlapSetSize {
		sigA.Add(fmt.Appendf(nil, "tokenA_%d", i))
		sigB.Add(fmt.Appendf(nil, "tokenB_%d", i))
	}

	sim, err := sigA.Similarity(sigB)

	require.NoError(t, err)
	assert.Less(t, sim, testDisjointThreshold,
		"disjoint sets should have similarity near 0.0, got %f", sim)
}

func TestSimilarity_PartialOverlap(t *testing.T) {
	t.Parallel()

	sigA, err := New(testNumHashes)
	require.NoError(t, err)

	sigB, err := New(testNumHashes)
	require.NoError(t, err)

	// Create sets with 50% overlap:
	// A = {shared_0..shared_499, uniqueA_0..uniqueA_499}
	// B = {shared_0..shared_499, uniqueB_0..uniqueB_499}
	// Jaccard = 500 / 1500 = 0.333.
	halfSize := testOverlapSetSize / 2

	for i := range halfSize {
		shared := fmt.Appendf(nil, "shared_%d", i)
		sigA.Add(shared)
		sigB.Add(shared)
	}

	for i := range halfSize {
		sigA.Add(fmt.Appendf(nil, "uniqueA_%d", i))
		sigB.Add(fmt.Appendf(nil, "uniqueB_%d", i))
	}

	sim, err := sigA.Similarity(sigB)

	require.NoError(t, err)

	// Jaccard(A, B) = |A ∩ B| / |A ∪ B| = 500 / 1500 ≈ 0.333.
	expectedJaccard := 1.0 / 3.0
	assert.InDelta(t, expectedJaccard, sim, testOverlapTolerance,
		"50%% overlap should have Jaccard near 0.333, got %f", sim)
}

func TestSimilarity_HighOverlap(t *testing.T) {
	t.Parallel()

	sigA, err := New(testNumHashes)
	require.NoError(t, err)

	sigB, err := New(testNumHashes)
	require.NoError(t, err)

	// 90% shared tokens, 10% unique.
	sharedCount := 900
	uniqueCount := 100

	for i := range sharedCount {
		shared := fmt.Appendf(nil, "shared_%d", i)
		sigA.Add(shared)
		sigB.Add(shared)
	}

	for i := range uniqueCount {
		sigA.Add(fmt.Appendf(nil, "uniqueA_%d", i))
		sigB.Add(fmt.Appendf(nil, "uniqueB_%d", i))
	}

	sim, err := sigA.Similarity(sigB)

	require.NoError(t, err)

	// Jaccard = 900 / (900 + 100 + 100) = 900/1100 ≈ 0.818.
	expectedJaccard := 900.0 / 1100.0
	assert.InDelta(t, expectedJaccard, sim, testOverlapTolerance,
		"high overlap should have Jaccard near 0.818, got %f", sim)
}

func TestSimilarity_SizeMismatch(t *testing.T) {
	t.Parallel()

	sigA, err := New(testNumHashes)
	require.NoError(t, err)

	sigB, err := New(testSmallNumHashes)
	require.NoError(t, err)

	_, err = sigA.Similarity(sigB)

	require.Error(t, err)
	assert.ErrorIs(t, err, ErrSizeMismatch)
}

func TestSimilarity_Empty(t *testing.T) {
	t.Parallel()

	sigA, err := New(testNumHashes)
	require.NoError(t, err)

	sigB, err := New(testNumHashes)
	require.NoError(t, err)

	sim, err := sigA.Similarity(sigB)

	require.NoError(t, err)
	assert.InDelta(t, 1.0, sim, 0.001, "two empty signatures should have similarity 1.0")
}

func TestSimilarity_NilOther(t *testing.T) {
	t.Parallel()

	sig, err := New(testNumHashes)
	require.NoError(t, err)

	_, err = sig.Similarity(nil)

	require.Error(t, err)
	assert.ErrorIs(t, err, ErrNilSignature)
}

// --- Merge Tests ---.

func TestMerge_Basic(t *testing.T) {
	t.Parallel()

	sigA, err := New(testSmallNumHashes)
	require.NoError(t, err)

	sigB, err := New(testSmallNumHashes)
	require.NoError(t, err)

	sigA.Add([]byte("alpha"))
	sigB.Add([]byte("beta"))

	err = sigA.Merge(sigB)
	require.NoError(t, err)

	// After merge, sigA should have element-wise min of both.
	// This means sigA should be similar to a signature built from both tokens.
	sigCombined, err := New(testSmallNumHashes)
	require.NoError(t, err)

	sigCombined.Add([]byte("alpha"))
	sigCombined.Add([]byte("beta"))

	sim, err := sigA.Similarity(sigCombined)
	require.NoError(t, err)
	assert.InDelta(t, 1.0, sim, 0.001, "merged signature should match combined")
}

func TestMerge_SizeMismatch(t *testing.T) {
	t.Parallel()

	sigA, err := New(testNumHashes)
	require.NoError(t, err)

	sigB, err := New(testSmallNumHashes)
	require.NoError(t, err)

	err = sigA.Merge(sigB)

	require.Error(t, err)
	assert.ErrorIs(t, err, ErrSizeMismatch)
}

func TestMerge_NilOther(t *testing.T) {
	t.Parallel()

	sig, err := New(testNumHashes)
	require.NoError(t, err)

	err = sig.Merge(nil)

	require.Error(t, err)
	assert.ErrorIs(t, err, ErrNilSignature)
}

// --- Serialization Tests ---.

func TestBytes_FromBytes_RoundTrip(t *testing.T) {
	t.Parallel()

	sig, err := New(testNumHashes)
	require.NoError(t, err)

	sig.Add([]byte("hello"))
	sig.Add([]byte("world"))

	data := sig.Bytes()

	restored, err := FromBytes(data)

	require.NoError(t, err)
	require.NotNil(t, restored)
	assert.Equal(t, sig.Len(), restored.Len())

	sim, err := sig.Similarity(restored)
	require.NoError(t, err)
	assert.InDelta(t, 1.0, sim, 0.001, "round-trip should produce identical signature")
}

func TestFromBytes_InvalidData_TooShort(t *testing.T) {
	t.Parallel()

	_, err := FromBytes([]byte{1, 2})

	require.Error(t, err)
}

func TestFromBytes_InvalidData_WrongLength(t *testing.T) {
	t.Parallel()

	// Header says 128 hashes but only 10 bytes of data.
	data := make([]byte, HeaderSize+10)

	data[0] = 0
	data[1] = 0
	data[2] = 0
	data[3] = byte(testNumHashes)

	_, err := FromBytes(data)

	require.Error(t, err)
}

func TestFromBytes_ZeroHashes(t *testing.T) {
	t.Parallel()

	data := make([]byte, HeaderSize)

	_, err := FromBytes(data)

	require.Error(t, err)
}

// --- Reset Tests ---.

func TestReset(t *testing.T) {
	t.Parallel()

	sig, err := New(testSmallNumHashes)
	require.NoError(t, err)

	sig.Add([]byte("token"))
	assert.False(t, sig.IsEmpty())

	sig.Reset()

	assert.True(t, sig.IsEmpty(), "signature should be empty after Reset")
}

// --- Clone Tests ---.

func TestClone(t *testing.T) {
	t.Parallel()

	sig, err := New(testSmallNumHashes)
	require.NoError(t, err)

	sig.Add([]byte("hello"))

	cloned := sig.Clone()
	require.NotNil(t, cloned)

	// Cloned should be identical.
	sim, err := sig.Similarity(cloned)
	require.NoError(t, err)
	assert.InDelta(t, 1.0, sim, 0.001)

	// Modifying clone should not affect original.
	cloned.Add([]byte("world"))

	sim2, err := sig.Similarity(cloned)
	require.NoError(t, err)
	assert.Less(t, sim2, 1.0, "clone should be independent")
}

// --- IsEmpty Tests ---.

func TestIsEmpty_New(t *testing.T) {
	t.Parallel()

	sig, err := New(testSmallNumHashes)
	require.NoError(t, err)

	assert.True(t, sig.IsEmpty())
}

func TestIsEmpty_AfterAdd(t *testing.T) {
	t.Parallel()

	sig, err := New(testSmallNumHashes)
	require.NoError(t, err)

	sig.Add([]byte("token"))

	assert.False(t, sig.IsEmpty())
}

// --- Determinism Tests ---.

func TestDeterministic(t *testing.T) {
	t.Parallel()

	sigA, err := New(testNumHashes)
	require.NoError(t, err)

	sigB, err := New(testNumHashes)
	require.NoError(t, err)

	tokens := []string{"func", "main", "return", "if", "else", "for", "range"}
	for _, tok := range tokens {
		sigA.Add([]byte(tok))
		sigB.Add([]byte(tok))
	}

	sim, err := sigA.Similarity(sigB)

	require.NoError(t, err)
	assert.InDelta(t, 1.0, sim, 0.001, "same tokens in same order should produce identical signatures")
}

// --- Concurrent Access Tests ---.

func TestConcurrent_Add(t *testing.T) {
	t.Parallel()

	sig, err := New(testNumHashes)
	require.NoError(t, err)

	var wg sync.WaitGroup

	for g := range testConcurrentGoroutines {
		wg.Add(1)

		go func(id int) {
			defer wg.Done()

			for i := range testConcurrentTokensPerGoroutine {
				sig.Add(fmt.Appendf(nil, "goroutine_%d_token_%d", id, i))
			}
		}(g)
	}

	wg.Wait()

	// Signature should not be empty after concurrent adds.
	assert.False(t, sig.IsEmpty())
}

// --- Len Tests ---.

func TestLen(t *testing.T) {
	t.Parallel()

	sig, err := New(testNumHashes)
	require.NoError(t, err)

	assert.Equal(t, testNumHashes, sig.Len())
}

// --- Accuracy Tests ---.

func TestAccuracy_KnownJaccard(t *testing.T) {
	t.Parallel()

	// Build sets with known Jaccard index.
	// A = {0, 1, ..., 99}, B = {50, 51, ..., 149}
	// |A ∩ B| = 50, |A ∪ B| = 150
	// Jaccard = 50/150 = 1/3 ≈ 0.333.
	sigA, err := New(testNumHashes)
	require.NoError(t, err)

	sigB, err := New(testNumHashes)
	require.NoError(t, err)

	setSize := 100

	for i := range setSize {
		sigA.Add(fmt.Appendf(nil, "element_%d", i))
	}

	for i := range setSize {
		sigB.Add(fmt.Appendf(nil, "element_%d", i+setSize/2))
	}

	sim, err := sigA.Similarity(sigB)

	require.NoError(t, err)

	expectedJaccard := float64(setSize/2) / float64(setSize+setSize/2)
	assert.InDelta(t, expectedJaccard, sim, testOverlapTolerance,
		"expected Jaccard ~%.3f, got %.3f", expectedJaccard, sim)
}

// --- Seed Generation Tests ---.

func TestSeedGeneration_Deterministic(t *testing.T) {
	t.Parallel()

	sigA, err := New(testSmallNumHashes)
	require.NoError(t, err)

	sigB, err := New(testSmallNumHashes)
	require.NoError(t, err)

	// Both should generate the same seeds.
	sigA.Add([]byte("test"))
	sigB.Add([]byte("test"))

	sim, err := sigA.Similarity(sigB)

	require.NoError(t, err)
	assert.InDelta(t, 1.0, sim, 0.001, "deterministic seeds should produce identical results")
}

// --- Bytes Size Tests ---.

func TestBytes_CorrectSize(t *testing.T) {
	t.Parallel()

	sig, err := New(testNumHashes)
	require.NoError(t, err)

	data := sig.Bytes()

	// Header (4 bytes for numHashes) + numHashes * 8 bytes per uint64.
	expectedSize := HeaderSize + testNumHashes*BytesPerHash
	assert.Len(t, data, expectedSize)
}

// --- Edge Case: Very Large Signature ---.

func TestNew_LargeNumHashes(t *testing.T) {
	t.Parallel()

	sig, err := New(1024)

	require.NoError(t, err)
	require.NotNil(t, sig)
	assert.Equal(t, 1024, sig.Len())
}

// --- IsEmpty after Reset ---.

func TestIsEmpty_AfterReset(t *testing.T) {
	t.Parallel()

	sig, err := New(testSmallNumHashes)
	require.NoError(t, err)

	sig.Add([]byte("token"))
	sig.Reset()

	// All minimums should be back to MaxUint64.
	for i := range testSmallNumHashes {
		data := sig.Bytes()
		// Skip header, read uint64 at position i.
		offset := HeaderSize + i*BytesPerHash
		val := uint64(data[offset])<<56 | uint64(data[offset+1])<<48 |
			uint64(data[offset+2])<<40 | uint64(data[offset+3])<<32 |
			uint64(data[offset+4])<<24 | uint64(data[offset+5])<<16 |
			uint64(data[offset+6])<<8 | uint64(data[offset+7])
		assert.Equal(t, uint64(math.MaxUint64), val, "min[%d] should be MaxUint64 after reset", i)
	}
}
