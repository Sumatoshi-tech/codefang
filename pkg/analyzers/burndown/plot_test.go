package burndown //nolint:testpackage // testing internal implementation.

import (
	"bytes"
	"testing"
	"time"

	"github.com/go-echarts/go-echarts/v2/render"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
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

	b := &HistoryAnalyzer{}
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
		sample := make([]int64, 48) // 48 bands = monthly for 4 years
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

	b := &HistoryAnalyzer{}
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
