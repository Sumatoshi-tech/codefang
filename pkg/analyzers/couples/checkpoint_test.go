package couples

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestSaveCheckpoint_CreatesFile(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	c := &HistoryAnalyzer{}
	require.NoError(t, c.Initialize(nil))

	err := c.SaveCheckpoint(dir)
	require.NoError(t, err)

	expectedPath := filepath.Join(dir, checkpointBasename+".json")
	_, statErr := os.Stat(expectedPath)
	require.NoError(t, statErr, "checkpoint file should exist")
}

func TestLoadCheckpoint_RestoresState(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	original := &HistoryAnalyzer{PeopleNumber: 2}
	require.NoError(t, original.Initialize(nil))
	original.seenFiles["a.go"] = true
	original.seenFiles["b.go"] = true

	hash := gitlib.NewHash("1111111111111111111111111111111111111111")
	original.merges[hash] = true

	require.NoError(t, original.SaveCheckpoint(dir))

	restored := &HistoryAnalyzer{}
	require.NoError(t, restored.LoadCheckpoint(dir))

	assert.True(t, restored.seenFiles["a.go"])
	assert.True(t, restored.seenFiles["b.go"])
	assert.True(t, restored.merges[hash])
	assert.Equal(t, 2, restored.PeopleNumber)
}

func TestCheckpointSize_ReturnsPositiveValue(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{
		PeopleNumber:       5,
		reversedPeopleDict: []string{"a", "b", "c", "d", "e"},
	}
	require.NoError(t, c.Initialize(nil))

	c.seenFiles["a.go"] = true
	c.merges[gitlib.NewHash("abc123")] = true

	size := c.CheckpointSize()
	assert.Positive(t, size, "checkpoint size should be positive")
}

func TestCheckpointRoundTrip_PreservesAllState(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	original := &HistoryAnalyzer{
		PeopleNumber:       3,
		reversedPeopleDict: []string{"alice", "bob", "carol"},
	}
	require.NoError(t, original.Initialize(nil))

	original.seenFiles["main.go"] = true
	original.seenFiles["util.go"] = true

	hash1 := gitlib.NewHash("1111111111111111111111111111111111111111")
	hash2 := gitlib.NewHash("2222222222222222222222222222222222222222")
	original.merges[hash1] = true
	original.merges[hash2] = true

	require.NoError(t, original.SaveCheckpoint(dir))

	restored := &HistoryAnalyzer{}
	require.NoError(t, restored.LoadCheckpoint(dir))

	assert.Equal(t, original.PeopleNumber, restored.PeopleNumber)
	assert.Equal(t, original.reversedPeopleDict, restored.reversedPeopleDict)
	assert.True(t, restored.seenFiles["main.go"])
	assert.True(t, restored.seenFiles["util.go"])
	assert.Len(t, restored.seenFiles, 2)
	assert.True(t, restored.merges[hash1])
	assert.True(t, restored.merges[hash2])
	assert.Len(t, restored.merges, 2)
}
