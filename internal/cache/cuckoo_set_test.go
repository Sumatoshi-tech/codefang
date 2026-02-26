package cache_test

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/cache"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

const (
	// cuckooSetTestElements is the expected element count for CuckooHashSet tests.
	cuckooSetTestElements = 1000

	// cuckooSetFloatDelta is the tolerance for float comparisons.
	cuckooSetFloatDelta = 0.001
)

func TestCuckooHashSet_AddContains(t *testing.T) {
	t.Parallel()

	cs, err := cache.NewCuckooHashSet(cuckooSetTestElements)
	require.NoError(t, err)

	hash := gitlib.Hash{0x01, 0x02, 0x03}

	// Initially not contained.
	assert.False(t, cs.Contains(hash))

	// Add returns true (insertion succeeded).
	assert.True(t, cs.Add(hash))

	// Now contained.
	assert.True(t, cs.Contains(hash))
}

func TestCuckooHashSet_AddRemoveContains(t *testing.T) {
	t.Parallel()

	cs, err := cache.NewCuckooHashSet(cuckooSetTestElements)
	require.NoError(t, err)

	hash := gitlib.Hash{0xAA, 0xBB, 0xCC}

	cs.Add(hash)
	assert.True(t, cs.Contains(hash))

	// Remove returns true (found and removed).
	assert.True(t, cs.Remove(hash))

	// After removal, not contained.
	assert.False(t, cs.Contains(hash))
}

func TestCuckooHashSet_RemoveNonExistent(t *testing.T) {
	t.Parallel()

	cs, err := cache.NewCuckooHashSet(cuckooSetTestElements)
	require.NoError(t, err)

	hash := gitlib.Hash{0xDE, 0xAD}

	assert.False(t, cs.Remove(hash))
}

func TestCuckooHashSet_Len(t *testing.T) {
	t.Parallel()

	cs, err := cache.NewCuckooHashSet(cuckooSetTestElements)
	require.NoError(t, err)

	assert.Equal(t, uint(0), cs.Len())

	cs.Add(gitlib.Hash{0x01})
	assert.Equal(t, uint(1), cs.Len())

	cs.Add(gitlib.Hash{0x02})
	assert.Equal(t, uint(2), cs.Len())

	cs.Remove(gitlib.Hash{0x01})
	assert.Equal(t, uint(1), cs.Len())
}

func TestCuckooHashSet_Clear(t *testing.T) {
	t.Parallel()

	cs, err := cache.NewCuckooHashSet(cuckooSetTestElements)
	require.NoError(t, err)

	hash := gitlib.Hash{0x01}

	cs.Add(hash)
	assert.True(t, cs.Contains(hash))

	cs.Clear()

	assert.False(t, cs.Contains(hash))
	assert.Equal(t, uint(0), cs.Len())
}

func TestCuckooHashSet_LoadFactor(t *testing.T) {
	t.Parallel()

	cs, err := cache.NewCuckooHashSet(cuckooSetTestElements)
	require.NoError(t, err)

	assert.InDelta(t, 0.0, cs.LoadFactor(), cuckooSetFloatDelta)

	cs.Add(gitlib.Hash{0x01})

	assert.Positive(t, cs.LoadFactor())
}

func TestCuckooHashSet_ConstructorError(t *testing.T) {
	t.Parallel()

	cs, err := cache.NewCuckooHashSet(0)
	require.Error(t, err)
	assert.Nil(t, cs)
}

func TestCuckooHashSet_NoFalseNegatives(t *testing.T) {
	t.Parallel()

	cs, err := cache.NewCuckooHashSet(cuckooSetTestElements)
	require.NoError(t, err)

	// Insert elements.
	for i := range uint16(cuckooSetTestElements) {
		hash := makeTestHashU16(i)
		cs.Add(hash)
	}

	// Every inserted element must be found.
	for i := range uint16(cuckooSetTestElements) {
		hash := makeTestHashU16(i)
		assert.True(t, cs.Contains(hash), "inserted hash %d must be found", i)
	}
}

func TestCuckooHashSet_RemovePreservesOthers(t *testing.T) {
	t.Parallel()

	cs, err := cache.NewCuckooHashSet(cuckooSetTestElements)
	require.NoError(t, err)

	hashA := gitlib.Hash{0x01}
	hashB := gitlib.Hash{0x02}

	cs.Add(hashA)
	cs.Add(hashB)

	cs.Remove(hashA)

	assert.False(t, cs.Contains(hashA))
	assert.True(t, cs.Contains(hashB))
}

func TestCuckooHashSet_ZeroHash(t *testing.T) {
	t.Parallel()

	cs, err := cache.NewCuckooHashSet(cuckooSetTestElements)
	require.NoError(t, err)

	zeroHash := gitlib.ZeroHash()

	assert.False(t, cs.Contains(zeroHash))

	assert.True(t, cs.Add(zeroHash))
	assert.True(t, cs.Contains(zeroHash))

	assert.True(t, cs.Remove(zeroHash))
	assert.False(t, cs.Contains(zeroHash))
}
