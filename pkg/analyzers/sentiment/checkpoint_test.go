package sentiment

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestSaveCheckpoint_CreatesFile(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	err := s.SaveCheckpoint(dir)
	require.NoError(t, err)

	expectedPath := filepath.Join(dir, checkpointBasename+".json")
	_, statErr := os.Stat(expectedPath)
	require.NoError(t, statErr, "checkpoint file should exist")
}

func TestLoadCheckpoint_RestoresState(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	original := &HistoryAnalyzer{}
	require.NoError(t, original.Initialize(nil))

	// Add comment data.
	original.commentsByTick[0] = []string{"comment 1", "comment 2"}
	original.commentsByTick[1] = []string{"comment 3"}

	require.NoError(t, original.SaveCheckpoint(dir))

	restored := &HistoryAnalyzer{}
	require.NoError(t, restored.LoadCheckpoint(dir))

	require.Len(t, restored.commentsByTick, 2)
	require.Len(t, restored.commentsByTick[0], 2)
	require.Len(t, restored.commentsByTick[1], 1)
	require.Equal(t, "comment 1", restored.commentsByTick[0][0])
	require.Equal(t, "comment 3", restored.commentsByTick[1][0])
}

func TestCheckpointSize_ReturnsPositiveValue(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	s.commentsByTick[0] = []string{"This is a comment for testing"}

	size := s.CheckpointSize()
	require.Positive(t, size)
}

func TestCheckpointRoundTrip_PreservesAllState(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	original := &HistoryAnalyzer{}
	require.NoError(t, original.Initialize(nil))

	// Add multiple ticks with comments.
	original.commentsByTick[0] = []string{"tick 0 comment 1", "tick 0 comment 2"}
	original.commentsByTick[5] = []string{"tick 5 comment"}
	original.commentsByTick[10] = []string{"tick 10 a", "tick 10 b", "tick 10 c"}

	require.NoError(t, original.SaveCheckpoint(dir))

	restored := &HistoryAnalyzer{}
	require.NoError(t, restored.LoadCheckpoint(dir))

	// Verify all ticks.
	require.Len(t, restored.commentsByTick, 3)

	require.Len(t, restored.commentsByTick[0], 2)
	require.Equal(t, "tick 0 comment 1", restored.commentsByTick[0][0])
	require.Equal(t, "tick 0 comment 2", restored.commentsByTick[0][1])

	require.Len(t, restored.commentsByTick[5], 1)
	require.Equal(t, "tick 5 comment", restored.commentsByTick[5][0])

	require.Len(t, restored.commentsByTick[10], 3)
	require.Equal(t, "tick 10 a", restored.commentsByTick[10][0])
	require.Equal(t, "tick 10 b", restored.commentsByTick[10][1])
	require.Equal(t, "tick 10 c", restored.commentsByTick[10][2])
}
