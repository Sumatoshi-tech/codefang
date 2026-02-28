package filehistory

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"

	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
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
	original.merges.SeenOrAdd(gitlib.NewHash("def456"))

	require.NoError(t, original.SaveCheckpoint(dir))

	restored := NewAnalyzer()
	require.NoError(t, restored.LoadCheckpoint(dir))

	require.Len(t, restored.files, 1)
	require.NotNil(t, restored.files["test.go"])
	require.Equal(t, 10, restored.files["test.go"].People[0].Added)
	// Verify the restored merge tracker recognizes the previously-added hash.
	require.True(t, restored.merges.SeenOrAdd(gitlib.NewHash("def456")), "restored tracker should contain the saved merge")
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
	original.merges.SeenOrAdd(gitlib.NewHash("merge1"))
	original.merges.SeenOrAdd(gitlib.NewHash("merge2"))

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

	// Verify merges: restored tracker should recognize both previously-added hashes.
	require.True(t, restored.merges.SeenOrAdd(gitlib.NewHash("merge1")), "restored tracker should contain merge1")
	require.True(t, restored.merges.SeenOrAdd(gitlib.NewHash("merge2")), "restored tracker should contain merge2")
}
