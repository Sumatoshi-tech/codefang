// FRD: specs/frds/FRD-20260302-generic-lru-cache.md.
package lru_test

import (
	"encoding/binary"
	"sync"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/lru"
)

const (
	// testMaxEntries is the default max entries for count-based tests.
	testMaxEntries = 100

	// smallMaxEntries limits the cache to 3 entries for eviction tests.
	smallMaxEntries = 3

	// testBloomExpectedN is the expected element count for Bloom filter tests.
	testBloomExpectedN = 1000

	// testBloomInsertCount is the number of items to insert for Bloom filter tests.
	testBloomInsertCount = 100

	// testBloomProbeCount is the number of absent items to probe.
	testBloomProbeCount = 200

	// testMaxBytes is a small byte limit for size-based tests.
	testMaxBytes = 100

	// testConcurrentGoroutines is the number of goroutines for concurrency tests.
	testConcurrentGoroutines = 50

	// testConcurrentOps is the number of operations per goroutine.
	testConcurrentOps = 100

	// testEvictionSampleSize is the sample size for cost-based eviction tests.
	testEvictionSampleSize = 5
)

// intToBytes converts an int key to bytes for Bloom filter tests.
func intToBytes(k int) []byte {
	var buf [8]byte

	binary.BigEndian.PutUint64(buf[:], uint64(k))

	return buf[:]
}

// intValueSize returns the "size" of an int value for size-based tests.
func intValueSize(v int) int64 {
	return int64(v)
}

func TestCache_GetPut_CountBased(t *testing.T) {
	t.Parallel()

	cache := lru.New(lru.WithMaxEntries[int, string](testMaxEntries))

	// Get on empty cache returns zero value, false.
	got, found := cache.Get(1)
	assert.False(t, found)
	assert.Empty(t, got)

	// Put and Get.
	cache.Put(1, "hello")

	got, found = cache.Get(1)
	require.True(t, found)
	assert.Equal(t, "hello", got)
}

func TestCache_LRUEviction_CountBased(t *testing.T) {
	t.Parallel()

	cache := lru.New(lru.WithMaxEntries[int, string](smallMaxEntries))

	cache.Put(1, "a")
	cache.Put(2, "b")
	cache.Put(3, "c")

	// Access key 1 to make it recently used.
	cache.Get(1)

	// Adding key 4 should evict key 2 (LRU).
	cache.Put(4, "d")

	_, found := cache.Get(2)
	assert.False(t, found, "key 2 should be evicted (LRU)")

	_, found = cache.Get(1)
	assert.True(t, found, "key 1 should still exist (recently accessed)")

	_, found = cache.Get(3)
	assert.True(t, found, "key 3 should still exist")

	_, found = cache.Get(4)
	assert.True(t, found, "key 4 should exist")
}

func TestCache_DuplicatePut(t *testing.T) {
	t.Parallel()

	cache := lru.New(lru.WithMaxEntries[int, string](testMaxEntries))

	cache.Put(1, "first")
	cache.Put(1, "second")

	got, found := cache.Get(1)
	require.True(t, found)
	assert.Equal(t, "second", got, "duplicate Put should update value")
	assert.Equal(t, 1, cache.Len())
}

func TestCache_Clear(t *testing.T) {
	t.Parallel()

	cache := lru.New(lru.WithMaxEntries[int, string](testMaxEntries))

	cache.Put(1, "a")
	cache.Put(2, "b")
	assert.Equal(t, 2, cache.Len())

	cache.Clear()

	assert.Equal(t, 0, cache.Len())

	_, found := cache.Get(1)
	assert.False(t, found)
}

func TestCache_Stats_CountBased(t *testing.T) {
	t.Parallel()

	cache := lru.New(lru.WithMaxEntries[int, string](testMaxEntries))

	cache.Put(1, "a")
	cache.Get(1) // Hit.
	cache.Get(2) // Miss.

	stats := cache.Stats()
	assert.Equal(t, int64(1), stats.Hits)
	assert.Equal(t, int64(1), stats.Misses)
	assert.Equal(t, 1, stats.Entries)
	assert.Equal(t, testMaxEntries, stats.MaxEntries)
	assert.InDelta(t, 0.5, stats.HitRate(), 0.001)
}

func TestStats_HitRate_Empty(t *testing.T) {
	t.Parallel()

	stats := lru.Stats{}
	assert.InDelta(t, 0.0, stats.HitRate(), 0.001)
}

func TestCache_CacheHitsMisses(t *testing.T) {
	t.Parallel()

	cache := lru.New(lru.WithMaxEntries[int, string](testMaxEntries))

	cache.Put(1, "a")
	cache.Get(1)
	cache.Get(2)

	assert.Equal(t, int64(1), cache.CacheHits())
	assert.Equal(t, int64(1), cache.CacheMisses())
}

