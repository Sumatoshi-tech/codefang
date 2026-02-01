// Package cache provides caching utilities for analyzers.
package cache

import (
	"sync"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// HashSet is a thread-safe set of blob hashes.
// Useful for tracking seen commits, merges, etc.
type HashSet struct {
	data map[gitlib.Hash]struct{}
	mu   sync.RWMutex
}

// NewHashSet creates a new hash set.
func NewHashSet() *HashSet {
	return &HashSet{
		data: make(map[gitlib.Hash]struct{}),
	}
}

// Add adds a hash to the set. Returns true if the hash was new.
func (s *HashSet) Add(hash gitlib.Hash) bool {
	s.mu.Lock()
	defer s.mu.Unlock()

	if _, exists := s.data[hash]; exists {
		return false
	}

	s.data[hash] = struct{}{}

	return true
}

// Contains returns true if the hash is in the set.
func (s *HashSet) Contains(hash gitlib.Hash) bool {
	s.mu.RLock()
	defer s.mu.RUnlock()

	_, exists := s.data[hash]

	return exists
}

// Len returns the number of hashes in the set.
func (s *HashSet) Len() int {
	s.mu.RLock()
	defer s.mu.RUnlock()

	return len(s.data)
}

// Clear removes all hashes from the set.
func (s *HashSet) Clear() {
	s.mu.Lock()
	defer s.mu.Unlock()

	s.data = make(map[gitlib.Hash]struct{})
}

// BlobCache is a generic cache keyed by blob hash.
// It is safe for concurrent use.
type BlobCache[T any] struct {
	data map[gitlib.Hash]T
	mu   sync.RWMutex
}

// NewBlobCache creates a new blob cache.
func NewBlobCache[T any]() *BlobCache[T] {
	return &BlobCache[T]{
		data: make(map[gitlib.Hash]T),
	}
}

// Get retrieves a value from the cache.
// Returns the value and true if found, zero value and false otherwise.
func (c *BlobCache[T]) Get(hash gitlib.Hash) (T, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()

	val, found := c.data[hash]

	return val, found
}

// Set stores a value in the cache.
func (c *BlobCache[T]) Set(hash gitlib.Hash, value T) {
	c.mu.Lock()
	defer c.mu.Unlock()

	c.data[hash] = value
}

// GetOrCompute retrieves a value from the cache, or computes and stores it if not found.
// The compute function is called without holding the lock, so concurrent calls for the
// same key may compute the value multiple times (but only one will be stored).
func (c *BlobCache[T]) GetOrCompute(hash gitlib.Hash, compute func() (T, error)) (T, error) {
	// Fast path: check if already cached.
	if val, found := c.Get(hash); found {
		return val, nil
	}

	// Slow path: compute the value.
	val, err := compute()
	if err != nil {
		var zero T

		return zero, err
	}

	c.Set(hash, val)

	return val, nil
}

// Len returns the number of items in the cache.
func (c *BlobCache[T]) Len() int {
	c.mu.RLock()
	defer c.mu.RUnlock()

	return len(c.data)
}

// Clear removes all items from the cache.
func (c *BlobCache[T]) Clear() {
	c.mu.Lock()
	defer c.mu.Unlock()

	c.data = make(map[gitlib.Hash]T)
}
