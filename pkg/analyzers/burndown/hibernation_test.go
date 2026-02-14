package burndown

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/burndown"
	"github.com/Sumatoshi-tech/codefang/pkg/streaming"
)

func TestHistoryAnalyzer_ImplementsHibernatable(t *testing.T) {
	t.Parallel()

	var _ streaming.Hibernatable = (*HistoryAnalyzer)(nil)
}

func TestHibernate_ClearsShardTrackingMaps(t *testing.T) {
	t.Parallel()

	analyzer := createAnalyzerWithShards(shardCount)

	// Populate tracking maps.
	for i, shard := range analyzer.shards {
		shard.mergedByID[PathID(i)] = true
		shard.mergedByID[PathID(i+100)] = true
		shard.deletionsByID[PathID(i)] = true
	}

	// Verify maps have data.
	for _, shard := range analyzer.shards {
		if len(shard.mergedByID) == 0 {
			t.Fatal("test setup: mergedByID should have data")
		}

		if len(shard.deletionsByID) == 0 {
			t.Fatal("test setup: deletionsByID should have data")
		}
	}

	err := analyzer.Hibernate()
	if err != nil {
		t.Fatalf("Hibernate() failed: %v", err)
	}

	// Verify tracking maps are cleared.
	for i, shard := range analyzer.shards {
		if len(shard.mergedByID) != 0 {
			t.Errorf("shard[%d].mergedByID not cleared: got %d entries", i, len(shard.mergedByID))
		}

		if len(shard.deletionsByID) != 0 {
			t.Errorf("shard[%d].deletionsByID not cleared: got %d entries", i, len(shard.deletionsByID))
		}
	}
}

func TestHibernate_PreservesHistoryData(t *testing.T) {
	t.Parallel()

	analyzer := createAnalyzerWithShards(shardCount)

	// Add history data.
	analyzer.globalHistory[1] = map[int]int64{0: 100}
	analyzer.shards[0].globalHistory[1] = map[int]int64{0: 50}
	analyzer.shards[0].activeIDs = []PathID{1, 2, 3}

	err := analyzer.Hibernate()
	if err != nil {
		t.Fatalf("Hibernate() failed: %v", err)
	}

	// Verify history data is preserved.
	if analyzer.globalHistory[1][0] != 100 {
		t.Error("globalHistory was modified")
	}

	if analyzer.shards[0].globalHistory[1][0] != 50 {
		t.Error("shard globalHistory was modified")
	}

	if len(analyzer.shards[0].activeIDs) != 3 {
		t.Error("activeIDs was modified")
	}
}

func TestBoot_InitializesNilMaps(t *testing.T) {
	t.Parallel()

	analyzer := createAnalyzerWithShards(shardCount)

	// Set maps to nil.
	for _, shard := range analyzer.shards {
		shard.mergedByID = nil
		shard.deletionsByID = nil
	}

	err := analyzer.Boot()
	if err != nil {
		t.Fatalf("Boot() failed: %v", err)
	}

	// Verify maps are initialized.
	for i, shard := range analyzer.shards {
		if shard.mergedByID == nil {
			t.Errorf("shard[%d].mergedByID is nil after Boot()", i)
		}

		if shard.deletionsByID == nil {
			t.Errorf("shard[%d].deletionsByID is nil after Boot()", i)
		}
	}
}

func TestBoot_PreservesExistingMaps(t *testing.T) {
	t.Parallel()

	analyzer := createAnalyzerWithShards(shardCount)

	// Add data to maps.
	analyzer.shards[0].mergedByID[PathID(42)] = true
	analyzer.shards[0].deletionsByID[PathID(99)] = true

	err := analyzer.Boot()
	if err != nil {
		t.Fatalf("Boot() failed: %v", err)
	}

	// Verify existing data is preserved.
	if !analyzer.shards[0].mergedByID[PathID(42)] {
		t.Error("existing mergedByID entry was lost")
	}

	if !analyzer.shards[0].deletionsByID[PathID(99)] {
		t.Error("existing deletionsByID entry was lost")
	}
}

func TestHibernate_Boot_RoundTrip(t *testing.T) {
	t.Parallel()

	analyzer := createAnalyzerWithShards(shardCount)

	// Setup: add history and tracking data.
	analyzer.globalHistory[1] = map[int]int64{0: 100}
	analyzer.shards[0].activeIDs = []PathID{1, 2, 3}
	analyzer.shards[0].mergedByID[PathID(1)] = true

	// Hibernate.
	err := analyzer.Hibernate()
	if err != nil {
		t.Fatalf("Hibernate() failed: %v", err)
	}

	// Boot.
	err = analyzer.Boot()
	if err != nil {
		t.Fatalf("Boot() failed: %v", err)
	}

	// Verify history survived.
	if analyzer.globalHistory[1][0] != 100 {
		t.Error("globalHistory not preserved after round trip")
	}

	if len(analyzer.shards[0].activeIDs) != 3 {
		t.Error("activeIDs not preserved after round trip")
	}

	// Verify tracking maps are ready for new data.
	if analyzer.shards[0].mergedByID == nil {
		t.Error("mergedByID should be ready after round trip")
	}
}

