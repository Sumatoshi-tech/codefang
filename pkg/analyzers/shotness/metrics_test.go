package shotness

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// Test constants to avoid magic strings/numbers.
const (
	testNodeName1 = "TestFunc1"
	testNodeName2 = "TestFunc2"
	testNodeName3 = "TestFunc3"
	testNodeType  = "function"
	testFile1     = "file1.go"
	testFile2     = "file2.go"

	floatDelta = 0.01
)

// --- ParseReportData Tests ---.

func TestParseReportData_Empty(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	assert.Empty(t, result.Nodes)
	assert.Empty(t, result.Counters)
}

func TestParseReportData_AllFields(t *testing.T) {
	t.Parallel()

	nodes := []NodeSummary{
		{Name: testNodeName1, Type: testNodeType, File: testFile1},
		{Name: testNodeName2, Type: testNodeType, File: testFile2},
	}
	counters := []map[int]int{
		{0: 10, 1: 5},
		{0: 5, 1: 8},
	}

	report := analyze.Report{
		"Nodes":    nodes,
		"Counters": counters,
	}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	require.Len(t, result.Nodes, 2)
	require.Len(t, result.Counters, 2)

	assert.Equal(t, testNodeName1, result.Nodes[0].Name)
	assert.Equal(t, testNodeType, result.Nodes[0].Type)
	assert.Equal(t, testFile1, result.Nodes[0].File)
}

// --- NodeHotnessMetric Tests ---.

func TestNewNodeHotnessMetric_Metadata(t *testing.T) {
	t.Parallel()

	m := NewNodeHotnessMetric()

	assert.Equal(t, "node_hotness", m.Name())
	assert.Equal(t, "Node Hotness", m.DisplayName())
	assert.Contains(t, m.Description(), "Per-node change frequency")
	assert.Equal(t, "list", m.Type())
}

func TestNodeHotnessMetric_Empty(t *testing.T) {
	t.Parallel()

	m := NewNodeHotnessMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestNodeHotnessMetric_SingleNode(t *testing.T) {
	t.Parallel()

	m := NewNodeHotnessMetric()
	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1, Type: testNodeType, File: testFile1},
		},
		Counters: []map[int]int{
			{0: 15},
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, testNodeName1, result[0].Name)
	assert.Equal(t, testNodeType, result[0].Type)
	assert.Equal(t, testFile1, result[0].File)
	assert.Equal(t, 15, result[0].ChangeCount)
	assert.Equal(t, 0, result[0].CoupledNodes)                 // No other nodes.
	assert.InDelta(t, 1.0, result[0].HotnessScore, floatDelta) // max=15, self=15.
}

func TestNodeHotnessMetric_MultipleNodes_SortedByChangeCount(t *testing.T) {
	t.Parallel()

	m := NewNodeHotnessMetric()
	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1, Type: testNodeType, File: testFile1},
			{Name: testNodeName2, Type: testNodeType, File: testFile1},
			{Name: testNodeName3, Type: testNodeType, File: testFile2},
		},
		Counters: []map[int]int{
			{0: 5, 1: 3},
			{0: 3, 1: 20},
			{0: 0, 1: 2, 2: 10},
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 3)
	// Sorted by change count descending.
	assert.Equal(t, testNodeName2, result[0].Name)
	assert.Equal(t, 20, result[0].ChangeCount)
	assert.Equal(t, testNodeName3, result[1].Name)
	assert.Equal(t, 10, result[1].ChangeCount)
	assert.Equal(t, testNodeName1, result[2].Name)
	assert.Equal(t, 5, result[2].ChangeCount)
}

func TestNodeHotnessMetric_CoupledNodesCount(t *testing.T) {
	t.Parallel()

	m := NewNodeHotnessMetric()
	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1, Type: testNodeType, File: testFile1},
		},
		Counters: []map[int]int{
			{0: 10, 1: 3, 2: 5}, // Self + 2 coupled nodes.
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, 2, result[0].CoupledNodes) // 3 entries minus self.
}

