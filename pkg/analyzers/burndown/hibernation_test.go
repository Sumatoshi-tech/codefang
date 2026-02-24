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
	defer analyzer.CleanupSpills()

	// Setup: add tracking data and active IDs.
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

	// activeIDs must survive (files are live objects).
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
			mergedByID:        make(map[PathID]bool),
			deletionsByID:     make(map[PathID]bool),
		}
	}

	analyzer := NewHistoryAnalyzer()
	analyzer.shards = shards
	analyzer.shardSpills = make([]shardSpillState, numShards)
	analyzer.pathInterner = NewPathInterner()

	return analyzer
}

func TestHibernate_SpillsFilesToDisk(t *testing.T) {
	t.Parallel()

	analyzer := createAnalyzerWithShards(shardCount)
	defer analyzer.CleanupSpills()

	// Create files with known state.
	id := analyzer.pathInterner.Intern("test.go")
	analyzer.ensureCapacity(analyzer.shards[0], id)

	file := burndown.NewFile(1, 100)
	file.Update(2, 50, 20, 10) // Modify to create multiple segments.
	analyzer.shards[0].filesByID[id] = file
	analyzer.shards[0].activeIDs = append(analyzer.shards[0].activeIDs, id)

	err := analyzer.Hibernate()
	if err != nil {
		t.Fatalf("Hibernate() failed: %v", err)
	}

	// File should be nil after spill (freed from memory).
	if analyzer.shards[0].filesByID[id] != nil {
		t.Error("file should be nil after Hibernate spill")
	}

	// activeIDs must be preserved.
	if len(analyzer.shards[0].activeIDs) != 1 {
		t.Error("activeIDs should be preserved")
	}
}

func TestBoot_RestoresFiles(t *testing.T) {
	t.Parallel()

	analyzer := createAnalyzerWithShards(shardCount)
	defer analyzer.CleanupSpills()

	// Create a file with known state.
	id := analyzer.pathInterner.Intern("restore.go")
	analyzer.ensureCapacity(analyzer.shards[0], id)

	file := burndown.NewFile(1, 100)
	file.Update(2, 50, 20, 10)
	lenBefore := file.Len()
	analyzer.shards[0].filesByID[id] = file
	analyzer.shards[0].activeIDs = append(analyzer.shards[0].activeIDs, id)

	// Hibernate spills files to disk.
	err := analyzer.Hibernate()
	if err != nil {
		t.Fatalf("Hibernate() failed: %v", err)
	}

	// Boot restores files from disk.
	err = analyzer.Boot()
	if err != nil {
		t.Fatalf("Boot() failed: %v", err)
	}

	// File should be restored.
	restored := analyzer.shards[0].filesByID[id]
	if restored == nil {
		t.Fatal("file should be restored after Boot")
	}

	if restored.Len() != lenBefore {
		t.Errorf("restored file Len: got %d, want %d", restored.Len(), lenBefore)
	}

	// File should be usable for updates after restoration.
	restored.Update(3, 0, 5, 0) // Insert 5 lines at start.

	if restored.Len() != lenBefore+5 {
		t.Errorf("update after restore: Len got %d, want %d", restored.Len(), lenBefore+5)
	}
}

func TestHibernate_Boot_PreservesFileState(t *testing.T) {
	t.Parallel()

	analyzer := createAnalyzerWithShards(shardCount)
	defer analyzer.CleanupSpills()

	// Create multiple files across shards.
	ids := make([]PathID, 3)
	names := []string{"a.go", "b.go", "c.go"}
	lengths := []int{100, 200, 50}

	for i, name := range names {
		ids[i] = analyzer.pathInterner.Intern(name)
		shardIdx := analyzer.getShardIndex(name)
		shard := analyzer.shards[shardIdx]
		analyzer.ensureCapacity(shard, ids[i])
		file := burndown.NewFile(1, lengths[i])
		shard.filesByID[ids[i]] = file
		shard.activeIDs = append(shard.activeIDs, ids[i])
	}

	// Hibernate + Boot round-trip.
	err := analyzer.Hibernate()
	if err != nil {
		t.Fatalf("Hibernate() failed: %v", err)
	}

	err = analyzer.Boot()
	if err != nil {
		t.Fatalf("Boot() failed: %v", err)
	}

	// Verify all files are restored with correct lengths.
	for i, name := range names {
		shardIdx := analyzer.getShardIndex(name)
		shard := analyzer.shards[shardIdx]
		file := shard.filesByID[ids[i]]

		if file == nil {
			t.Errorf("file %s not restored", name)

			continue
		}

		if file.Len() != lengths[i] {
			t.Errorf("file %s: Len got %d, want %d", name, file.Len(), lengths[i])
		}
	}
}

func TestHibernate_Boot_SpillsAndRestoresFileHistories(t *testing.T) {
	t.Parallel()

	analyzer := createAnalyzerWithShards(shardCount)
	defer analyzer.CleanupSpills()

	analyzer.TrackFiles = true

	id := analyzer.pathInterner.Intern("history.go")
	shard := analyzer.shards[0]
	analyzer.ensureCapacity(shard, id)
	shard.filesByID[id] = burndown.NewFile(1, 50)
	shard.activeIDs = append(shard.activeIDs, id)

	// Add file history data.
	shard.fileHistoriesByID[id] = sparseHistory{
		1: {0: 50},
		2: {1: 10},
	}

	err := analyzer.Hibernate()
	if err != nil {
		t.Fatalf("Hibernate() failed: %v", err)
	}

	// File history should be cleared from memory.
	if len(shard.fileHistoriesByID[id]) != 0 {
		t.Error("fileHistoriesByID should be cleared after Hibernate")
	}

	err = analyzer.Boot()
	if err != nil {
		t.Fatalf("Boot() failed: %v", err)
	}

	// File history should be restored.
	history := shard.fileHistoriesByID[id]
	if history == nil {
		t.Fatal("fileHistoriesByID not restored after Boot")
	}

	if history[1][0] != 50 {
		t.Errorf("file history[1][0]: got %d, want 50", history[1][0])
	}

	if history[2][1] != 10 {
		t.Errorf("file history[2][1]: got %d, want 10", history[2][1])
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
