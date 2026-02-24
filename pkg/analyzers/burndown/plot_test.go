package burndown

import (
	"bytes"
	"encoding/json"
	"testing"
	"time"

	"github.com/go-echarts/go-echarts/v2/render"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

func TestBandLabel(t *testing.T) {
	t.Parallel()

	tick := 24 * time.Hour
	ref2025 := time.Date(2025, 1, 15, 0, 0, 0, 0, time.UTC)

	makeParams := func(granularity int, numBands int, endTime time.Time) *burndownParams {
		history := make(DenseHistory, 1)
		history[0] = make([]int64, numBands)

		return &burndownParams{
			globalHistory: history,
			granularity:   granularity,
			tickSize:      tick,
			endTime:       endTime,
		}
	}

	tests := []struct {
		bandIdx int
		params  *burndownParams
		want    string
	}{
		{0, makeParams(30, 3, time.Time{}), "1mo"},
		{1, makeParams(30, 3, time.Time{}), "2mo"},
		{2, makeParams(30, 3, time.Time{}), "3mo"},
		{11, makeParams(30, 36, time.Time{}), "1y"},
		{23, makeParams(30, 36, time.Time{}), "2y"},
		{35, makeParams(30, 36, time.Time{}), "3y"},
		{0, makeParams(30, 36, time.Time{}), "1mo"},
		{11, makeParams(30, 36, ref2025), "2024"},
		{23, makeParams(30, 36, ref2025), "2023"},
		{35, makeParams(30, 36, ref2025), "2022"},
	}
	for _, tt := range tests {
		got := bandLabel(tt.bandIdx, tt.params)
		require.Equal(t, tt.want, got, "bandIdx=%d granularity=%d", tt.bandIdx, tt.params.granularity)
	}
}

func TestGenerateChart_WithData(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"GlobalHistory": DenseHistory{
			{100, 50, 25},
			{120, 60, 30},
		},
		"Sampling":    30,
		"Granularity": 30,
		"TickSize":    24 * time.Hour,
	}

	b := NewHistoryAnalyzer()
	chart, err := b.GenerateChart(report)
	require.NoError(t, err)
	require.NotNil(t, chart)

	var buf bytes.Buffer

	renderer := render.NewChartRender(chart)
	err = renderer.Render(&buf)
	require.NoError(t, err)
	require.Greater(t, buf.Len(), 100, "rendered HTML should have substantial content")
	require.Contains(t, buf.String(), "project x 210", "title should include project and max lines")
	require.Contains(t, buf.String(), "granularity 30", "title should include granularity")
	require.Contains(t, buf.String(), "sampling 30", "title should include sampling")
	require.Contains(t, buf.String(), "transparent", "chart should have transparent background")
	require.Contains(t, buf.String(), "1mo", "legend should use month labels for short history")
}

func TestGenerateChart_YearAggregation(t *testing.T) {
	t.Parallel()

	// 48 samples, sampling 30 = span ~4 years. EndTime Jan 2025.
	// At sample 0: repo start (~2021). At sample 47: endTime (2025).
	endTime := time.Date(2025, 1, 15, 0, 0, 0, 0, time.UTC)

	globalHistory := make(DenseHistory, 48)
	for i := range 48 {
		sample := make([]int64, 48) // 48 bands = monthly for 4 years.
		for b := range 48 {
			sample[b] = int64(100 * (b + 1))
		}

		globalHistory[i] = sample
	}

	report := analyze.Report{
		"GlobalHistory": globalHistory,
		"Sampling":      30,
		"Granularity":   30,
		"TickSize":      24 * time.Hour,
		"EndTime":       endTime,
		"ProjectName":   "codefang",
	}

	b := NewHistoryAnalyzer()
	chart, err := b.GenerateChart(report)
	require.NoError(t, err)
	require.NotNil(t, chart)

	var buf bytes.Buffer

	renderer := render.NewChartRender(chart)
	err = renderer.Render(&buf)
	require.NoError(t, err)

	html := buf.String()
	require.Contains(t, html, "codefang x ", "title should use project name")
	require.Contains(t, html, "2021", "legend should show year 2021")
	require.Contains(t, html, "2024", "legend should show year 2024")
}

func TestGenerateChart_EmptyReport(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	_, err := b.GenerateChart(analyze.Report{})
	require.Error(t, err)
	assert.Contains(t, err.Error(), "expected DenseHistory")
}

func TestGenerateChart_SingleSample(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"GlobalHistory": DenseHistory{{100, 50}},
		"Sampling":      30,
		"Granularity":   30,
		"TickSize":      24 * time.Hour,
	}

	b := NewHistoryAnalyzer()
	chart, err := b.GenerateChart(report)
	require.NoError(t, err)
	require.NotNil(t, chart)
}