func TestNodeHotnessMetric_HotnessScoreNormalized(t *testing.T) {
	t.Parallel()

	m := NewNodeHotnessMetric()
	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1, Type: testNodeType, File: testFile1},
			{Name: testNodeName2, Type: testNodeType, File: testFile1},
		},
		Counters: []map[int]int{
			{0: 20}, // max changes.
			{1: 10}, // half of max.
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 2)
	// node1 has score 1.0 (20/20).
	assert.InDelta(t, 1.0, result[0].HotnessScore, floatDelta)
	// node2 has score 0.5 (10/20).
	assert.InDelta(t, 0.5, result[1].HotnessScore, floatDelta)
}

func TestNodeHotnessMetric_OutOfBoundsCounter(t *testing.T) {
	t.Parallel()

	m := NewNodeHotnessMetric()
	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1, Type: testNodeType, File: testFile1},
			{Name: testNodeName2, Type: testNodeType, File: testFile1},
		},
		Counters: []map[int]int{
			{0: 10}, // Only 1 counter for 2 nodes.
		},
	}

	result := m.Compute(input)

	// Should only process nodes with corresponding counters.
	require.Len(t, result, 1)
	assert.Equal(t, testNodeName1, result[0].Name)
}

// --- NodeCouplingMetric Tests ---.

func TestNewNodeCouplingMetric_Metadata(t *testing.T) {
	t.Parallel()

	m := NewNodeCouplingMetric()

	assert.Equal(t, "node_coupling", m.Name())
	assert.Equal(t, "Node Coupling", m.DisplayName())
	assert.Contains(t, m.Description(), "change together")
	assert.Equal(t, "list", m.Type())
}

func TestNodeCouplingMetric_Empty(t *testing.T) {
	t.Parallel()

	m := NewNodeCouplingMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestNodeCouplingMetric_SinglePair(t *testing.T) {
	t.Parallel()

	m := NewNodeCouplingMetric()
	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1, Type: testNodeType, File: testFile1},
			{Name: testNodeName2, Type: testNodeType, File: testFile2},
		},
		Counters: []map[int]int{
			{0: 10, 1: 5}, // node1-node2 coupled with 5 changes.
			{0: 5, 1: 8},
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, testNodeName1, result[0].Node1Name)
	assert.Equal(t, testFile1, result[0].Node1File)
	assert.Equal(t, testNodeName2, result[0].Node2Name)
	assert.Equal(t, testFile2, result[0].Node2File)
	assert.Equal(t, 5, result[0].CoChanges)
}

func TestNodeCouplingMetric_MultiplePairs_SortedByCoChanges(t *testing.T) {
	t.Parallel()

	m := NewNodeCouplingMetric()
	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1, Type: testNodeType, File: testFile1},
			{Name: testNodeName2, Type: testNodeType, File: testFile1},
			{Name: testNodeName3, Type: testNodeType, File: testFile2},
		},
		Counters: []map[int]int{
			{0: 10, 1: 3, 2: 8}, // co-occurrence: node1 with node2 is 3, node1 with node3 is 8.
			{0: 3, 1: 5, 2: 2},  // co-occurrence: node2 with node3 is 2.
			{0: 8, 1: 2, 2: 6},
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 3)
	// Sorted by CoChanges descending.
	assert.Equal(t, 8, result[0].CoChanges) // node1-node3.
	assert.Equal(t, 3, result[1].CoChanges) // node1-node2.
	assert.Equal(t, 2, result[2].CoChanges) // node2-node3.
}

func TestNodeCouplingMetric_SkipsZeroCoChanges(t *testing.T) {
	t.Parallel()

	m := NewNodeCouplingMetric()
	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1, Type: testNodeType, File: testFile1},
			{Name: testNodeName2, Type: testNodeType, File: testFile1},
		},
		Counters: []map[int]int{
			{0: 10}, // No coupling with node2.
			{1: 8},
		},
	}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestNodeCouplingMetric_OutOfBoundsNodeIndex(t *testing.T) {
	t.Parallel()

	m := NewNodeCouplingMetric()
	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1, Type: testNodeType, File: testFile1},
		},
		Counters: []map[int]int{
			{0: 10, 5: 3}, // Index 5 is out of bounds.
		},
	}

	result := m.Compute(input)

	assert.Empty(t, result) // No valid pairs.
}

