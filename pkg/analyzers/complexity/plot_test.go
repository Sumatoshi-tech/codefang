package complexity

import (
	"bytes"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func testPlotFunctions() []map[string]any {
	return []map[string]any{
		{
			"name":                  "Alpha",
			"cyclomatic_complexity": 12,
			"cognitive_complexity":  8,
			"nesting_depth":         3,
		},
		{
			"name":                  "Beta",
			"cyclomatic_complexity": 6,
			"cognitive_complexity":  4,
			"nesting_depth":         2,
		},
		{
			"name":                  "Gamma",
			"cyclomatic_complexity": 3,
			"cognitive_complexity":  2,
			"nesting_depth":         1,
		},
	}
}

func TestFormatReportPlot_GeneratesHTML(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	report := analyze.Report{
		"functions": testPlotFunctions(),
	}

	var out bytes.Buffer

	err := analyzer.FormatReportPlot(report, &out)
	require.NoError(t, err)

	html := out.String()
	assert.Contains(t, html, "Top Complex Functions")
	assert.Contains(t, html, "Cyclomatic vs Cognitive Complexity")
	assert.Contains(t, html, "Complexity Distribution")
}

func TestFormatReportPlot_InvalidFunctionsData(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	var out bytes.Buffer

	err := analyzer.FormatReportPlot(analyze.Report{}, &out)
	require.Error(t, err)
	assert.ErrorIs(t, err, ErrInvalidFunctionsData)
}

func TestGeneratePlotCharts_EmptyFunctions(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	report := analyze.Report{
		"functions": []map[string]any{},
	}

	bar, barErr := analyzer.generateComplexityBarChart(report)
	require.NoError(t, barErr)
	require.NotNil(t, bar)

	scatter, scatterErr := analyzer.generateComplexityScatterChart(report)
	require.NoError(t, scatterErr)
	require.NotNil(t, scatter)

	pie := analyzer.generateComplexityPieChart(report)
	require.NotNil(t, pie)
}

func TestGenerateComplexityBarChart_FunctionComplexityFallback(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	report := analyze.Report{
		"function_complexity": testPlotFunctions(),
	}

	bar, err := analyzer.generateComplexityBarChart(report)
	require.NoError(t, err)
	require.NotNil(t, bar)
}

func TestPlotHelpers_SortingAndColorAndDistribution(t *testing.T) {
	t.Parallel()

	functions := []map[string]any{
		{"name": "Low", "cyclomatic_complexity": 2, "cognitive_complexity": 1, "nesting_depth": 1},
		{"name": "High", "cyclomatic_complexity": 11, "cognitive_complexity": 9, "nesting_depth": 4},
		{"name": "Mid", "cyclomatic_complexity": 6, "cognitive_complexity": 5, "nesting_depth": 2},
	}

	sorted := sortByComplexity(functions)
	require.Len(t, sorted, 3)
	assert.Equal(t, "High", sorted[0]["name"])
	assert.Equal(t, "Mid", sorted[1]["name"])
	assert.Equal(t, "Low", sorted[2]["name"])

	labels, cyclomatic, cognitive, colors := extractComplexityData(sorted)
	assert.Equal(t, []string{"High", "Mid", "Low"}, labels)
	assert.Equal(t, []int{11, 6, 2}, cyclomatic)
	assert.Equal(t, []int{9, 5, 1}, cognitive)
	assert.Equal(t, "#ee6666", colors[0])
	assert.Equal(t, "#fac858", colors[1])
	assert.Equal(t, "#91cc75", colors[2])

	distribution := countComplexityDistribution(functions)
	assert.Equal(t, 1, distribution["Simple"])
	assert.Equal(t, 1, distribution["Moderate"])
	assert.Equal(t, 1, distribution["Complex"])

	assert.Equal(t, "#91cc75", getComplexityColor(5))
	assert.Equal(t, "#fac858", getComplexityColor(8))
	assert.Equal(t, "#ee6666", getComplexityColor(11))
}

func TestRegisterPlotSections_CanBeCalled(t *testing.T) {
	t.Parallel()

	// Smoke test for global registration hook.
	RegisterPlotSections()
}
