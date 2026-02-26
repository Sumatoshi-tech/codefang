package halstead

import (
	"bytes"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

func TestGenerateEffortBarChart_InvalidData(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	_, err := analyzer.generateEffortBarChart(analyze.Report{})
	require.Error(t, err)
	assert.ErrorIs(t, err, ErrInvalidFunctionsData)
}

func TestGenerateEffortBarChart_EmptyData(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	report := analyze.Report{"functions": []map[string]any{}}

	chart, err := analyzer.generateEffortBarChart(report)
	require.NoError(t, err)
	require.NotNil(t, chart)
}

func TestGenerateVolumeVsDifficultyChart_InvalidData(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	_, err := analyzer.generateVolumeVsDifficultyChart(analyze.Report{})
	require.Error(t, err)
	assert.ErrorIs(t, err, ErrInvalidFunctionsData)
}

func TestGenerateVolumeVsDifficultyChart_EmptyData(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	report := analyze.Report{"functions": []map[string]any{}}

	chart, err := analyzer.generateVolumeVsDifficultyChart(report)
	require.NoError(t, err)
	require.NotNil(t, chart)
}

func TestGenerateVolumePieChart_EmptyData(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	chart := analyzer.generateVolumePieChart(analyze.Report{})
	require.NotNil(t, chart)
}

func TestGenerateSectionsAndFormatPlot(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()
	report := analyze.Report{
		"functions": []map[string]any{
			{
				"name":           "small",
				"effort":         500.0,
				"volume":         50.0,
				"difficulty":     3.0,
				"delivered_bugs": 0.05,
			},
			{
				"name":           "risky",
				"effort":         75000.0,
				"volume":         7000.0,
				"difficulty":     35.0,
				"delivered_bugs": 1.2,
			},
		},
	}

	sections, err := analyzer.generateSections(report)
	require.NoError(t, err)
	assert.Len(t, sections, 3)

	var buf bytes.Buffer

	err = analyzer.FormatReportPlot(report, &buf)
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "Halstead Complexity Analysis")
}

func TestSortByEffortDescending(t *testing.T) {
	t.Parallel()

	input := []map[string]any{
		{"name": "a", "effort": 100.0},
		{"name": "b", "effort": 300.0},
		{"name": "c", "effort": 200.0},
	}

	got := sortByEffort(input)
	require.Len(t, got, 3)
	assert.Equal(t, "b", got[0]["name"])
	assert.Equal(t, "c", got[1]["name"])
	assert.Equal(t, "a", got[2]["name"])
}

func TestExtractEffortData_UnknownNameFallback(t *testing.T) {
	t.Parallel()

	functions := []map[string]any{
		{"name": "f1", "effort": 100.0},
		{"effort": 20000.0},
	}

	labels, efforts, colors := extractEffortData(functions)
	require.Len(t, labels, 2)
	require.Len(t, efforts, 2)
	require.Len(t, colors, 2)
	assert.Equal(t, "f1", labels[0])
	assert.Equal(t, "unknown", labels[1])
	assert.Equal(t, "#91cc75", colors[0])
	assert.Equal(t, "#ee6666", colors[1])
}

func TestGetEffortColorThresholds(t *testing.T) {
	t.Parallel()

	assert.Equal(t, "#91cc75", getEffortColor(100.0))
	assert.Equal(t, "#fac858", getEffortColor(5000.0))
	assert.Equal(t, "#ee6666", getEffortColor(50000.0))
}

func TestCountVolumeDistribution(t *testing.T) {
	t.Parallel()

	functions := []map[string]any{
		{"volume": 50.0},
		{"volume": 500.0},
		{"volume": 2000.0},
		{"volume": 9000.0},
	}

	dist := countVolumeDistribution(functions)
	assert.Equal(t, 1, dist["Low"])
	assert.Equal(t, 1, dist["Medium"])
	assert.Equal(t, 1, dist["High"])
	assert.Equal(t, 1, dist["Very High"])
}

func TestClassifyScatterRisk(t *testing.T) {
	t.Parallel()

	assert.Equal(t, riskLow, classifyScatterRisk(50, 2, 0.01))
	assert.Equal(t, riskMedium, classifyScatterRisk(1200, 8, 0.2))
	assert.Equal(t, riskMedium, classifyScatterRisk(100, 20, 0.1))
	assert.Equal(t, riskHigh, classifyScatterRisk(7000, 40, 2.0))
}
