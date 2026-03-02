// Package cache provides LRU blob caching with Bloom pre-filter and cost-based eviction.
package cache

import (
	"github.com/Sumatoshi-tech/codefang/pkg/alg/lru"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// DefaultLRUCacheSize is the default maximum memory size for the LRU blob cache (256 MB).
const DefaultLRUCacheSize = 256 * 1024 * 1024

// bytesPerKB is the number of bytes in a kilobyte for eviction cost normalization.
const bytesPerKB = 1024.0

// averageBlobSizeEstimate is the estimated average blob size in bytes for Bloom filter sizing.
// Typical source files are ~4 KB; this conservative estimate ensures the Bloom filter is
// sized for more elements than likely needed, keeping the false-positive rate low.
const averageBlobSizeEstimate = 4096

// minBloomElements is the minimum number of expected elements for the Bloom filter.
// Prevents degenerate sizing for very small caches.
const minBloomElements = 64

// evictionSampleSize is the number of LRU candidates to sample for size-aware eviction.
// Sampling reduces O(n) scan to O(k) where k is constant.
const evictionSampleSize = 5

// LRUBlobCache provides a cross-commit LRU cache for blob data.
// It tracks memory usage and evicts least recently used entries when the limit is exceeded.
// A Bloom filter pre-filters Get/GetMulti lookups to skip lock acquisition for definite misses.
type LRUBlobCache struct {
	cache *lru.Cache[gitlib.Hash, *gitlib.CachedBlob]
}

// LRUStats holds cache performance metrics.
type LRUStats struct {
	Hits          int64
	Misses        int64
	BloomFiltered int64 // Lookups short-circuited by the Bloom pre-filter.
	Entries       int
	CurrentSize   int64
	MaxSize       int64
}

// HitRate returns the cache hit rate (0.0 to 1.0).
func (s LRUStats) HitRate() float64 {
	total := s.Hits + s.Misses
	if total == 0 {
		return 0.0
	}

	return float64(s.Hits) / float64(total)
}

// hashToBytes converts a gitlib.Hash to a byte slice for Bloom filter operations.
func hashToBytes(h gitlib.Hash) []byte {
	return h[:]
}

// blobSize returns the data length of a cached blob.
func blobSize(blob *gitlib.CachedBlob) int64 {
	if blob == nil {
		return 0
	}

	return int64(len(blob.Data))
}

// cloneBlob creates a detached copy of a blob.
func cloneBlob(blob *gitlib.CachedBlob) *gitlib.CachedBlob {
	return blob.Clone()
}

// evictionCost calculates the cost of evicting an entry.
// Higher cost = less desirable to evict.
// Cost = accessCount / sizeKB — we want to evict large, rarely-accessed items first.
func evictionCost(accessCount, sizeBytes int64) float64 {
	if sizeBytes == 0 {
		return float64(accessCount)
	}

	sizeKB := float64(sizeBytes) / bytesPerKB
	if sizeKB < 1 {
		sizeKB = 1
	}

	return float64(accessCount) / sizeKB
}

// NewLRUBlobCache creates a new LRU blob cache with the specified maximum size in bytes.
// A Bloom filter is initialized to pre-filter lookups, sized for the estimated element count.
func NewLRUBlobCache(maxSize int64) *LRUBlobCache {
	if maxSize <= 0 {
		maxSize = DefaultLRUCacheSize
	}

	expectedN := max(uint(maxSize/averageBlobSizeEstimate), minBloomElements)

	return &LRUBlobCache{
		cache: lru.New(
			lru.WithMaxBytes[gitlib.Hash, *gitlib.CachedBlob](maxSize, blobSize),
			lru.WithBloomFilter[gitlib.Hash, *gitlib.CachedBlob](hashToBytes, expectedN),
			lru.WithCostEviction[gitlib.Hash, *gitlib.CachedBlob](evictionSampleSize, evictionCost),
			lru.WithCloneFunc[gitlib.Hash, *gitlib.CachedBlob](cloneBlob),
		),
	}
}

// Get retrieves a blob from the cache. Returns nil if not found.
// Uses a Bloom filter to skip lock acquisition for definite cache misses.
func (c *LRUBlobCache) Get(hash gitlib.Hash) *gitlib.CachedBlob {
	blob, _ := c.cache.Get(hash)

	return blob
}

// Put adds a blob to the cache. If the cache exceeds maxSize, entries are evicted
// using size-aware eviction (large, infrequently accessed items evicted first).
func (c *LRUBlobCache) Put(hash gitlib.Hash, blob *gitlib.CachedBlob) {
	if blob == nil {
		return
	}

	c.cache.Put(hash, blob)
}

// GetMulti retrieves multiple blobs from the cache.
// Returns a map of found blobs and a slice of missing hashes.
func (c *LRUBlobCache) GetMulti(hashes []gitlib.Hash) (found map[gitlib.Hash]*gitlib.CachedBlob, missing []gitlib.Hash) {
	return c.cache.GetMulti(hashes)
}

// PutMulti adds multiple blobs to the cache.
func (c *LRUBlobCache) PutMulti(blobs map[gitlib.Hash]*gitlib.CachedBlob) {
	c.cache.PutMulti(blobs)
}

// Stats returns cache statistics.
func (c *LRUBlobCache) Stats() LRUStats {
	s := c.cache.Stats()

	return LRUStats{
		Hits:          s.Hits,
		Misses:        s.Misses,
		BloomFiltered: s.BloomFiltered,
		Entries:       s.Entries,
		CurrentSize:   s.CurrentSize,
		MaxSize:       s.MaxSize,
	}
}

// CacheHits returns the total cache hit count (atomic, lock-free).
func (c *LRUBlobCache) CacheHits() int64 { return c.cache.CacheHits() }

// CacheMisses returns the total cache miss count (atomic, lock-free).
func (c *LRUBlobCache) CacheMisses() int64 { return c.cache.CacheMisses() }

// Clear removes all entries from the cache and resets the Bloom filter.
func (c *LRUBlobCache) Clear() {
	c.cache.Clear()
}