func TestCache_SizeBased(t *testing.T) {
	t.Parallel()

	// Size = value itself. Max 100 bytes.
	cache := lru.New(lru.WithMaxBytes[int, int](testMaxBytes, intValueSize))

	cache.Put(1, 40)
	cache.Put(2, 40)

	// Both should fit (80 < 100).
	_, found1 := cache.Get(1)
	_, found2 := cache.Get(2)

	assert.True(t, found1)
	assert.True(t, found2)

	// Access key 2 to make key 1 LRU.
	cache.Get(2)

	// Adding value=40 would exceed 100, so key 1 is evicted.
	cache.Put(3, 40)

	_, found1 = cache.Get(1)
	assert.False(t, found1, "key 1 should be evicted (size limit)")

	_, found2 = cache.Get(2)
	assert.True(t, found2, "key 2 should still exist")

	stats := cache.Stats()
	assert.Equal(t, int64(testMaxBytes), stats.MaxSize)
}

func TestCache_SizeBased_RejectOversized(t *testing.T) {
	t.Parallel()

	cache := lru.New(lru.WithMaxBytes[int, int](testMaxBytes, intValueSize))

	// Value larger than entire cache should be rejected.
	cache.Put(1, 200)

	_, found := cache.Get(1)
	assert.False(t, found, "oversized value should not be cached")
}

func TestCache_SizeBased_CurrentSize(t *testing.T) {
	t.Parallel()

	cache := lru.New(lru.WithMaxBytes[int, int](testMaxBytes, intValueSize))

	cache.Put(1, 30)
	cache.Put(2, 20)

	stats := cache.Stats()
	assert.Equal(t, int64(50), stats.CurrentSize)

	cache.Clear()

	stats = cache.Stats()
	assert.Equal(t, int64(0), stats.CurrentSize)
}

func TestCache_BloomFilter(t *testing.T) {
	t.Parallel()

	cache := lru.New(
		lru.WithMaxEntries[int, string](testBloomExpectedN),
		lru.WithBloomFilter[int, string](intToBytes, uint(testBloomExpectedN)),
	)

	// Insert items.
	for i := range testBloomInsertCount {
		cache.Put(i, "val")
	}

	// Query absent items — Bloom should filter most.
	for i := testBloomInsertCount; i < testBloomInsertCount+testBloomProbeCount; i++ {
		_, found := cache.Get(i)
		assert.False(t, found)
	}

	stats := cache.Stats()
	assert.Positive(t, stats.BloomFiltered,
		"Bloom filter should short-circuit at least some absent lookups")
}

func TestCache_BloomFilter_NoFalseNegatives(t *testing.T) {
	t.Parallel()

	cache := lru.New(
		lru.WithMaxEntries[int, string](testBloomExpectedN),
		lru.WithBloomFilter[int, string](intToBytes, uint(testBloomExpectedN)),
	)

	for i := range testBloomInsertCount {
		cache.Put(i, "val")
	}

	// Every inserted item must be found (no false negatives).
	for i := range testBloomInsertCount {
		_, found := cache.Get(i)
		require.True(t, found, "inserted key %d must be found (no false negatives)", i)
	}
}

func TestCache_BloomFilter_ResetOnClear(t *testing.T) {
	t.Parallel()

	cache := lru.New(
		lru.WithMaxEntries[int, string](testBloomExpectedN),
		lru.WithBloomFilter[int, string](intToBytes, uint(testBloomExpectedN)),
	)

	cache.Put(1, "val")

	_, found := cache.Get(1)
	require.True(t, found)

	cache.Clear()

	_, found = cache.Get(1)
	assert.False(t, found, "cleared key should not be found")

	stats := cache.Stats()
	assert.Positive(t, stats.BloomFiltered,
		"lookup after clear should be Bloom-filtered")
}

func TestCache_BloomFilter_EmptyCache(t *testing.T) {
	t.Parallel()

	cache := lru.New(
		lru.WithMaxEntries[int, string](testBloomExpectedN),
		lru.WithBloomFilter[int, string](intToBytes, uint(testBloomExpectedN)),
	)

	// Query absent keys on empty cache.
	for i := range testBloomProbeCount {
		cache.Get(i)
	}

	stats := cache.Stats()
	assert.Equal(t, int64(testBloomProbeCount), stats.Misses)
	assert.Equal(t, int64(testBloomProbeCount), stats.BloomFiltered,
		"all lookups on empty cache should be Bloom-filtered")
}

func TestCache_CostEviction(t *testing.T) {
	t.Parallel()

	// Cost = accessCount / sizeKB. Lower cost = evicted first.
	// Large, rarely-accessed items should be evicted before small, frequently-accessed ones.
	costFn := func(accessCount, sizeBytes int64) float64 {
		sizeKB := float64(sizeBytes) / 1024.0
		if sizeKB < 1 {
			sizeKB = 1
		}

		return float64(accessCount) / sizeKB
	}

	cache := lru.New(
		lru.WithMaxBytes[int, int](testMaxBytes, intValueSize),
		lru.WithCostEviction[int, int](testEvictionSampleSize, costFn),
	)

	// Insert a small item (size=10) and access it many times.
	cache.Put(1, 10)

	for range 10 {
		cache.Get(1)
	}

	// Insert a large item (size=40).
	cache.Put(2, 40)

	// Insert another item that triggers eviction.
	cache.Put(3, 40)

	// Key 2 (large, low access) should be evicted before key 1 (small, high access).
	_, found1 := cache.Get(1)
	assert.True(t, found1, "key 1 (small, frequently accessed) should survive")

	_, found3 := cache.Get(3)
	assert.True(t, found3, "key 3 (just inserted) should survive")
}