// --- HotspotNodeMetric Tests ---.

func TestNewHotspotNodeMetric_Metadata(t *testing.T) {
	t.Parallel()

	m := NewHotspotNodeMetric()

	assert.Equal(t, "hotspot_nodes", m.Name())
	assert.Equal(t, "Hotspot Nodes", m.DisplayName())
	assert.Contains(t, m.Description(), "high change frequency")
	assert.Equal(t, "risk", m.Type())
}

func TestHotspotNodeMetric_Empty(t *testing.T) {
	t.Parallel()

	m := NewHotspotNodeMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestHotspotNodeMetric_BelowThreshold(t *testing.T) {
	t.Parallel()

	m := NewHotspotNodeMetric()
	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1, Type: testNodeType, File: testFile1},
		},
		Counters: []map[int]int{
			{0: 5}, // Below HotspotThresholdMedium (10).
		},
	}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestHotspotNodeMetric_RiskLevels(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name        string
		changeCount int
		expected    string
	}{
		{"high_risk", 25, "HIGH"},
		{"high_boundary", 20, "HIGH"},
		{"medium_risk", 15, "MEDIUM"},
		{"medium_boundary", 10, "MEDIUM"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			m := NewHotspotNodeMetric()
			input := &ReportData{
				Nodes: []NodeSummary{
					{Name: testNodeName1, Type: testNodeType, File: testFile1},
				},
				Counters: []map[int]int{
					{0: tt.changeCount},
				},
			}

			result := m.Compute(input)

			require.Len(t, result, 1)
			assert.Equal(t, testNodeName1, result[0].Name)
			assert.Equal(t, tt.changeCount, result[0].ChangeCount)
			assert.Equal(t, tt.expected, result[0].RiskLevel)
		})
	}
}

func TestHotspotNodeMetric_SortedByChangeCount(t *testing.T) {
	t.Parallel()

	m := NewHotspotNodeMetric()
	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1, Type: testNodeType, File: testFile1},
			{Name: testNodeName2, Type: testNodeType, File: testFile1},
			{Name: testNodeName3, Type: testNodeType, File: testFile2},
		},
		Counters: []map[int]int{
			{0: 15}, // MEDIUM.
			{1: 30}, // HIGH.
			{2: 5},  // Below threshold - excluded.
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 2)
	// Sorted by change count descending.
	assert.Equal(t, testNodeName2, result[0].Name)
	assert.Equal(t, "HIGH", result[0].RiskLevel)
	assert.Equal(t, testNodeName1, result[1].Name)
	assert.Equal(t, "MEDIUM", result[1].RiskLevel)
}

// --- ShotnessAggregateMetric Tests ---.

func TestNewAggregateMetric_Metadata(t *testing.T) {
	t.Parallel()

	m := NewAggregateMetric()

	assert.Equal(t, "shotness_aggregate", m.Name())
	assert.Equal(t, "Shotness Summary", m.DisplayName())
	assert.Contains(t, m.Description(), "Aggregate statistics")
	assert.Equal(t, "aggregate", m.Type())
}

func TestShotnessAggregateMetric_Empty(t *testing.T) {
	t.Parallel()

	m := NewAggregateMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Equal(t, 0, result.TotalNodes)
	assert.Equal(t, 0, result.TotalChanges)
	assert.Equal(t, 0, result.TotalCouplings)
	assert.InDelta(t, 0.0, result.AvgChangesPerNode, floatDelta)
	assert.Equal(t, 0, result.HotNodes)
}

