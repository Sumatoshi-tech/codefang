package lsh

import (
	"fmt"
	"sync"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/minhash"
)

func (idx *Index) Size() int {
	idx.mu.RLock()
	defer idx.mu.RUnlock()

	return len(idx.sigs)
}

func (idx *Index) Clear() {
	idx.mu.Lock()
	defer idx.mu.Unlock()

	for i := range idx.bands {
		idx.bands[i] = make(map[uint64]map[string]bool)
	}

	idx.sigs = make(map[string]*minhash.Signature)
}

func (idx *Index) NumBands() int {
	return idx.numBands
}

func (idx *Index) NumRows() int {
	return idx.numRows
}

// Test constants for LSH tests.
const (
	// testBands is the default number of bands for tests.
	testBands = 16

	// testRows is the default number of rows per band for tests.
	testRows = 8

	// testNumHashes is the total number of hash functions (bands * rows).
	testNumHashes = testBands * testRows

	// testLargeIndexSize is the number of signatures for large index tests.
	testLargeIndexSize = 1000

	// testHighThreshold is the similarity threshold for high-similarity queries.
	testHighThreshold = 0.8

	// testLowThreshold is a low similarity threshold.
	testLowThreshold = 0.0

	// testConcurrentGoroutines is the number of goroutines for concurrency tests.
	testConcurrentGoroutines = 50

	// testConcurrentOpsPerGoroutine is the number of operations per goroutine.
	testConcurrentOpsPerGoroutine = 20
)

// --- Constructor Tests ---.

func TestNew_Valid(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)

	require.NoError(t, err)
	require.NotNil(t, idx)
	assert.Equal(t, testBands, idx.NumBands())
	assert.Equal(t, testRows, idx.NumRows())
	assert.Equal(t, 0, idx.Size())
}

func TestNew_ZeroBands(t *testing.T) {
	t.Parallel()

	idx, err := New(0, testRows)

	require.Error(t, err)
	assert.Nil(t, idx)
	assert.ErrorIs(t, err, ErrInvalidParams)
}

func TestNew_ZeroRows(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, 0)

	require.Error(t, err)
	assert.Nil(t, idx)
	assert.ErrorIs(t, err, ErrInvalidParams)
}

// --- Insert and Query Tests ---.

func TestInsert_Query_Duplicate(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)
	require.NoError(t, err)

	// Create two identical signatures.
	sigA, err := minhash.New(testNumHashes)
	require.NoError(t, err)

	sigB, err := minhash.New(testNumHashes)
	require.NoError(t, err)

	tokens := []string{"func", "main", "return", "if", "else", "for", "range", "var", "int", "string"}
	for _, tok := range tokens {
		sigA.Add([]byte(tok))
		sigB.Add([]byte(tok))
	}

	err = idx.Insert("funcA", sigA)
	require.NoError(t, err)

	// Query with identical signature.
	candidates, err := idx.Query(sigB)
	require.NoError(t, err)
	assert.Contains(t, candidates, "funcA")
}

func TestInsert_Query_Dissimilar(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)
	require.NoError(t, err)

	sigA, err := minhash.New(testNumHashes)
	require.NoError(t, err)

	sigB, err := minhash.New(testNumHashes)
	require.NoError(t, err)

	// Completely different token sets.
	for i := range testLargeIndexSize {
		sigA.Add(fmt.Appendf(nil, "tokenA_%d", i))
	}

	for i := range testLargeIndexSize {
		sigB.Add(fmt.Appendf(nil, "tokenB_%d", i))
	}

	err = idx.Insert("funcA", sigA)
	require.NoError(t, err)

	candidates, err := idx.Query(sigB)
	require.NoError(t, err)

	// Dissimilar signatures should not share any bands.
	assert.NotContains(t, candidates, "funcA")
}

