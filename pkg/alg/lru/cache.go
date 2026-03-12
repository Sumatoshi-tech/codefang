// Package lru provides a generic thread-safe LRU cache with optional Bloom
// pre-filtering, size-based eviction, and cost-aware eviction sampling.
package lru

import (
	"sync"
	"sync/atomic"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/bloom"
)

// defaultBloomFPRate is the default false-positive rate for the Bloom pre-filter.
// At 1%, 99% of definite cache misses are short-circuited without lock acquisition.
const defaultBloomFPRate = 0.01

// entry is a doubly-linked list node holding a key-value pair.
type entry[K comparable, V any] struct {
	key         K
	value       V
	size        int64
	accessCount int64
	prev        *entry[K, V]
	next        *entry[K, V]
}

// Cache is a thread-safe generic LRU cache.
// It supports optional Bloom pre-filtering, size-based eviction,
// cost-aware eviction sampling, and value cloning on insertion.
type Cache[K comparable, V any] struct {
	mu      sync.RWMutex
	entries map[K]*entry[K, V]
	head    *entry[K, V] // Most recently used.
	tail    *entry[K, V] // Least recently used.

	// Capacity limits.
	maxEntries int
	maxSize    int64
	curSize    int64

	// Optional features.
	filter     *bloom.Filter
	keyToBytes func(K) []byte
	sizeFunc   func(V) int64
	cloneFunc  func(V) V

	// Cost-based eviction.
	costFunc   func(accessCount, sizeBytes int64) float64
	sampleSize int

	// Metrics (atomic for lock-free reads).
	hits          atomic.Int64
	misses        atomic.Int64
	bloomFiltered atomic.Int64
}

// Option configures a Cache.
type Option[K comparable, V any] func(*Cache[K, V])

// WithMaxEntries sets the maximum number of entries (count-based eviction).
func WithMaxEntries[K comparable, V any](n int) Option[K, V] {
	return func(c *Cache[K, V]) {
		c.maxEntries = n
	}
}

// WithMaxBytes sets the maximum total size in bytes and a function to
// compute the size of each value. Enables size-based eviction.
func WithMaxBytes[K comparable, V any](maxBytes int64, sizeFunc func(V) int64) Option[K, V] {
	return func(c *Cache[K, V]) {
		c.maxSize = maxBytes
		c.sizeFunc = sizeFunc
	}
}

// WithBloomFilter enables a Bloom pre-filter for Get and GetMulti.
// keyToBytes converts a key to its byte representation.
// expectedN is the expected number of elements for Bloom filter sizing.
func WithBloomFilter[K comparable, V any](keyToBytes func(K) []byte, expectedN uint) Option[K, V] {
	return func(c *Cache[K, V]) {
		c.keyToBytes = keyToBytes

		// Error is structurally impossible: expectedN > 0 enforced below, FP rate is constant.
		bf, err := bloom.NewWithEstimates(max(expectedN, 1), defaultBloomFPRate)
		if err != nil {
			panic("lru: bloom filter initialization failed: " + err.Error())
		}

		c.filter = bf
	}
}

// WithCostEviction enables sampling-based eviction with a cost function.
// Higher cost = less desirable to evict. sampleSize entries are sampled
// from the LRU tail; the one with lowest cost is evicted.
func WithCostEviction[K comparable, V any](sampleSize int, costFunc func(accessCount, sizeBytes int64) float64) Option[K, V] {
	return func(c *Cache[K, V]) {
		c.sampleSize = sampleSize
		c.costFunc = costFunc
	}
}

// WithCloneFunc sets a function to clone values before insertion.
// Useful to detach values from shared memory arenas.
func WithCloneFunc[K comparable, V any](clone func(V) V) Option[K, V] {
	return func(c *Cache[K, V]) {
		c.cloneFunc = clone
	}
}

// New creates a new LRU cache. At least one capacity limit (WithMaxEntries
// or WithMaxBytes) must be provided; otherwise New panics.
func New[K comparable, V any](opts ...Option[K, V]) *Cache[K, V] {
	c := &Cache[K, V]{
		entries: make(map[K]*entry[K, V]),
	}

	for _, opt := range opts {
		opt(c)
	}

	if c.maxEntries <= 0 && c.maxSize <= 0 {
		panic("lru: at least one capacity limit (WithMaxEntries or WithMaxBytes) is required")
	}

	return c
}

// Len returns the number of entries in the cache.
func (c *Cache[K, V]) Len() int {
	c.mu.RLock()
	defer c.mu.RUnlock()

	return len(c.entries)
}
