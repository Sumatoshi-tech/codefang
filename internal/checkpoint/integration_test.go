package checkpoint_test

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/checkpoint"
)

const testRepoPath = "/test/repo"

// mockAnalyzer simulates an analyzer that can be checkpointed.
type mockAnalyzer struct {
	name       string
	counter    int
	processLog []int // Records which commits were processed.
}

func (m *mockAnalyzer) SaveCheckpoint(dir string) error {
	// Write counter and processLog to file.
	data := make([]byte, 0, len(m.processLog))
	for _, v := range m.processLog {
		data = append(data, byte(v))
	}

	err := os.WriteFile(filepath.Join(dir, m.name+".bin"), data, 0o600)
	if err != nil {
		return fmt.Errorf("writing analyzer checkpoint %s: %w", m.name, err)
	}

	return nil
}

func (m *mockAnalyzer) LoadCheckpoint(dir string) error {
	data, err := os.ReadFile(filepath.Join(dir, m.name+".bin"))
	if err != nil {
		return fmt.Errorf("reading analyzer checkpoint %s: %w", m.name, err)
	}

	m.processLog = make([]int, len(data))
	for i, v := range data {
		m.processLog[i] = int(v)
	}

	m.counter = len(m.processLog)

	return nil
}

func (m *mockAnalyzer) CheckpointSize() int64 {
	return int64(len(m.processLog))
}

func (m *mockAnalyzer) Process(commitIndex int) {
	m.processLog = append(m.processLog, commitIndex)
	m.counter++
}

// TestCheckpoint_CrashAndResume simulates a crash mid-processing and verifies
// that the analysis can resume from the checkpoint.
func TestCheckpoint_CrashAndResume(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	repoPath := testRepoPath
	repoHash := checkpoint.RepoHash(repoPath)

	// Phase 1: Process chunks 0 and 1, save checkpoint after chunk 1.
	analyzer1 := &mockAnalyzer{name: "test"}

	// Simulate processing chunk 0 (commits 0-9).
	for i := range 10 {
		analyzer1.Process(i)
	}

	// Simulate processing chunk 1 (commits 10-19).
	for i := 10; i < 20; i++ {
		analyzer1.Process(i)
	}

	// Save checkpoint after chunk 1.
	mgr := checkpoint.NewManager(dir, repoHash)
	state := checkpoint.StreamingState{
		TotalCommits:     30,
		ProcessedCommits: 20,
		CurrentChunk:     1,
		TotalChunks:      3,
		LastCommitHash:   "abc123",
	}

	checkpointables := []checkpoint.Checkpointable{analyzer1}
	err := mgr.Save(checkpointables, state, repoPath, []string{"test"})
	require.NoError(t, err)

	// Verify checkpoint exists.
	require.True(t, mgr.Exists())

	// Phase 2: Simulate crash and restart with new analyzer instance.
	analyzer2 := &mockAnalyzer{name: "test"}

	// Validate checkpoint.
	err = mgr.Validate(repoPath, []string{"test"})
	require.NoError(t, err)

	// Load checkpoint.
	restoredCheckpointables := []checkpoint.Checkpointable{analyzer2}
	loadedState, err := mgr.Load(restoredCheckpointables)
	require.NoError(t, err)

	// Verify restored state.
	assert.Len(t, analyzer2.processLog, 20)
	assert.Equal(t, 20, analyzer2.counter)
	assert.Equal(t, 1, loadedState.CurrentChunk)
	assert.Equal(t, 20, loadedState.ProcessedCommits)

	// Resume from chunk 2 (commits 20-29).
	for i := 20; i < 30; i++ {
		analyzer2.Process(i)
	}

	// Verify final state matches what we'd have without crash.
	assert.Len(t, analyzer2.processLog, 30)

	for i := range 30 {
		assert.Equal(t, i, analyzer2.processLog[i], "commit %d mismatch", i)
	}
}

// TestCheckpoint_ResumeWithMismatchedRepo verifies that resume fails
// when the repository path doesn't match.
func TestCheckpoint_ResumeWithMismatchedRepo(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	repoPath := testRepoPath
	repoHash := checkpoint.RepoHash(repoPath)

	mgr := checkpoint.NewManager(dir, repoHash)
	state := checkpoint.StreamingState{TotalCommits: 100}

	err := mgr.Save(nil, state, repoPath, []string{"burndown"})
	require.NoError(t, err)

	// Try to validate with different repo path.
	err = mgr.Validate("/different/repo", []string{"burndown"})
	require.Error(t, err)
	require.ErrorIs(t, err, checkpoint.ErrRepoPathMismatch)
}

// TestCheckpoint_ResumeWithMismatchedAnalyzers verifies that resume fails
// when the analyzer set doesn't match.
func TestCheckpoint_ResumeWithMismatchedAnalyzers(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	repoPath := testRepoPath
	repoHash := checkpoint.RepoHash(repoPath)

	mgr := checkpoint.NewManager(dir, repoHash)
	state := checkpoint.StreamingState{TotalCommits: 100}

	err := mgr.Save(nil, state, repoPath, []string{"burndown"})
	require.NoError(t, err)

	// Try to validate with different analyzers.
	err = mgr.Validate(repoPath, []string{"devs"})
	require.Error(t, err)
	require.ErrorIs(t, err, checkpoint.ErrAnalyzerMismatch)
}

// TestCheckpoint_ClearAfterCompletion verifies that checkpoint is cleared
// after successful completion.
func TestCheckpoint_ClearAfterCompletion(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	repoHash := checkpoint.RepoHash(testRepoPath)

	mgr := checkpoint.NewManager(dir, repoHash)
	state := checkpoint.StreamingState{TotalCommits: 100}

	err := mgr.Save(nil, state, testRepoPath, []string{"burndown"})
	require.NoError(t, err)
	require.True(t, mgr.Exists())

	// Clear checkpoint (simulating successful completion).
	err = mgr.Clear()
	require.NoError(t, err)
	require.False(t, mgr.Exists())
}

// TestCheckpoint_MultipleAnalyzers verifies checkpoint/resume with multiple analyzers.
func TestCheckpoint_MultipleAnalyzers(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	repoPath := testRepoPath
	repoHash := checkpoint.RepoHash(repoPath)

	// Create two analyzers.
	analyzer1 := &mockAnalyzer{name: "burndown"}
	analyzer2 := &mockAnalyzer{name: "devs"}

	// Process some data.
	for i := range 5 {
		analyzer1.Process(i)
		analyzer2.Process(i * 10)
	}

	// Save checkpoint.
	mgr := checkpoint.NewManager(dir, repoHash)
	state := checkpoint.StreamingState{
		TotalCommits:     10,
		ProcessedCommits: 5,
		CurrentChunk:     0,
		TotalChunks:      2,
	}

	checkpointables := []checkpoint.Checkpointable{analyzer1, analyzer2}
	err := mgr.Save(checkpointables, state, repoPath, []string{"burndown", "devs"})
	require.NoError(t, err)

	// Restore into new analyzer instances.
	restored1 := &mockAnalyzer{name: "burndown"}
	restored2 := &mockAnalyzer{name: "devs"}

	restoredCheckpointables := []checkpoint.Checkpointable{restored1, restored2}
	_, err = mgr.Load(restoredCheckpointables)
	require.NoError(t, err)

	// Verify both analyzers restored correctly.
	assert.Equal(t, analyzer1.processLog, restored1.processLog)
	assert.Equal(t, analyzer2.processLog, restored2.processLog)
}
