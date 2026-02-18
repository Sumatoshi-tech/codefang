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
	original.commentsByCommit["aaa"] = []string{"comment 1", "comment 2"}
	original.commentsByCommit["bbb"] = []string{"comment 3"}

	require.NoError(t, original.SaveCheckpoint(dir))

	restored := &HistoryAnalyzer{}
	require.NoError(t, restored.LoadCheckpoint(dir))

	require.Len(t, restored.commentsByCommit, 2)
	require.Len(t, restored.commentsByCommit["aaa"], 2)
	require.Len(t, restored.commentsByCommit["bbb"], 1)
	require.Equal(t, "comment 1", restored.commentsByCommit["aaa"][0])
	require.Equal(t, "comment 3", restored.commentsByCommit["bbb"][0])
}

func TestCheckpointSize_ReturnsPositiveValue(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	s.commentsByCommit["aaa"] = []string{"This is a comment for testing"}

	size := s.CheckpointSize()
	require.Positive(t, size)
}

func TestCheckpointRoundTrip_PreservesAllState(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	original := &HistoryAnalyzer{}
	require.NoError(t, original.Initialize(nil))

	// Add multiple commits with comments.
	original.commentsByCommit["aaa"] = []string{"commit aaa comment 1", "commit aaa comment 2"}
	original.commentsByCommit["bbb"] = []string{"commit bbb comment"}
	original.commentsByCommit["ccc"] = []string{"commit ccc a", "commit ccc b", "commit ccc c"}

	require.NoError(t, original.SaveCheckpoint(dir))

	restored := &HistoryAnalyzer{}
	require.NoError(t, restored.LoadCheckpoint(dir))

	// Verify all commits.
	require.Len(t, restored.commentsByCommit, 3)

	require.Len(t, restored.commentsByCommit["aaa"], 2)
	require.Equal(t, "commit aaa comment 1", restored.commentsByCommit["aaa"][0])
	require.Equal(t, "commit aaa comment 2", restored.commentsByCommit["aaa"][1])

	require.Len(t, restored.commentsByCommit["bbb"], 1)
	require.Equal(t, "commit bbb comment", restored.commentsByCommit["bbb"][0])

	require.Len(t, restored.commentsByCommit["ccc"], 3)
	require.Equal(t, "commit ccc a", restored.commentsByCommit["ccc"][0])
	require.Equal(t, "commit ccc b", restored.commentsByCommit["ccc"][1])
	require.Equal(t, "commit ccc c", restored.commentsByCommit["ccc"][2])
}
