package framework

import (
	"sync"
	"sync/atomic"

	"github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/bloom"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// DefaultDiffCacheSize is the default maximum number of diff entries to cache.
const DefaultDiffCacheSize = 10000

// diffBloomFPRate is the false-positive rate for the Bloom pre-filter.
// At 1%, 99% of definite cache misses are short-circuited without lock acquisition.
const diffBloomFPRate = 0.01

// DiffKey uniquely identifies a diff computation by blob hashes.
type DiffKey struct {
	OldHash gitlib.Hash
	NewHash gitlib.Hash
}

// diffCacheEntry is a node in the LRU doubly-linked list.
type diffCacheEntry struct {
	key  DiffKey
	diff plumbing.FileDiffData
	prev *diffCacheEntry
	next *diffCacheEntry
}

// DiffCache provides an LRU cache for diff results.
// It caches computed diffs to avoid redundant diff computations.
// A Bloom filter pre-filters Get lookups to skip lock acquisition for definite misses.
type DiffCache struct {
	mu         sync.RWMutex
	entries    map[DiffKey]*diffCacheEntry
	filter     *bloom.Filter
	head       *diffCacheEntry // Most recently used.
	tail       *diffCacheEntry // Least recently used.
	maxEntries int
	hits       atomic.Int64
	misses     atomic.Int64
	bloomSkips atomic.Int64
}

// NewDiffCache creates a new diff cache with the specified maximum entries.
// A Bloom filter is initialized to pre-filter lookups, sized for maxEntries.
func NewDiffCache(maxEntries int) *DiffCache {
	if maxEntries <= 0 {
		maxEntries = DefaultDiffCacheSize
	}

	// Error is structurally impossible: maxEntries > 0 and diffBloomFPRate is in (0, 1).
	bf, err := bloom.NewWithEstimates(uint(maxEntries), diffBloomFPRate)
	if err != nil {
		panic("diff_cache: bloom filter initialization failed: " + err.Error())
	}

	return &DiffCache{
		entries:    make(map[DiffKey]*diffCacheEntry),
		filter:     bf,
		maxEntries: maxEntries,
	}
}

// diffKeyBloomBytes returns the concatenated hash bytes for Bloom filter lookup.
func diffKeyBloomBytes(key DiffKey) []byte {
	var buf [2 * gitlib.HashSize]byte
	copy(buf[:gitlib.HashSize], key.OldHash[:])
	copy(buf[gitlib.HashSize:], key.NewHash[:])

	return buf[:]
}

// Get retrieves a cached diff result.
// Uses a Bloom filter to skip lock acquisition for definite cache misses.
func (c *DiffCache) Get(key DiffKey) (plumbing.FileDiffData, bool) {
	if !c.filter.Test(diffKeyBloomBytes(key)) {
		c.bloomSkips.Add(1)
		c.misses.Add(1)

		return plumbing.FileDiffData{}, false
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	entry, exists := c.entries[key]
	if !exists {
		c.misses.Add(1)

		return plumbing.FileDiffData{}, false
	}

	c.hits.Add(1)
	c.moveToFront(entry)

	return entry.diff, true
}

// Put adds a diff result to the cache.
func (c *DiffCache) Put(key DiffKey, diff plumbing.FileDiffData) {
	c.mu.Lock()
	defer c.mu.Unlock()

	// Check if already exists.
	if entry, exists := c.entries[key]; exists {
		entry.diff = diff
		c.moveToFront(entry)

		return
	}

	// Create new entry.
	entry := &diffCacheEntry{
		key:  key,
		diff: diff,
	}

	c.entries[key] = entry
	c.addToFront(entry)
	c.filter.Add(diffKeyBloomBytes(key))

	// Evict if over capacity.
	for len(c.entries) > c.maxEntries {
		c.evictLRU()
	}
}

// Clear removes all entries from the cache and resets the Bloom filter.
func (c *DiffCache) Clear() {
	c.mu.Lock()
	defer c.mu.Unlock()

	c.entries = make(map[DiffKey]*diffCacheEntry)
	c.head = nil
	c.tail = nil
	c.filter.Reset()
}

// CacheHits returns the total cache hit count (atomic, lock-free).
func (c *DiffCache) CacheHits() int64 { return c.hits.Load() }

// CacheMisses returns the total cache miss count (atomic, lock-free).
func (c *DiffCache) CacheMisses() int64 { return c.misses.Load() }

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

// Stats returns current cache statistics.
func (c *DiffCache) Stats() DiffCacheStats {
	c.mu.RLock()
	defer c.mu.RUnlock()

	return DiffCacheStats{
		Hits:       c.hits.Load(),
		Misses:     c.misses.Load(),
		BloomSkips: c.bloomSkips.Load(),
		Entries:    len(c.entries),
		MaxEntries: c.maxEntries,
	}
}

// moveToFront moves an entry to the front of the LRU list.
func (c *DiffCache) moveToFront(entry *diffCacheEntry) {
	if entry == c.head {
		return
	}

	c.removeFromList(entry)
	c.addToFront(entry)
}

// addToFront adds an entry to the front of the LRU list.
func (c *DiffCache) addToFront(entry *diffCacheEntry) {
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
func (c *DiffCache) removeFromList(entry *diffCacheEntry) {
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

// evictLRU removes the least recently used entry.
func (c *DiffCache) evictLRU() {
	if c.tail == nil {
		return
	}

	entry := c.tail
	c.removeFromList(entry)
	delete(c.entries, entry.key)
}
