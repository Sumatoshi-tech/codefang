package rbtree_test

import (
	"fmt"
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/pkg/rbtree"
)

func TestNewShardedAllocator(t *testing.T) {
	t.Parallel()

	sa := rbtree.NewShardedAllocator(4, 1000)
	assert.Len(t, sa.Shards(), 4)
	assert.Equal(t, 250, sa.Shards()[0].HibernationThreshold)
}

func TestShardedAllocator_GetShard(t *testing.T) {
	t.Parallel()

	sa := rbtree.NewShardedAllocator(4, 0)

	s1 := sa.GetShard("file1")
	s2 := sa.GetShard("file1")
	_ = sa.GetShard("file2") // Ensure it doesn't crash.

	assert.Equal(t, s1, s2)
	// S3 might be same or different depending on hash.

	// Check distribution.
	counts := make(map[*rbtree.Allocator]int)

	for idx := range 100 {
		shard := sa.GetShard(fmt.Sprintf("file%d", idx))
		counts[shard]++
	}

	assert.Len(t, counts, 4) // Likely to hit all 4 with 100 files.
}

func TestShardedAllocator_HibernateBoot(t *testing.T) {
	t.Parallel()

	sa := rbtree.NewShardedAllocator(2, 0) // Threshold 0 to force hibernate.

	// Add some data.
	a1 := sa.GetShard("a")
	a1.Clone() // Ensure it's usable before hibernate.

	sa.Hibernate()

	// Verify shards are hibernated.
	// Allocator.Clone panics if hibernated (storage == nil).
	assert.Panics(t, func() {
		a1.Clone()
	})

	sa.Boot()

	assert.NotPanics(t, func() {
		a1.Clone()
	})
}
