package typos

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestSaveCheckpoint_CreatesFile(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{
		typos: []Typo{
			{Wrong: "tset", Correct: "test", File: "main.go", Line: 10},
		},
	}

	dir := t.TempDir()

	err := h.SaveCheckpoint(dir)
	require.NoError(t, err)

	// Verify file was created.
	expectedFile := filepath.Join(dir, "typos_state.json")
	_, err = os.Stat(expectedFile)
	require.NoError(t, err, "checkpoint file should exist")
}

func TestLoadCheckpoint_RestoresState(t *testing.T) {
	t.Parallel()

	original := &HistoryAnalyzer{
		typos: []Typo{
			{Wrong: "tset", Correct: "test", File: "main.go", Line: 10},
			{Wrong: "functon", Correct: "function", File: "util.go", Line: 20},
		},
	}

	dir := t.TempDir()

	err := original.SaveCheckpoint(dir)
	require.NoError(t, err)

	// Load into fresh analyzer.
	restored := &HistoryAnalyzer{}

	err = restored.LoadCheckpoint(dir)
	require.NoError(t, err)

	// Verify state was restored.
	require.Len(t, restored.typos, 2)
	require.Equal(t, "tset", restored.typos[0].Wrong)
	require.Equal(t, "test", restored.typos[0].Correct)
	require.Equal(t, "main.go", restored.typos[0].File)
	require.Equal(t, 10, restored.typos[0].Line)
	require.Equal(t, "functon", restored.typos[1].Wrong)
}

func TestCheckpointSize_ReturnsPositiveValue(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{
		typos: []Typo{
			{Wrong: "tset", Correct: "test", File: "main.go", Line: 10},
		},
	}

	size := h.CheckpointSize()
	require.Positive(t, size, "checkpoint size should be positive")
}

func TestCheckpointSize_ScalesWithTypos(t *testing.T) {
	t.Parallel()

	small := &HistoryAnalyzer{
		typos: []Typo{
			{Wrong: "a", Correct: "b"},
		},
	}

	large := &HistoryAnalyzer{
		typos: []Typo{
			{Wrong: "a", Correct: "b"},
			{Wrong: "c", Correct: "d"},
			{Wrong: "e", Correct: "f"},
		},
	}

	require.Greater(t, large.CheckpointSize(), small.CheckpointSize(),
		"more typos should increase checkpoint size")
}

func TestCheckpointRoundTrip_PreservesAllState(t *testing.T) {
	t.Parallel()

	// Create hash for testing.
	var hash gitlib.Hash
	copy(hash[:], "12345678901234567890")

	original := &HistoryAnalyzer{
		typos: []Typo{
			{Wrong: "tset", Correct: "test", File: "main.go", Line: 10, Commit: hash},
			{Wrong: "functon", Correct: "function", File: "util.go", Line: 20, Commit: hash},
		},
	}

	dir := t.TempDir()

	err := original.SaveCheckpoint(dir)
	require.NoError(t, err)

	restored := &HistoryAnalyzer{}

	err = restored.LoadCheckpoint(dir)
	require.NoError(t, err)

	// Verify all typos with all fields.
	require.Len(t, restored.typos, len(original.typos))

	for i, orig := range original.typos {
		rest := restored.typos[i]
		require.Equal(t, orig.Wrong, rest.Wrong)
		require.Equal(t, orig.Correct, rest.Correct)
		require.Equal(t, orig.File, rest.File)
		require.Equal(t, orig.Line, rest.Line)
		require.Equal(t, orig.Commit, rest.Commit)
	}
}

func TestCheckpointRoundTrip_EmptyTypos(t *testing.T) {
	t.Parallel()

	original := &HistoryAnalyzer{
		typos: nil,
	}

	dir := t.TempDir()

	err := original.SaveCheckpoint(dir)
	require.NoError(t, err)

	restored := &HistoryAnalyzer{}

	err = restored.LoadCheckpoint(dir)
	require.NoError(t, err)

	require.Empty(t, restored.typos)
}
