package burndown

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestHistoryAnalyzer_CheckpointRoundTrip(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	// Create analyzer with some state.
	original := &HistoryAnalyzer{
		pathInterner:       NewPathInterner(),
		globalHistory:      make(sparseHistory),
		peopleHistories:    make([]sparseHistory, 2),
		matrix:             make([]map[int]int64, 2),
		reversedPeopleDict: []string{"alice", "bob"},
		tick:               42,
		previousTick:       41,
		renames:            map[string]string{"old.go": "new.go"},
		renamesReverse:     map[string]map[string]bool{"new.go": {"old.go": true}},
		shards:             make([]*Shard, 2),
	}

	// Add some path interner entries.
	original.pathInterner.Intern("src/main.go")
	original.pathInterner.Intern("src/util.go")
	original.pathInterner.Intern("pkg/lib.go")

	// Add some global history.
	original.globalHistory[10] = map[int]int64{0: 100, 1: 50}
	original.globalHistory[20] = map[int]int64{0: 90, 1: 60}

	// Initialize people histories.
	original.peopleHistories[0] = sparseHistory{10: {0: 80}}
	original.peopleHistories[1] = sparseHistory{10: {1: 40}}

	// Initialize matrix.
	original.matrix[0] = map[int]int64{0: 1000, 1: 500}
	original.matrix[1] = map[int]int64{0: 200, 1: 800}

	// Initialize shards with some state.
	for i := range original.shards {
		original.shards[i] = &Shard{
			fileHistoriesByID: make([]sparseHistory, 0),
			activeIDs:         []PathID{PathID(i)},
			globalHistory:     sparseHistory{5: {0: int64(i * 10)}},
			peopleHistories:   make([]sparseHistory, 0),
			matrix:            make([]map[int]int64, 0),
			mergedByID:        map[PathID]bool{PathID(i): true},
			deletionsByID:     map[PathID]bool{},
		}
	}

	// Save checkpoint.
	err := original.SaveCheckpoint(dir)
	require.NoError(t, err)

	// Create new analyzer and restore.
	restored := &HistoryAnalyzer{
		pathInterner: NewPathInterner(),
		shards:       make([]*Shard, 2),
	}

	for i := range restored.shards {
		restored.shards[i] = &Shard{}
	}

	err = restored.LoadCheckpoint(dir)
	require.NoError(t, err)

	// Verify path interner.
	assert.Len(t, restored.pathInterner.rev, 3)
	id1, ok := restored.pathInterner.ids["src/main.go"]
	assert.True(t, ok)
	id2, ok := restored.pathInterner.ids["src/util.go"]
	assert.True(t, ok)
	id3, ok := restored.pathInterner.ids["pkg/lib.go"]
	assert.True(t, ok)
	assert.NotEqual(t, id1, id2)
	assert.NotEqual(t, id2, id3)

	// Verify global history.
	assert.Equal(t, original.globalHistory, restored.globalHistory)

	// Verify people histories.
	assert.Equal(t, original.peopleHistories, restored.peopleHistories)

	// Verify matrix.
	assert.Equal(t, original.matrix, restored.matrix)

	// Verify reversed people dict.
	assert.Equal(t, original.reversedPeopleDict, restored.reversedPeopleDict)

	// Verify tick state.
	assert.Equal(t, original.tick, restored.tick)
	assert.Equal(t, original.previousTick, restored.previousTick)

	// Verify renames.
	assert.Equal(t, original.renames, restored.renames)
	assert.Equal(t, original.renamesReverse, restored.renamesReverse)

	// Verify shards.
	for i := range original.shards {
		assert.Equal(t, original.shards[i].activeIDs, restored.shards[i].activeIDs)
		assert.Equal(t, original.shards[i].globalHistory, restored.shards[i].globalHistory)
		assert.Equal(t, original.shards[i].mergedByID, restored.shards[i].mergedByID)
	}
}

func TestHistoryAnalyzer_CheckpointSize(t *testing.T) {
	t.Parallel()

	analyzer := &HistoryAnalyzer{
		pathInterner:  NewPathInterner(),
		globalHistory: make(sparseHistory),
		shards:        make([]*Shard, 2),
	}

	// Add some entries.
	analyzer.pathInterner.Intern("a.go")
	analyzer.pathInterner.Intern("b.go")
	analyzer.globalHistory[1] = map[int]int64{0: 10}

	for i := range analyzer.shards {
		analyzer.shards[i] = &Shard{activeIDs: []PathID{PathID(i)}}
	}

	size := analyzer.CheckpointSize()
	assert.Positive(t, size)
}

func TestHistoryAnalyzer_Checkpointable(t *testing.T) {
	t.Parallel()

	// Verify HistoryAnalyzer implements Checkpointable interface.
	var analyzer interface {
		SaveCheckpoint(dir string) error
		LoadCheckpoint(dir string) error
		CheckpointSize() int64
	} = &HistoryAnalyzer{}

	_ = analyzer
}
