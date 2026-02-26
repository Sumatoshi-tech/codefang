package cache_test

import (
	"encoding/binary"
	"sync"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/cache"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

const (
	// bloomTestCacheSize is the cache size for Bloom filter tests (64 KB).
	bloomTestCacheSize = 64 * 1024

	// bloomTestInsertCount is the number of items to insert for Bloom filter tests.
	bloomTestInsertCount = 100

	// bloomTestProbeCount is the number of absent items to probe for Bloom filter tests.
	bloomTestProbeCount = 200

	// bloomTestBlobData is the standard blob data for Bloom filter tests.
	bloomTestBlobData = "bloom-test-data"
)

func makeTestBlob(data []byte) *gitlib.CachedBlob {
	return gitlib.NewCachedBlobForTest(data)
}

func makeTestHash(b byte) gitlib.Hash {
	var h gitlib.Hash

	h[0] = b

	return h
}

// makeTestHashU16 creates a test hash from a uint16 value for wider hash variety.
func makeTestHashU16(val uint16) gitlib.Hash {
	var h gitlib.Hash

	binary.BigEndian.PutUint16(h[:], val)

	return h
}

func TestLRUBlobCache_GetPut(t *testing.T) {
	t.Parallel()

	c := cache.NewLRUBlobCache(1024) // 1KB cache.

	hash := makeTestHash(1)
	blob := makeTestBlob([]byte("hello world"))

	// Get on empty cache returns nil.
	got := c.Get(hash)
	assert.Nil(t, got)

	// Put and Get.
	c.Put(hash, blob)
	got = c.Get(hash)
	require.NotNil(t, got)
	assert.Equal(t, blob.Data, got.Data)
}

func TestLRUBlobCache_LRUEviction(t *testing.T) {
	t.Parallel()

	// Cache with 100 bytes max.
	c := cache.NewLRUBlobCache(100)

	// Add 3 blobs of 40 bytes each (120 bytes total, exceeds limit).
	hash1 := makeTestHash(1)
	hash2 := makeTestHash(2)
	hash3 := makeTestHash(3)

	blob1 := makeTestBlob(make([]byte, 40))
	blob2 := makeTestBlob(make([]byte, 40))
	blob3 := makeTestBlob(make([]byte, 40))

	c.Put(hash1, blob1)
	c.Put(hash2, blob2)

	// Both should be in cache (80 bytes < 100).
	assert.NotNil(t, c.Get(hash1))
	assert.NotNil(t, c.Get(hash2))

	// Adding third blob should evict hash1 (LRU after accessing hash2 above).
	// Actually, after Get(hash1), Get(hash2), hash1 is most recent, hash2 is LRU.
	// Let's re-access to make hash2 most recent.
	c.Get(hash2)

	c.Put(hash3, blob3)

	// hash1 should be evicted (it was LRU).
	assert.Nil(t, c.Get(hash1), "hash1 should be evicted")
	assert.NotNil(t, c.Get(hash2), "hash2 should still be in cache")
	assert.NotNil(t, c.Get(hash3), "hash3 should be in cache")
}

func TestLRUBlobCache_SkipLargeBlobs(t *testing.T) {
	t.Parallel()

	c := cache.NewLRUBlobCache(100) // 100 bytes max.

	hash := makeTestHash(1)
	blob := makeTestBlob(make([]byte, 200)) // 200 bytes > max.

	c.Put(hash, blob)

	// Should not be cached.
	assert.Nil(t, c.Get(hash))
}

func TestLRUBlobCache_NilBlob(t *testing.T) {
	t.Parallel()

	c := cache.NewLRUBlobCache(1024)

	hash := makeTestHash(1)

	// Should not panic.
	c.Put(hash, nil)

	assert.Nil(t, c.Get(hash))
}

func TestLRUBlobCache_DuplicatePut(t *testing.T) {
	t.Parallel()

	c := cache.NewLRUBlobCache(1024)

	hash := makeTestHash(1)
	blob := makeTestBlob([]byte("data"))

	c.Put(hash, blob)
	c.Put(hash, blob) // Duplicate.

	stats := c.Stats()
	assert.Equal(t, 1, stats.Entries)
}

func TestLRUBlobCache_GetMulti(t *testing.T) {
	t.Parallel()

	c := cache.NewLRUBlobCache(1024)

	hash1 := makeTestHash(1)
	hash2 := makeTestHash(2)
	hash3 := makeTestHash(3)

	blob1 := makeTestBlob([]byte("blob1"))
	blob2 := makeTestBlob([]byte("blob2"))

	c.Put(hash1, blob1)
	c.Put(hash2, blob2)

	found, missing := c.GetMulti([]gitlib.Hash{hash1, hash2, hash3})

	assert.Len(t, found, 2)
	assert.Len(t, missing, 1)
	assert.Equal(t, hash3, missing[0])
	assert.NotNil(t, found[hash1])
	assert.NotNil(t, found[hash2])
}

func TestLRUBlobCache_PutMulti(t *testing.T) {
	t.Parallel()

	c := cache.NewLRUBlobCache(1024)

	hash1 := makeTestHash(1)
	hash2 := makeTestHash(2)

	blobs := map[gitlib.Hash]*gitlib.CachedBlob{
		hash1: makeTestBlob([]byte("blob1")),
		hash2: makeTestBlob([]byte("blob2")),
	}

	c.PutMulti(blobs)

	stats := c.Stats()
	assert.Equal(t, 2, stats.Entries)

	assert.NotNil(t, c.Get(hash1))
	assert.NotNil(t, c.Get(hash2))
}

func TestLRUBlobCache_Stats(t *testing.T) {
	t.Parallel()

	c := cache.NewLRUBlobCache(1024)

	hash1 := makeTestHash(1)
	hash2 := makeTestHash(2)

	blob := makeTestBlob([]byte("hello"))

	c.Put(hash1, blob)

	// One hit, one miss.
	c.Get(hash1) // hit.
	c.Get(hash2) // miss.

	stats := c.Stats()
	assert.Equal(t, int64(1), stats.Hits)
	assert.Equal(t, int64(1), stats.Misses)
	assert.Equal(t, 1, stats.Entries)
	assert.InDelta(t, 0.5, stats.HitRate(), 0.001)
}

func TestLRUBlobCache_Clear(t *testing.T) {
	t.Parallel()

	c := cache.NewLRUBlobCache(1024)

	hash := makeTestHash(1)
	blob := makeTestBlob([]byte("data"))

	c.Put(hash, blob)
	assert.NotNil(t, c.Get(hash))

	c.Clear()

	assert.Nil(t, c.Get(hash))

	stats := c.Stats()
	assert.Equal(t, 0, stats.Entries)
	assert.Equal(t, int64(0), stats.CurrentSize)
}

func TestLRUBlobCache_ConcurrentAccess(t *testing.T) {
	t.Parallel()

	c := cache.NewLRUBlobCache(10 * 1024) // 10KB.

	const goroutines = 50

	const operations = 100

	var wg sync.WaitGroup

	wg.Add(goroutines)

	for g := range goroutines {
		go func(id int) {
			defer wg.Done()

			for i := range operations {
				hash := makeTestHash(byte((id*operations + i) % 256))
				blob := makeTestBlob([]byte("data"))

				c.Put(hash, blob)
				c.Get(hash)
			}
		}(g)
	}

	wg.Wait()

	// Just verify no panics and stats are reasonable.
	stats := c.Stats()
	assert.Positive(t, stats.Entries)
	assert.LessOrEqual(t, stats.CurrentSize, stats.MaxSize)
}

func TestLRUStats_HitRate_Empty(t *testing.T) {
	t.Parallel()

	stats := cache.LRUStats{}
	assert.InDelta(t, 0.0, stats.HitRate(), 0.001)
}

func TestLRUBlobCache_DefaultSize(t *testing.T) {
	t.Parallel()

	c := cache.NewLRUBlobCache(0)

	stats := c.Stats()
	assert.Equal(t, int64(cache.DefaultLRUCacheSize), stats.MaxSize)
}

func TestLRUBlobCache_BloomFiltersAbsentKeys(t *testing.T) {
	t.Parallel()

	lru := cache.NewLRUBlobCache(bloomTestCacheSize)

	// Insert some items.
	for i := range bloomTestInsertCount {
		hash := makeTestHashU16(uint16(i))
		blob := makeTestBlob([]byte(bloomTestBlobData))
		lru.Put(hash, blob)
	}

	// Query absent items — Bloom should filter most of them.
	for i := bloomTestInsertCount; i < bloomTestInsertCount+bloomTestProbeCount; i++ {
		hash := makeTestHashU16(uint16(i))
		got := lru.Get(hash)
		assert.Nil(t, got, "absent key %d should return nil", i)
	}

	stats := lru.Stats()
	assert.Positive(t, stats.BloomFiltered,
		"Bloom filter should short-circuit at least some absent lookups")
}

func TestLRUBlobCache_BloomNoFalseNegatives(t *testing.T) {
	t.Parallel()

	lru := cache.NewLRUBlobCache(bloomTestCacheSize)

	// Insert items.
	for i := range bloomTestInsertCount {
		hash := makeTestHashU16(uint16(i))
		blob := makeTestBlob([]byte(bloomTestBlobData))
		lru.Put(hash, blob)
	}

	// Every inserted item must be found (no false negatives).
	for i := range bloomTestInsertCount {
		hash := makeTestHashU16(uint16(i))
		got := lru.Get(hash)
		require.NotNil(t, got, "inserted key %d must be found (no false negatives)", i)
	}
}

func TestLRUBlobCache_BloomFilteredStats(t *testing.T) {
	t.Parallel()

	lru := cache.NewLRUBlobCache(bloomTestCacheSize)

	// Query absent keys on an empty cache.
	for i := range bloomTestProbeCount {
		hash := makeTestHashU16(uint16(i))
		lru.Get(hash)
	}

	stats := lru.Stats()

	// All lookups should be misses.
	assert.Equal(t, int64(bloomTestProbeCount), stats.Misses)

	// BloomFiltered should equal misses since nothing was ever inserted.
	assert.Equal(t, int64(bloomTestProbeCount), stats.BloomFiltered,
		"all lookups on empty cache should be Bloom-filtered")
}

func TestLRUBlobCache_BloomResetOnClear(t *testing.T) {
	t.Parallel()

	lru := cache.NewLRUBlobCache(bloomTestCacheSize)

	hash := makeTestHash(1)
	blob := makeTestBlob([]byte(bloomTestBlobData))

	lru.Put(hash, blob)
	require.NotNil(t, lru.Get(hash))

	lru.Clear()

	// After clear, the hash should not be found.
	got := lru.Get(hash)
	assert.Nil(t, got, "cleared key should not be found")

	// Bloom filter was reset, so the lookup should be bloom-filtered.
	stats := lru.Stats()
	assert.Positive(t, stats.BloomFiltered,
		"lookup after clear should be Bloom-filtered")
}

func TestLRUBlobCache_GetMultiBloomFiltering(t *testing.T) {
	t.Parallel()

	lru := cache.NewLRUBlobCache(bloomTestCacheSize)

	// Insert only even-numbered hashes.
	for i := range bloomTestInsertCount {
		hash := makeTestHashU16(uint16(i * 2))
		blob := makeTestBlob([]byte(bloomTestBlobData))
		lru.Put(hash, blob)
	}

	// Build a batch with alternating present/absent hashes.
	hashes := make([]gitlib.Hash, 0, bloomTestInsertCount*2)

	for i := range bloomTestInsertCount {
		hashes = append(hashes,
			makeTestHashU16(uint16(i*2)),   // Present.
			makeTestHashU16(uint16(i*2+1)), // Absent.
		)
	}

	found, missing := lru.GetMulti(hashes)

	assert.Len(t, found, bloomTestInsertCount, "all inserted hashes should be found")
	assert.Len(t, missing, bloomTestInsertCount, "all absent hashes should be missing")

	stats := lru.Stats()
	assert.Positive(t, stats.BloomFiltered,
		"GetMulti should Bloom-filter absent hashes")
}

func TestLRUBlobCache_BloomAfterEviction(t *testing.T) {
	t.Parallel()

	// Small cache: only fits 2 blobs of 40 bytes.
	lru := cache.NewLRUBlobCache(100)

	hash1 := makeTestHash(1)
	hash2 := makeTestHash(2)
	hash3 := makeTestHash(3)

	blob40 := makeTestBlob(make([]byte, 40))

	lru.Put(hash1, blob40)
	lru.Put(hash2, blob40)

	// Access hash2 to make hash1 LRU.
	lru.Get(hash2)

	// Put hash3 triggers eviction of hash1.
	lru.Put(hash3, blob40)

	// hash1 is evicted from the map but still in the Bloom filter.
	// Get should return nil (either via Bloom false positive + map miss, or Bloom says absent).
	got := lru.Get(hash1)
	assert.Nil(t, got, "evicted key should return nil")

	// hash2 and hash3 must still be findable.
	assert.NotNil(t, lru.Get(hash2), "hash2 should still be in cache")
	assert.NotNil(t, lru.Get(hash3), "hash3 should be in cache")
}
