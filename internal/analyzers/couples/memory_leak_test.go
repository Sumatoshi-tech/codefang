package couples

import (
	"fmt"
	"runtime"
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestPruneAndCapEntries_MapCompactionMemory(t *testing.T) {
	t.Parallel()

	// Skip in short mode as this allocates ~500 MiB.
	if testing.Short() {
		t.Skip("skipping memory test in short mode")
	}

	// 1. Create bloated state.
	files := make(map[string]map[string]int)

	// Create 50 files, each with 50,000 dependencies (weight 1).
	for i := range 50 {
		file1 := fmt.Sprintf("file_%d.go", i)
		lane := make(map[string]int, 50000)

		for j := range 50000 {
			file2 := fmt.Sprintf("dep_%d.go", j)
			lane[file2] = 1
		}

		// Add some strong dependencies to survive pruning.
		for j := range 500 {
			file2 := fmt.Sprintf("strong_dep_%d.go", j)
			lane[file2] = 10
		}

		files[file1] = lane
	}

	runtime.GC()

	var msBefore runtime.MemStats

	runtime.ReadMemStats(&msBefore)
	t.Logf("HeapInuse before prune: %d MiB", msBefore.HeapInuse/(1024*1024))

	// 2. Prune and cap (this should trigger compaction).
	// minWeight 2 will remove all the 50,000 weak dependencies.
	// maxEntries 500 will keep the 500 strong dependencies.
	pruneAndCapEntries(files, 2, 500)

	runtime.GC()

	var msAfter runtime.MemStats

	runtime.ReadMemStats(&msAfter)
	t.Logf("HeapInuse after prune: %d MiB", msAfter.HeapInuse/(1024*1024))

	// Assert that memory dropped by at least 80% due to compaction.
	// If compaction didn't work, HeapInuse would stay roughly the same
	// because Go map buckets are never freed upon delete().
	assert.Less(t, msAfter.HeapInuse, msBefore.HeapInuse/5, "Memory should drop significantly after map compaction")

	// Assert data correctness.
	for i := range 50 {
		file1 := fmt.Sprintf("file_%d.go", i)
		assert.Len(t, files[file1], 500, "Should cap at 500 entries")
	}
}
