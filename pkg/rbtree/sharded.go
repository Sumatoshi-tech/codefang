package rbtree

import (
	"hash/fnv"
	"sync"
)

// minHibernationThreshold is the minimal reasonable default if division results in 0.
const minHibernationThreshold = 1000

// ShardedAllocator manages multiple Allocators to allow parallel access.
type ShardedAllocator struct {
	shards []*Allocator
}

// NewShardedAllocator creates a new ShardedAllocator with n shards.
func NewShardedAllocator(shardCount, hibernationThreshold int) *ShardedAllocator {
	if shardCount <= 0 {
		shardCount = 1
	}

	shards := make([]*Allocator, shardCount)

	for idx := range shardCount {
		shards[idx] = NewAllocator()

		if hibernationThreshold > 0 {
			shards[idx].HibernationThreshold = hibernationThreshold / shardCount
			if shards[idx].HibernationThreshold == 0 {
				shards[idx].HibernationThreshold = minHibernationThreshold
			}
		}
	}

	return &ShardedAllocator{shards: shards}
}

// GetShard returns the allocator shard for the given key.
func (sa *ShardedAllocator) GetShard(key string) *Allocator {
	hasher := fnv.New32a()
	hasher.Write([]byte(key))

	idx := int(hasher.Sum32()) % len(sa.shards)
	if idx < 0 {
		idx = -idx
	}

	return sa.shards[idx]
}

// Shards returns all underlying allocators.
func (sa *ShardedAllocator) Shards() []*Allocator {
	return sa.shards
}

// Hibernate hibernates all shards in parallel.
func (sa *ShardedAllocator) Hibernate() {
	wg := sync.WaitGroup{}
	wg.Add(len(sa.shards))

	for _, shard := range sa.shards {
		go func(alloc *Allocator) {
			defer wg.Done()

			// Force hibernation even if below threshold by temporarily setting threshold to 0.
			originalThreshold := alloc.HibernationThreshold
			alloc.HibernationThreshold = 0
			alloc.Hibernate()
			alloc.HibernationThreshold = originalThreshold
		}(shard)
	}

	wg.Wait()
}

// Boot boots all shards in parallel.
func (sa *ShardedAllocator) Boot() {
	wg := sync.WaitGroup{}
	wg.Add(len(sa.shards))

	for _, shard := range sa.shards {
		go func(alloc *Allocator) {
			defer wg.Done()

			alloc.Boot()
		}(shard)
	}

	wg.Wait()
}
