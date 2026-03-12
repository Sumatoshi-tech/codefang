package shotness

import (
	"bytes"
	"encoding/json"
	"errors"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

func TestAnalyzer_Name(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	if s.Name() == "" {
		t.Error("Name empty")
	}
}

func TestAnalyzer_Flag(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	if s.Flag() == "" {
		t.Error("Flag empty")
	}
}

func TestAnalyzer_Description(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	if s.Description() == "" {
		t.Error("Description empty")
	}
}

func TestAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	opts := s.ListConfigurationOptions()
	// May or may not have options.
	_ = opts
}

func TestAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	err := s.Configure(nil)
	require.NoError(t, err)
}

func TestAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	err := s.Initialize(nil)
	require.NoError(t, err)
}

func TestAnalyzer_Fork(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	clones := s.Fork(2)
	if len(clones) != 2 {
		t.Error("expected 2 clones")
	}
}

func TestFork_CreatesIndependentCopies(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	// Add some state to original.
	s.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "test.go"},
		Count:   5,
	}

	forks := s.Fork(2)
	require.Len(t, forks, 2)

	fork1, ok := forks[0].(*Analyzer)
	require.True(t, ok)
	fork2, ok := forks[1].(*Analyzer)
	require.True(t, ok)

	// Forks should have empty independent maps (not inherit parent state).
	require.Empty(t, fork1.nodes, "fork should have empty nodes map")
	require.Empty(t, fork2.nodes, "fork should have empty nodes map")

	// Modifying one fork should not affect the other.
	fork1.nodes["newFunc"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "newFunc", File: "a.go"},
		Count:   1,
	}

	require.Len(t, fork1.nodes, 1)
	require.Empty(t, fork2.nodes, "fork2 should not see fork1's changes")
}

func TestFork_SharesDependencies(t *testing.T) {
	t.Parallel()

	s := &Analyzer{
		DSLStruct: "filter(.roles has \"Function\")",
		DSLName:   ".props.name",
	}
	require.NoError(t, s.Initialize(nil))

	forks := s.Fork(2)
	fork1, ok := forks[0].(*Analyzer)
	require.True(t, ok)

	// Config should be shared.
	require.Equal(t, s.DSLStruct, fork1.DSLStruct)
	require.Equal(t, s.DSLName, fork1.DSLName)
}

func TestExtractNodes_ShallowDuplicateNamesLastWins(t *testing.T) {
	t.Parallel()

	s := &Analyzer{
		DSLStruct: "filter(.roles has \"Function\")",
		DSLName:   ".props.name",
	}

	// Two function nodes with the same name; shallow design keeps only the last.
	root := &node.Node{
		Type: "file",
		Children: []*node.Node{
			{
				Type:  "Function",
				Token: "first",
				Roles: []node.Role{"Function"},
				Props: map[string]string{"name": "helper"},
			},
			{
				Type:  "Function",
				Token: "second",
				Roles: []node.Role{"Function"},
				Props: map[string]string{"name": "helper"},
			},
		},
	}

	res, err := s.extractNodes(root)
	require.NoError(t, err)

	// Shallow-only: duplicate names overwrite; exactly one entry.
	assert.Len(t, res, 1)
	assert.Contains(t, res, "helper")
	// Last wins: the second node's Token.
	assert.Equal(t, "second", res["helper"].Token)
}

func TestMerge_CombinesNodes(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	// Original has a node.
	s.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "test.go"},
		Count:   5,
	}

	// Create a branch with different node.
	branch := NewAnalyzer()
	require.NoError(t, branch.Initialize(nil))
	branch.nodes["func2"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func2", File: "test.go"},
		Count:   3,
	}

	s.Merge([]analyze.HistoryAnalyzer{branch})

	// Should have both nodes.
	require.Len(t, s.nodes, 2)
	require.NotNil(t, s.nodes["func1"])
	require.NotNil(t, s.nodes["func2"])
	require.Equal(t, 5, s.nodes["func1"].Count)
	require.Equal(t, 3, s.nodes["func2"].Count)
}

func TestMerge_SumsNodeCounts(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	// Original has a node with count 5.
	s.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "test.go"},
		Count:   5,
	}

	// Branch has same node with count 3.
	branch := NewAnalyzer()
	require.NoError(t, branch.Initialize(nil))
	branch.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "test.go"},
		Count:   3,
	}

	s.Merge([]analyze.HistoryAnalyzer{branch})

	// Counts should be summed.
	require.Equal(t, 8, s.nodes["func1"].Count)
}