func TestInsert_Query_SimilarPair(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)
	require.NoError(t, err)

	sigA, err := minhash.New(testNumHashes)
	require.NoError(t, err)

	sigB, err := minhash.New(testNumHashes)
	require.NoError(t, err)

	// Create highly similar signatures (90% shared tokens).
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

	err = idx.Insert("funcA", sigA)
	require.NoError(t, err)

	candidates, err := idx.Query(sigB)
	require.NoError(t, err)
	assert.Contains(t, candidates, "funcA", "similar signatures should be candidates")
}

// --- QueryThreshold Tests ---.

func TestQueryThreshold_FiltersCorrectly(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)
	require.NoError(t, err)

	// Insert a similar and a dissimilar signature.
	sigSimilar, err := minhash.New(testNumHashes)
	require.NoError(t, err)

	sigDifferent, err := minhash.New(testNumHashes)
	require.NoError(t, err)

	sigQuery, err := minhash.New(testNumHashes)
	require.NoError(t, err)

	// Similar: 90% shared tokens.
	for i := range 900 {
		shared := fmt.Appendf(nil, "shared_%d", i)
		sigSimilar.Add(shared)
		sigQuery.Add(shared)
	}

	for i := range 100 {
		sigSimilar.Add(fmt.Appendf(nil, "simUnique_%d", i))
		sigQuery.Add(fmt.Appendf(nil, "queryUnique_%d", i))
	}

	// Different: completely different tokens.
	for i := range testLargeIndexSize {
		sigDifferent.Add(fmt.Appendf(nil, "different_%d", i))
	}

	err = idx.Insert("similar", sigSimilar)
	require.NoError(t, err)

	err = idx.Insert("different", sigDifferent)
	require.NoError(t, err)

	// QueryThreshold with high threshold should only return the similar one.
	results, err := idx.QueryThreshold(sigQuery, testHighThreshold)
	require.NoError(t, err)

	assert.Contains(t, results, "similar")
	assert.NotContains(t, results, "different")
}

func TestQueryThreshold_ZeroThreshold(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)
	require.NoError(t, err)

	sig, err := minhash.New(testNumHashes)
	require.NoError(t, err)

	sig.Add([]byte("token"))

	err = idx.Insert("funcA", sig)
	require.NoError(t, err)

	// Zero threshold should return all candidates.
	results, err := idx.QueryThreshold(sig, testLowThreshold)
	require.NoError(t, err)
	assert.Contains(t, results, "funcA")
}

// --- Empty Index Tests ---.

func TestQuery_EmptyIndex(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)
	require.NoError(t, err)

	sig, err := minhash.New(testNumHashes)
	require.NoError(t, err)

	sig.Add([]byte("token"))

	candidates, err := idx.Query(sig)
	require.NoError(t, err)
	assert.Empty(t, candidates)
}

// --- Nil Signature Tests ---.

func TestInsert_NilSignature(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)
	require.NoError(t, err)

	err = idx.Insert("funcA", nil)

	require.Error(t, err)
	assert.ErrorIs(t, err, ErrNilSignature)
}

func TestQuery_NilSignature(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)
	require.NoError(t, err)

	_, err = idx.Query(nil)

	require.Error(t, err)
	assert.ErrorIs(t, err, ErrNilSignature)
}

func TestQueryThreshold_NilSignature(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)
	require.NoError(t, err)

	_, err = idx.QueryThreshold(nil, testHighThreshold)

	require.Error(t, err)
	assert.ErrorIs(t, err, ErrNilSignature)
}

// --- Size Mismatch Tests ---.

func TestInsert_SizeMismatch(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)
	require.NoError(t, err)

	// Create a signature with wrong number of hashes.
	wrongSig, err := minhash.New(testNumHashes + 1)
	require.NoError(t, err)

	err = idx.Insert("funcA", wrongSig)

	require.Error(t, err)
	assert.ErrorIs(t, err, ErrSizeMismatch)
}

func TestQuery_SizeMismatch(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)
	require.NoError(t, err)

	wrongSig, err := minhash.New(testNumHashes + 1)
	require.NoError(t, err)

	_, err = idx.Query(wrongSig)

	require.Error(t, err)
	assert.ErrorIs(t, err, ErrSizeMismatch)
}

// --- Size and Clear Tests ---.

