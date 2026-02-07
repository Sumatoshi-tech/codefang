package devs

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

func TestSaveCheckpoint_EmptyState(t *testing.T) {
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
	analyzer := &HistoryAnalyzer{}

	size := analyzer.CheckpointSize()

	// Empty state should return base overhead only.
	if size <= 0 {
		t.Errorf("CheckpointSize() = %d, want > 0", size)
	}
}

func TestSaveCheckpoint_InvalidDirectory(t *testing.T) {
	// Use a path that doesn't exist and can't be created.
	invalidDir := "/nonexistent/path/that/does/not/exist"

	analyzer := &HistoryAnalyzer{}
	err := analyzer.SaveCheckpoint(invalidDir)

	if err == nil {
		t.Fatal("SaveCheckpoint should fail with invalid directory")
	}
}

func TestCheckpointRoundTrip_WithTicks(t *testing.T) {
	dir := t.TempDir()

	// Create an analyzer with populated ticks data.
	original := &HistoryAnalyzer{
		ticks: map[int]map[int]*DevTick{
			0: {
				1: {
					LineStats: plumbing.LineStats{Added: 100, Removed: 20, Changed: 10},
					Commits:   5,
					Languages: map[string]plumbing.LineStats{
						"go": {Added: 80, Removed: 15, Changed: 8},
					},
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

	// Verify ticks were restored.
	if len(loaded.ticks) != 1 {
		t.Fatalf("loaded.ticks has %d entries, want 1", len(loaded.ticks))
	}

	tick0 := loaded.ticks[0]
	if tick0 == nil {
		t.Fatal("loaded.ticks[0] is nil")
	}

	dev1 := tick0[1]
	if dev1 == nil {
		t.Fatal("loaded.ticks[0][1] is nil")
	}

	if dev1.Commits != 5 {
		t.Errorf("dev1.Commits = %d, want 5", dev1.Commits)
	}

	if dev1.Added != 100 {
		t.Errorf("dev1.Added = %d, want 100", dev1.Added)
	}
}

func TestCheckpointRoundTrip_WithMerges(t *testing.T) {
	dir := t.TempDir()

	// Create an analyzer with merges data.
	hash1 := gitlib.NewHash("0123456789abcdef0123456789abcdef01234567")
	hash2 := gitlib.NewHash("fedcba9876543210fedcba9876543210fedcba98")

	original := &HistoryAnalyzer{
		ticks: map[int]map[int]*DevTick{},
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
	dir := t.TempDir()

	analyzer := &HistoryAnalyzer{}
	err := analyzer.LoadCheckpoint(dir)

	if err == nil {
		t.Fatal("LoadCheckpoint should fail when file doesn't exist")
	}
}

func TestLoadCheckpoint_CorruptedFile(t *testing.T) {
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
	dir := t.TempDir()

	// Create analyzer with realistic data.
	analyzer := &HistoryAnalyzer{
		ticks: map[int]map[int]*DevTick{
			0: {
				1: {
					LineStats: plumbing.LineStats{Added: 100, Removed: 20, Changed: 10},
					Commits:   5,
					Languages: map[string]plumbing.LineStats{
						"go":     {Added: 80, Removed: 15, Changed: 8},
						"python": {Added: 20, Removed: 5, Changed: 2},
					},
				},
				2: {
					LineStats: plumbing.LineStats{Added: 50, Removed: 10, Changed: 5},
					Commits:   3,
					Languages: map[string]plumbing.LineStats{
						"go": {Added: 50, Removed: 10, Changed: 5},
					},
				},
			},
			1: {
				1: {
					LineStats: plumbing.LineStats{Added: 200, Removed: 40, Changed: 20},
					Commits:   10,
					Languages: map[string]plumbing.LineStats{
						"go": {Added: 200, Removed: 40, Changed: 20},
					},
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
	ticks := make(map[int]map[int]*DevTick, numTicks)
	for tick := 0; tick < numTicks; tick++ {
		ticks[tick] = make(map[int]*DevTick, numDevs)
		for dev := 0; dev < numDevs; dev++ {
			ticks[tick][dev] = &DevTick{
				LineStats: plumbing.LineStats{
					Added:   100 + tick*10 + dev,
					Removed: 20 + tick + dev,
					Changed: 10 + tick + dev,
				},
				Commits: 5 + tick%10,
				Languages: map[string]plumbing.LineStats{
					"go":     {Added: 80, Removed: 15, Changed: 8},
					"python": {Added: 20, Removed: 5, Changed: 2},
				},
			}
		}
	}

	merges := make(map[gitlib.Hash]bool, numMerges)
	for i := 0; i < numMerges; i++ {
		hash := gitlib.NewHash("0123456789abcdef0123456789abcdef01234567")
		merges[hash] = true
	}

	return &HistoryAnalyzer{
		ticks:  ticks,
		merges: merges,
	}
}

// BenchmarkCheckpointSave benchmarks checkpoint save operation with realistic data.
func BenchmarkCheckpointSave(b *testing.B) {
	dir := b.TempDir()
	// Simulate ~300 ticks (1 year of daily data), 50 developers, 100 merges
	analyzer := createRealisticAnalyzer(300, 50, 100)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		err := analyzer.SaveCheckpoint(dir)
		if err != nil {
			b.Fatalf("SaveCheckpoint failed: %v", err)
		}
	}
}

// BenchmarkCheckpointLoad benchmarks checkpoint load operation with realistic data.
func BenchmarkCheckpointLoad(b *testing.B) {
	dir := b.TempDir()
	// Simulate ~300 ticks (1 year of daily data), 50 developers, 100 merges
	analyzer := createRealisticAnalyzer(300, 50, 100)

	// Save once to create the file
	err := analyzer.SaveCheckpoint(dir)
	if err != nil {
		b.Fatalf("SaveCheckpoint failed: %v", err)
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
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
	// Simulate ~300 ticks (1 year of daily data), 50 developers, 100 merges
	analyzer := createRealisticAnalyzer(300, 50, 100)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
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
