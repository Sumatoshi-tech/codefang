package lru

// Stats holds cache performance metrics.
type Stats struct {
	Hits          int64
	Misses        int64
	BloomFiltered int64 // Lookups short-circuited by the Bloom pre-filter.
	Entries       int
	CurrentSize   int64
	MaxEntries    int   // 0 when count-based limit is not set.
	MaxSize       int64 // 0 when size-based limit is not set.
}

// HitRate returns the cache hit rate as a fraction (0.0 to 1.0).
func (s Stats) HitRate() float64 {
	total := s.Hits + s.Misses
	if total == 0 {
		return 0
	}

	return float64(s.Hits) / float64(total)
}

// Stats returns current cache statistics.
func (c *Cache[K, V]) Stats() Stats {
	c.mu.RLock()
	defer c.mu.RUnlock()

	return Stats{
		Hits:          c.hits.Load(),
		Misses:        c.misses.Load(),
		BloomFiltered: c.bloomFiltered.Load(),
		Entries:       len(c.entries),
		CurrentSize:   c.curSize,
		MaxEntries:    c.maxEntries,
		MaxSize:       c.maxSize,
	}
}

// CacheHits returns the total cache hit count (atomic, lock-free).
func (c *Cache[K, V]) CacheHits() int64 { return c.hits.Load() }

// CacheMisses returns the total cache miss count (atomic, lock-free).
func (c *Cache[K, V]) CacheMisses() int64 { return c.misses.Load() }
