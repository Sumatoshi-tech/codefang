package rbtree

import (
	"fmt"
	"hash/fnv"
	"sync"
)

// ShardedAllocator manages multiple Allocators to allow parallel access.
type ShardedAllocator struct {
	shards []*Allocator
}

// NewShardedAllocator creates a new ShardedAllocator with n shards.
func NewShardedAllocator(n int, hibernationThreshold int) *ShardedAllocator {
	if n <= 0 {
		n = 1
	}
	shards := make([]*Allocator, n)
	for i := 0; i < n; i++ {
		shards[i] = NewAllocator()
		if hibernationThreshold > 0 {
			shards[i].HibernationThreshold = hibernationThreshold / n
			if shards[i].HibernationThreshold == 0 {
				shards[i].HibernationThreshold = 1000 // minimal reasonable default if division results in 0
			}
		}
	}
	return &ShardedAllocator{shards: shards}
}

// GetShard returns the allocator shard for the given key.
func (s *ShardedAllocator) GetShard(key string) *Allocator {
	h := fnv.New32a()
	h.Write([]byte(key))
	idx := int(h.Sum32()) % len(s.shards)
	if idx < 0 {
		idx = -idx
	}
	return s.shards[idx]
}

// Shards returns all underlying allocators.
func (s *ShardedAllocator) Shards() []*Allocator {
	return s.shards
}

// Hibernate hibernates all shards in parallel.
func (s *ShardedAllocator) Hibernate() {
	wg := sync.WaitGroup{}
	wg.Add(len(s.shards))
	for _, shard := range s.shards {
		go func(a *Allocator) {
			defer wg.Done()
			// Force hibernation even if below threshold by temporarily setting threshold to 0
			originalThreshold := a.HibernationThreshold
			a.HibernationThreshold = 0
			a.Hibernate()
			a.HibernationThreshold = originalThreshold
		}(shard)
	}
	wg.Wait()
}

// Boot boots all shards in parallel.
func (s *ShardedAllocator) Boot() {
	wg := sync.WaitGroup{}
	wg.Add(len(s.shards))
	for _, shard := range s.shards {
		go func(a *Allocator) {
			defer wg.Done()
			a.Boot()
		}(shard)
	}
	wg.Wait()
}

// Serialize serializes all shards to disk.
// It uses basePath as a prefix and appends ".shard.N".
// Only hibernated shards are serialized (empty shards are skipped).
func (s *ShardedAllocator) Serialize(basePath string) error {
	var errs []error
	var mu sync.Mutex
	wg := sync.WaitGroup{}
	wg.Add(len(s.shards))

	for i, shard := range s.shards {
		go func(idx int, a *Allocator) {
			defer wg.Done()
			// Only serialize if hibernated (storage == nil)
			if a.storage != nil {
				// Skip non-hibernated shards (they're likely empty and below threshold)
				return
			}
			path := fmt.Sprintf("%s.shard.%d", basePath, idx)
			if err := a.Serialize(path); err != nil {
				mu.Lock()
				errs = append(errs, err)
				mu.Unlock()
			}
		}(i, shard)
	}
	wg.Wait()

	if len(errs) > 0 {
		return fmt.Errorf("failed to serialize shards: %v", errs)
	}
	return nil
}

// Deserialize reads all shards from disk.
func (s *ShardedAllocator) Deserialize(basePath string) error {
	var errs []error
	var mu sync.Mutex
	wg := sync.WaitGroup{}
	wg.Add(len(s.shards))

	for i, shard := range s.shards {
		go func(idx int, a *Allocator) {
			defer wg.Done()
			path := fmt.Sprintf("%s.shard.%d", basePath, idx)
			if err := a.Deserialize(path); err != nil {
				mu.Lock()
				errs = append(errs, err)
				mu.Unlock()
			}
		}(i, shard)
	}
	wg.Wait()

	if len(errs) > 0 {
		return fmt.Errorf("failed to deserialize shards: %v", errs)
	}
	return nil
}