func TestGenerateSections_WithData(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"GlobalHistory": DenseHistory{
			{100, 50, 25},
			{120, 60, 30},
		},
		"Sampling":    30,
		"Granularity": 30,
		"TickSize":    24 * time.Hour,
	}

	b := NewHistoryAnalyzer()
	sections, err := b.GenerateSections(report)
	require.NoError(t, err)
	assert.NotEmpty(t, sections)
}

func TestGenerateSections_EmptyReport(t *testing.T) {
	t.Parallel()

	b := NewHistoryAnalyzer()
	_, err := b.GenerateSections(analyze.Report{})
	require.Error(t, err)
}

func TestInterpolatePoint_ZeroN(t *testing.T) {
	t.Parallel()

	result := interpolatePoint([]float64{100.0}, 0, 1)
	assert.InDelta(t, 100.0, result, 0.001)
}

func TestInterpolatePoint_Boundary(t *testing.T) {
	t.Parallel()

	values := []float64{100.0, 200.0, 300.0}
	// idx = (n-1)*interpolationFactor = 10 hits the last value exactly.
	result := interpolatePoint(values, 10, 3)
	assert.InDelta(t, 300.0, result, 0.001)
}

func TestComputeMaxLines(t *testing.T) {
	t.Parallel()

	h := DenseHistory{
		{100, 50, 25},
		{120, 60, 30},
	}

	result := computeMaxLines(h)
	assert.Equal(t, int64(210), result)
}

func TestComputeMaxLines_Empty(t *testing.T) {
	t.Parallel()

	result := computeMaxLines(DenseHistory{})
	assert.Equal(t, int64(0), result)
}

func TestSortedYears(t *testing.T) {
	t.Parallel()

	data := map[int]bool{2024: true, 2022: true, 2023: true}

	years := sortedYears(data)
	assert.Equal(t, []int{2022, 2023, 2024}, years)
}

func TestCanAggregateByYear(t *testing.T) {
	t.Parallel()

	endTime := time.Date(2025, 1, 15, 0, 0, 0, 0, time.UTC)

	params := &burndownParams{
		granularity: 30,
		tickSize:    24 * time.Hour,
	}

	// No endTime → cannot aggregate.
	short := make(DenseHistory, 2)
	for i := range short {
		short[i] = make([]int64, 6)
	}

	params.globalHistory = short
	assert.False(t, canAggregateByYear(params), "zero endTime should prevent aggregation")

	// With endTime + enough data → can aggregate.
	long := make(DenseHistory, 24)
	for i := range long {
		long[i] = make([]int64, 24)
	}

	params.globalHistory = long
	params.endTime = endTime
	assert.True(t, canAggregateByYear(params), "non-zero endTime + data should allow aggregation")
}

// Additional tests for extraction/helper functions.

func TestExtractParams_ValidReport(t *testing.T) {
	t.Parallel()

	endTime := time.Date(2025, 6, 1, 0, 0, 0, 0, time.UTC)
	report := analyze.Report{
		"GlobalHistory": DenseHistory{
			{100, 50},
			{120, 60},
		},
		"Sampling":    10,
		"Granularity": 5,
		"TickSize":    48 * time.Hour,
		"EndTime":     endTime,
		"ProjectName": "myrepo",
	}

	params := extractParams(report)
	require.NotNil(t, params)
	assert.Equal(t, DenseHistory{{100, 50}, {120, 60}}, params.globalHistory)
	assert.Equal(t, 10, params.sampling)
	assert.Equal(t, 5, params.granularity)
	assert.Equal(t, 48*time.Hour, params.tickSize)
	assert.Equal(t, endTime, params.endTime)
	assert.Equal(t, "myrepo", params.projectName)
}

func TestExtractParams_NilHistory(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"GlobalHistory": "not-a-dense-history",
		"Sampling":      10,
	}

	params := extractParams(report)
	assert.Nil(t, params, "should return nil when GlobalHistory is not DenseHistory and binary fallback also fails")
}

func TestExtractTickSize_Default(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}
	got := extractTickSize(report)
	assert.Equal(t, 24*time.Hour, got, "should return 24h when TickSize key is missing")
}

func TestExtractTickSize_Present(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"TickSize": 12 * time.Hour,
	}
	got := extractTickSize(report)
	assert.Equal(t, 12*time.Hour, got, "should return the value from the report")
}

func TestExtractEndTime_Default(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}
	got := extractEndTime(report)
	assert.True(t, got.IsZero(), "should return zero time when EndTime key is missing")
}

func TestExtractEndTime_Present(t *testing.T) {
	t.Parallel()

	expected := time.Date(2025, 3, 15, 12, 0, 0, 0, time.UTC)
	report := analyze.Report{
		"EndTime": expected,
	}
	got := extractEndTime(report)
	assert.Equal(t, expected, got)
}