func TestMerge_MultipleBranches(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	s.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "test.go"},
		Count:   5,
	}

	branch1 := NewAnalyzer()
	require.NoError(t, branch1.Initialize(nil))
	branch1.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "test.go"},
		Count:   3,
	}

	branch2 := NewAnalyzer()
	require.NoError(t, branch2.Initialize(nil))
	branch2.nodes["func1"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "func1", File: "test.go"},
		Count:   2,
	}

	s.Merge([]analyze.HistoryAnalyzer{branch1, branch2})

	// Counts from all branches should be summed.
	require.Equal(t, 10, s.nodes["func1"].Count)
}

func TestMerge_DoesNotCombineMergeTrackers(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	branch := NewAnalyzer()
	require.NoError(t, branch.Initialize(nil))

	// Add merges to branch via SeenOrAdd.
	hash1 := [20]byte{1, 2, 3}
	hash2 := [20]byte{4, 5, 6}

	branch.merges.SeenOrAdd(hash1)
	branch.merges.SeenOrAdd(hash2)

	s.Merge([]analyze.HistoryAnalyzer{branch})

	// Merge trackers are not combined: each fork processes a disjoint
	// subset of commits, so merge dedup state stays independent.
	require.False(t, s.merges.SeenOrAdd(hash1), "parent should not have branch's merges")
}

func TestNewAggregator_ReturnsAggregator(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()

	agg := s.NewAggregator(analyze.AggregatorOptions{})
	require.NotNil(t, agg)
}

func TestSerializeTICKs_ProducesValidOutput(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()

	ticks := []analyze.TICK{
		{
			Tick: 0,
			Data: &TickData{
				Nodes: map[string]*nodeShotnessData{
					"Function_testFunc_test.go": {
						Summary: NodeSummary{Type: "Function", Name: "testFunc", File: "test.go"},
						Count:   15,
						Couples: map[string]int{},
					},
				},
			},
		},
	}

	var buf bytes.Buffer

	err := s.SerializeTICKs(ticks, analyze.FormatJSON, &buf)
	require.NoError(t, err)

	var result map[string]any

	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	assert.Contains(t, result, "node_hotness")
	assert.Contains(t, result, "aggregate")
}

func TestBuildCommitData_EmptyAllNodes(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	cd := s.buildCommitData(map[string]bool{})
	assert.Nil(t, cd)
}

func TestBuildCommitData_WithNodes(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	s.nodes["Function_foo_main.go"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "foo", File: "main.go"},
		Count:   3,
	}
	s.nodes["Function_bar_main.go"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "bar", File: "main.go"},
		Count:   1,
	}

	allNodes := map[string]bool{
		"Function_foo_main.go": true,
		"Function_bar_main.go": true,
	}

	cd := s.buildCommitData(allNodes)
	require.NotNil(t, cd)

	// Verify nodes touched.
	assert.Len(t, cd.NodesTouched, 2)
	assert.Equal(t, 1, cd.NodesTouched["Function_foo_main.go"].CountDelta)
	assert.Equal(t, "foo", cd.NodesTouched["Function_foo_main.go"].Summary.Name)

	// Coupling pairs are no longer pre-computed in CommitData;
	// they are derived inline by the aggregator from NodesTouched.
}

func TestAnalyzer_Serialize_JSON_UsesComputedMetrics(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()

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

	// Should have computed metrics structure.
	assert.Contains(t, result, "node_hotness")
	assert.Contains(t, result, "node_coupling")
	assert.Contains(t, result, "hotspot_nodes")
	assert.Contains(t, result, "aggregate")
}

func TestAnalyzer_Serialize_YAML_UsesComputedMetrics(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()

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
	// Should have computed metrics structure (YAML keys).
	assert.Contains(t, output, "node_hotness:")
	assert.Contains(t, output, "node_coupling:")
	assert.Contains(t, output, "hotspot_nodes:")
	assert.Contains(t, output, "aggregate:")
}

