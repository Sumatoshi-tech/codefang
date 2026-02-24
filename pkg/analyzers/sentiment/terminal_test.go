package sentiment

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/terminal"
)

func TestRenderTerminal_Empty(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{}

	result := RenderTerminal(metrics)

	assert.Contains(t, result, headerTitle)
	assert.Contains(t, result, sectionSummary)
}

func TestRenderTerminal_WithData(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{
		TimeSeries: []TimeSeriesData{
			{Tick: 0, Sentiment: 0.8, CommentCount: 5, CommitCount: 3, Classification: "positive"},
			{Tick: 1, Sentiment: 0.3, CommentCount: 2, CommitCount: 1, Classification: "negative"},
			{Tick: 2, Sentiment: 0.5, CommentCount: 4, CommitCount: 2, Classification: "neutral"},
		},
		Trend: TrendData{
			StartTick:      0,
			EndTick:        2,
			StartSentiment: 0.7,
			EndSentiment:   0.4,
			TrendDirection: "declining",
			ChangePercent:  -42.86,
		},
		LowSentimentPeriods: []LowSentimentPeriodData{
			{Tick: 1, Sentiment: 0.3, RiskLevel: "MEDIUM"},
		},
		Aggregate: AggregateData{
			TotalTicks:       3,
			TotalComments:    11,
			TotalCommits:     6,
			AverageSentiment: 0.533,
			PositiveTicks:    1,
			NeutralTicks:     1,
			NegativeTicks:    1,
		},
	}

	result := RenderTerminal(metrics)

	assert.Contains(t, result, headerTitle)
	assert.Contains(t, result, sectionSummary)
	assert.Contains(t, result, sectionDistribution)
	assert.Contains(t, result, sectionTrend)
	assert.Contains(t, result, sectionSparkline)
	assert.Contains(t, result, sectionRisk)
	assert.Contains(t, result, "declining")
}

func TestRenderTerminal_NoRiskPeriods(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{
		Aggregate: AggregateData{
			TotalTicks:       1,
			AverageSentiment: 0.8,
			PositiveTicks:    1,
		},
		TimeSeries: []TimeSeriesData{
			{Tick: 0, Sentiment: 0.8, Classification: "positive"},
		},
		Trend: TrendData{
			TrendDirection: "stable",
		},
	}

	result := RenderTerminal(metrics)

	assert.NotContains(t, result, sectionRisk)
}

func TestBuildSparkline(t *testing.T) {
	t.Parallel()

	cfg := terminal.Config{NoColor: true}

	timeSeries := []TimeSeriesData{
		{Sentiment: 0.0},
		{Sentiment: 0.25},
		{Sentiment: 0.5},
		{Sentiment: 0.75},
		{Sentiment: 1.0},
	}

	result := buildSparkline(timeSeries, cfg)

	require.NotEmpty(t, result)
	runes := []rune(result)
	assert.Len(t, runes, len(timeSeries))
}

func TestSentimentColor(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		score    float64
		expected terminal.Color
	}{
		{"positive", 0.8, terminal.ColorGreen},
		{"neutral", 0.5, terminal.ColorYellow},
		{"negative", 0.2, terminal.ColorRed},
		{"boundary_positive", 0.6, terminal.ColorGreen},
		{"boundary_negative", 0.4, terminal.ColorRed},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			assert.Equal(t, tt.expected, sentimentColor(tt.score))
		})
	}
}

func TestSentimentLabel(t *testing.T) {
	t.Parallel()

	assert.Equal(t, positiveEmoji, sentimentLabel(0.8))
	assert.Equal(t, negativeEmoji, sentimentLabel(0.2))
	assert.Equal(t, neutralEmoji, sentimentLabel(0.5))
}