const shardCount = 4

func createAnalyzerWithShards(numShards int) *HistoryAnalyzer {
	shards := make([]*Shard, numShards)
	for i := range numShards {
		shards[i] = &Shard{
			filesByID:         make([]*burndown.File, 0),
			fileHistoriesByID: make([]sparseHistory, 0),
			activeIDs:         make([]PathID, 0),
			globalHistory:     make(sparseHistory),
			peopleHistories:   make([]sparseHistory, 0),
			matrix:            make([]map[int]int64, 0),
			mergedByID:        make(map[PathID]bool),
			deletionsByID:     make(map[PathID]bool),
		}
	}

	return &HistoryAnalyzer{
		shards:        shards,
		globalHistory: make(sparseHistory),
		pathInterner:  NewPathInterner(),
	}
}

func TestHibernate_CompactsFileTimelines(t *testing.T) {
	t.Parallel()

	analyzer := createAnalyzerWithShards(shardCount)

	// Create a file with multiple segments that can be compacted.
	file := burndown.NewFile(1, 100) // tick 1, 100 lines.

	// Make some updates that create segments.
	file.Update(1, 50, 10, 0) // Insert 10 lines at position 50, same tick.
	file.Update(1, 60, 5, 0)  // Insert 5 more lines, same tick.
	file.Update(2, 0, 0, 10)  // Delete 10 lines at start, tick 2.
	file.Update(2, 10, 20, 0) // Insert 20 lines, tick 2.

	nodesBefore := file.Nodes()

	// Add file to shard.
	id := analyzer.pathInterner.Intern("test.go")
	analyzer.shards[0].filesByID = append(analyzer.shards[0].filesByID, file)
	analyzer.shards[0].activeIDs = append(analyzer.shards[0].activeIDs, id)

	// Ensure capacity.
	for len(analyzer.shards[0].filesByID) <= int(id) {
		analyzer.shards[0].filesByID = append(analyzer.shards[0].filesByID, nil)
	}

	analyzer.shards[0].filesByID[id] = file

	err := analyzer.Hibernate()
	if err != nil {
		t.Fatalf("Hibernate() failed: %v", err)
	}

	nodesAfter := file.Nodes()

	// Node count should decrease or stay the same (compaction).
	if nodesAfter > nodesBefore {
		t.Errorf("node count increased: before=%d, after=%d", nodesBefore, nodesAfter)
	}

	// File should still be functional.
	if file.Len() <= 0 {
		t.Error("file length should be positive after hibernation")
	}
}

func TestHibernate_PreservesFileState(t *testing.T) {
	t.Parallel()

	analyzer := createAnalyzerWithShards(shardCount)

	// Create a file with known state.
	file := burndown.NewFile(1, 50) // tick 1, 50 lines.
	lenBefore := file.Len()

	id := analyzer.pathInterner.Intern("preserve.go")
	analyzer.shards[0].filesByID = make([]*burndown.File, int(id)+1)
	analyzer.shards[0].filesByID[id] = file
	analyzer.shards[0].activeIDs = append(analyzer.shards[0].activeIDs, id)

	err := analyzer.Hibernate()
	if err != nil {
		t.Fatalf("Hibernate() failed: %v", err)
	}

	// File length should be preserved.
	if file.Len() != lenBefore {
		t.Errorf("file length changed: before=%d, after=%d", lenBefore, file.Len())
	}

	// File should still be usable for updates.
	file.Update(2, 25, 10, 0) // Insert 10 lines at position 25.

	if file.Len() != lenBefore+10 {
		t.Errorf("file update after hibernation failed: expected %d, got %d",
			lenBefore+10, file.Len())
	}
}

// BenchmarkHibernate measures hibernation performance with realistic shard data.
func BenchmarkHibernate(b *testing.B) {
	const (
		numShards       = 8
		entriesPerShard = 5000
	)

	analyzer := createAnalyzerWithShards(numShards)

	// Populate tracking maps.
	for _, shard := range analyzer.shards {
		for i := range entriesPerShard {
			shard.mergedByID[PathID(i)] = true
			shard.deletionsByID[PathID(i)] = true
		}
	}

	b.ReportAllocs()

	for b.Loop() {
		err := analyzer.Hibernate()
		if err != nil {
			b.Fatal(err)
		}

		// Re-populate for next iteration.
		for _, shard := range analyzer.shards {
			for i := range entriesPerShard {
				shard.mergedByID[PathID(i)] = true
				shard.deletionsByID[PathID(i)] = true
			}
		}
	}
}

// BenchmarkBoot measures boot performance.
func BenchmarkBoot(b *testing.B) {
	const numShards = 8

	analyzer := createAnalyzerWithShards(numShards)

	b.ReportAllocs()

	for b.Loop() {
		// Set to nil to simulate post-hibernate state.
		for _, shard := range analyzer.shards {
			shard.mergedByID = nil
			shard.deletionsByID = nil
		}

		err := analyzer.Boot()
		if err != nil {
			b.Fatal(err)
		}
	}
}
