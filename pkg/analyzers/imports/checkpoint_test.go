package imports

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestSaveCheckpoint_CreatesFile(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	h := &HistoryAnalyzer{}
	require.NoError(t, h.Initialize(nil))

	err := h.SaveCheckpoint(dir)
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

	// Add import data
	original.imports[0] = map[string]map[string]map[int]int64{
		"go": {
			"fmt": {0: 5, 1: 3},
			"os":  {0: 2},
		},
	}
	original.imports[1] = map[string]map[string]map[int]int64{
		"python": {
			"sys": {0: 10},
		},
	}

	require.NoError(t, original.SaveCheckpoint(dir))

	restored := &HistoryAnalyzer{}
	require.NoError(t, restored.LoadCheckpoint(dir))

	require.Len(t, restored.imports, 2)
	require.Equal(t, int64(5), restored.imports[0]["go"]["fmt"][0])
	require.Equal(t, int64(3), restored.imports[0]["go"]["fmt"][1])
	require.Equal(t, int64(2), restored.imports[0]["go"]["os"][0])
	require.Equal(t, int64(10), restored.imports[1]["python"]["sys"][0])
}

func TestCheckpointSize_ReturnsPositiveValue(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	require.NoError(t, h.Initialize(nil))

	h.imports[0] = map[string]map[string]map[int]int64{
		"go": {"fmt": {0: 5}},
	}

	size := h.CheckpointSize()
	require.Positive(t, size)
}

func TestCheckpointRoundTrip_PreservesAllState(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	original := &HistoryAnalyzer{}
	require.NoError(t, original.Initialize(nil))

	// Add complex import data
	original.imports[0] = map[string]map[string]map[int]int64{
		"go": {
			"fmt":     {0: 10, 1: 5, 2: 3},
			"os":      {0: 7},
			"strings": {1: 2},
		},
		"rust": {
			"std::io": {0: 4},
		},
	}
	original.imports[1] = map[string]map[string]map[int]int64{
		"python": {
			"os":  {0: 15},
			"sys": {0: 8, 1: 4},
		},
	}
	original.imports[2] = map[string]map[string]map[int]int64{
		"javascript": {
			"lodash": {0: 20},
		},
	}

	require.NoError(t, original.SaveCheckpoint(dir))

	restored := &HistoryAnalyzer{}
	require.NoError(t, restored.LoadCheckpoint(dir))

	// Verify all authors
	require.Len(t, restored.imports, 3)

	// Verify author 0
	require.Equal(t, int64(10), restored.imports[0]["go"]["fmt"][0])
	require.Equal(t, int64(5), restored.imports[0]["go"]["fmt"][1])
	require.Equal(t, int64(3), restored.imports[0]["go"]["fmt"][2])
	require.Equal(t, int64(7), restored.imports[0]["go"]["os"][0])
	require.Equal(t, int64(2), restored.imports[0]["go"]["strings"][1])
	require.Equal(t, int64(4), restored.imports[0]["rust"]["std::io"][0])

	// Verify author 1
	require.Equal(t, int64(15), restored.imports[1]["python"]["os"][0])
	require.Equal(t, int64(8), restored.imports[1]["python"]["sys"][0])
	require.Equal(t, int64(4), restored.imports[1]["python"]["sys"][1])

	// Verify author 2
	require.Equal(t, int64(20), restored.imports[2]["javascript"]["lodash"][0])
}
