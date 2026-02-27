// Package cache provides LRU blob caching with Bloom pre-filter and cost-based eviction.
package cache

import (
	"sync"
	"sync/atomic"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/bloom"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// DefaultLRUCacheSize is the default maximum memory size for the LRU blob cache (256 MB).
const DefaultLRUCacheSize = 256 * 1024 * 1024

// bytesPerKB is the number of bytes in a kilobyte.
const bytesPerKB = 1024.0

// averageBlobSizeEstimate is the estimated average blob size in bytes for Bloom filter sizing.
// Typical source files are ~4 KB; this conservative estimate ensures the Bloom filter is
// sized for more elements than likely needed, keeping the false-positive rate low.
const averageBlobSizeEstimate = 4096

// bloomFPRate is the false-positive rate for the Bloom pre-filter.
// At 1%, 99% of definite cache misses are short-circuited without lock acquisition.
const bloomFPRate = 0.01

// minBloomElements is the minimum number of expected elements for the Bloom filter.
// Prevents degenerate sizing for very small caches.
const minBloomElements = 64

// LRUBlobCache provides a cross-commit LRU cache for blob data.
// It tracks memory usage and evicts least recently used entries when the limit is exceeded.
// A Bloom filter pre-filters Get/GetMulti lookups to skip lock acquisition for definite misses.
type LRUBlobCache struct {
	mu          sync.RWMutex
	entries     map[gitlib.Hash]*lruEntry
	head        *lruEntry // Most recently used.
	tail        *lruEntry // Least recently used.
	filter      *bloom.Filter
	maxSize     int64
	currentSize int64

	// Metrics (atomic for lock-free reads).
	hits          atomic.Int64
	misses        atomic.Int64
	bloomFiltered atomic.Int64
}

// lruEntry is a doubly-linked list node for LRU tracking.
type lruEntry struct {
	hash        gitlib.Hash
	blob        *gitlib.CachedBlob
	size        int64
	accessCount int64 // Number of times this entry has been accessed.
	prev        *lruEntry
	next        *lruEntry
}

// evictionCost calculates the cost of evicting this entry.
// Higher cost = less desirable to evict.
// Cost = AccessCount / Size (normalized) - we want to evict large, rarely-accessed items first.
func (e *lruEntry) evictionCost() float64 {
	if e.size == 0 {
		return float64(e.accessCount)
	}

	// Normalize size to KB to avoid tiny fractions.
	sizeKB := float64(e.size) / bytesPerKB
	if sizeKB < 1 {
		sizeKB = 1
	}

	return float64(e.accessCount) / sizeKB
}

// NewLRUBlobCache creates a new LRU blob cache with the specified maximum size in bytes.
// A Bloom filter is initialized to pre-filter lookups, sized for the estimated element count.
func NewLRUBlobCache(maxSize int64) *LRUBlobCache {
	if maxSize <= 0 {
		maxSize = DefaultLRUCacheSize
	}

	expectedElements := max(uint(maxSize/averageBlobSizeEstimate), minBloomElements)

	// Error is structurally impossible: expectedElements > 0 and bloomFPRate is in (0, 1).
	bf, err := bloom.NewWithEstimates(expectedElements, bloomFPRate)
	if err != nil {
		panic("cache: bloom filter initialization failed: " + err.Error())
	}

	return &LRUBlobCache{
		entries: make(map[gitlib.Hash]*lruEntry),
		filter:  bf,
		maxSize: maxSize,
	}
}

// Get retrieves a blob from the cache. Returns nil if not found.
// Uses a Bloom filter to skip lock acquisition for definite cache misses.
func (c *LRUBlobCache) Get(hash gitlib.Hash) *gitlib.CachedBlob {
	if !c.filter.Test(hash[:]) {
		c.bloomFiltered.Add(1)
		c.misses.Add(1)

		return nil
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	entry, ok := c.entries[hash]
	if !ok {
		c.misses.Add(1)

		return nil
	}

	c.hits.Add(1)

	entry.accessCount++
	c.moveToFront(entry)

	return entry.blob
}

// Put adds a blob to the cache. If the cache exceeds maxSize, entries are evicted
// using size-aware eviction (large, infrequently accessed items evicted first).
// The hash is added to the Bloom filter after successful insertion.
func (c *LRUBlobCache) Put(hash gitlib.Hash, blob *gitlib.CachedBlob) {
	if blob == nil {
		return
	}

	blobSize := int64(len(blob.Data))

	// Don't cache blobs larger than the entire cache.
	if blobSize > c.maxSize {
		return
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	// Check if already exists.
	if entry, ok := c.entries[hash]; ok {
		entry.accessCount++
		c.moveToFront(entry)

		return
	}

	// Evict entries until we have room using size-aware eviction.
	for c.currentSize+blobSize > c.maxSize && c.tail != nil {
		c.evictLowestCost()
	}

	// If still can't fit after full eviction, skip entirely (avoids wasted clone).
	if c.currentSize+blobSize > c.maxSize {
		return
	}

	// Clone the blob to ensure data is detached from any large arena.
	safeBlob := blob.Clone()

	entry := &lruEntry{
		hash:        hash,
		blob:        safeBlob,
		size:        blobSize,
		accessCount: 1,
	}

	c.entries[hash] = entry
	c.currentSize += blobSize
	c.addToFront(entry)
	c.filter.Add(hash[:])
}

// GetMulti retrieves multiple blobs from the cache.
// Returns a map of found blobs and a slice of missing hashes.
// Uses a Bloom filter to skip lock acquisition for hashes that are definitely not cached.
func (c *LRUBlobCache) GetMulti(hashes []gitlib.Hash) (found map[gitlib.Hash]*gitlib.CachedBlob, missing []gitlib.Hash) {
	found = make(map[gitlib.Hash]*gitlib.CachedBlob)
	missing = make([]gitlib.Hash, 0)

	// Partition hashes: only candidates that pass the Bloom test need map lookup.
	candidates := make([]gitlib.Hash, 0, len(hashes))

	for _, hash := range hashes {
		if c.filter.Test(hash[:]) {
			candidates = append(candidates, hash)
		} else {
			c.bloomFiltered.Add(1)
			c.misses.Add(1)

			missing = append(missing, hash)
		}
	}

	if len(candidates) == 0 {
		return found, missing
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	for _, hash := range candidates {
		if entry, ok := c.entries[hash]; ok {
			c.hits.Add(1)

			entry.accessCount++
			c.moveToFront(entry)
			found[hash] = entry.blob
		} else {
			c.misses.Add(1)

			missing = append(missing, hash)
		}
	}

	return found, missing
}

// PutMulti adds multiple blobs to the cache.
// Each successfully inserted hash is added to the Bloom filter.
func (c *LRUBlobCache) PutMulti(blobs map[gitlib.Hash]*gitlib.CachedBlob) {
	c.mu.Lock()
	defer c.mu.Unlock()

	for hash, blob := range blobs {
		if blob == nil {
			continue
		}

		blobSize := int64(len(blob.Data))

		// Skip blobs larger than entire cache.
		if blobSize > c.maxSize {
			continue
		}

		// Skip if already exists (but increment access count).
		if entry, ok := c.entries[hash]; ok {
			entry.accessCount++
			c.moveToFront(entry)

			continue
		}

		// Evict entries until we have room using size-aware eviction.
		for c.currentSize+blobSize > c.maxSize && c.tail != nil {
			c.evictLowestCost()
		}

		// If still can't fit after full eviction, skip (avoids wasted clone).
		if c.currentSize+blobSize > c.maxSize {
			continue
		}

		safeBlob := blob.Clone()

		entry := &lruEntry{
			hash:        hash,
			blob:        safeBlob,
			size:        blobSize,
			accessCount: 1,
		}

		c.entries[hash] = entry
		c.currentSize += blobSize
		c.addToFront(entry)
		c.filter.Add(hash[:])
	}
}

// Stats returns cache statistics.
func (c *LRUBlobCache) Stats() LRUStats {
	c.mu.RLock()
	defer c.mu.RUnlock()

	return LRUStats{
		Hits:          c.hits.Load(),
		Misses:        c.misses.Load(),
		BloomFiltered: c.bloomFiltered.Load(),
		Entries:       len(c.entries),
		CurrentSize:   c.currentSize,
		MaxSize:       c.maxSize,
	}
}

// CacheHits returns the total cache hit count (atomic, lock-free).
func (c *LRUBlobCache) CacheHits() int64 { return c.hits.Load() }

// CacheMisses returns the total cache miss count (atomic, lock-free).
func (c *LRUBlobCache) CacheMisses() int64 { return c.misses.Load() }

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

// Clear removes all entries from the cache and resets the Bloom filter.
func (c *LRUBlobCache) Clear() {
	c.mu.Lock()
	defer c.mu.Unlock()

	c.entries = make(map[gitlib.Hash]*lruEntry)
	c.head = nil
	c.tail = nil
	c.currentSize = 0
	c.filter.Reset()
}

// moveToFront moves an entry to the front of the LRU list (most recently used).
func (c *LRUBlobCache) moveToFront(entry *lruEntry) {
	if entry == c.head {
		return
	}

	c.removeFromList(entry)
	c.addToFront(entry)
}

// addToFront adds an entry to the front of the LRU list.
func (c *LRUBlobCache) addToFront(entry *lruEntry) {
	entry.prev = nil
	entry.next = c.head

	if c.head != nil {
		c.head.prev = entry
	}

	c.head = entry

	if c.tail == nil {
		c.tail = entry
	}
}

// removeFromList removes an entry from the LRU list.
func (c *LRUBlobCache) removeFromList(entry *lruEntry) {
	if entry.prev != nil {
		entry.prev.next = entry.next
	} else {
		c.head = entry.next
	}

	if entry.next != nil {
		entry.next.prev = entry.prev
	} else {
		c.tail = entry.prev
	}
}

// evictionSampleSize is the number of LRU candidates to sample for size-aware eviction.
// Sampling reduces O(n) scan to O(k) where k is constant.
const evictionSampleSize = 5

// evictLowestCost removes the entry with the lowest eviction cost from the LRU tail region.
// This implements size-aware eviction: large, infrequently accessed items are evicted first.
// We sample up to evictionSampleSize entries from the tail to avoid O(n) scans.
func (c *LRUBlobCache) evictLowestCost() {
	if c.tail == nil {
		return
	}

	// Sample candidates from the tail (LRU region).
	var candidates [evictionSampleSize]*lruEntry

	count := 0
	entry := c.tail

	for entry != nil && count < evictionSampleSize {
		candidates[count] = entry
		count++
		entry = entry.prev
	}

	if count == 0 {
		return
	}

	// Find the entry with lowest eviction cost (large size, low access count).
	victim := candidates[0]
	lowestCost := victim.evictionCost()

	for i := 1; i < count; i++ {
		cost := candidates[i].evictionCost()
		if cost < lowestCost {
			lowestCost = cost
			victim = candidates[i]
		}
	}

	// Evict the victim.
	c.removeFromList(victim)
	delete(c.entries, victim.hash)
	c.currentSize -= victim.size
}
