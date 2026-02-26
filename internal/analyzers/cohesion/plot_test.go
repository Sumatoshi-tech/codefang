package cohesion

import (
	"bytes"
	"testing"

	"github.com/go-echarts/go-echarts/v2/render"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

func TestGenerateHistogram_WithData(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	report := analyze.Report{
		"functions": []map[string]any{
			{"name": "processData", "cohesion": 0.8},
			{"name": "handleError", "cohesion": 0.5},
			{"name": "validateInput", "cohesion": 0.2},
		},
	}

	bar, err := analyzer.generateHistogram(report)
	require.NoError(t, err)
	require.NotNil(t, bar)

	var buf bytes.Buffer

	renderer := render.NewChartRender(bar)
	err = renderer.Render(&buf)
	require.NoError(t, err)
	assert.Positive(t, buf.Len())
}

func TestGenerateHistogram_EmptyFunctions(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	report := analyze.Report{
		"functions": []map[string]any{},
	}

	bar, err := analyzer.generateHistogram(report)
	require.NoError(t, err)
	require.NotNil(t, bar, "empty function list should return empty chart")
}

func TestGenerateHistogram_InvalidReport(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	report := analyze.Report{
		"total_functions": 0,
	}

	_, err := analyzer.generateHistogram(report)
	require.Error(t, err)
	assert.ErrorIs(t, err, ErrInvalidFunctions)
}

func TestGenerateHistogram_FallbackKey(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	report := analyze.Report{
		"function_cohesion": []map[string]any{
			{"name": "fnA", "cohesion": 0.7},
		},
	}

	bar, err := analyzer.generateHistogram(report)
	require.NoError(t, err)
	require.NotNil(t, bar)
}

func TestGeneratePieChart_WithData(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	report := analyze.Report{
		"functions": []map[string]any{
			{"name": "fn1", "cohesion": 0.9},
			{"name": "fn2", "cohesion": 0.5},
			{"name": "fn3", "cohesion": 0.35},
			{"name": "fn4", "cohesion": 0.1},
		},
	}

	pie := analyzer.generatePieChart(report)
	require.NotNil(t, pie)

	var buf bytes.Buffer

	renderer := render.NewChartRender(pie)
	err := renderer.Render(&buf)
	require.NoError(t, err)
	assert.Positive(t, buf.Len())
}

func TestGeneratePieChart_NoFunctions(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	report := analyze.Report{}
	pie := analyzer.generatePieChart(report)
	require.NotNil(t, pie, "should return empty pie chart")
}

func TestGeneratePieChart_EmptyFunctions(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	report := analyze.Report{
		"functions": []map[string]any{},
	}

	pie := analyzer.generatePieChart(report)
	require.NotNil(t, pie, "should return empty pie chart for empty list")
}

func TestGetCohesionValue(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		fn       map[string]any
		expected float64
	}{
		{"valid float", map[string]any{"cohesion": 0.75}, 0.75},
		{"missing key", map[string]any{}, 0.0},
		{"wrong type", map[string]any{"cohesion": "high"}, 0.0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			result := getCohesionValue(tt.fn)
			assert.InDelta(t, tt.expected, result, 0.001)
		})
	}
}

func TestExtractScores(t *testing.T) {
	t.Parallel()

	functions := []map[string]any{
		{"name": "fn1", "cohesion": 0.8},
		{"name": "fn2", "cohesion": 0.3},
		{"cohesion": 0.5},
	}

	scores := extractScores(functions)

	require.Len(t, scores, 3)
	assert.InDelta(t, 0.8, scores[0], 0.001)
	assert.InDelta(t, 0.3, scores[1], 0.001)
	assert.InDelta(t, 0.5, scores[2], 0.001)
}

