package shotness //nolint:testpackage // testing internal implementation.

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func TestHistoryAnalyzer_Name(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	if s.Name() == "" {
		t.Error("Name empty")
	}
}

func TestHistoryAnalyzer_Flag(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	if s.Flag() == "" {
		t.Error("Flag empty")
	}
}

func TestHistoryAnalyzer_Description(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	if s.Description() == "" {
		t.Error("Description empty")
	}
}

func TestHistoryAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	opts := s.ListConfigurationOptions()
	// May or may not have options.
	_ = opts
}

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	err := s.Configure(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	err := s.Initialize(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Fork(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	clones := s.Fork(2)
	if len(clones) != 2 {
		t.Error("expected 2 clones")
	}
}

func TestFork_CreatesIndependentCopies(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	// Add some state to original
	s.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "test.go"},
		Count:   5,
		Couples: map[string]int{"func2": 3},
	}

	forks := s.Fork(2)
	require.Len(t, forks, 2)

	fork1, ok := forks[0].(*HistoryAnalyzer)
	require.True(t, ok)
	fork2, ok := forks[1].(*HistoryAnalyzer)
	require.True(t, ok)

	// Forks should have empty independent maps (not inherit parent state)
	require.Empty(t, fork1.nodes, "fork should have empty nodes map")
	require.Empty(t, fork2.nodes, "fork should have empty nodes map")

	// Modifying one fork should not affect the other
	fork1.nodes["newFunc"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "newFunc", File: "a.go"},
		Count:   1,
		Couples: map[string]int{},
	}

	require.Len(t, fork1.nodes, 1)
	require.Empty(t, fork2.nodes, "fork2 should not see fork1's changes")
}

func TestFork_SharesDependencies(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{
		DSLStruct: "filter(.roles has \"Function\")",
		DSLName:   ".props.name",
	}
	require.NoError(t, s.Initialize(nil))

	forks := s.Fork(2)
	fork1, ok := forks[0].(*HistoryAnalyzer)
	require.True(t, ok)

	// Config should be shared
	require.Equal(t, s.DSLStruct, fork1.DSLStruct)
	require.Equal(t, s.DSLName, fork1.DSLName)
}

func TestMerge_CombinesNodes(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	// Original has a node
	s.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "test.go"},
		Count:   5,
		Couples: map[string]int{},
	}

	// Create a branch with different node
	branch := &HistoryAnalyzer{}
	require.NoError(t, branch.Initialize(nil))
	branch.nodes["func2"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func2", File: "test.go"},
		Count:   3,
		Couples: map[string]int{},
	}

	s.Merge([]analyze.HistoryAnalyzer{branch})

	// Should have both nodes
	require.Len(t, s.nodes, 2)
	require.NotNil(t, s.nodes["func1"])
	require.NotNil(t, s.nodes["func2"])
	require.Equal(t, 5, s.nodes["func1"].Count)
	require.Equal(t, 3, s.nodes["func2"].Count)
}

func TestMerge_SumsNodeCounts(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	// Original has a node with count 5
	s.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "test.go"},
		Count:   5,
		Couples: map[string]int{},
	}

	// Branch has same node with count 3
	branch := &HistoryAnalyzer{}
	require.NoError(t, branch.Initialize(nil))
	branch.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "test.go"},
		Count:   3,
		Couples: map[string]int{},
	}

	s.Merge([]analyze.HistoryAnalyzer{branch})

	// Counts should be summed
	require.Equal(t, 8, s.nodes["func1"].Count)
}

func TestMerge_CombinesCouples(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	s.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "test.go"},
		Count:   5,
		Couples: map[string]int{"func2": 2},
	}

	branch := &HistoryAnalyzer{}
	require.NoError(t, branch.Initialize(nil))
	branch.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "test.go"},
		Count:   3,
		Couples: map[string]int{"func2": 4, "func3": 1},
	}

	s.Merge([]analyze.HistoryAnalyzer{branch})

	// Couples should be summed
	require.Equal(t, 6, s.nodes["func1"].Couples["func2"])
	require.Equal(t, 1, s.nodes["func1"].Couples["func3"])
}

func TestMerge_CombinesMerges(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	branch := &HistoryAnalyzer{}
	require.NoError(t, branch.Initialize(nil))

	// Add merges to branch
	hash1 := [20]byte{1, 2, 3}
	hash2 := [20]byte{4, 5, 6}
	branch.merges[hash1] = true
	branch.merges[hash2] = true

	s.Merge([]analyze.HistoryAnalyzer{branch})

	require.Len(t, s.merges, 2)
	require.True(t, s.merges[hash1])
	require.True(t, s.merges[hash2])
}

func TestHistoryAnalyzer_Serialize_JSON_UsesComputedMetrics(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}

	nodes := []NodeSummary{
		{Type: "Function", Name: "testFunc", File: "test.go"},
	}
	counters := []map[int]int{
		{0: 15},
	}

	report := analyze.Report{
		"Nodes":    nodes,
		"Counters": counters,
	}

	var buf bytes.Buffer
	err := s.Serialize(report, analyze.FormatJSON, &buf)
	require.NoError(t, err)

	var result map[string]any
	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	// Should have computed metrics structure
	assert.Contains(t, result, "node_hotness")
	assert.Contains(t, result, "node_coupling")
	assert.Contains(t, result, "hotspot_nodes")
	assert.Contains(t, result, "aggregate")
}

func TestHistoryAnalyzer_Serialize_YAML_UsesComputedMetrics(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}

	nodes := []NodeSummary{
		{Type: "Function", Name: "testFunc", File: "test.go"},
	}
	counters := []map[int]int{
		{0: 15},
	}

	report := analyze.Report{
		"Nodes":    nodes,
		"Counters": counters,
	}

	var buf bytes.Buffer
	err := s.Serialize(report, analyze.FormatYAML, &buf)
	require.NoError(t, err)

	output := buf.String()
	// Should have computed metrics structure (YAML keys)
	assert.Contains(t, output, "node_hotness:")
	assert.Contains(t, output, "node_coupling:")
	assert.Contains(t, output, "hotspot_nodes:")
	assert.Contains(t, output, "aggregate:")
}
