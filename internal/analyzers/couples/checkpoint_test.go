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
	original.seenFiles.Add([]byte("a.go"))
	original.seenFiles.Add([]byte("b.go"))

	hash := gitlib.NewHash("1111111111111111111111111111111111111111")
	original.merges.SeenOrAdd(hash)

	require.NoError(t, original.SaveCheckpoint(dir))

	restored := &HistoryAnalyzer{}
	require.NoError(t, restored.LoadCheckpoint(dir))

	assert.True(t, restored.seenFiles.Test([]byte("a.go")))
	assert.True(t, restored.seenFiles.Test([]byte("b.go")))
	assert.True(t, restored.merges.SeenOrAdd(hash), "restored tracker should contain the saved merge")
	assert.Equal(t, 2, restored.PeopleNumber)
}

func TestCheckpointSize_ReturnsPositiveValue(t *testing.T) {
	t.Parallel()

	c := &HistoryAnalyzer{
		PeopleNumber:       5,
		reversedPeopleDict: []string{"a", "b", "c", "d", "e"},
	}
	require.NoError(t, c.Initialize(nil))

	c.seenFiles.Add([]byte("a.go"))
	c.merges.SeenOrAdd(gitlib.NewHash("abc123"))

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

	original.seenFiles.Add([]byte("main.go"))
	original.seenFiles.Add([]byte("util.go"))

	hash1 := gitlib.NewHash("1111111111111111111111111111111111111111")
	hash2 := gitlib.NewHash("2222222222222222222222222222222222222222")

	original.merges.SeenOrAdd(hash1)
	original.merges.SeenOrAdd(hash2)

	require.NoError(t, original.SaveCheckpoint(dir))

	restored := &HistoryAnalyzer{}
	require.NoError(t, restored.LoadCheckpoint(dir))

	assert.Equal(t, original.PeopleNumber, restored.PeopleNumber)
	assert.Equal(t, original.reversedPeopleDict, restored.reversedPeopleDict)
	assert.True(t, restored.seenFiles.Test([]byte("main.go")))
	assert.True(t, restored.seenFiles.Test([]byte("util.go")))
	assert.False(t, restored.seenFiles.Test([]byte("nonexistent.go")))
	assert.True(t, restored.merges.SeenOrAdd(hash1), "restored tracker should contain merge1")
	assert.True(t, restored.merges.SeenOrAdd(hash2), "restored tracker should contain merge2")
}