func TestBinScores(t *testing.T) {
	t.Parallel()

	scores := []float64{0.0, 0.05, 0.15, 0.5, 0.55, 0.7, 0.8, 0.95}
	bins := binScores(scores)

	require.Len(t, bins, 10)

	// Bin 0: [0.0–0.1) should have 2 (0.0, 0.05).
	assert.Equal(t, 2, bins[0].Count)
	assert.Equal(t, "0.0–0.1", bins[0].Label)

	// Bin 1: [0.1–0.2) should have 1 (0.15).
	assert.Equal(t, 1, bins[1].Count)

	// Bin 5: [0.5–0.6) should have 2 (0.5, 0.55).
	assert.Equal(t, 2, bins[5].Count)

	// Bin 6+7: 0.7 may fall in bin 6 due to float precision.
	assert.Equal(t, 1, bins[6].Count+bins[7].Count)

	// Bin 8: [0.8–0.9) should have 1 (0.8).
	assert.Equal(t, 1, bins[8].Count)

	// Bin 9: [0.9–1.0] should have 1 (0.95).
	assert.Equal(t, 1, bins[9].Count)

	// Check colors: low bins should be red, high bins green.
	assert.Equal(t, "#ee6666", bins[0].Color) // 0.0-0.1 midpoint 0.05 = poor.
	assert.Equal(t, "#91cc75", bins[9].Color) // 0.9-1.0 midpoint 0.95 = excellent.
}

func TestBinScores_Empty(t *testing.T) {
	t.Parallel()

	bins := binScores([]float64{})

	require.Len(t, bins, 10)

	for _, b := range bins {
		assert.Equal(t, 0, b.Count)
	}
}

func TestBinScores_AllSameBin(t *testing.T) {
	t.Parallel()

	scores := []float64{0.01, 0.02, 0.03, 0.04}
	bins := binScores(scores)

	assert.Equal(t, 4, bins[0].Count)

	for i := 1; i < 10; i++ {
		assert.Equal(t, 0, bins[i].Count)
	}
}

func TestBinScores_BoundaryValue(t *testing.T) {
	t.Parallel()

	// Score of exactly 1.0 should go into last bin.
	bins := binScores([]float64{1.0})
	assert.Equal(t, 1, bins[9].Count)
}

func TestCreateHistogramChart(t *testing.T) {
	t.Parallel()

	bins := []histogramBin{
		{Label: "0.0–0.1", Count: 5, Color: "#ee6666"},
		{Label: "0.1–0.2", Count: 3, Color: "#ee6666"},
		{Label: "0.9–1.0", Count: 10, Color: "#91cc75"},
	}

	co := plotpage.DefaultChartOpts()
	bar := createHistogramChart(bins, co)
	require.NotNil(t, bar)

	var buf bytes.Buffer

	renderer := render.NewChartRender(bar)
	err := renderer.Render(&buf)
	require.NoError(t, err)
	assert.Positive(t, buf.Len())
}

func TestGetCohesionColor(t *testing.T) {
	t.Parallel()

	tests := []struct {
		cohesion float64
		expected string
	}{
		{0.9, "#91cc75"},  // Excellent (green).
		{0.6, "#91cc75"},  // Excellent boundary.
		{0.5, "#fac858"},  // Good (yellow).
		{0.4, "#fac858"},  // Good boundary.
		{0.35, "#fd8c73"}, // Fair (orange).
		{0.3, "#fd8c73"},  // Fair boundary.
		{0.2, "#ee6666"},  // Poor (red).
		{0.0, "#ee6666"},  // Zero.
	}

	for _, tt := range tests {
		result := getCohesionColor(tt.cohesion)
		assert.Equal(t, tt.expected, result, "cohesion=%.2f", tt.cohesion)
	}
}

func TestCountCohesionDistribution(t *testing.T) {
	t.Parallel()

	functions := []map[string]any{
		{"name": "fn1", "cohesion": 0.9},  // Excellent.
		{"name": "fn2", "cohesion": 0.6},  // Excellent.
		{"name": "fn3", "cohesion": 0.5},  // Good.
		{"name": "fn4", "cohesion": 0.4},  // Good.
		{"name": "fn5", "cohesion": 0.35}, // Fair.
		{"name": "fn6", "cohesion": 0.1},  // Poor.
	}

	dist := countCohesionDistribution(functions)

	assert.Equal(t, 2, dist["Excellent"])
	assert.Equal(t, 2, dist["Good"])
	assert.Equal(t, 1, dist["Fair"])
	assert.Equal(t, 1, dist["Poor"])
}

func TestCreateEmptyCohesionChart(t *testing.T) {
	t.Parallel()

	bar := createEmptyCohesionChart()
	require.NotNil(t, bar)

	var buf bytes.Buffer

	renderer := render.NewChartRender(bar)
	err := renderer.Render(&buf)
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "Function Cohesion")
	assert.Contains(t, buf.String(), "No data")
}

func TestCreateEmptyPieChart(t *testing.T) {
	t.Parallel()

	pie := createEmptyPieChart()
	require.NotNil(t, pie)

	var buf bytes.Buffer

	renderer := render.NewChartRender(pie)
	err := renderer.Render(&buf)
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "Cohesion Distribution")
	assert.Contains(t, buf.String(), "No data")
}

