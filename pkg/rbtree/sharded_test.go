package rbtree

import (
	"fmt"
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestNewShardedAllocator(t *testing.T) {
	sa := NewShardedAllocator(4, 1000)
	assert.Equal(t, 4, len(sa.shards))
	assert.Equal(t, 250, sa.shards[0].HibernationThreshold)
}

func TestShardedAllocator_GetShard(t *testing.T) {
	sa := NewShardedAllocator(4, 0)
	
	s1 := sa.GetShard("file1")
	s2 := sa.GetShard("file1")
	_ = sa.GetShard("file2") // Ensure it doesn't crash
	
	assert.Equal(t, s1, s2)
	// s3 might be same or different depending on hash
	
	// Check distribution
	counts := make(map[*Allocator]int)
	for i := 0; i < 100; i++ {
		s := sa.GetShard(fmt.Sprintf("file%d", i))
		counts[s]++
	}
	
	assert.Equal(t, 4, len(counts)) // Likely to hit all 4 with 100 files
}

func TestShardedAllocator_HibernateBoot(t *testing.T) {
	sa := NewShardedAllocator(2, 0) // Threshold 0 to force hibernate
	
	// Add some data
	a1 := sa.GetShard("a")
	a1.malloc()
	
	sa.Hibernate()
	
	// Verify shards are hibernated
	// We can't check private fields easily, but calling malloc should panic or work depending on implementation?
	// Allocator.malloc panics if hibernated (storage == nil).
	
	assert.Panics(t, func() {
		a1.malloc()
	})
	
	sa.Boot()
	
	assert.NotPanics(t, func() {
		a1.malloc()
	})
}