func TestNodeSummary_String(t *testing.T) {
	t.Parallel()

	ns := NodeSummary{Type: "Function", Name: "foo", File: "main.go"}
	assert.Equal(t, "Function_foo_main.go", ns.String())
}

func TestAnalyzer_CPUHeavy(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	assert.True(t, s.CPUHeavy())
}

func TestAnalyzer_SequentialOnly(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	assert.False(t, s.SequentialOnly())
}

func TestAnalyzer_NeedsUAST(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	assert.True(t, s.NeedsUAST())
}

func TestShouldConsumeCommit_SingleParent(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	commit := &mockCommit{hash: [20]byte{1}, parents: 1}
	assert.True(t, s.shouldConsumeCommit(commit))
}

func TestShouldConsumeCommit_FirstMerge(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	commit := &mockCommit{hash: [20]byte{1}, parents: 2}
	assert.True(t, s.shouldConsumeCommit(commit))
}

func TestShouldConsumeCommit_DuplicateMerge(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	commit := &mockCommit{hash: [20]byte{1}, parents: 2}
	assert.True(t, s.shouldConsumeCommit(commit))
	assert.False(t, s.shouldConsumeCommit(commit))
}

func TestAddNode_NewNode(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	n := &node.Node{Type: "Function", Token: "test"}
	allNodes := map[string]bool{}

	s.addNode("testFunc", n, "main.go", allNodes)

	assert.True(t, allNodes["Function_testFunc_main.go"])
	assert.NotNil(t, s.nodes["Function_testFunc_main.go"])
	assert.Equal(t, 1, s.nodes["Function_testFunc_main.go"].Count)
}

func TestAddNode_ExistingNode_DifferentCommit(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	n := &node.Node{Type: "Function", Token: "test"}
	allNodes := map[string]bool{}

	s.addNode("testFunc", n, "main.go", allNodes)

	// Second time with fresh allNodes.
	allNodes2 := map[string]bool{}
	s.addNode("testFunc", n, "main.go", allNodes2)

	assert.Equal(t, 2, s.nodes["Function_testFunc_main.go"].Count)
}

func TestAddNode_ExistingNode_SameCommit(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	n := &node.Node{Type: "Function", Token: "test"}
	allNodes := map[string]bool{}

	s.addNode("testFunc", n, "main.go", allNodes)
	s.addNode("testFunc", n, "main.go", allNodes)

	// Same commit: should not increment count.
	assert.Equal(t, 1, s.nodes["Function_testFunc_main.go"].Count)
}

func TestApplyRename(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	key := "Function_foo_old.go"
	s.nodes[key] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "foo", File: "old.go"},
		Count:   5,
	}
	s.files["old.go"] = map[string]*nodeShotness{key: s.nodes[key]}

	s.applyRename("old.go", "new.go")

	newKey := "Function_foo_new.go"

	assert.Nil(t, s.nodes[key])
	assert.NotNil(t, s.nodes[newKey])
	assert.Equal(t, "new.go", s.nodes[newKey].Summary.File)
	assert.NotNil(t, s.files["new.go"])
	assert.Nil(t, s.files["old.go"])
}

func TestGenLine2Node(t *testing.T) {
	t.Parallel()

	n := &node.Node{
		Type: "Function",
		Pos:  &node.Positions{StartLine: 2, EndLine: 4},
	}

	result := genLine2Node(map[string]*node.Node{"fn": n}, 5)
	require.Len(t, result, 5)
	assert.Nil(t, result[0])    // Line 1.
	assert.Len(t, result[1], 1) // Line 2.
	assert.Len(t, result[2], 1) // Line 3.
	assert.Len(t, result[3], 1) // Line 4.
	assert.Nil(t, result[4])    // Line 5.
}

func TestGenLine2Node_NilPos(t *testing.T) {
	t.Parallel()

	n := &node.Node{Type: "Function", Pos: nil}

	result := genLine2Node(map[string]*node.Node{"fn": n}, 3)
	require.Len(t, result, 3)
	assert.Nil(t, result[0])
	assert.Nil(t, result[1])
	assert.Nil(t, result[2])
}

func TestResolveEndLine_WithEndLine(t *testing.T) {
	t.Parallel()

	n := &node.Node{
		Pos: &node.Positions{StartLine: 5, EndLine: 10},
	}

	assert.Equal(t, 10, resolveEndLine(n, n.Pos))
}

