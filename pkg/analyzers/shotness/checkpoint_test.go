package shotness

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
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

	// Add node data
	original.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "test.go"},
		Count:   10,
		Couples: map[string]int{"func2": 5},
	}
	original.merges[gitlib.NewHash("abc123")] = true

	require.NoError(t, original.SaveCheckpoint(dir))

	restored := &HistoryAnalyzer{}
	require.NoError(t, restored.LoadCheckpoint(dir))

	require.Len(t, restored.nodes, 1)
	require.NotNil(t, restored.nodes["func1"])
	require.Equal(t, 10, restored.nodes["func1"].Count)
	require.Equal(t, 5, restored.nodes["func1"].Couples["func2"])
	require.Len(t, restored.merges, 1)
}

func TestCheckpointSize_ReturnsPositiveValue(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	s.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "test.go"},
		Count:   10,
		Couples: map[string]int{"func2": 5},
	}

	size := s.CheckpointSize()
	require.Positive(t, size)
}

func TestCheckpointRoundTrip_PreservesAllState(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	original := &HistoryAnalyzer{}
	require.NoError(t, original.Initialize(nil))

	// Add multiple nodes with coupling data
	original.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "main.go"},
		Count:   15,
		Couples: map[string]int{"func2": 7, "func3": 3},
	}
	original.nodes["func2"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func2", File: "main.go"},
		Count:   8,
		Couples: map[string]int{"func1": 7},
	}
	original.nodes["func3"] = &nodeShotness{
		Summary: NodeSummary{Type: "Method", Name: "func3", File: "util.go"},
		Count:   3,
		Couples: map[string]int{"func1": 3},
	}

	// Add merges
	original.merges[gitlib.NewHash("merge1")] = true
	original.merges[gitlib.NewHash("merge2")] = true

	// Rebuild files map
	original.rebuildFilesMap()

	require.NoError(t, original.SaveCheckpoint(dir))

	restored := &HistoryAnalyzer{}
	require.NoError(t, restored.LoadCheckpoint(dir))

	// Verify nodes
	require.Len(t, restored.nodes, 3)

	func1 := restored.nodes["func1"]
	require.NotNil(t, func1)
	require.Equal(t, "Function", func1.Summary.Type)
	require.Equal(t, "func1", func1.Summary.Name)
	require.Equal(t, "main.go", func1.Summary.File)
	require.Equal(t, 15, func1.Count)
	require.Equal(t, 7, func1.Couples["func2"])
	require.Equal(t, 3, func1.Couples["func3"])

	func2 := restored.nodes["func2"]
	require.NotNil(t, func2)
	require.Equal(t, 8, func2.Count)

	func3 := restored.nodes["func3"]
	require.NotNil(t, func3)
	require.Equal(t, "Method", func3.Summary.Type)

	// Verify merges
	require.Len(t, restored.merges, 2)

	// Verify files map was rebuilt
	require.Len(t, restored.files, 2)
	require.Len(t, restored.files["main.go"], 2)
	require.Len(t, restored.files["util.go"], 1)
}