func TestShotnessAggregateMetric_WithData(t *testing.T) {
	t.Parallel()

	m := NewAggregateMetric()
	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1, Type: testNodeType, File: testFile1},
			{Name: testNodeName2, Type: testNodeType, File: testFile1},
			{Name: testNodeName3, Type: testNodeType, File: testFile2},
		},
		Counters: []map[int]int{
			{0: 25, 1: 5}, // self=25 (hot), coupled with node2.
			{0: 5, 1: 10}, // self=10 (hot, at boundary).
			{2: 5},        // self=5 (not hot).
		},
	}

	result := m.Compute(input)

	assert.Equal(t, 3, result.TotalNodes)
	// Total changes = 25 + 10 + 5 = 40.
	assert.Equal(t, 40, result.TotalChanges)
	// Couplings: node1-node2 (counted from both sides, divided by 2).
	assert.GreaterOrEqual(t, result.TotalCouplings, 0)
	// Avg = 40/3 = 13.33.
	assert.InDelta(t, 40.0/3.0, result.AvgChangesPerNode, floatDelta)
	// Hot nodes (>=10): node1 (25) and node2 (10) = 2.
	assert.Equal(t, 2, result.HotNodes)
}

func TestShotnessAggregateMetric_CouplingCount(t *testing.T) {
	t.Parallel()

	m := NewAggregateMetric()
	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1, Type: testNodeType, File: testFile1},
			{Name: testNodeName2, Type: testNodeType, File: testFile1},
		},
		Counters: []map[int]int{
			{0: 10, 1: 5}, // 1 coupling (node1-node2).
			{0: 5, 1: 8},  // Same coupling counted again.
		},
	}

	result := m.Compute(input)

	// Each coupling is counted twice (once from each side), then divided by 2.
	assert.Equal(t, 1, result.TotalCouplings)
}

// --- ComputeAllMetrics Tests ---.

func TestComputeAllMetrics_Empty(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)
	assert.Empty(t, result.NodeHotness)
	assert.Empty(t, result.NodeCoupling)
	assert.Empty(t, result.HotspotNodes)
	assert.Equal(t, 0, result.Aggregate.TotalNodes)
}

func TestComputeAllMetrics_Full(t *testing.T) {
	t.Parallel()

	nodes := []NodeSummary{
		{Name: testNodeName1, Type: testNodeType, File: testFile1},
		{Name: testNodeName2, Type: testNodeType, File: testFile2},
	}
	counters := []map[int]int{
		{0: 25, 1: 10}, // node1: high risk, coupled with node2
		{0: 10, 1: 15}, // node2: medium risk
	}

	report := analyze.Report{
		"Nodes":    nodes,
		"Counters": counters,
	}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)

	// NodeHotness.
	require.Len(t, result.NodeHotness, 2)

	// NodeCoupling.
	require.Len(t, result.NodeCoupling, 1)
	assert.Equal(t, 10, result.NodeCoupling[0].CoChanges)

	// HotspotNodes.
	require.Len(t, result.HotspotNodes, 2)
	assert.Equal(t, "HIGH", result.HotspotNodes[0].RiskLevel)
	assert.Equal(t, "MEDIUM", result.HotspotNodes[1].RiskLevel)

	// Aggregate.
	assert.Equal(t, 2, result.Aggregate.TotalNodes)
	assert.Equal(t, 40, result.Aggregate.TotalChanges)
}

// --- MetricsOutput Interface Tests ---.

func TestComputedMetrics_AnalyzerName(t *testing.T) {
	t.Parallel()

	m := &ComputedMetrics{}

	name := m.AnalyzerName()

	assert.Equal(t, "shotness", name)
}

func TestComputedMetrics_ToJSON(t *testing.T) {
	t.Parallel()

	m := &ComputedMetrics{
		NodeHotness: []NodeHotnessData{{Name: "testFunc"}},
	}

	result := m.ToJSON()

	assert.Equal(t, m, result)
}

func TestComputedMetrics_ToYAML(t *testing.T) {
	t.Parallel()

	m := &ComputedMetrics{
		NodeHotness: []NodeHotnessData{{Name: "testFunc"}},
	}

	result := m.ToYAML()

	assert.Equal(t, m, result)
}
