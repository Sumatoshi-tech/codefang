package couples

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestSaveCheckpoint_CreatesFile(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	c := &HistoryAnalyzer{}
	require.NoError(t, c.Initialize(nil))

	err := c.SaveCheckpoint(dir)
	require.NoError(t, err)

	// Verify file was created
	expectedPath := filepath.Join(dir, checkpointBasename+".json")
	_, statErr := os.Stat(expectedPath)
	require.NoError(t, statErr, "checkpoint file should exist")
}

func TestLoadCheckpoint_RestoresState(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	// Create analyzer with some state
	original := &HistoryAnalyzer{PeopleNumber: 2}
	require.NoError(t, original.Initialize(nil))
	original.files["a.go"] = map[string]int{"b.go": 5}
	original.peopleCommits[0] = 10
	original.peopleCommits[1] = 20

	// Save checkpoint
	require.NoError(t, original.SaveCheckpoint(dir))

	// Load into new analyzer
	restored := &HistoryAnalyzer{}
	require.NoError(t, restored.LoadCheckpoint(dir))

	// Verify state restored
	require.Equal(t, original.files, restored.files)
	require.Equal(t, original.peopleCommits, restored.peopleCommits)
	require.Equal(t, original.PeopleNumber, restored.PeopleNumber)
}

func TestCheckpointSize_ReturnsPositiveValue(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{PeopleNumber: 5}
	require.NoError(t, c.Initialize(nil))

	// Add some state
	c.files["a.go"] = map[string]int{"b.go": 3, "c.go": 2}
	c.peopleCommits[0] = 10

	size := c.CheckpointSize()
	require.Positive(t, size, "checkpoint size should be positive")
}

func TestCheckpointRoundTrip_PreservesAllState(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	// Create analyzer with comprehensive state
	original := &HistoryAnalyzer{
		PeopleNumber:       3,
		reversedPeopleDict: []string{"alice", "bob", "carol"},
	}
	require.NoError(t, original.Initialize(nil))

	// Populate files
	original.files["main.go"] = map[string]int{"util.go": 10, "test.go": 5}
	original.files["util.go"] = map[string]int{"main.go": 10}

	// Populate people
	original.people[0]["main.go"] = 15
	original.people[1]["util.go"] = 8

	// Populate peopleCommits
	original.peopleCommits[0] = 100
	original.peopleCommits[1] = 50
	original.peopleCommits[2] = 25

	// Add renames
	*original.renames = []rename{
		{FromName: "old.go", ToName: "new.go"},
	}

	// Save and load
	require.NoError(t, original.SaveCheckpoint(dir))

	restored := &HistoryAnalyzer{}
	require.NoError(t, restored.LoadCheckpoint(dir))

	// Verify all state
	require.Equal(t, original.files, restored.files)
	require.Equal(t, original.people, restored.people)
	require.Equal(t, original.peopleCommits, restored.peopleCommits)
	require.Equal(t, original.PeopleNumber, restored.PeopleNumber)
	require.Equal(t, original.reversedPeopleDict, restored.reversedPeopleDict)
	require.Equal(t, *original.renames, *restored.renames)
}
