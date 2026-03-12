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
	original := NewHistoryAnalyzer()
	original.pathInterner = NewPathInterner()
	original.ReversedPeopleDict = []string{"alice", "bob"}
	original.tick = 42
	original.previousTick = 41
	original.renames = map[string]string{"old.go": "new.go"}
	original.renamesReverse = map[string]map[string]bool{"new.go": {"old.go": true}}
	original.shards = make([]*Shard, 2)

	// Add some path interner entries.
	original.pathInterner.Intern("src/main.go")
	original.pathInterner.Intern("src/util.go")
	original.pathInterner.Intern("pkg/lib.go")

	// Initialize shards with some state.
	for i := range original.shards {
		original.shards[i] = &Shard{
			fileHistoriesByID: make([]sparseHistory, 0),
			activeIDs:         []PathID{PathID(i)},
			mergedByID:        map[PathID]bool{PathID(i): true},
			deletionsByID:     map[PathID]bool{},
		}
	}

	// Save checkpoint.
	err := original.SaveCheckpoint(dir)
	require.NoError(t, err)

	// Create new analyzer and restore.
	restored := NewHistoryAnalyzer()
	restored.pathInterner = NewPathInterner()
	restored.shards = make([]*Shard, 2)

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

	// Verify reversed people dict.
	assert.Equal(t, original.ReversedPeopleDict, restored.ReversedPeopleDict)

	// Verify tick state.
	assert.Equal(t, original.tick, restored.tick)
	assert.Equal(t, original.previousTick, restored.previousTick)

	// Verify renames.
	assert.Equal(t, original.renames, restored.renames)
	assert.Equal(t, original.renamesReverse, restored.renamesReverse)

	// Verify shards.
	for i := range original.shards {
		assert.Equal(t, original.shards[i].activeIDs, restored.shards[i].activeIDs)
		assert.Equal(t, original.shards[i].mergedByID, restored.shards[i].mergedByID)
	}
}

func TestHistoryAnalyzer_CheckpointSize(t *testing.T) {
	t.Parallel()

	analyzer := NewHistoryAnalyzer()
	analyzer.pathInterner = NewPathInterner()
	analyzer.shards = make([]*Shard, 2)

	// Add some entries.
	analyzer.pathInterner.Intern("a.go")
	analyzer.pathInterner.Intern("b.go")

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
	} = NewHistoryAnalyzer()

	_ = analyzer
}
