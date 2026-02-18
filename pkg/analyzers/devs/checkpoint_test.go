package devs

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

func TestSaveCheckpoint_EmptyState(t *testing.T) {
	t.Parallel()

	// Create a temporary directory for the checkpoint.
	dir := t.TempDir()

	// Create an analyzer with empty state (as after Initialize).
	analyzer := &HistoryAnalyzer{}

	// Save checkpoint should succeed even with empty state.
	err := analyzer.SaveCheckpoint(dir)
	if err != nil {
		t.Fatalf("SaveCheckpoint failed: %v", err)
	}

	// Verify the checkpoint file was created.
	checkpointPath := filepath.Join(dir, checkpointBasename+".json")

	_, statErr := os.Stat(checkpointPath)
	if os.IsNotExist(statErr) {
		t.Fatalf("checkpoint file not created at %s", checkpointPath)
	}
}

func TestLoadCheckpoint_EmptyState(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	// Save an empty checkpoint first.
	original := &HistoryAnalyzer{}

	err := original.SaveCheckpoint(dir)
	if err != nil {
		t.Fatalf("SaveCheckpoint failed: %v", err)
	}

	// Load into a new analyzer.
	loaded := &HistoryAnalyzer{}

	err = loaded.LoadCheckpoint(dir)
	if err != nil {
		t.Fatalf("LoadCheckpoint failed: %v", err)
	}
}

func TestCheckpointSize_EmptyState(t *testing.T) {
	t.Parallel()

	analyzer := &HistoryAnalyzer{}

	size := analyzer.CheckpointSize()

	// Empty state should return base overhead only.
	if size <= 0 {
		t.Errorf("CheckpointSize() = %d, want > 0", size)
	}
}

func TestSaveCheckpoint_InvalidDirectory(t *testing.T) {
	t.Parallel()

	// Use a path that doesn't exist and can't be created.
	invalidDir := "/nonexistent/path/that/does/not/exist"

	analyzer := &HistoryAnalyzer{}

	err := analyzer.SaveCheckpoint(invalidDir)
	if err == nil {
		t.Fatal("SaveCheckpoint should fail with invalid directory")
	}
}

