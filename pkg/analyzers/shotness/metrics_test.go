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
	assert.Equal(t, 10, result.Counters[0][0])
}

// --- NodeHotnessMetric Tests ---.

func TestNodeHotnessMetric_Empty(t *testing.T) {
	t.Parallel()

	input := &ReportData{}

	result := computeNodeHotness(input)

	assert.Empty(t, result)
}

func TestNodeHotnessMetric_ValidData(t *testing.T) {
	t.Parallel()

	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1, Type: testNodeType, File: testFile1},
			{Name: testNodeName2, Type: testNodeType, File: testFile1},
		},
		Counters: []map[int]int{
			{0: 10, 1: 5}, // Node 1: 10 changes, coupled with Node 2.
			{0: 5, 1: 20}, // Node 2: 20 changes (max), coupled with Node 1.
		},
	}

	result := computeNodeHotness(input)

	require.Len(t, result, 2)

	// Node 2 should be first because it has more changes (20 > 10).
	assert.Equal(t, testNodeName2, result[0].Name)
	assert.Equal(t, 20, result[0].ChangeCount)
	assert.Equal(t, 1, result[0].CoupledNodes)
	assert.InDelta(t, 1.0, result[0].HotnessScore, floatDelta)

	// Node 1 should be second.
	assert.Equal(t, testNodeName1, result[1].Name)
	assert.Equal(t, 10, result[1].ChangeCount)
	assert.Equal(t, 1, result[1].CoupledNodes)
	assert.InDelta(t, 0.5, result[1].HotnessScore, floatDelta)
}

func TestNodeHotnessMetric_OutOfBounds(t *testing.T) {
	t.Parallel()

	// More nodes than counters should not panic.
	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1},
			{Name: testNodeName2},
		},
		Counters: []map[int]int{
			{0: 10},
		},
	}

	result := computeNodeHotness(input)

	require.Len(t, result, 1) // Only processed first node.
	assert.Equal(t, testNodeName1, result[0].Name)
}

// --- NodeCouplingMetric Tests ---.

func TestNodeCouplingMetric_Empty(t *testing.T) {
	t.Parallel()

	input := &ReportData{}

	result := computeNodeCoupling(input)

	assert.Empty(t, result)
}

func TestNodeCouplingMetric_ValidData(t *testing.T) {
	t.Parallel()

	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1, Type: testNodeType, File: testFile1},
			{Name: testNodeName2, Type: testNodeType, File: testFile2},
			{Name: testNodeName3, Type: testNodeType, File: testFile2},
		},
		Counters: []map[int]int{
			{0: 10, 1: 5, 2: 2}, // Node 1 coupled with 2 (5 times) and 3 (2 times).
			{0: 5, 1: 20, 2: 8}, // Node 2 coupled with 1 (5 times) and 3 (8 times).
			{0: 2, 1: 8, 2: 5},  // Node 3 coupled with 1 (2 times) and 2 (8 times).
		},
	}

	result := computeNodeCoupling(input)

	// Expected pairs: (1,2), (1,3), (2,3).
	require.Len(t, result, 3)

	// Should be sorted by co-changes descending: (2,3)=8, (1,2)=5, (1,3)=2.
	assert.Equal(t, testNodeName2, result[0].Node1Name)
	assert.Equal(t, testNodeName3, result[0].Node2Name)
	assert.Equal(t, 8, result[0].CoChanges)

	assert.Equal(t, testNodeName1, result[1].Node1Name)
	assert.Equal(t, testNodeName2, result[1].Node2Name)
	assert.Equal(t, 5, result[1].CoChanges)

	assert.Equal(t, testNodeName1, result[2].Node1Name)
	assert.Equal(t, testNodeName3, result[2].Node2Name)
	assert.Equal(t, 2, result[2].CoChanges)
}

func TestNodeCouplingMetric_ZeroCoupling(t *testing.T) {
	t.Parallel()

	// Zero couplings should be omitted.
	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1},
			{Name: testNodeName2},
		},
		Counters: []map[int]int{
			{0: 10, 1: 0},
			{0: 0, 1: 20},
		},
	}

	result := computeNodeCoupling(input)

	assert.Empty(t, result)
}

// --- HotspotNodeMetric Tests ---.

func TestHotspotNodeMetric_Empty(t *testing.T) {
	t.Parallel()

	input := &ReportData{}

	result := computeHotspotNodes(input)

	assert.Empty(t, result)
}