func TestResolveEndLine_WalksChildren(t *testing.T) {
	t.Parallel()

	child := &node.Node{
		Pos: &node.Positions{StartLine: 8, EndLine: 15},
	}
	parent := &node.Node{
		Pos:      &node.Positions{StartLine: 5, EndLine: 5},
		Children: []*node.Node{child},
	}

	assert.Equal(t, 15, resolveEndLine(parent, parent.Pos))
}

func TestReverseNodeMap(t *testing.T) {
	t.Parallel()

	n1 := &node.Node{ID: "id1"}
	n2 := &node.Node{ID: "id2"}

	result := reverseNodeMap(map[string]*node.Node{"name1": n1, "name2": n2})
	assert.Equal(t, "name1", result["id1"])
	assert.Equal(t, "name2", result["id2"])
}

func TestRebuildFilesMap(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	s.nodes["Function_foo_a.go"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "foo", File: "a.go"},
		Count:   1,
	}
	s.nodes["Function_bar_b.go"] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "bar", File: "b.go"},
		Count:   2,
	}

	s.rebuildFilesMap()

	assert.Len(t, s.files, 2)
	assert.NotNil(t, s.files["a.go"]["Function_foo_a.go"])
	assert.NotNil(t, s.files["b.go"]["Function_bar_b.go"])
}

func TestExtractTC(t *testing.T) {
	t.Parallel()

	byTick := make(map[int]*TickData)

	cd := &CommitData{
		NodesTouched: map[string]NodeDelta{
			"Function_foo_a.go": {
				Summary:    NodeSummary{Type: "Function", Name: "foo", File: "a.go"},
				CountDelta: 1,
			},
		},
	}

	tc := analyze.TC{Tick: 0, Data: cd}

	err := extractTC(tc, byTick)
	require.NoError(t, err)
	require.Contains(t, byTick, 0)
	assert.Equal(t, 1, byTick[0].Nodes["Function_foo_a.go"].Count)
}

func TestExtractTC_NilData(t *testing.T) {
	t.Parallel()

	byTick := make(map[int]*TickData)
	tc := analyze.TC{Tick: 0, Data: nil}

	err := extractTC(tc, byTick)
	require.NoError(t, err)
	assert.Empty(t, byTick)
}

func TestExtractTC_WrongDataType(t *testing.T) {
	t.Parallel()

	byTick := make(map[int]*TickData)
	tc := analyze.TC{Tick: 0, Data: "not_commit_data"}

	err := extractTC(tc, byTick)
	require.NoError(t, err)
	assert.Empty(t, byTick)
}

func TestExtractTC_WithCouples(t *testing.T) {
	t.Parallel()

	byTick := make(map[int]*TickData)

	// Coupling pairs are now computed inline from NodesTouched keys.
	cd := &CommitData{
		NodesTouched: map[string]NodeDelta{
			"a": {Summary: NodeSummary{Name: "foo"}, CountDelta: 1},
			"b": {Summary: NodeSummary{Name: "bar"}, CountDelta: 1},
		},
	}

	err := extractTC(analyze.TC{Tick: 0, Data: cd}, byTick)
	require.NoError(t, err)

	assert.Equal(t, 1, byTick[0].Nodes["a"].Couples["b"])
	assert.Equal(t, 1, byTick[0].Nodes["b"].Couples["a"])
}

func TestExtractTC_CouplingCapped(t *testing.T) {
	t.Parallel()

	byTick := make(map[int]*TickData)

	// Create a commit touching more than maxCouplingNodes nodes.
	nodesTouched := make(map[string]NodeDelta, maxCouplingNodes+10)
	for i := range maxCouplingNodes + 10 {
		key := "Function_fn" + string(rune('A'+i/26)) + string(rune('a'+i%26)) + "_file.go"
		nodesTouched[key] = NodeDelta{
			Summary:    NodeSummary{Type: "Function", Name: key, File: "file.go"},
			CountDelta: 1,
		}
	}

	cd := &CommitData{NodesTouched: nodesTouched}
	hash := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")

	err := extractTC(analyze.TC{Tick: 0, Data: cd, CommitHash: hash}, byTick)
	require.NoError(t, err)

	// All nodes should be recorded.
	assert.Len(t, byTick[0].Nodes, maxCouplingNodes+10)

	// But coupling maps should be empty (capped).
	for _, nd := range byTick[0].Nodes {
		assert.Empty(t, nd.Couples, "coupling should be skipped for large commits")
	}

	// Coupling pair count should still be recorded in commit stats.
	n := maxCouplingNodes + 10
	expectedPairs := n * (n - 1) / 2
	assert.Equal(t, expectedPairs, byTick[0].CommitStats[hash.String()].CouplingPairs)
}

