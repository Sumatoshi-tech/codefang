package filehistory

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

func TestSaveCheckpoint_CreatesFile(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	h := NewAnalyzer()
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

	original := NewAnalyzer()
	require.NoError(t, original.Initialize(nil))
	original.files["test.go"] = &FileHistory{
		People: map[int]pkgplumbing.LineStats{0: {Added: 10, Removed: 5}},
		Hashes: []gitlib.Hash{gitlib.NewHash("abc123")},
	}
	original.merges[gitlib.NewHash("def456")] = true

	require.NoError(t, original.SaveCheckpoint(dir))

	restored := NewAnalyzer()
	require.NoError(t, restored.LoadCheckpoint(dir))

	require.Len(t, restored.files, 1)
	require.NotNil(t, restored.files["test.go"])
	require.Equal(t, 10, restored.files["test.go"].People[0].Added)
	require.Len(t, restored.merges, 1)
}

func TestCheckpointSize_ReturnsPositiveValue(t *testing.T) {
	t.Parallel()

	h := NewAnalyzer()
	require.NoError(t, h.Initialize(nil))
	h.files["test.go"] = &FileHistory{
		People: map[int]pkgplumbing.LineStats{0: {Added: 10}},
	}

	size := h.CheckpointSize()
	require.Positive(t, size)
}

func TestCheckpointRoundTrip_PreservesAllState(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	original := NewAnalyzer()
	require.NoError(t, original.Initialize(nil))

	// Add multiple files with various state.
	original.files["main.go"] = &FileHistory{
		People: map[int]pkgplumbing.LineStats{
			0: {Added: 100, Removed: 50, Changed: 25},
			1: {Added: 30, Removed: 10, Changed: 5},
		},
		Hashes: []gitlib.Hash{
			gitlib.NewHash("abc123"),
			gitlib.NewHash("def456"),
		},
	}
	original.files["util.go"] = &FileHistory{
		People: map[int]pkgplumbing.LineStats{2: {Added: 200}},
		Hashes: []gitlib.Hash{gitlib.NewHash("ghi789")},
	}
	original.merges[gitlib.NewHash("merge1")] = true
	original.merges[gitlib.NewHash("merge2")] = true

	require.NoError(t, original.SaveCheckpoint(dir))

	restored := NewAnalyzer()
	require.NoError(t, restored.LoadCheckpoint(dir))

	// Verify files.
	require.Len(t, restored.files, 2)

	mainFile := restored.files["main.go"]
	require.NotNil(t, mainFile)
	require.Equal(t, 100, mainFile.People[0].Added)
	require.Equal(t, 50, mainFile.People[0].Removed)
	require.Equal(t, 30, mainFile.People[1].Added)
	require.Len(t, mainFile.Hashes, 2)

	utilFile := restored.files["util.go"]
	require.NotNil(t, utilFile)
	require.Equal(t, 200, utilFile.People[2].Added)

	// Verify merges.
	require.Len(t, restored.merges, 2)
}