func TestFormatReportPlot_WithData(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	report := analyze.Report{
		"functions": []map[string]any{
			{"name": "fn1", "cohesion": 0.8},
			{"name": "fn2", "cohesion": 0.3},
		},
	}

	var buf bytes.Buffer

	err := analyzer.FormatReportPlot(report, &buf)
	require.NoError(t, err)
	assert.Positive(t, buf.Len())
	assert.Contains(t, buf.String(), "Code Cohesion Analysis")
}

func TestFormatReportPlot_InvalidReport(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	report := analyze.Report{
		"total_functions": 5,
	}

	var buf bytes.Buffer

	err := analyzer.FormatReportPlot(report, &buf)
	require.Error(t, err)
	assert.ErrorIs(t, err, ErrInvalidFunctions)
}

func TestGenerateSections_WithData(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	report := analyze.Report{
		"functions": []map[string]any{
			{"name": "fn1", "cohesion": 0.9},
			{"name": "fn2", "cohesion": 0.4},
		},
	}

	sections, err := analyzer.generateSections(report)
	require.NoError(t, err)
	require.Len(t, sections, 3)
	assert.Equal(t, "Cohesion Score Distribution", sections[0].Title)
	assert.Equal(t, "Cohesion Distribution", sections[1].Title)
	assert.Equal(t, "Cohesion by Package", sections[2].Title)
	assert.NotNil(t, sections[0].Chart)
	assert.NotNil(t, sections[1].Chart)
	assert.NotNil(t, sections[2].Chart)
	assert.NotEmpty(t, sections[0].Hint.Items)
	assert.NotEmpty(t, sections[1].Hint.Items)
	assert.NotEmpty(t, sections[2].Hint.Items)
}

func TestGenerateSections_InvalidReport(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	_, err := analyzer.generateSections(analyze.Report{})
	require.Error(t, err)
}

func TestCreateCohesionPieChart(t *testing.T) {
	t.Parallel()

	distribution := map[string]int{
		"Excellent": 5,
		"Good":      3,
		"Fair":      2,
		"Poor":      1,
	}

	pie := createCohesionPieChart(distribution)
	require.NotNil(t, pie)

	var buf bytes.Buffer

	renderer := render.NewChartRender(pie)
	err := renderer.Render(&buf)
	require.NoError(t, err)
	assert.Positive(t, buf.Len())
}

// Box plot tests.

func TestGenerateBoxPlot_WithData(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	report := analyze.Report{
		"functions": []map[string]any{
			{"name": "fnA", "cohesion": 0.2, "_source_file": "/repo/pkg/auth/handler.go"},
			{"name": "fnB", "cohesion": 0.3, "_source_file": "/repo/pkg/auth/middleware.go"},
			{"name": "fnC", "cohesion": 0.5, "_source_file": "/repo/pkg/auth/token.go"},
			{"name": "fnD", "cohesion": 0.8, "_source_file": "/repo/pkg/api/server.go"},
			{"name": "fnE", "cohesion": 0.9, "_source_file": "/repo/pkg/api/routes.go"},
			{"name": "fnF", "cohesion": 0.7, "_source_file": "/repo/pkg/api/handler.go"},
		},
	}

	bp := analyzer.generateBoxPlot(report)
	require.NotNil(t, bp)

	var buf bytes.Buffer

	renderer := render.NewChartRender(bp)
	err := renderer.Render(&buf)
	require.NoError(t, err)
	assert.Positive(t, buf.Len())
}

func TestGenerateBoxPlot_NoSourceFile(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	report := analyze.Report{
		"functions": []map[string]any{
			{"name": "fnA", "cohesion": 0.5},
			{"name": "fnB", "cohesion": 0.3},
		},
	}

	bp := analyzer.generateBoxPlot(report)
	require.NotNil(t, bp)

	var buf bytes.Buffer

	renderer := render.NewChartRender(bp)
	err := renderer.Render(&buf)
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "No package data available")
}

func TestGenerateBoxPlot_EmptyFunctions(t *testing.T) {
	t.Parallel()

	analyzer := NewAnalyzer()

	report := analyze.Report{
		"functions": []map[string]any{},
	}

	bp := analyzer.generateBoxPlot(report)
	require.NotNil(t, bp)
}

