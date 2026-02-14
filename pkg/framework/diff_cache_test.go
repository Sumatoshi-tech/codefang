package framework_test

import (
	"sync"
	"testing"

	"github.com/sergi/go-diff/diffmatchpatch"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

func makeDiffKey(oldB, newB byte) framework.DiffKey {
	var oldHash, newHash gitlib.Hash

	oldHash[0] = oldB
	newHash[0] = newB

	return framework.DiffKey{OldHash: oldHash, NewHash: newHash}
}

func makeTestFileDiff() plumbing.FileDiffData {
	return plumbing.FileDiffData{
		OldLinesOfCode: 100,
		NewLinesOfCode: 110,
		Diffs: []diffmatchpatch.Diff{
			{Type: diffmatchpatch.DiffEqual, Text: "LLLLLLLLLL"},
			{Type: diffmatchpatch.DiffInsert, Text: "LLLLLLLLLL"},
		},
	}
}

func TestDiffCache_GetPut(t *testing.T) {
	t.Parallel()

	cache := framework.NewDiffCache(1000)

	key := makeDiffKey(1, 2)
	diff := makeTestFileDiff()

	// Get on empty cache returns nil, false.
	got, found := cache.Get(key)
	assert.False(t, found)
	assert.Equal(t, plumbing.FileDiffData{}, got)

	// Put and Get.
	cache.Put(key, diff)
	got, found = cache.Get(key)
	require.True(t, found)
	assert.Equal(t, diff.OldLinesOfCode, got.OldLinesOfCode)
	assert.Equal(t, diff.NewLinesOfCode, got.NewLinesOfCode)
	assert.Len(t, got.Diffs, 2)
}

func TestDiffCache_LRUEviction(t *testing.T) {
	t.Parallel()

	// Cache with max 50 entries.
	cache := framework.NewDiffCache(50)

	// Add 60 entries.
	for i := range 60 {
		key := makeDiffKey(byte(i), byte(i+100))
		cache.Put(key, makeTestFileDiff())
	}

	stats := cache.Stats()
	assert.LessOrEqual(t, stats.Entries, 50)

	// First entries should be evicted.
	key0 := makeDiffKey(0, 100)
	_, found := cache.Get(key0)
	assert.False(t, found, "first entry should be evicted")

	// Last entries should still be present.
	key59 := makeDiffKey(59, 159)
	_, found = cache.Get(key59)
	assert.True(t, found, "last entry should still be present")
}

func TestDiffCache_Stats(t *testing.T) {
	t.Parallel()

	cache := framework.NewDiffCache(100)

	key1 := makeDiffKey(1, 2)
	key2 := makeDiffKey(3, 4)

	cache.Put(key1, makeTestFileDiff())

	// One hit, one miss.
	cache.Get(key1) // hit.
	cache.Get(key2) // miss.

	stats := cache.Stats()
	assert.Equal(t, int64(1), stats.Hits)
	assert.Equal(t, int64(1), stats.Misses)
	assert.Equal(t, 1, stats.Entries)
	assert.InDelta(t, 0.5, stats.HitRate(), 0.001)
}

func TestDiffCache_ConcurrentAccess(t *testing.T) {
	t.Parallel()

	cache := framework.NewDiffCache(1000)

	const goroutines = 50

	const operations = 100

	var wg sync.WaitGroup

	wg.Add(goroutines)

	for g := range goroutines {
		go func(id int) {
			defer wg.Done()

			for i := range operations {
				key := makeDiffKey(byte((id*operations+i)%256), byte((id*operations+i+1)%256))
				diff := makeTestFileDiff()

				cache.Put(key, diff)
				cache.Get(key)
			}
		}(g)
	}

	wg.Wait()

	// Just verify no panics and stats are reasonable.
	stats := cache.Stats()
	assert.Positive(t, stats.Entries)
}

func TestDiffCache_Clear(t *testing.T) {
	t.Parallel()

	cache := framework.NewDiffCache(100)

	key := makeDiffKey(1, 2)
	cache.Put(key, makeTestFileDiff())

	_, found := cache.Get(key)
	require.True(t, found)

	cache.Clear()

	_, found = cache.Get(key)
	assert.False(t, found)

	stats := cache.Stats()
	assert.Equal(t, 0, stats.Entries)
}

func TestDiffCacheStats_HitRate_Empty(t *testing.T) {
	t.Parallel()

	stats := framework.DiffCacheStats{}
	assert.InDelta(t, 0.0, stats.HitRate(), 0.001)
}

func TestDiffCache_DefaultSize(t *testing.T) {
	t.Parallel()

	cache := framework.NewDiffCache(0)

	stats := cache.Stats()
	assert.Equal(t, framework.DefaultDiffCacheSize, stats.MaxEntries)
}
