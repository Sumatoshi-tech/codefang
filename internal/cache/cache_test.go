package cache

import (
	"errors"
	"sync"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

var errComputeFailed = errors.New("compute failed")

func TestHashSet_AddContains(t *testing.T) {
	t.Parallel()

	set := NewHashSet()

	hash := gitlib.Hash{0x01, 0x02, 0x03}

	// Initially not contained.
	assert.False(t, set.Contains(hash))

	// Add returns true for new hash.
	assert.True(t, set.Add(hash))
	assert.True(t, set.Contains(hash))

	// Add returns false for existing hash.
	assert.False(t, set.Add(hash))
}

func TestHashSet_Len(t *testing.T) {
	t.Parallel()

	set := NewHashSet()

	assert.Equal(t, 0, set.Len())

	set.Add(gitlib.Hash{0x01})
	assert.Equal(t, 1, set.Len())

	set.Add(gitlib.Hash{0x02})
	assert.Equal(t, 2, set.Len())

	// Duplicate doesn't increase len.
	set.Add(gitlib.Hash{0x01})
	assert.Equal(t, 2, set.Len())
}

func TestHashSet_Clear(t *testing.T) {
	t.Parallel()

	set := NewHashSet()

	set.Add(gitlib.Hash{0x01})
	set.Add(gitlib.Hash{0x02})
	assert.Equal(t, 2, set.Len())

	set.Clear()
	assert.Equal(t, 0, set.Len())
	assert.False(t, set.Contains(gitlib.Hash{0x01}))
}

func TestHashSet_Concurrent(t *testing.T) {
	t.Parallel()

	set := NewHashSet()

	var wg sync.WaitGroup

	// Concurrent adds.
	for i := range 100 {
		wg.Add(1)

		go func(i int) {
			defer wg.Done()

			hash := gitlib.Hash{byte(i)}
			set.Add(hash)
		}(i)
	}

	wg.Wait()

	assert.Equal(t, 100, set.Len())

	// Concurrent reads.
	for i := range 100 {
		wg.Add(1)

		go func(i int) {
			defer wg.Done()

			hash := gitlib.Hash{byte(i)}
			assert.True(t, set.Contains(hash))
		}(i)
	}

	wg.Wait()
}

func TestBlobCache_GetSet(t *testing.T) {
	t.Parallel()

	cache := NewBlobCache[string]()

	hash := gitlib.Hash{0x01, 0x02, 0x03}

	// Initially not found.
	val, found := cache.Get(hash)
	assert.False(t, found)
	assert.Empty(t, val)

	// Set and get.
	cache.Set(hash, "test-value")

	val, found = cache.Get(hash)
	assert.True(t, found)
	assert.Equal(t, "test-value", val)
}

func TestBlobCache_GetOrCompute(t *testing.T) {
	t.Parallel()

	cache := NewBlobCache[int]()

	hash := gitlib.Hash{0x01, 0x02, 0x03}
	computeCount := 0

	compute := func() (int, error) {
		computeCount++

		return 42, nil
	}

	// First call computes.
	val, err := cache.GetOrCompute(hash, compute)
	require.NoError(t, err)
	assert.Equal(t, 42, val)
	assert.Equal(t, 1, computeCount)

	// Second call uses cache.
	val, err = cache.GetOrCompute(hash, compute)
	require.NoError(t, err)
	assert.Equal(t, 42, val)
	assert.Equal(t, 1, computeCount) // Not incremented.
}

func TestBlobCache_GetOrCompute_Error(t *testing.T) {
	t.Parallel()

	cache := NewBlobCache[int]()

	hash := gitlib.Hash{0x01, 0x02, 0x03}
	expectedErr := errComputeFailed

	compute := func() (int, error) {
		return 0, expectedErr
	}

	val, err := cache.GetOrCompute(hash, compute)
	require.ErrorIs(t, err, expectedErr)
	assert.Equal(t, 0, val)

	// Value should not be cached on error.
	_, found := cache.Get(hash)
	assert.False(t, found)
}

func TestBlobCache_Len(t *testing.T) {
	t.Parallel()

	cache := NewBlobCache[string]()

	assert.Equal(t, 0, cache.Len())

	cache.Set(gitlib.Hash{0x01}, "a")
	assert.Equal(t, 1, cache.Len())

	cache.Set(gitlib.Hash{0x02}, "b")
	assert.Equal(t, 2, cache.Len())

	// Overwrite doesn't change len.
	cache.Set(gitlib.Hash{0x01}, "c")
	assert.Equal(t, 2, cache.Len())
}

func TestBlobCache_Clear(t *testing.T) {
	t.Parallel()

	cache := NewBlobCache[string]()

	cache.Set(gitlib.Hash{0x01}, "a")
	cache.Set(gitlib.Hash{0x02}, "b")
	assert.Equal(t, 2, cache.Len())

	cache.Clear()
	assert.Equal(t, 0, cache.Len())

	_, found := cache.Get(gitlib.Hash{0x01})
	assert.False(t, found)
}

func TestBlobCache_Concurrent(t *testing.T) {
	t.Parallel()

	cache := NewBlobCache[int]()

	var wg sync.WaitGroup

	// Concurrent writes.
	for i := range 100 {
		wg.Add(1)

		go func(i int) {
			defer wg.Done()

			hash := gitlib.Hash{byte(i)}
			cache.Set(hash, i)
		}(i)
	}

	wg.Wait()

	assert.Equal(t, 100, cache.Len())

	// Concurrent reads.
	for i := range 100 {
		wg.Add(1)

		go func(i int) {
			defer wg.Done()

			hash := gitlib.Hash{byte(i)}
			val, found := cache.Get(hash)
			assert.True(t, found)
			assert.Equal(t, i, val)
		}(i)
	}

	wg.Wait()
}