func TestGroupByDirectory(t *testing.T) {
	t.Parallel()

	functions := []map[string]any{
		{"cohesion": 0.1, "_source_file": "/repo/pkg/bad/a.go"},
		{"cohesion": 0.2, "_source_file": "/repo/pkg/bad/b.go"},
		{"cohesion": 0.3, "_source_file": "/repo/pkg/bad/c.go"},
		{"cohesion": 0.7, "_source_file": "/repo/pkg/good/a.go"},
		{"cohesion": 0.8, "_source_file": "/repo/pkg/good/b.go"},
		{"cohesion": 0.9, "_source_file": "/repo/pkg/good/c.go"},
	}

	groups := groupByDirectory(functions)
	require.Len(t, groups, 2)

	// Sorted by median ascending (worst first).
	assert.Equal(t, "repo/pkg/bad", groups[0].Label)
	assert.Equal(t, "repo/pkg/good", groups[1].Label)
}

func TestGroupByDirectory_MinGroupSize(t *testing.T) {
	t.Parallel()

	functions := []map[string]any{
		{"cohesion": 0.5, "_source_file": "/repo/pkg/small/a.go"},
		{"cohesion": 0.6, "_source_file": "/repo/pkg/small/b.go"},
		// Only 2 functions — below minGroupSize=3.
	}

	groups := groupByDirectory(functions)
	assert.Empty(t, groups)
}

func TestShortenDirectory(t *testing.T) {
	t.Parallel()

	tests := []struct {
		input    string
		expected string
	}{
		{"/home/user/repo/pkg/auth", "repo/pkg/auth"},
		{"/repo/a/b/c/d/e", "c/d/e"},
		{"pkg/auth", "pkg/auth"},
		{"auth", "auth"},
	}

	for _, tt := range tests {
		result := shortenDirectory(tt.input)
		assert.Equal(t, tt.expected, result, "input=%s", tt.input)
	}
}

func TestBoxStats(t *testing.T) {
	t.Parallel()

	sorted := []float64{0.1, 0.2, 0.3, 0.5, 0.9}
	stats := boxStats(sorted)

	assert.InDelta(t, 0.1, stats[0], 0.001) // Min.
	assert.InDelta(t, 0.2, stats[1], 0.001) // Q1.
	assert.InDelta(t, 0.3, stats[2], 0.001) // Median.
	assert.InDelta(t, 0.5, stats[3], 0.001) // Q3.
	assert.InDelta(t, 0.9, stats[4], 0.001) // Max.
}

func TestBoxStats_Empty(t *testing.T) {
	t.Parallel()

	stats := boxStats([]float64{})
	assert.Equal(t, [5]float64{}, stats)
}

func TestPercentile(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		sorted   []float64
		p        float64
		expected float64
	}{
		{"single element", []float64{0.5}, 0.25, 0.5},
		{"single element median", []float64{0.5}, 0.50, 0.5},
		{"two elements Q1", []float64{0.2, 0.8}, 0.25, 0.35},
		{"two elements median", []float64{0.2, 0.8}, 0.50, 0.5},
		{"two elements Q3", []float64{0.2, 0.8}, 0.75, 0.65},
		{"four elements median", []float64{0.1, 0.3, 0.5, 0.7}, 0.50, 0.4},
		{"empty", []float64{}, 0.50, 0.0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			result := percentile(tt.sorted, tt.p)
			assert.InDelta(t, tt.expected, result, 0.001)
		})
	}
}

func TestBuildBoxPlotChart(t *testing.T) {
	t.Parallel()

	groups := []directoryGroup{
		{Label: "pkg/bad", Scores: []float64{0.1, 0.2, 0.3}},
		{Label: "pkg/good", Scores: []float64{0.7, 0.8, 0.9}},
	}

	bp := buildBoxPlotChart(groups)
	require.NotNil(t, bp)

	var buf bytes.Buffer

	renderer := render.NewChartRender(bp)
	err := renderer.Render(&buf)
	require.NoError(t, err)
	assert.Positive(t, buf.Len())
}

func TestCreateEmptyBoxPlot(t *testing.T) {
	t.Parallel()

	bp := createEmptyBoxPlot()
	require.NotNil(t, bp)

	var buf bytes.Buffer

	renderer := render.NewChartRender(bp)
	err := renderer.Render(&buf)
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "Cohesion by Package")
	assert.Contains(t, buf.String(), "No package data available")
}
