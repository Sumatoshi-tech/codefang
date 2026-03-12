package framework

import (
	"github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/lru"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// DefaultDiffCacheSize is the default maximum number of diff entries to cache.
const DefaultDiffCacheSize = 10000

// DiffKey uniquely identifies a diff computation by blob hashes.
type DiffKey struct {
	OldHash gitlib.Hash
	NewHash gitlib.Hash
}

// DiffCache provides an LRU cache for diff results.
// It caches computed diffs to avoid redundant diff computations.
// A Bloom filter pre-filters Get lookups to skip lock acquisition for definite misses.
type DiffCache struct {
	cache *lru.Cache[DiffKey, plumbing.FileDiffData]
}

// DiffCacheStats holds statistics about diff cache usage.
type DiffCacheStats struct {
	Hits       int64
	Misses     int64
	BloomSkips int64 // Lookups short-circuited by the Bloom pre-filter.
	Entries    int
	MaxEntries int
}

// HitRate returns the cache hit rate as a fraction.
func (s DiffCacheStats) HitRate() float64 {
	total := s.Hits + s.Misses
	if total == 0 {
		return 0
	}

	return float64(s.Hits) / float64(total)
}

// diffKeyToBytes returns the concatenated hash bytes for Bloom filter lookup.
func diffKeyToBytes(key DiffKey) []byte {
	var buf [2 * gitlib.HashSize]byte

	copy(buf[:gitlib.HashSize], key.OldHash[:])
	copy(buf[gitlib.HashSize:], key.NewHash[:])

	return buf[:]
}

// NewDiffCache creates a new diff cache with the specified maximum entries.
// A Bloom filter is initialized to pre-filter lookups, sized for maxEntries.
func NewDiffCache(maxEntries int) *DiffCache {
	if maxEntries <= 0 {
		maxEntries = DefaultDiffCacheSize
	}

	return &DiffCache{
		cache: lru.New(
			lru.WithMaxEntries[DiffKey, plumbing.FileDiffData](maxEntries),
			lru.WithBloomFilter[DiffKey, plumbing.FileDiffData](diffKeyToBytes, uint(maxEntries)),
		),
	}
}

// Get retrieves a cached diff result.
// Uses a Bloom filter to skip lock acquisition for definite cache misses.
func (c *DiffCache) Get(key DiffKey) (plumbing.FileDiffData, bool) {
	return c.cache.Get(key)
}

// Put adds a diff result to the cache.
func (c *DiffCache) Put(key DiffKey, diff plumbing.FileDiffData) {
	c.cache.Put(key, diff)
}

// Clear removes all entries from the cache and resets the Bloom filter.
func (c *DiffCache) Clear() {
	c.cache.Clear()
}

// CacheHits returns the total cache hit count (atomic, lock-free).
func (c *DiffCache) CacheHits() int64 { return c.cache.CacheHits() }

// CacheMisses returns the total cache miss count (atomic, lock-free).
func (c *DiffCache) CacheMisses() int64 { return c.cache.CacheMisses() }

// Stats returns current cache statistics.
func (c *DiffCache) Stats() DiffCacheStats {
	s := c.cache.Stats()

	return DiffCacheStats{
		Hits:       s.Hits,
		Misses:     s.Misses,
		BloomSkips: s.BloomFiltered,
		Entries:    s.Entries,
		MaxEntries: s.MaxEntries,
	}
}
