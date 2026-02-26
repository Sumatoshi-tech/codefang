package framework_test

import (
	"sync"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func makeTestBlob(data []byte) *gitlib.CachedBlob {
	return gitlib.NewCachedBlobForTest(data)
}

func makeTestHash(b byte) gitlib.Hash {
	var h gitlib.Hash

	h[0] = b

	return h
}

func TestGlobalBlobCache_GetPut(t *testing.T) {
	t.Parallel()

	cache := framework.NewGlobalBlobCache(1024) // 1KB cache.

	hash := makeTestHash(1)
	blob := makeTestBlob([]byte("hello world"))

	// Get on empty cache returns nil.
	got := cache.Get(hash)
	assert.Nil(t, got)

	// Put and Get.
	cache.Put(hash, blob)
	got = cache.Get(hash)
	require.NotNil(t, got)
	assert.Equal(t, blob.Data, got.Data)
}

func TestGlobalBlobCache_LRUEviction(t *testing.T) {
	t.Parallel()

	// Cache with 100 bytes max.
	cache := framework.NewGlobalBlobCache(100)

	// Add 3 blobs of 40 bytes each (120 bytes total, exceeds limit).
	hash1 := makeTestHash(1)
	hash2 := makeTestHash(2)
	hash3 := makeTestHash(3)

	blob1 := makeTestBlob(make([]byte, 40))
	blob2 := makeTestBlob(make([]byte, 40))
	blob3 := makeTestBlob(make([]byte, 40))

	cache.Put(hash1, blob1)
	cache.Put(hash2, blob2)

	// Both should be in cache (80 bytes < 100).
	assert.NotNil(t, cache.Get(hash1))
	assert.NotNil(t, cache.Get(hash2))

	// Adding third blob should evict hash1 (LRU after accessing hash2 above)
	// Actually, after Get(hash1), Get(hash2), hash1 is most recent, hash2 is LRU
	// Let's re-access to make hash2 most recent.
	cache.Get(hash2)

	cache.Put(hash3, blob3)

	// hash1 should be evicted (it was LRU).
	assert.Nil(t, cache.Get(hash1), "hash1 should be evicted")
	assert.NotNil(t, cache.Get(hash2), "hash2 should still be in cache")
	assert.NotNil(t, cache.Get(hash3), "hash3 should be in cache")
}

func TestGlobalBlobCache_SkipLargeBlobs(t *testing.T) {
	t.Parallel()

	cache := framework.NewGlobalBlobCache(100) // 100 bytes max.

	hash := makeTestHash(1)
	blob := makeTestBlob(make([]byte, 200)) // 200 bytes > max.

	cache.Put(hash, blob)

	// Should not be cached.
	assert.Nil(t, cache.Get(hash))
}

func TestGlobalBlobCache_NilBlob(t *testing.T) {
	t.Parallel()

	cache := framework.NewGlobalBlobCache(1024)

	hash := makeTestHash(1)

	// Should not panic.
	cache.Put(hash, nil)

	assert.Nil(t, cache.Get(hash))
}

func TestGlobalBlobCache_DuplicatePut(t *testing.T) {
	t.Parallel()

	cache := framework.NewGlobalBlobCache(1024)

	hash := makeTestHash(1)
	blob := makeTestBlob([]byte("data"))

	cache.Put(hash, blob)
	cache.Put(hash, blob) // Duplicate.

	stats := cache.Stats()
	assert.Equal(t, 1, stats.Entries)
}

func TestGlobalBlobCache_GetMulti(t *testing.T) {
	t.Parallel()

	cache := framework.NewGlobalBlobCache(1024)

	hash1 := makeTestHash(1)
	hash2 := makeTestHash(2)
	hash3 := makeTestHash(3)

	blob1 := makeTestBlob([]byte("blob1"))
	blob2 := makeTestBlob([]byte("blob2"))

	cache.Put(hash1, blob1)
	cache.Put(hash2, blob2)

	found, missing := cache.GetMulti([]gitlib.Hash{hash1, hash2, hash3})

	assert.Len(t, found, 2)
	assert.Len(t, missing, 1)
	assert.Equal(t, hash3, missing[0])
	assert.NotNil(t, found[hash1])
	assert.NotNil(t, found[hash2])
}

func TestGlobalBlobCache_PutMulti(t *testing.T) {
	t.Parallel()

	cache := framework.NewGlobalBlobCache(1024)

	hash1 := makeTestHash(1)
	hash2 := makeTestHash(2)

	blobs := map[gitlib.Hash]*gitlib.CachedBlob{
		hash1: makeTestBlob([]byte("blob1")),
		hash2: makeTestBlob([]byte("blob2")),
	}

	cache.PutMulti(blobs)

	stats := cache.Stats()
	assert.Equal(t, 2, stats.Entries)

	assert.NotNil(t, cache.Get(hash1))
	assert.NotNil(t, cache.Get(hash2))
}

func TestGlobalBlobCache_Stats(t *testing.T) {
	t.Parallel()

	cache := framework.NewGlobalBlobCache(1024)

	hash1 := makeTestHash(1)
	hash2 := makeTestHash(2)

	blob := makeTestBlob([]byte("hello"))

	cache.Put(hash1, blob)

	// One hit, one miss.
	cache.Get(hash1) // hit.
	cache.Get(hash2) // miss.

	stats := cache.Stats()
	assert.Equal(t, int64(1), stats.Hits)
	assert.Equal(t, int64(1), stats.Misses)
	assert.Equal(t, 1, stats.Entries)
	assert.InDelta(t, 0.5, stats.HitRate(), 0.001)
}

func TestGlobalBlobCache_Clear(t *testing.T) {
	t.Parallel()

	cache := framework.NewGlobalBlobCache(1024)

	hash := makeTestHash(1)
	blob := makeTestBlob([]byte("data"))

	cache.Put(hash, blob)
	assert.NotNil(t, cache.Get(hash))

	cache.Clear()

	assert.Nil(t, cache.Get(hash))

	stats := cache.Stats()
	assert.Equal(t, 0, stats.Entries)
	assert.Equal(t, int64(0), stats.CurrentSize)
}

func TestGlobalBlobCache_ConcurrentAccess(t *testing.T) {
	t.Parallel()

	cache := framework.NewGlobalBlobCache(10 * 1024) // 10KB.

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

				cache.Put(hash, blob)
				cache.Get(hash)
			}
		}(g)
	}

	wg.Wait()

	// Just verify no panics and stats are reasonable.
	stats := cache.Stats()
	assert.Positive(t, stats.Entries)
	assert.LessOrEqual(t, stats.CurrentSize, stats.MaxSize)
}

func TestCacheStats_HitRate_Empty(t *testing.T) {
	t.Parallel()

	stats := framework.CacheStats{}
	assert.InDelta(t, 0.0, stats.HitRate(), 0.001)
}

func TestGlobalBlobCache_DefaultSize(t *testing.T) {
	t.Parallel()

	cache := framework.NewGlobalBlobCache(0)

	stats := cache.Stats()
	assert.Equal(t, int64(framework.DefaultGlobalCacheSize), stats.MaxSize)
}
