package shotness

import (
	"bytes"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func TestExtractShotnessData_TypedInput(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"Nodes": []NodeSummary{
			{Type: "Function", Name: "foo", File: "a.go"},
		},
		"Counters": []map[int]int{
			{0: 5},
		},
	}

	nodes, counters, err := extractShotnessData(report)
	require.NoError(t, err)
	require.Len(t, nodes, 1)
	require.Len(t, counters, 1)
	assert.Equal(t, "foo", nodes[0].Name)
	assert.Equal(t, 5, counters[0][0])
}

func TestExtractShotnessData_MissingNodes(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	_, _, err := extractShotnessData(report)
	require.ErrorIs(t, err, ErrInvalidNodes)
}

func TestExtractShotnessData_MissingCounters(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"Nodes": []NodeSummary{{Name: "f"}},
	}

	_, _, err := extractShotnessData(report)
	require.ErrorIs(t, err, ErrInvalidCounters)
}

func TestExtractShotnessFromJSON_HotnessPath(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"node_hotness": []any{
			map[string]any{
				"name":         "funcA",
				"type":         "Function",
				"file":         "a.go",
				"change_count": float64(10),
			},
			map[string]any{
				"name":         "funcB",
				"type":         "Function",
				"file":         "b.go",
				"change_count": float64(5),
			},
		},
		"node_coupling": []any{
			map[string]any{
				"node1_name": "funcA",
				"node2_name": "funcB",
				"co_changes": float64(3),
			},
		},
	}

	nodes, counters, err := extractShotnessData(report)
	require.NoError(t, err)
	require.Len(t, nodes, 2)
	require.Len(t, counters, 2)
	assert.Equal(t, "funcA", nodes[0].Name)
	assert.Equal(t, 10, counters[0][0])
	assert.Equal(t, 3, counters[0][1])
	assert.Equal(t, 3, counters[1][0])
}

func TestExtractShotnessFromJSON_NilHotness(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"node_hotness": nil,
	}

	nodes, counters, err := extractShotnessData(report)
	require.NoError(t, err)
	assert.Nil(t, nodes)
	assert.Nil(t, counters)
}

func TestExtractShotnessFromJSON_EmptyHotness(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"node_hotness": []any{},
	}

	nodes, counters, err := extractShotnessData(report)
	require.NoError(t, err)
	assert.Nil(t, nodes)
	assert.Nil(t, counters)
}

func TestExtractShotnessFromJSON_InvalidHotnessType(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"node_hotness": "not_a_list",
	}

	_, _, err := extractShotnessData(report)
	require.ErrorIs(t, err, ErrInvalidNodes)
}

func TestShotnessToInt_Float64(t *testing.T) {
	t.Parallel()

	assert.Equal(t, 42, shotnessToInt(float64(42)))
}

func TestShotnessToInt_Int(t *testing.T) {
	t.Parallel()

	assert.Equal(t, 7, shotnessToInt(7))
}

func TestShotnessToInt_Int64(t *testing.T) {
	t.Parallel()

	assert.Equal(t, 99, shotnessToInt(int64(99)))
}

func TestShotnessToInt_Unknown(t *testing.T) {
	t.Parallel()

	assert.Equal(t, 0, shotnessToInt("not_a_number"))
}

func TestAssertString_Valid(t *testing.T) {
	t.Parallel()

	val, ok := assertString(map[string]any{"key": "value"}, "key")
	assert.True(t, ok)
	assert.Equal(t, "value", val)
}

func TestAssertString_Missing(t *testing.T) {
	t.Parallel()

	val, ok := assertString(map[string]any{}, "key")
	assert.False(t, ok)
	assert.Empty(t, val)
}

func TestBuildFileHierarchy(t *testing.T) {
	t.Parallel()

	nodes := []NodeSummary{
		{Name: "f1", File: "a.go"},
		{Name: "f2", File: "a.go"},
		{Name: "f3", File: "b.go"},
	}
	counters := []map[int]int{
		{0: 10},
		{1: 5},
		{2: 3},
	}

	fm, ft := buildFileHierarchy(nodes, counters)
	require.Len(t, fm, 2)
	assert.Equal(t, 15, ft["a.go"])
	assert.Equal(t, 3, ft["b.go"])
}

func TestBuildRootNodes_LimitMaxFiles(t *testing.T) {
	t.Parallel()

	fileMap := make(map[string][]NodeSummary)
	fileTotals := make(map[string]int)

	for i := range maxFiles + 5 {
		fname := "file_" + string(rune('a'+i))
		fileMap[fname] = nil
		fileTotals[fname] = i
	}

	result := buildRootNodes(nil, fileTotals)
	assert.LessOrEqual(t, len(result), maxFiles)
}

func TestGetActiveNodes(t *testing.T) {
	t.Parallel()

	nodes := []NodeSummary{
		{Name: "active"},
		{Name: "inactive"},
	}
	counters := []map[int]int{
		{0: 5},
		{1: 0},
	}

	actives := getActiveNodes(nodes, counters)
	require.Len(t, actives, 1)
	assert.Equal(t, "active", actives[0].name)
}

func TestExtractNames(t *testing.T) {
	t.Parallel()

	actives := []activeNode{
		{name: "a"},
		{name: "b"},
	}

	names := extractNames(actives)
	assert.Equal(t, []string{"a", "b"}, names)
}