func TestHotspotNodeMetric_ValidData(t *testing.T) {
	t.Parallel()

	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1}, // Low risk.
			{Name: testNodeName2}, // High risk.
			{Name: testNodeName3}, // Medium risk.
		},
		Counters: []map[int]int{
			{0: HotspotThresholdMedium - 1},         // Less than 10.
			{0: 0, 1: HotspotThresholdHigh},         // High risk.
			{0: 0, 1: 0, 2: HotspotThresholdMedium}, // Medium risk.
		},
	}

	result := computeHotspotNodes(input)

	// Low risk node should be filtered out.
	require.Len(t, result, 2)

	// Should be sorted by change count descending.
	assert.Equal(t, testNodeName2, result[0].Name)
	assert.Equal(t, RiskLevelHigh, result[0].RiskLevel)
	assert.Equal(t, HotspotThresholdHigh, result[0].ChangeCount)

	assert.Equal(t, testNodeName3, result[1].Name)
	assert.Equal(t, RiskLevelMedium, result[1].RiskLevel)
	assert.Equal(t, HotspotThresholdMedium, result[1].ChangeCount)
}

// --- AggregateMetric Tests ---.

func TestAggregateMetric_Empty(t *testing.T) {
	t.Parallel()

	input := &ReportData{}

	result := computeAggregate(input)

	assert.Equal(t, 0, result.TotalNodes)
	assert.Equal(t, 0, result.TotalChanges)
	assert.Equal(t, 0, result.TotalCouplings)
	assert.InDelta(t, 0.0, result.AvgChangesPerNode, floatDelta)
	assert.Equal(t, 0, result.HotNodes)
}

func TestAggregateMetric_ValidData(t *testing.T) {
	t.Parallel()

	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1},
			{Name: testNodeName2},
		},
		Counters: []map[int]int{
			{0: HotspotThresholdHigh, 1: 5}, // 20 changes, 1 coupling. High risk.
			{0: 5, 1: 10},                   // 10 changes, 1 coupling. Medium risk.
		},
	}

	result := computeAggregate(input)

	assert.Equal(t, 2, result.TotalNodes)
	assert.Equal(t, HotspotThresholdHigh+10, result.TotalChanges) // Total is 30.
	assert.Equal(t, 1, result.TotalCouplings)                     // Upper triangle only.
	assert.InDelta(t, 15.0, result.AvgChangesPerNode, floatDelta) // Average is 15.0.
	assert.Equal(t, 2, result.HotNodes)                           // Both nodes are hot.
	assert.InDelta(t, 0.25, result.AvgCouplingStrength, floatDelta)
}

func TestClassifyChangeRisk(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		count    int
		expected string
	}{
		{"Low Risk", HotspotThresholdMedium - 1, RiskLevelLow},
		{"Medium Risk Min", HotspotThresholdMedium, RiskLevelMedium},
		{"Medium Risk Max", HotspotThresholdHigh - 1, RiskLevelMedium},
		{"High Risk Min", HotspotThresholdHigh, RiskLevelHigh},
		{"High Risk Max", HotspotThresholdHigh + 100, RiskLevelHigh},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			assert.Equal(t, tt.expected, classifyChangeRisk(tt.count))
		})
	}
}

// --- Coupling Strength Tests ---.

func TestComputeCouplingStrength_Basic(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		co       int
		a        int
		b        int
		expected float64
	}{
		{"equal changes", 5, 5, 5, 1.0},
		{"half coupled", 5, 10, 10, 0.5},
		{"asymmetric", 3, 3, 10, 0.3},
		{"zero max", 0, 0, 0, 0.0},
		{"co exceeds self", 5, 3, 4, 1.0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			result := computeCouplingStrength(tt.co, tt.a, tt.b)
			assert.InDelta(t, tt.expected, result, floatDelta)
		})
	}
}

func TestNodeCouplingMetric_IncludesStrength(t *testing.T) {
	t.Parallel()

	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1, Type: testNodeType, File: testFile1},
			{Name: testNodeName2, Type: testNodeType, File: testFile2},
		},
		Counters: []map[int]int{
			{0: 10, 1: 5},
			{0: 5, 1: 20},
		},
	}

	result := computeNodeCoupling(input)

	require.Len(t, result, 1)
	assert.Equal(t, 5, result[0].CoChanges)
	assert.InDelta(t, 0.25, result[0].Strength, floatDelta)
}

func TestAggregateMetric_IncludesAvgCouplingStrength(t *testing.T) {
	t.Parallel()

	input := &ReportData{
		Nodes: []NodeSummary{
			{Name: testNodeName1},
			{Name: testNodeName2},
		},
		Counters: []map[int]int{
			{0: 10, 1: 5},
			{0: 5, 1: 10},
		},
	}

	result := computeAggregate(input)

	assert.InDelta(t, 0.5, result.AvgCouplingStrength, floatDelta)
}