func TestExtractProjectName_Default(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}
	got := extractProjectName(report)
	assert.Equal(t, "project", got, "should return 'project' when key is missing")

	report2 := analyze.Report{"ProjectName": ""}
	got2 := extractProjectName(report2)
	assert.Equal(t, "project", got2, "should return 'project' when value is empty string")
}

func TestExtractProjectName_Present(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"ProjectName": "codefang",
	}
	got := extractProjectName(report)
	assert.Equal(t, "codefang", got)
}

func TestExtractInt_Int(t *testing.T) {
	t.Parallel()

	report := analyze.Report{"Sampling": 42}
	got := extractInt(report, "Sampling", 0)
	assert.Equal(t, 42, got)
}

func TestExtractInt_Float64(t *testing.T) {
	t.Parallel()

	report := analyze.Report{"Sampling": float64(99)}
	got := extractInt(report, "Sampling", 0)
	assert.Equal(t, 99, got)
}

func TestExtractInt_Fallback(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}
	got := extractInt(report, "Sampling", 7)
	assert.Equal(t, 7, got, "should return fallback when key is missing")
}

func TestExtractIntFromMap_Float64(t *testing.T) {
	t.Parallel()

	m := map[string]any{"num_samples": float64(15)}
	got := extractIntFromMap(m, "num_samples", 0)
	assert.Equal(t, 15, got)
}

func TestExtractIntFromMap_Int(t *testing.T) {
	t.Parallel()

	m := map[string]any{"num_bands": 8}
	got := extractIntFromMap(m, "num_bands", 0)
	assert.Equal(t, 8, got)
}

func TestExtractIntFromMap_Fallback(t *testing.T) {
	t.Parallel()

	m := map[string]any{}
	got := extractIntFromMap(m, "missing_key", 42)
	assert.Equal(t, 42, got, "should return fallback when key is missing")
}

func TestToInt64_Float64(t *testing.T) {
	t.Parallel()

	got := toInt64(float64(123.7))
	assert.Equal(t, int64(123), got)
}

func TestToInt64_Int64(t *testing.T) {
	t.Parallel()

	got := toInt64(int64(456))
	assert.Equal(t, int64(456), got)
}

func TestToInt64_Int(t *testing.T) {
	t.Parallel()

	got := toInt64(int(789))
	assert.Equal(t, int64(789), got)
}

func TestToInt64_JSONNumber(t *testing.T) {
	t.Parallel()

	num := json.Number("1024")
	got := toInt64(num)
	assert.Equal(t, int64(1024), got)
}

func TestToInt64_Unknown(t *testing.T) {
	t.Parallel()

	got := toInt64("not-a-number")
	assert.Equal(t, int64(0), got, "unsupported type should return 0")
}

func TestExtractDenseHistoryFromBinary_Missing(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}
	got := extractDenseHistoryFromBinary(report)
	assert.Nil(t, got, "should return nil when key is not present")
}

func TestExtractDenseHistoryFromBinary_Nil(t *testing.T) {
	t.Parallel()

	report := analyze.Report{"global_survival": nil}
	got := extractDenseHistoryFromBinary(report)
	require.NotNil(t, got)
	assert.Empty(t, got, "should return empty DenseHistory when value is nil")
}

func TestExtractDenseHistoryFromBinary_EmptyList(t *testing.T) {
	t.Parallel()

	report := analyze.Report{"global_survival": []any{}}
	got := extractDenseHistoryFromBinary(report)
	require.NotNil(t, got)
	assert.Empty(t, got, "should return empty DenseHistory when list is empty")
}

func TestExtractDenseHistoryFromBinary_Valid(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"global_survival": []any{
			map[string]any{
				"band_breakdown": []any{float64(10), float64(20), float64(30)},
			},
			map[string]any{
				"band_breakdown": []any{float64(40), float64(50)},
			},
		},
	}

	got := extractDenseHistoryFromBinary(report)
	require.NotNil(t, got)
	require.Len(t, got, 2)
	assert.Equal(t, []int64{10, 20, 30}, got[0])
	assert.Equal(t, []int64{40, 50}, got[1])
}

func TestExtractSamplingGranularity_Direct(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"Sampling":    30,
		"Granularity": 15,
	}

	s, g := extractSamplingGranularity(report)
	assert.Equal(t, 30, s)
	assert.Equal(t, 15, g)
}

func TestExtractSamplingGranularity_FromAggregate(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"aggregate": map[string]any{
			"num_samples": float64(10),
			"num_bands":   float64(5),
		},
	}

	s, g := extractSamplingGranularity(report)
	assert.Equal(t, 1, s, "sampling should be inferred as 1 from aggregate")
	assert.Equal(t, 1, g, "granularity should be inferred as 1 from aggregate")
}