func TestBuildHeatMapData(t *testing.T) {
	t.Parallel()

	actives := []activeNode{
		{idx: 0, name: "a", count: 5},
		{idx: 1, name: "b", count: 3},
	}
	counters := []map[int]int{
		{0: 5, 1: 2},
		{0: 2, 1: 3},
	}

	data, maxVal := buildHeatMapData(actives, counters)
	assert.Len(t, data, 4) // 2x2 matrix.
	assert.InDelta(t, 5.0, maxVal, 0.01)
}

func TestComputeScores(t *testing.T) {
	t.Parallel()

	nodes := []NodeSummary{
		{Name: "hot"},
		{Name: "cold"},
	}
	counters := []map[int]int{
		{0: 20, 1: 5},
		{0: 5, 1: 2},
	}

	scores := computeScores(nodes, counters)
	require.Len(t, scores, 2)
	assert.Equal(t, "hot", scores[0].name)
	assert.Equal(t, 20, scores[0].self)
}

func TestBuildBarData(t *testing.T) {
	t.Parallel()

	scores := []nodeScore{
		{name: "a", self: 10, coupled: 5},
		{name: "b", self: 3, coupled: 1},
	}

	labels, selfData, coupledData := buildBarData(scores)
	assert.Equal(t, []string{"a", "b"}, labels)
	assert.Equal(t, []int{10, 3}, selfData)
	assert.Equal(t, []int{5, 1}, coupledData)
}

func TestCreateEmptyChart(t *testing.T) {
	t.Parallel()

	chart := createEmptyChart()
	require.NotNil(t, chart)
}

func TestGenerateChart_WithData(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	report := analyze.Report{
		"Nodes": []NodeSummary{
			{Type: "Function", Name: "foo", File: "a.go"},
			{Type: "Function", Name: "bar", File: "b.go"},
		},
		"Counters": []map[int]int{
			{0: 10, 1: 3},
			{0: 3, 1: 5},
		},
	}

	chart, err := s.GenerateChart(report)
	require.NoError(t, err)
	require.NotNil(t, chart)
}

func TestGenerateChart_Empty(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	report := analyze.Report{
		"Nodes":    []NodeSummary{},
		"Counters": []map[int]int{},
	}

	chart, err := s.GenerateChart(report)
	require.NoError(t, err)
	require.NotNil(t, chart) // Returns empty chart.
}

func TestGenerateSections_WithData(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	report := analyze.Report{
		"Nodes": []NodeSummary{
			{Type: "Function", Name: "foo", File: "a.go"},
			{Type: "Function", Name: "bar", File: "b.go"},
			{Type: "Function", Name: "baz", File: "c.go"},
		},
		"Counters": []map[int]int{
			{0: 10, 1: 3, 2: 1},
			{0: 3, 1: 5, 2: 2},
			{0: 1, 1: 2, 2: 8},
		},
	}

	sections, err := s.GenerateSections(report)
	require.NoError(t, err)
	require.Len(t, sections, 3)
	assert.Equal(t, "Code Hotness TreeMap", sections[0].Title)
	assert.Equal(t, "Function Coupling Matrix", sections[1].Title)
	assert.Equal(t, "Top Hot Functions", sections[2].Title)
}

func TestGenerateSections_Empty(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	report := analyze.Report{
		"Nodes":    []NodeSummary{},
		"Counters": []map[int]int{},
	}

	sections, err := s.GenerateSections(report)
	require.NoError(t, err)
	assert.Nil(t, sections)
}

func TestGeneratePlot_WithData(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	report := analyze.Report{
		"Nodes": []NodeSummary{
			{Type: "Function", Name: "foo", File: "a.go"},
			{Type: "Function", Name: "bar", File: "b.go"},
			{Type: "Function", Name: "baz", File: "c.go"},
		},
		"Counters": []map[int]int{
			{0: 10, 1: 3, 2: 1},
			{0: 3, 1: 5, 2: 2},
			{0: 1, 1: 2, 2: 8},
		},
	}

	var buf bytes.Buffer

	err := s.generatePlot(report, &buf)
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "Shotness Analysis")
}

func TestSerialize_PlotFormat(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	report := analyze.Report{
		"Nodes": []NodeSummary{
			{Type: "Function", Name: "foo", File: "a.go"},
			{Type: "Function", Name: "bar", File: "b.go"},
			{Type: "Function", Name: "baz", File: "c.go"},
		},
		"Counters": []map[int]int{
			{0: 10, 1: 3, 2: 1},
			{0: 3, 1: 5, 2: 2},
			{0: 1, 1: 2, 2: 8},
		},
	}

	var buf bytes.Buffer

	err := s.Serialize(report, analyze.FormatPlot, &buf)
	require.NoError(t, err)
	assert.Positive(t, buf.Len())
}

func TestApplyCouplingData_NoCouplingField(t *testing.T) {
	t.Parallel()

	counters := []map[int]int{{0: 5}, {1: 3}}
	nameToIdx := map[string]int{"a": 0, "b": 1}

	applyCouplingData(analyze.Report{}, counters, nameToIdx)

	// No coupling data -> counters unchanged.
	assert.Equal(t, 5, counters[0][0])
	assert.Equal(t, 3, counters[1][1])
}

func TestApplyCouplingData_InvalidCouplingType(t *testing.T) {
	t.Parallel()

	counters := []map[int]int{{0: 5}}
	nameToIdx := map[string]int{"a": 0}

	report := analyze.Report{"node_coupling": "not_a_list"}
	applyCouplingData(report, counters, nameToIdx)

	assert.Equal(t, 5, counters[0][0])
}