func TestMergeState_BothNil(t *testing.T) {
	t.Parallel()

	result := mergeState(nil, nil)
	assert.Nil(t, result)
}

func TestMergeState_ExistingNil(t *testing.T) {
	t.Parallel()

	incoming := &TickData{Nodes: map[string]*nodeShotnessData{"a": {Count: 1}}}
	result := mergeState(nil, incoming)
	assert.Equal(t, incoming, result)
}

func TestMergeState_IncomingNil(t *testing.T) {
	t.Parallel()

	existing := &TickData{Nodes: map[string]*nodeShotnessData{"a": {Count: 1}}}
	result := mergeState(existing, nil)
	assert.Equal(t, existing, result)
}

func TestMergeState_BothPresent(t *testing.T) {
	t.Parallel()

	existing := &TickData{Nodes: map[string]*nodeShotnessData{
		"a": {Count: 5, Couples: map[string]int{"b": 2}},
	}}
	incoming := &TickData{Nodes: map[string]*nodeShotnessData{
		"a": {Count: 3, Couples: map[string]int{"b": 1, "c": 4}},
		"d": {Count: 7, Couples: map[string]int{}},
	}}

	result := mergeState(existing, incoming)
	assert.Equal(t, 8, result.Nodes["a"].Count)
	assert.Equal(t, 3, result.Nodes["a"].Couples["b"])
	assert.Equal(t, 4, result.Nodes["a"].Couples["c"])
	assert.Equal(t, 7, result.Nodes["d"].Count)
}

func TestMergeState_NilNodesMap(t *testing.T) {
	t.Parallel()

	existing := &TickData{Nodes: nil}
	incoming := &TickData{Nodes: map[string]*nodeShotnessData{
		"a": {Count: 1, Couples: map[string]int{}},
	}}

	result := mergeState(existing, incoming)
	assert.NotNil(t, result.Nodes)
	assert.Equal(t, 1, result.Nodes["a"].Count)
}

func TestSizeState_Nil(t *testing.T) {
	t.Parallel()

	assert.Equal(t, int64(0), sizeState(nil))
}

func TestSizeState_WithData(t *testing.T) {
	t.Parallel()

	state := &TickData{Nodes: map[string]*nodeShotnessData{
		"a": {Count: 1, Couples: map[string]int{"b": 1, "c": 2}},
	}}

	size := sizeState(state)
	assert.Positive(t, size)
}

func TestBuildTick_Nil(t *testing.T) {
	t.Parallel()

	tick, err := buildTick(5, nil)
	require.NoError(t, err)
	assert.Equal(t, 5, tick.Tick)
}

func TestBuildTick_WithData(t *testing.T) {
	t.Parallel()

	state := &TickData{Nodes: map[string]*nodeShotnessData{
		"a": {Count: 1},
	}}

	tick, err := buildTick(3, state)
	require.NoError(t, err)
	assert.Equal(t, 3, tick.Tick)
	assert.Equal(t, state, tick.Data)
}

func TestCopyIntMap(t *testing.T) {
	t.Parallel()

	src := map[string]int{"a": 1, "b": 2}
	dst := copyIntMap(src)

	assert.Equal(t, src, dst)

	dst["c"] = 3

	assert.NotContains(t, src, "c")
}

func TestComputeMetricsSafe_EmptyReport(t *testing.T) {
	t.Parallel()

	safe := analyze.SafeMetricComputer(ComputeAllMetrics, &ComputedMetrics{})
	result, err := safe(analyze.Report{})
	require.NoError(t, err)
	require.NotNil(t, result)
}

func TestComputeMetricsSafe_WithData(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"Nodes": []NodeSummary{
			{Type: "Function", Name: "foo", File: "a.go"},
		},
		"Counters": []map[int]int{
			{0: 10},
		},
	}

	safe := analyze.SafeMetricComputer(ComputeAllMetrics, &ComputedMetrics{})
	result, err := safe(report)
	require.NoError(t, err)
	assert.Len(t, result.NodeHotness, 1)
}

