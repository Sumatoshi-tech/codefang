package rbtree

import (
	"errors"
	"fmt"
	"hash/fnv"
	"sync"
)

// ErrSerializeShards is returned when shard serialization fails.
var ErrSerializeShards = errors.New("failed to serialize shards")

// ErrDeserializeShards is returned when shard deserialization fails.
var ErrDeserializeShards = errors.New("failed to deserialize shards")

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

// Serialize serializes all shards to disk.
// It uses basePath as a prefix and appends ".shard.N".
// Only hibernated shards are serialized (empty shards are skipped).
func (sa *ShardedAllocator) Serialize(basePath string) error {
	var errs []error

	var mu sync.Mutex

	wg := sync.WaitGroup{}
	wg.Add(len(sa.shards))

	for idx, shard := range sa.shards {
		go func(shardIdx int, alloc *Allocator) {
			defer wg.Done()

			// Only serialize if hibernated (storage == nil).
			if alloc.storage != nil {
				// Skip non-hibernated shards (they're likely empty and below threshold).
				return
			}

			path := fmt.Sprintf("%s.shard.%d", basePath, shardIdx)

			err := alloc.Serialize(path)
			if err != nil {
				mu.Lock()

				errs = append(errs, err)

				mu.Unlock()
			}
		}(idx, shard)
	}

	wg.Wait()

	if len(errs) > 0 {
		return fmt.Errorf("%w: %v", ErrSerializeShards, errs)
	}

	return nil
}

// Deserialize reads all shards from disk.
func (sa *ShardedAllocator) Deserialize(basePath string) error {
	var errs []error

	var mu sync.Mutex

	wg := sync.WaitGroup{}
	wg.Add(len(sa.shards))

	for idx, shard := range sa.shards {
		go func(shardIdx int, alloc *Allocator) {
			defer wg.Done()

			path := fmt.Sprintf("%s.shard.%d", basePath, shardIdx)

			err := alloc.Deserialize(path)
			if err != nil {
				mu.Lock()

				errs = append(errs, err)

				mu.Unlock()
			}
		}(idx, shard)
	}

	wg.Wait()

	if len(errs) > 0 {
		return fmt.Errorf("%w: %v", ErrDeserializeShards, errs)
	}

	return nil
}