func TestCache_CloneFunc(t *testing.T) {
	t.Parallel()

	cloneCalled := false
	cloneFn := func(v []byte) []byte {
		cloneCalled = true
		clone := make([]byte, len(v))
		copy(clone, v)

		return clone
	}

	cache := lru.New(
		lru.WithMaxEntries[int, []byte](testMaxEntries),
		lru.WithCloneFunc[int, []byte](cloneFn),
	)

	original := []byte("hello")
	cache.Put(1, original)

	require.True(t, cloneCalled, "clone function should be called on Put")

	got, found := cache.Get(1)
	require.True(t, found)
	assert.Equal(t, original, got)

	// Modifying original should not affect cached value.
	original[0] = 'X'
	got2, _ := cache.Get(1)
	assert.Equal(t, byte('h'), got2[0], "cached value should be independent of original")
}

func TestCache_GetMulti(t *testing.T) {
	t.Parallel()

	cache := lru.New(lru.WithMaxEntries[int, string](testMaxEntries))

	cache.Put(1, "a")
	cache.Put(2, "b")

	found, missing := cache.GetMulti([]int{1, 2, 3})

	assert.Len(t, found, 2)
	assert.Len(t, missing, 1)
	assert.Equal(t, 3, missing[0])
	assert.Equal(t, "a", found[1])
	assert.Equal(t, "b", found[2])
}

func TestCache_GetMulti_WithBloom(t *testing.T) {
	t.Parallel()

	cache := lru.New(
		lru.WithMaxEntries[int, string](testBloomExpectedN),
		lru.WithBloomFilter[int, string](intToBytes, uint(testBloomExpectedN)),
	)

	// Insert only even-numbered keys.
	for i := range testBloomInsertCount {
		cache.Put(i*2, "val")
	}

	// Build batch with alternating present/absent keys.
	keys := make([]int, 0, testBloomInsertCount*2)

	for i := range testBloomInsertCount {
		keys = append(keys, i*2, i*2+1)
	}

	found, missing := cache.GetMulti(keys)

	assert.Len(t, found, testBloomInsertCount)
	assert.Len(t, missing, testBloomInsertCount)

	stats := cache.Stats()
	assert.Positive(t, stats.BloomFiltered,
		"GetMulti should Bloom-filter absent keys")
}

func TestCache_PutMulti(t *testing.T) {
	t.Parallel()

	cache := lru.New(lru.WithMaxEntries[int, string](testMaxEntries))

	items := map[int]string{
		1: "a",
		2: "b",
		3: "c",
	}

	cache.PutMulti(items)

	assert.Equal(t, 3, cache.Len())

	for k, want := range items {
		got, found := cache.Get(k)
		require.True(t, found)
		assert.Equal(t, want, got)
	}
}

func TestCache_ConcurrentAccess(t *testing.T) {
	t.Parallel()

	cache := lru.New(lru.WithMaxEntries[int, string](testMaxEntries))

	var wg sync.WaitGroup

	wg.Add(testConcurrentGoroutines)

	for g := range testConcurrentGoroutines {
		go func(id int) {
			defer wg.Done()

			for i := range testConcurrentOps {
				key := (id*testConcurrentOps + i) % testMaxEntries
				cache.Put(key, "data")
				cache.Get(key)
			}
		}(g)
	}

	wg.Wait()

	stats := cache.Stats()
	assert.Positive(t, stats.Entries)
}

func TestCache_NoPanicOnEmptyOptions(t *testing.T) {
	t.Parallel()

	// Passing no capacity option should panic.
	assert.Panics(t, func() {
		lru.New[int, string]()
	})
}

func TestCache_BothLimits(t *testing.T) {
	t.Parallel()

	// Both count and size limits. Whichever is hit first wins.
	cache := lru.New(
		lru.WithMaxEntries[int, int](10),
		lru.WithMaxBytes[int, int](testMaxBytes, intValueSize),
	)

	// Insert items of size 30 each. After 3 items (90 bytes), the 4th exceeds 100.
	cache.Put(1, 30)
	cache.Put(2, 30)
	cache.Put(3, 30)
	cache.Put(4, 30)

	// Key 1 should be evicted (size limit reached before count limit).
	_, found := cache.Get(1)
	assert.False(t, found, "key 1 should be evicted due to size limit")

	assert.LessOrEqual(t, cache.Len(), 10)
}

func TestCache_Len(t *testing.T) {
	t.Parallel()

	cache := lru.New(lru.WithMaxEntries[int, string](testMaxEntries))

	assert.Equal(t, 0, cache.Len())

	cache.Put(1, "a")
	assert.Equal(t, 1, cache.Len())

	cache.Put(2, "b")
	assert.Equal(t, 2, cache.Len())

	cache.Put(1, "updated")
	assert.Equal(t, 2, cache.Len(), "duplicate Put should not increase Len")
}