func TestComputedMetrics_AnalyzerName(t *testing.T) {
	t.Parallel()

	m := &ComputedMetrics{}
	assert.Equal(t, "shotness", m.AnalyzerName())
}

func TestComputedMetrics_ToJSON(t *testing.T) {
	t.Parallel()

	m := &ComputedMetrics{}
	assert.Equal(t, m, m.ToJSON())
}

func TestComputedMetrics_ToYAML(t *testing.T) {
	t.Parallel()

	m := &ComputedMetrics{}
	assert.Equal(t, m, m.ToYAML())
}

func TestConfigure_WithFacts(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	err := s.Configure(map[string]any{
		ConfigShotnessDSLStruct: "filter(.roles has \"Class\")",
		ConfigShotnessDSLName:   ".props.className",
	})

	require.NoError(t, err)
	assert.Equal(t, "filter(.roles has \"Class\")", s.DSLStruct)
	assert.Equal(t, ".props.className", s.DSLName)
}

func TestConfigure_Defaults(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	err := s.Configure(map[string]any{})
	require.NoError(t, err)
	assert.Equal(t, DefaultShotnessDSLStruct, s.DSLStruct)
	assert.Equal(t, DefaultShotnessDSLName, s.DSLName)
}

func TestHandleDeletion(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	require.NoError(t, s.Initialize(nil))

	key := "Function_foo_deleted.go"
	s.nodes[key] = &nodeShotness{
		Summary: NodeSummary{Type: "Function", Name: "foo", File: "deleted.go"},
		Count:   3,
	}
	s.files["deleted.go"] = map[string]*nodeShotness{key: s.nodes[key]}

	change := uast.Change{
		Change: &gitlib.Change{
			From: gitlib.ChangeEntry{Name: "deleted.go"},
		},
	}

	s.handleDeletion(change)

	assert.Nil(t, s.nodes[key])
	assert.Nil(t, s.files["deleted.go"])
}

// errMockNotImpl is returned by mock methods that are not implemented.
var errMockNotImpl = errors.New("mock: not implemented")

type mockCommit struct {
	hash    gitlib.Hash
	parents int
}

func (m *mockCommit) Hash() gitlib.Hash           { return m.hash }
func (m *mockCommit) NumParents() int             { return m.parents }
func (m *mockCommit) Author() gitlib.Signature    { return gitlib.Signature{} }
func (m *mockCommit) Committer() gitlib.Signature { return gitlib.Signature{} }
func (m *mockCommit) Message() string             { return "" }

func (m *mockCommit) Parent(_ int) (*gitlib.Commit, error) {
	return nil, errMockNotImpl
}

func (m *mockCommit) Tree() (*gitlib.Tree, error) {
	return nil, errMockNotImpl
}

func (m *mockCommit) Files() (*gitlib.FileIter, error) {
	return nil, errMockNotImpl
}

func (m *mockCommit) File(_ string) (*gitlib.File, error) {
	return nil, errMockNotImpl
}

func TestExtractCommitTimeSeries(t *testing.T) {
	t.Parallel()

	hashA := "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
	hashB := "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"

	report := analyze.Report{
		"commit_stats": map[string]*CommitSummary{
			hashA: {NodesTouched: 5, CouplingPairs: 10},
			hashB: {NodesTouched: 2, CouplingPairs: 1},
		},
	}

	s := &Analyzer{}
	result := s.ExtractCommitTimeSeries(report)

	require.Len(t, result, 2)

	entryA, ok := result[hashA].(map[string]any)
	require.True(t, ok)
	assert.Equal(t, 5, entryA["nodes_touched"])
	assert.Equal(t, 10, entryA["coupling_pairs"])

	entryB, ok := result[hashB].(map[string]any)
	require.True(t, ok)
	assert.Equal(t, 2, entryB["nodes_touched"])
	assert.Equal(t, 1, entryB["coupling_pairs"])
}

func TestExtractCommitTimeSeries_Empty(t *testing.T) {
	t.Parallel()

	s := &Analyzer{}
	assert.Nil(t, s.ExtractCommitTimeSeries(analyze.Report{}))
}
