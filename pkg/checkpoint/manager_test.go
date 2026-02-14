package checkpoint

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestManager_New(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	m := NewManager(dir, "abc123")

	assert.Equal(t, dir, m.BaseDir)
	assert.Equal(t, "abc123", m.RepoHash)
	assert.Equal(t, DefaultMaxAge, m.MaxAge)
	assert.Equal(t, int64(DefaultMaxSize), m.MaxSize)
}

func TestManager_CheckpointDir(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	m := NewManager(dir, "abc123")
	expected := filepath.Join(dir, "abc123")
	assert.Equal(t, expected, m.CheckpointDir())
}

func TestManager_MetadataPath(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	m := NewManager(dir, "abc123")
	expected := filepath.Join(dir, "abc123", "checkpoint.json")
	assert.Equal(t, expected, m.MetadataPath())
}

func TestManager_Exists_NoCheckpoint(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	m := NewManager(dir, "abc123")

	assert.False(t, m.Exists())
}

func TestManager_Exists_WithCheckpoint(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	m := NewManager(dir, "abc123")

	// Create checkpoint directory and metadata file.
	cpDir := m.CheckpointDir()
	err := os.MkdirAll(cpDir, 0o750)
	require.NoError(t, err)

	err = os.WriteFile(m.MetadataPath(), []byte(`{"version":1}`), 0o600)
	require.NoError(t, err)

	assert.True(t, m.Exists())
}

func TestManager_Clear(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	m := NewManager(dir, "abc123")

	// Create checkpoint directory with files.
	cpDir := m.CheckpointDir()
	err := os.MkdirAll(cpDir, 0o750)
	require.NoError(t, err)

	err = os.WriteFile(m.MetadataPath(), []byte(`{"version":1}`), 0o600)
	require.NoError(t, err)

	require.True(t, m.Exists())

	// Clear checkpoint.
	err = m.Clear()
	require.NoError(t, err)

	assert.False(t, m.Exists())
}

func TestManager_Clear_NonExistent(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	m := NewManager(dir, "abc123")

	// Clear should not error if checkpoint doesn't exist.
	err := m.Clear()
	assert.NoError(t, err)
}

func TestManager_SaveLoad_Metadata(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	m := NewManager(dir, "abc123")

	state := StreamingState{
		TotalCommits:     100000,
		ProcessedCommits: 50000,
		CurrentChunk:     1,
		TotalChunks:      2,
		LastCommitHash:   "def456",
		LastTick:         42,
	}

	// Save with no checkpointables.
	err := m.Save(nil, state, "/path/to/repo", []string{"burndown"})
	require.NoError(t, err)

	assert.True(t, m.Exists())

	// Load metadata.
	meta, err := m.LoadMetadata()
	require.NoError(t, err)

	assert.Equal(t, MetadataVersion, meta.Version)
	assert.Equal(t, "/path/to/repo", meta.RepoPath)
	assert.Equal(t, "abc123", meta.RepoHash)
	assert.Equal(t, []string{"burndown"}, meta.Analyzers)
	assert.Equal(t, state.TotalCommits, meta.StreamingState.TotalCommits)
	assert.Equal(t, state.ProcessedCommits, meta.StreamingState.ProcessedCommits)
}

func TestManager_SaveLoad_Checkpointables(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	m := NewManager(dir, "abc123")

	state := StreamingState{
		TotalCommits:     100,
		ProcessedCommits: 50,
	}

	original := &mockCheckpointable{data: "analyzer state"}
	checkpointables := []Checkpointable{original}

	err := m.Save(checkpointables, state, "/path/to/repo", []string{"mock"})
	require.NoError(t, err)

	// Load checkpointables.
	restored := &mockCheckpointable{}
	restoredList := []Checkpointable{restored}

	loadedState, err := m.Load(restoredList)
	require.NoError(t, err)

	assert.Equal(t, original.data, restored.data)
	assert.Equal(t, state.TotalCommits, loadedState.TotalCommits)
	assert.Equal(t, state.ProcessedCommits, loadedState.ProcessedCommits)
}

func TestManager_DefaultValues(t *testing.T) {
	t.Parallel()

	assert.Equal(t, 7*24*time.Hour, DefaultMaxAge)
	assert.Equal(t, 1<<30, DefaultMaxSize) // 1GB.
}

func TestManager_Validate_Success(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	m := NewManager(dir, "abc123")

	state := StreamingState{
		TotalCommits:     100,
		ProcessedCommits: 50,
		LastCommitHash:   "def456",
	}

	err := m.Save(nil, state, "/path/to/repo", []string{"burndown"})
	require.NoError(t, err)

	// Validate with matching parameters.
	err = m.Validate("/path/to/repo", []string{"burndown"})
	assert.NoError(t, err)
}

func TestManager_Validate_WrongRepo(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	m := NewManager(dir, "abc123")

	state := StreamingState{}
	err := m.Save(nil, state, "/path/to/repo", []string{"burndown"})
	require.NoError(t, err)

	// Validate with different repo path.
	err = m.Validate("/different/repo", []string{"burndown"})
	require.Error(t, err)
	require.ErrorIs(t, err, ErrRepoPathMismatch)
}

func TestManager_Validate_WrongAnalyzers(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	m := NewManager(dir, "abc123")

	state := StreamingState{}
	err := m.Save(nil, state, "/path/to/repo", []string{"burndown"})
	require.NoError(t, err)

	// Validate with different analyzers.
	err = m.Validate("/path/to/repo", []string{"devs"})
	require.Error(t, err)
	require.ErrorIs(t, err, ErrAnalyzerMismatch)
}

func TestManager_Validate_NoCheckpoint(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	m := NewManager(dir, "abc123")

	err := m.Validate("/path/to/repo", []string{"burndown"})
	assert.Error(t, err)
}

func TestDefaultDir(t *testing.T) {
	t.Parallel()

	dir := DefaultDir()
	assert.Contains(t, dir, ".codefang")
	assert.Contains(t, dir, "checkpoints")
}

func TestRepoHash(t *testing.T) {
	t.Parallel()

	hash := RepoHash("/path/to/repo")
	assert.Len(t, hash, 16) // 8 bytes hex = 16 chars.

	// Same path should produce same hash.
	hash2 := RepoHash("/path/to/repo")
	assert.Equal(t, hash, hash2)

	// Different path should produce different hash.
	hash3 := RepoHash("/different/repo")
	assert.NotEqual(t, hash, hash3)
}

func TestManager_Save_ErrorOnMkdir(t *testing.T) {
	t.Parallel()

	// Use a path that can't be created (file instead of dir).
	tmpFile, err := os.CreateTemp(t.TempDir(), "checkpoint-test")
	require.NoError(t, err)
	tmpFile.Close()

	// Try to create checkpoint dir inside a file (should fail).
	m := NewManager(tmpFile.Name(), "abc123")
	err = m.Save(nil, StreamingState{}, "/repo", []string{})
	assert.Error(t, err)
}