func TestCheckpointRoundTrip_WithCommitData(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	// Create an analyzer with populated commit data.
	original := &HistoryAnalyzer{
		commitDevData: map[string]*CommitDevData{
			"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa": {
				Commits:  5,
				Added:    100,
				Removed:  20,
				Changed:  10,
				AuthorID: 1,
				Languages: map[string]plumbing.LineStats{
					"go": {Added: 80, Removed: 15, Changed: 8},
				},
			},
		},
	}

	// Save checkpoint.
	err := original.SaveCheckpoint(dir)
	if err != nil {
		t.Fatalf("SaveCheckpoint failed: %v", err)
	}

	// Load into a new analyzer.
	loaded := &HistoryAnalyzer{}

	err = loaded.LoadCheckpoint(dir)
	if err != nil {
		t.Fatalf("LoadCheckpoint failed: %v", err)
	}

	// Verify commit data was restored.
	if len(loaded.commitDevData) != 1 {
		t.Fatalf("loaded.commitDevData has %d entries, want 1", len(loaded.commitDevData))
	}

	cdd := loaded.commitDevData["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]
	if cdd == nil {
		t.Fatal("loaded.commitDevData entry is nil")
	}

	if cdd.Commits != 5 {
		t.Errorf("cdd.Commits = %d, want 5", cdd.Commits)
	}

	if cdd.Added != 100 {
		t.Errorf("cdd.Added = %d, want 100", cdd.Added)
	}

	if cdd.AuthorID != 1 {
		t.Errorf("cdd.AuthorID = %d, want 1", cdd.AuthorID)
	}
}

func TestCheckpointRoundTrip_WithMerges(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	// Create an analyzer with merges data.
	hash1 := gitlib.NewHash("0123456789abcdef0123456789abcdef01234567")
	hash2 := gitlib.NewHash("fedcba9876543210fedcba9876543210fedcba98")

	original := &HistoryAnalyzer{
		commitDevData: map[string]*CommitDevData{},
		merges: map[gitlib.Hash]bool{
			hash1: true,
			hash2: true,
		},
	}

	// Save checkpoint.
	err := original.SaveCheckpoint(dir)
	if err != nil {
		t.Fatalf("SaveCheckpoint failed: %v", err)
	}

	// Load into a new analyzer.
	loaded := &HistoryAnalyzer{}

	err = loaded.LoadCheckpoint(dir)
	if err != nil {
		t.Fatalf("LoadCheckpoint failed: %v", err)
	}

	// Verify merges were restored.
	if len(loaded.merges) != 2 {
		t.Fatalf("loaded.merges has %d entries, want 2", len(loaded.merges))
	}

	if !loaded.merges[hash1] {
		t.Error("loaded.merges missing hash1")
	}

	if !loaded.merges[hash2] {
		t.Error("loaded.merges missing hash2")
	}
}

func TestLoadCheckpoint_FileNotFound(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	analyzer := &HistoryAnalyzer{}

	err := analyzer.LoadCheckpoint(dir)
	if err == nil {
		t.Fatal("LoadCheckpoint should fail when file doesn't exist")
	}
}

func TestLoadCheckpoint_CorruptedFile(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	// Write corrupted JSON to checkpoint file.
	checkpointPath := filepath.Join(dir, checkpointBasename+".json")

	err := os.WriteFile(checkpointPath, []byte("not valid json{{{"), 0o600)
	if err != nil {
		t.Fatalf("failed to write corrupted file: %v", err)
	}

	analyzer := &HistoryAnalyzer{}

	err = analyzer.LoadCheckpoint(dir)
	if err == nil {
		t.Fatal("LoadCheckpoint should fail on corrupted file")
	}
}

func TestCheckpointSize_Accuracy(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	// Create analyzer with realistic data.
	analyzer := &HistoryAnalyzer{
		commitDevData: map[string]*CommitDevData{
			"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa": {
				Commits: 5, Added: 100, Removed: 20, Changed: 10, AuthorID: 1,
				Languages: map[string]plumbing.LineStats{
					"go":     {Added: 80, Removed: 15, Changed: 8},
					"python": {Added: 20, Removed: 5, Changed: 2},
				},
			},
			"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb": {
				Commits: 3, Added: 50, Removed: 10, Changed: 5, AuthorID: 2,
				Languages: map[string]plumbing.LineStats{
					"go": {Added: 50, Removed: 10, Changed: 5},
				},
			},
			"cccccccccccccccccccccccccccccccccccccccc": {
				Commits: 10, Added: 200, Removed: 40, Changed: 20, AuthorID: 1,
				Languages: map[string]plumbing.LineStats{
					"go": {Added: 200, Removed: 40, Changed: 20},
				},
			},
		},
		merges: map[gitlib.Hash]bool{
			gitlib.NewHash("0123456789abcdef0123456789abcdef01234567"): true,
			gitlib.NewHash("fedcba9876543210fedcba9876543210fedcba98"): true,
		},
		reversedPeopleDict: []string{"Alice", "Bob"},
	}

	// Get the estimate before saving.
	estimate := analyzer.CheckpointSize()

	// Save checkpoint.
	err := analyzer.SaveCheckpoint(dir)
	if err != nil {
		t.Fatalf("SaveCheckpoint failed: %v", err)
	}

	// Get actual file size.
	checkpointPath := filepath.Join(dir, checkpointBasename+".json")

	info, err := os.Stat(checkpointPath)
	if err != nil {
		t.Fatalf("failed to stat checkpoint file: %v", err)
	}

	actualSize := info.Size()

	// Verify estimate is within 2x of actual.
	ratio := float64(estimate) / float64(actualSize)
	if ratio < 0.5 || ratio > 2.0 {
		t.Errorf("CheckpointSize estimate %d is not within 2x of actual %d (ratio: %.2f)",
			estimate, actualSize, ratio)
	}
}

// createRealisticAnalyzer creates an analyzer with data similar to a real 10k commit analysis.
func createRealisticAnalyzer(numTicks, numDevs, numMerges int) *HistoryAnalyzer {
	commits := make(map[string]*CommitDevData, numTicks*numDevs)

	for tick := range numTicks {
		for dev := range numDevs {
			hash := fmt.Sprintf("%020d%020d", tick, dev)
			commits[hash] = &CommitDevData{
				Commits:  5 + tick%10,
				Added:    100 + tick*10 + dev,
				Removed:  20 + tick + dev,
				Changed:  10 + tick + dev,
				AuthorID: dev,
				Languages: map[string]plumbing.LineStats{
					"go":     {Added: 80, Removed: 15, Changed: 8},
					"python": {Added: 20, Removed: 5, Changed: 2},
				},
			}
		}
	}

	merges := make(map[gitlib.Hash]bool, numMerges)
	for range numMerges {
		hash := gitlib.NewHash("0123456789abcdef0123456789abcdef01234567")
		merges[hash] = true
	}

	return &HistoryAnalyzer{
		commitDevData: commits,
		merges:        merges,
	}
}

// BenchmarkCheckpointSave benchmarks checkpoint save operation with realistic data.
func BenchmarkCheckpointSave(b *testing.B) {
	dir := b.TempDir()
	// Simulate ~300 ticks (1 year of daily data), 50 developers, 100 merges.
	analyzer := createRealisticAnalyzer(300, 50, 100)

	for b.Loop() {
		err := analyzer.SaveCheckpoint(dir)
		if err != nil {
			b.Fatalf("SaveCheckpoint failed: %v", err)
		}
	}
}

// BenchmarkCheckpointLoad benchmarks checkpoint load operation with realistic data.
func BenchmarkCheckpointLoad(b *testing.B) {
	dir := b.TempDir()
	// Simulate ~300 ticks (1 year of daily data), 50 developers, 100 merges.
	analyzer := createRealisticAnalyzer(300, 50, 100)

	// Save once to create the file.
	err := analyzer.SaveCheckpoint(dir)
	if err != nil {
		b.Fatalf("SaveCheckpoint failed: %v", err)
	}

	for b.Loop() {
		loaded := &HistoryAnalyzer{}

		loadErr := loaded.LoadCheckpoint(dir)
		if loadErr != nil {
			b.Fatalf("LoadCheckpoint failed: %v", loadErr)
		}
	}
}

// BenchmarkCheckpointRoundTrip benchmarks full save+load cycle.
func BenchmarkCheckpointRoundTrip(b *testing.B) {
	dir := b.TempDir()
	// Simulate ~300 ticks (1 year of daily data), 50 developers, 100 merges.
	analyzer := createRealisticAnalyzer(300, 50, 100)

	for b.Loop() {
		err := analyzer.SaveCheckpoint(dir)
		if err != nil {
			b.Fatalf("SaveCheckpoint failed: %v", err)
		}

		loaded := &HistoryAnalyzer{}

		err = loaded.LoadCheckpoint(dir)
		if err != nil {
			b.Fatalf("LoadCheckpoint failed: %v", err)
		}
	}
}