func TestSize(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)
	require.NoError(t, err)

	assert.Equal(t, 0, idx.Size())

	sig, err := minhash.New(testNumHashes)
	require.NoError(t, err)

	sig.Add([]byte("token"))

	err = idx.Insert("funcA", sig)
	require.NoError(t, err)
	assert.Equal(t, 1, idx.Size())

	err = idx.Insert("funcB", sig)
	require.NoError(t, err)
	assert.Equal(t, 2, idx.Size())
}

func TestClear(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)
	require.NoError(t, err)

	sig, err := minhash.New(testNumHashes)
	require.NoError(t, err)

	sig.Add([]byte("token"))

	err = idx.Insert("funcA", sig)
	require.NoError(t, err)

	idx.Clear()

	assert.Equal(t, 0, idx.Size())

	candidates, err := idx.Query(sig)
	require.NoError(t, err)
	assert.Empty(t, candidates)
}

// --- Duplicate Insert Test ---.

func TestInsert_DuplicateID(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)
	require.NoError(t, err)

	sig, err := minhash.New(testNumHashes)
	require.NoError(t, err)

	sig.Add([]byte("token"))

	err = idx.Insert("funcA", sig)
	require.NoError(t, err)

	// Insert same ID again — should update, not duplicate.
	err = idx.Insert("funcA", sig)
	require.NoError(t, err)

	assert.Equal(t, 1, idx.Size())

	candidates, err := idx.Query(sig)
	require.NoError(t, err)

	// funcA should appear at most once.
	count := 0

	for _, c := range candidates {
		if c == "funcA" {
			count++
		}
	}

	assert.Equal(t, 1, count, "duplicate ID should appear only once in results")
}

// --- Concurrent Access Tests ---.

func TestConcurrent_InsertQuery(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)
	require.NoError(t, err)

	var wg sync.WaitGroup

	// Half goroutines insert, half query.
	for g := range testConcurrentGoroutines {
		wg.Add(1)

		go func(id int) {
			defer wg.Done()

			for i := range testConcurrentOpsPerGoroutine {
				sig, sigErr := minhash.New(testNumHashes)
				if sigErr != nil {
					continue
				}

				sig.Add(fmt.Appendf(nil, "goroutine_%d_token_%d", id, i))

				if id%2 == 0 {
					insertErr := idx.Insert(fmt.Sprintf("func_%d_%d", id, i), sig)
					_ = insertErr
				} else {
					candidates, queryErr := idx.Query(sig)
					_ = candidates
					_ = queryErr
				}
			}
		}(g)
	}

	wg.Wait()

	// Index should not be empty after concurrent inserts.
	assert.Positive(t, idx.Size())
}

// --- NumBands and NumRows Tests ---.

func TestNumBands(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)
	require.NoError(t, err)

	assert.Equal(t, testBands, idx.NumBands())
}

func TestNumRows(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)
	require.NoError(t, err)

	assert.Equal(t, testRows, idx.NumRows())
}

// --- Large Index Test ---.

func TestInsert_Query_LargeIndex(t *testing.T) {
	t.Parallel()

	idx, err := New(testBands, testRows)
	require.NoError(t, err)

	// Insert 1000 random signatures.
	for i := range testLargeIndexSize {
		sig, sigErr := minhash.New(testNumHashes)
		require.NoError(t, sigErr)

		for j := range 10 {
			sig.Add(fmt.Appendf(nil, "sig_%d_tok_%d", i, j))
		}

		err = idx.Insert(fmt.Sprintf("func_%d", i), sig)
		require.NoError(t, err)
	}

	assert.Equal(t, testLargeIndexSize, idx.Size())

	// Query with a signature identical to func_0.
	querySig, err := minhash.New(testNumHashes)
	require.NoError(t, err)

	for j := range 10 {
		querySig.Add(fmt.Appendf(nil, "sig_%d_tok_%d", 0, j))
	}

	candidates, err := idx.Query(querySig)
	require.NoError(t, err)
	assert.Contains(t, candidates, "func_0")
}
