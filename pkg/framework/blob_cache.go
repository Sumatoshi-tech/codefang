package framework

import (
	"sync"
	"sync/atomic"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// DefaultGlobalCacheSize is the default maximum memory size for the global blob cache (256 MB).
const DefaultGlobalCacheSize = 1024 * 1024 * 1024

// GlobalBlobCache provides a cross-commit LRU cache for blob data.
// It tracks memory usage and evicts least recently used entries when the limit is exceeded.
type GlobalBlobCache struct {
	mu          sync.RWMutex
	entries     map[gitlib.Hash]*cacheEntry
	head        *cacheEntry // Most recently used
	tail        *cacheEntry // Least recently used
	maxSize     int64
	currentSize int64

	// Metrics (atomic for lock-free reads)
	hits   atomic.Int64
	misses atomic.Int64
}

// cacheEntry is a doubly-linked list node for LRU tracking.
type cacheEntry struct {
	hash gitlib.Hash
	blob *gitlib.CachedBlob
	size int64
	prev *cacheEntry
	next *cacheEntry
}

// NewGlobalBlobCache creates a new global blob cache with the specified maximum size in bytes.
func NewGlobalBlobCache(maxSize int64) *GlobalBlobCache {
	if maxSize <= 0 {
		maxSize = DefaultGlobalCacheSize
	}

	return &GlobalBlobCache{
		entries: make(map[gitlib.Hash]*cacheEntry),
		maxSize: maxSize,
	}
}

// Get retrieves a blob from the cache. Returns nil if not found.
func (c *GlobalBlobCache) Get(hash gitlib.Hash) *gitlib.CachedBlob {
	c.mu.Lock()
	defer c.mu.Unlock()

	entry, ok := c.entries[hash]
	if !ok {
		c.misses.Add(1)

		return nil
	}

	c.hits.Add(1)
	c.moveToFront(entry)

	return entry.blob
}

// Put adds a blob to the cache. If the cache exceeds maxSize, LRU entries are evicted.
func (c *GlobalBlobCache) Put(hash gitlib.Hash, blob *gitlib.CachedBlob) {
	if blob == nil {
		return
	}

	blobSize := int64(len(blob.Data))

	// Don't cache blobs larger than the entire cache
	if blobSize > c.maxSize {
		return
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	// Check if already exists
	if entry, ok := c.entries[hash]; ok {
		c.moveToFront(entry)

		return
	}

	// Evict entries until we have room
	for c.currentSize+blobSize > c.maxSize && c.tail != nil {
		c.evictLRU()
	}

	// Add new entry
	// Clone the blob to ensure data is detached from any large arena
	safeBlob := blob.Clone()
	
	entry := &cacheEntry{
		hash: hash,
		blob: safeBlob,
		size: blobSize,
	}

	c.entries[hash] = entry
	c.currentSize += blobSize
	c.addToFront(entry)
}


// GetMulti retrieves multiple blobs from the cache.
// Returns a map of found blobs and a slice of missing hashes.
func (c *GlobalBlobCache) GetMulti(hashes []gitlib.Hash) (found map[gitlib.Hash]*gitlib.CachedBlob, missing []gitlib.Hash) {
	found = make(map[gitlib.Hash]*gitlib.CachedBlob)
	missing = make([]gitlib.Hash, 0)

	c.mu.Lock()
	defer c.mu.Unlock()

	for _, hash := range hashes {
		if entry, ok := c.entries[hash]; ok {
			c.hits.Add(1)
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
func (c *GlobalBlobCache) PutMulti(blobs map[gitlib.Hash]*gitlib.CachedBlob) {
	c.mu.Lock()
	defer c.mu.Unlock()

	for hash, blob := range blobs {
		if blob == nil {
			continue
		}

		blobSize := int64(len(blob.Data))

		// Skip blobs larger than entire cache
		if blobSize > c.maxSize {
			continue
		}

		// Skip if already exists
		if entry, ok := c.entries[hash]; ok {
			c.moveToFront(entry)

			continue
		}

		// Evict entries until we have room
		for c.currentSize+blobSize > c.maxSize && c.tail != nil {
			c.evictLRU()
		}

		// Add new entry
		safeBlob := blob.Clone()
		
		entry := &cacheEntry{
			hash: hash,
			blob: safeBlob,
			size: blobSize,
		}

		c.entries[hash] = entry
		c.currentSize += blobSize
		c.addToFront(entry)
	}
}

// Stats returns cache statistics.
func (c *GlobalBlobCache) Stats() CacheStats {
	c.mu.RLock()
	defer c.mu.RUnlock()

	return CacheStats{
		Hits:        c.hits.Load(),
		Misses:      c.misses.Load(),
		Entries:     len(c.entries),
		CurrentSize: c.currentSize,
		MaxSize:     c.maxSize,
	}
}

// CacheStats holds cache performance metrics.
type CacheStats struct {
	Hits        int64
	Misses      int64
	Entries     int
	CurrentSize int64
	MaxSize     int64
}

// HitRate returns the cache hit rate (0.0 to 1.0).
func (s CacheStats) HitRate() float64 {
	total := s.Hits + s.Misses
	if total == 0 {
		return 0.0
	}

	return float64(s.Hits) / float64(total)
}

// Clear removes all entries from the cache.
func (c *GlobalBlobCache) Clear() {
	c.mu.Lock()
	defer c.mu.Unlock()

	c.entries = make(map[gitlib.Hash]*cacheEntry)
	c.head = nil
	c.tail = nil
	c.currentSize = 0
}

// moveToFront moves an entry to the front of the LRU list (most recently used).
func (c *GlobalBlobCache) moveToFront(entry *cacheEntry) {
	if entry == c.head {
		return
	}

	c.removeFromList(entry)
	c.addToFront(entry)
}

// addToFront adds an entry to the front of the LRU list.
func (c *GlobalBlobCache) addToFront(entry *cacheEntry) {
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
func (c *GlobalBlobCache) removeFromList(entry *cacheEntry) {
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
func (c *GlobalBlobCache) evictLRU() {
	if c.tail == nil {
		return
	}

	entry := c.tail
	c.removeFromList(entry)
	delete(c.entries, entry.hash)
	c.currentSize -= entry.size
}
