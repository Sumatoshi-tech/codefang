package sentiment

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestGenerateSections_Empty(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	report := analyze.Report{}

	sections, err := s.GenerateSections(report)

	require.NoError(t, err)
	require.Len(t, sections, 1)
	assert.Equal(t, chartSectionTitle, sections[0].Title)
}

func TestGenerateSections_WithData(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	report := analyze.Report{
		"comments_by_commit": map[string][]string{
			testHashA: {"This is a great improvement"},
			testHashB: {"Why is this so terrible"},
		},
		"commits_by_tick": map[int][]gitlib.Hash{
			0: {gitlib.NewHash(testHashA)},
			1: {gitlib.NewHash(testHashB)},
		},
	}

	sections, err := s.GenerateSections(report)

	require.NoError(t, err)
	require.Len(t, sections, 2)
	assert.Equal(t, chartSectionTitle, sections[0].Title)
	assert.Equal(t, distributionTitle, sections[1].Title)
	assert.NotNil(t, sections[0].Chart)
	assert.NotNil(t, sections[1].Chart)
	assert.NotEmpty(t, sections[0].Hint.Items)
}

func TestGenerateChart_WithData(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	report := analyze.Report{
		"comments_by_commit": map[string][]string{
			testHashA: {"Excellent work"},
		},
		"commits_by_tick": map[int][]gitlib.Hash{
			0: {gitlib.NewHash(testHashA)},
		},
	}

	chart, err := s.GenerateChart(report)

	require.NoError(t, err)
	assert.NotNil(t, chart)
}

func TestGenerateChart_Empty(t *testing.T) {
	t.Parallel()

	s := NewAnalyzer()
	report := analyze.Report{}

	chart, err := s.GenerateChart(report)

	require.NoError(t, err)
	assert.NotNil(t, chart)
}

func TestBuildDistributionChart(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{
		Aggregate: AggregateData{
			TotalTicks:    3,
			PositiveTicks: 1,
			NeutralTicks:  1,
			NegativeTicks: 1,
		},
	}

	chart := buildDistributionChart(metrics)
	assert.NotNil(t, chart)
}

func TestBuildMainChartHint(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{
		Trend: TrendData{
			TrendDirection: "improving",
			ChangePercent:  25.0,
		},
		LowSentimentPeriods: []LowSentimentPeriodData{
			{Tick: 1, Sentiment: 0.2, RiskLevel: "HIGH"},
		},
	}

	hint := buildMainChartHint(metrics)

	assert.Equal(t, "How to interpret:", hint.Title)
	assert.GreaterOrEqual(t, len(hint.Items), 5)
}

func TestBuildMainChartHint_NoWarnings(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{
		Trend: TrendData{},
	}

	hint := buildMainChartHint(metrics)

	assert.NotEmpty(t, hint.Items)
}

func TestRegisterPlotSections(t *testing.T) {
	t.Parallel()

	RegisterPlotSections()

	renderer := analyze.StorePlotSectionsFor("sentiment")
	assert.NotNil(t, renderer)
}