func TestBuildXLabels(t *testing.T) {
	t.Parallel()

	params := &burndownParams{
		globalHistory: DenseHistory{
			{100, 50},
			{120, 60},
			{130, 70},
		},
		sampling: 30,
		tickSize: 24 * time.Hour,
	}

	labels := buildXLabels(params)
	// n=3, points = (3-1)*5 + 1 = 11.
	require.Len(t, labels, 11)
	assert.Equal(t, "0d", labels[0], "first label should be 0d")
	assert.Equal(t, "60d", labels[len(labels)-1], "last label should be 60d")
}

func TestInterpolateShort(t *testing.T) {
	t.Parallel()

	values := []float64{100.0}
	out := interpolateShort(values)
	require.Len(t, out, 1)
	assert.Equal(t, int64(100), out[0].Value, "single value should be rounded correctly")
}

func TestInterpolateFull(t *testing.T) {
	t.Parallel()

	values := []float64{0.0, 100.0, 200.0}
	out := interpolateFull(values)
	// points = (3-1)*5 + 1 = 11.
	require.Len(t, out, 11)
	assert.Equal(t, int64(0), out[0].Value, "first point should be 0")
	assert.Equal(t, int64(200), out[10].Value, "last point should be 200")
}

func TestInterpolatePoint_Negative(t *testing.T) {
	t.Parallel()

	// Construct values where linear interpolation produces a negative number.
	// Between index 0 (value=10) and index 1 (value=-50), at frac ~0.5 the
	// interpolation would be 10*0.5 + (-50)*0.5 = -20, which should clamp to 0.
	values := []float64{10.0, -50.0}
	// idx=3 => subIdx=3/5=0.6, lo=0, frac=0.6
	// val = 10*(1-0.6) + (-50)*0.6 = 4 - 30 = -26 => clamped to 0.
	result := interpolatePoint(values, 3, 2)
	assert.InDelta(t, 0.0, result, 0.001, "negative interpolated values should be clamped to 0")
}

func TestGeneratePlot_ValidReport(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"GlobalHistory": DenseHistory{
			{100, 50},
			{120, 60},
		},
		"Sampling":    30,
		"Granularity": 30,
		"TickSize":    24 * time.Hour,
		"ProjectName": "testproj",
	}

	b := NewHistoryAnalyzer()

	var buf bytes.Buffer

	err := b.generatePlot(report, &buf)
	require.NoError(t, err)
	assert.Positive(t, buf.Len(), "plot output should be non-empty")
	assert.Contains(t, buf.String(), "Burndown: testproj", "title should contain project name")
}

func TestCreateEmptyBurndown(t *testing.T) {
	t.Parallel()

	chart := createEmptyBurndown()
	require.NotNil(t, chart, "empty burndown chart should not be nil")

	var buf bytes.Buffer

	renderer := render.NewChartRender(chart)
	err := renderer.Render(&buf)
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "Burndown", "empty chart should contain 'Burndown' title")
	assert.Contains(t, buf.String(), "No data", "empty chart should contain 'No data' subtitle")
}

func TestBuildSummarySection_WithData(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"GlobalHistory": DenseHistory{
			{100, 50, 25},
			{120, 60, 30},
		},
		"Sampling":    30,
		"Granularity": 30,
		"TickSize":    24 * time.Hour,
	}

	section := buildSummarySection(report)
	assert.Equal(t, "Burndown Summary", section.Title)
	assert.NotNil(t, section.Chart)
}

func TestBuildSummarySection_EmptyReport(t *testing.T) {
	t.Parallel()

	section := buildSummarySection(analyze.Report{})
	assert.Equal(t, "Burndown Summary", section.Title)
	assert.NotNil(t, section.Chart)
}

func TestSurvivalBadgeColor(t *testing.T) {
	t.Parallel()

	assert.Equal(t, plotpage.BadgeSuccess, survivalBadgeColor(0.8))
	assert.Equal(t, plotpage.BadgeSuccess, survivalBadgeColor(0.7))
	assert.Equal(t, plotpage.BadgeWarning, survivalBadgeColor(0.6))
	assert.Equal(t, plotpage.BadgeWarning, survivalBadgeColor(0.5))
	assert.Equal(t, plotpage.BadgeError, survivalBadgeColor(0.3))
	assert.Equal(t, plotpage.BadgeError, survivalBadgeColor(0.0))
}

func TestGenerateSections_ReturnsSummaryAndChart(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"GlobalHistory": DenseHistory{
			{100, 50, 25},
			{120, 60, 30},
		},
		"Sampling":    30,
		"Granularity": 30,
		"TickSize":    24 * time.Hour,
	}

	b := NewHistoryAnalyzer()
	sections, err := b.GenerateSections(report)
	require.NoError(t, err)
	require.Len(t, sections, 2)
	assert.Equal(t, "Burndown Summary", sections[0].Title)
	assert.Equal(t, "Code Burndown Chart", sections[1].Title)
}
