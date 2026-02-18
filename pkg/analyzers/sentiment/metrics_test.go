package sentiment

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// Test constants to avoid magic strings/numbers.
const (
	testComment1 = "This is a great fix!"
	testComment2 = "Why is this so broken?"
	testComment3 = "Normal comment"

	testSentimentPositive = 0.8
	testSentimentNeutral  = 0.5
	testSentimentNegative = 0.3
	testSentimentVeryLow  = 0.1

	floatDelta = 0.01

	testHashA = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
	testHashB = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
)

// Helper function to create test hash.
func testHash(s string) gitlib.Hash {
	var h gitlib.Hash
	copy(h[:], s)

	return h
}

// --- ParseReportData Tests ---.

func TestParseReportData_Empty(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	assert.Empty(t, result.EmotionsByTick)
	assert.Empty(t, result.CommentsByTick)
	assert.Empty(t, result.CommitsByTick)
}

func TestParseReportData_AllFields(t *testing.T) {
	t.Parallel()

	emotions := map[int]float32{0: testSentimentPositive, 1: testSentimentNegative}
	comments := map[int][]string{0: {testComment1}, 1: {testComment2}}
	commits := map[int][]gitlib.Hash{0: {testHash("abc")}, 1: {testHash("def")}}

	report := analyze.Report{
		"emotions_by_tick": emotions,
		"comments_by_tick": comments,
		"commits_by_tick":  commits,
	}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	require.Len(t, result.EmotionsByTick, 2)
	require.Len(t, result.CommentsByTick, 2)
	require.Len(t, result.CommitsByTick, 2)

	assert.InDelta(t, testSentimentPositive, result.EmotionsByTick[0], floatDelta)
	assert.Equal(t, []string{testComment1}, result.CommentsByTick[0])
}

// --- classifySentiment Tests ---.

func TestClassifySentiment(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name      string
		sentiment float32
		expected  string
	}{
		{"positive_high", 0.9, "positive"},
		{"positive_boundary", 0.6, "positive"},
		{"neutral_upper", 0.59, "neutral"},
		{"neutral_middle", 0.5, "neutral"},
		{"neutral_lower", 0.41, "neutral"},
		{"negative_boundary", 0.4, "negative"},
		{"negative_low", 0.2, "negative"},
		{"negative_zero", 0.0, "negative"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			result := classifySentiment(tt.sentiment)
			assert.Equal(t, tt.expected, result)
		})
	}
}

// --- SentimentTimeSeriesMetric Tests ---.

func TestNewTimeSeriesMetric_Metadata(t *testing.T) {
	t.Parallel()

	m := NewTimeSeriesMetric()

	assert.Equal(t, "sentiment_time_series", m.Name())
	assert.Equal(t, "Sentiment Time Series", m.DisplayName())
	assert.Contains(t, m.Description(), "Time series")
	assert.Equal(t, "time_series", m.Type())
}

func TestSentimentTimeSeriesMetric_Empty(t *testing.T) {
	t.Parallel()

	m := NewTimeSeriesMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestSentimentTimeSeriesMetric_SingleTick(t *testing.T) {
	t.Parallel()

	m := NewTimeSeriesMetric()
	input := &ReportData{
		EmotionsByTick: map[int]float32{0: testSentimentPositive},
		CommentsByTick: map[int][]string{0: {testComment1, testComment2}},
		CommitsByTick:  map[int][]gitlib.Hash{0: {testHash("abc"), testHash("def")}},
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, 0, result[0].Tick)
	assert.InDelta(t, testSentimentPositive, result[0].Sentiment, floatDelta)
	assert.Equal(t, 2, result[0].CommentCount)
	assert.Equal(t, 2, result[0].CommitCount)
	assert.Equal(t, "positive", result[0].Classification)
}

func TestSentimentTimeSeriesMetric_MultipleTicks_SortedByTick(t *testing.T) {
	t.Parallel()

	m := NewTimeSeriesMetric()
	input := &ReportData{
		EmotionsByTick: map[int]float32{
			2: testSentimentNeutral,
			0: testSentimentPositive,
			1: testSentimentNegative,
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 3)
	// Sorted by tick.
	assert.Equal(t, 0, result[0].Tick)
	assert.Equal(t, 1, result[1].Tick)
	assert.Equal(t, 2, result[2].Tick)

	// Classifications.
	assert.Equal(t, "positive", result[0].Classification)
	assert.Equal(t, "negative", result[1].Classification)
	assert.Equal(t, "neutral", result[2].Classification)
}

func TestSentimentTimeSeriesMetric_MissingCommmentsAndCommits(t *testing.T) {
	t.Parallel()

	m := NewTimeSeriesMetric()
	input := &ReportData{
		EmotionsByTick: map[int]float32{0: testSentimentNeutral},
		// No comments or commits for tick 0.
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, 0, result[0].CommentCount)
	assert.Equal(t, 0, result[0].CommitCount)
}

// --- SentimentTrendMetric Tests ---.

func TestNewTrendMetric_Metadata(t *testing.T) {
	t.Parallel()

	m := NewTrendMetric()

	assert.Equal(t, "sentiment_trend", m.Name())
	assert.Equal(t, "Sentiment Trend", m.DisplayName())
	assert.Contains(t, m.Description(), "Overall trend")
	assert.Equal(t, "aggregate", m.Type())
}

func TestSentimentTrendMetric_Empty(t *testing.T) {
	t.Parallel()

	m := NewTrendMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Equal(t, 0, result.StartTick)
	assert.Equal(t, 0, result.EndTick)
	assert.Empty(t, result.TrendDirection)
}

func TestSentimentTrendMetric_SingleTick(t *testing.T) {
	t.Parallel()

	m := NewTrendMetric()
	input := &ReportData{
		EmotionsByTick: map[int]float32{0: testSentimentNeutral},
	}

	result := m.Compute(input)

	assert.Equal(t, 0, result.StartTick)
	assert.Equal(t, 0, result.EndTick)
	assert.Equal(t, "stable", result.TrendDirection)
}

func TestSentimentTrendMetric_TrendDirections(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name           string
		startSentiment float32
		endSentiment   float32
		expectedTrend  string
	}{
		{"improving", 0.3, 0.8, "improving"},
		{"declining", 0.8, 0.3, "declining"},
		{"stable_same", 0.5, 0.5, "stable"},
		{"stable_small_change", 0.5, 0.55, "stable"}, // Within 0.1 threshold.
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			m := NewTrendMetric()
			input := &ReportData{
				EmotionsByTick: map[int]float32{
					0: tt.startSentiment,
					5: tt.endSentiment,
				},
			}

			result := m.Compute(input)

			assert.Equal(t, tt.expectedTrend, result.TrendDirection)
			assert.InDelta(t, tt.startSentiment, result.StartSentiment, floatDelta)
			assert.InDelta(t, tt.endSentiment, result.EndSentiment, floatDelta)
		})
	}
}

func TestSentimentTrendMetric_ChangePercent(t *testing.T) {
	t.Parallel()

	m := NewTrendMetric()
	input := &ReportData{
		EmotionsByTick: map[int]float32{
			0: 0.5,
			1: 0.75, // 50% increase.
		},
	}

	result := m.Compute(input)

	// Change = (0.75 - 0.5) / 0.5 * 100 = 50%.
	assert.InDelta(t, 50.0, result.ChangePercent, floatDelta)
}

func TestSentimentTrendMetric_ZeroStartSentiment(t *testing.T) {
	t.Parallel()

	m := NewTrendMetric()
	input := &ReportData{
		EmotionsByTick: map[int]float32{
			0: 0.0,
			1: 0.5,
		},
	}

	result := m.Compute(input)

	// Change percent should be 0 when start is 0 (avoid division by zero).
	assert.InDelta(t, 0.0, result.ChangePercent, floatDelta)
}

// --- LowSentimentPeriodMetric Tests ---.

func TestNewLowSentimentPeriodMetric_Metadata(t *testing.T) {
	t.Parallel()

	m := NewLowSentimentPeriodMetric()

	assert.Equal(t, "low_sentiment_periods", m.Name())
	assert.Equal(t, "Low Sentiment Periods", m.DisplayName())
	assert.Contains(t, m.Description(), "negative or low sentiment")
	assert.Equal(t, "risk", m.Type())
}

func TestLowSentimentPeriodMetric_Empty(t *testing.T) {
	t.Parallel()

	m := NewLowSentimentPeriodMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestLowSentimentPeriodMetric_NoLowSentiment(t *testing.T) {
	t.Parallel()

	m := NewLowSentimentPeriodMetric()
	input := &ReportData{
		EmotionsByTick: map[int]float32{
			0: testSentimentPositive,
			1: testSentimentNeutral,
		},
	}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestLowSentimentPeriodMetric_RiskLevels(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name         string
		sentiment    float32
		expectedRisk string
	}{
		{"high_risk", 0.1, "HIGH"},
		{"high_risk_boundary", 0.2, "HIGH"},
		{"medium_risk", 0.3, "MEDIUM"},
		{"medium_risk_boundary", 0.4, "MEDIUM"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			m := NewLowSentimentPeriodMetric()
			input := &ReportData{
				EmotionsByTick: map[int]float32{0: tt.sentiment},
				CommentsByTick: map[int][]string{0: {testComment2}},
			}

			result := m.Compute(input)

			require.Len(t, result, 1)
			assert.InDelta(t, tt.sentiment, result[0].Sentiment, floatDelta)
			assert.Equal(t, tt.expectedRisk, result[0].RiskLevel)
			assert.Equal(t, []string{testComment2}, result[0].Comments)
		})
	}
}

func TestLowSentimentPeriodMetric_SortedBySentiment(t *testing.T) {
	t.Parallel()

	m := NewLowSentimentPeriodMetric()
	input := &ReportData{
		EmotionsByTick: map[int]float32{
			0: 0.3, // MEDIUM.
			1: 0.1, // HIGH - worst.
			2: 0.2, // HIGH.
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 3)
	// Sorted by sentiment ascending (worst first).
	assert.InDelta(t, 0.1, result[0].Sentiment, floatDelta)
	assert.InDelta(t, 0.2, result[1].Sentiment, floatDelta)
	assert.InDelta(t, 0.3, result[2].Sentiment, floatDelta)
}

// --- SentimentAggregateMetric Tests ---.

func TestNewAggregateMetric_Metadata(t *testing.T) {
	t.Parallel()

	m := NewAggregateMetric()

	assert.Equal(t, "sentiment_aggregate", m.Name())
	assert.Equal(t, "Sentiment Summary", m.DisplayName())
	assert.Contains(t, m.Description(), "Aggregate sentiment statistics")
	assert.Equal(t, "aggregate", m.Type())
}

func TestSentimentAggregateMetric_Empty(t *testing.T) {
	t.Parallel()

	m := NewAggregateMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Equal(t, 0, result.TotalTicks)
	assert.Equal(t, 0, result.TotalComments)
	assert.Equal(t, 0, result.TotalCommits)
	assert.InDelta(t, 0.0, result.AverageSentiment, floatDelta)
	assert.Equal(t, 0, result.PositiveTicks)
	assert.Equal(t, 0, result.NeutralTicks)
	assert.Equal(t, 0, result.NegativeTicks)
}

func TestSentimentAggregateMetric_AllClassifications(t *testing.T) {
	t.Parallel()

	m := NewAggregateMetric()
	input := &ReportData{
		EmotionsByTick: map[int]float32{
			0: testSentimentPositive, // positive.
			1: testSentimentNeutral,  // neutral.
			2: testSentimentNegative, // negative.
		},
		CommentsByTick: map[int][]string{
			0: {testComment1, testComment2},
			1: {testComment3},
		},
		CommitsByTick: map[int][]gitlib.Hash{
			0: {testHash("abc")},
			2: {testHash("def"), testHash("ghi")},
		},
	}

	result := m.Compute(input)

	assert.Equal(t, 3, result.TotalTicks)
	assert.Equal(t, 3, result.TotalComments) // 2 + 1 + 0
	assert.Equal(t, 3, result.TotalCommits)  // 1 + 0 + 2

	// Average = (0.8 + 0.5 + 0.3) / 3 = 0.533...
	expectedAvg := (testSentimentPositive + testSentimentNeutral + testSentimentNegative) / 3.0
	assert.InDelta(t, expectedAvg, result.AverageSentiment, floatDelta)

	assert.Equal(t, 1, result.PositiveTicks)
	assert.Equal(t, 1, result.NeutralTicks)
	assert.Equal(t, 1, result.NegativeTicks)
}

// --- AggregateCommitsToTicks Tests ---.

func TestAggregateCommitsToTicks_SingleCommitPerTick(t *testing.T) {
	t.Parallel()

	commentsByCommit := map[string][]string{
		testHashA: {"comment 1", "comment 2"},
		testHashB: {"comment 3"},
	}
	commitsByTick := map[int][]gitlib.Hash{
		0: {gitlib.NewHash(testHashA)},
		1: {gitlib.NewHash(testHashB)},
	}

	cbt, ebt := AggregateCommitsToTicks(commentsByCommit, commitsByTick)

	require.Len(t, cbt, 2)
	assert.Len(t, cbt[0], 2)
	assert.Len(t, cbt[1], 1)
	assert.InDelta(t, mockSentimentValue, ebt[0], floatDelta)
	assert.InDelta(t, mockSentimentValue, ebt[1], floatDelta)
}

func TestAggregateCommitsToTicks_MultipleCommitsPerTick(t *testing.T) {
	t.Parallel()

	commentsByCommit := map[string][]string{
		testHashA: {"comment 1"},
		testHashB: {"comment 2", "comment 3"},
	}
	commitsByTick := map[int][]gitlib.Hash{
		0: {gitlib.NewHash(testHashA), gitlib.NewHash(testHashB)},
	}

	cbt, ebt := AggregateCommitsToTicks(commentsByCommit, commitsByTick)

	require.Len(t, cbt, 1)
	assert.Len(t, cbt[0], 3)
	assert.InDelta(t, mockSentimentValue, ebt[0], floatDelta)
}

func TestAggregateCommitsToTicks_Empty(t *testing.T) {
	t.Parallel()

	cbt, ebt := AggregateCommitsToTicks(nil, nil)

	assert.Nil(t, cbt)
	assert.Nil(t, ebt)
}

func TestComputeAllMetrics_FromCommitData(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"comments_by_commit": map[string][]string{
			testHashA: {"good work on this", "nice refactor here"},
			testHashB: {"this code is broken"},
		},
		"commits_by_tick": map[int][]gitlib.Hash{
			0: {gitlib.NewHash(testHashA)},
			1: {gitlib.NewHash(testHashB)},
		},
	}

	result, err := ComputeAllMetrics(report)
	require.NoError(t, err)

	assert.Len(t, result.TimeSeries, 2)
	assert.Equal(t, 2, result.Aggregate.TotalTicks)
	assert.Equal(t, 3, result.Aggregate.TotalComments)
}

// --- ComputeAllMetrics Tests ---.

func TestComputeAllMetrics_Empty(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)
	assert.Empty(t, result.TimeSeries)
	assert.Empty(t, result.LowSentimentPeriods)
	assert.Empty(t, result.Trend.TrendDirection)
	assert.Equal(t, 0, result.Aggregate.TotalTicks)
}

// --- MetricsOutput Interface Tests ---.

func TestComputedMetrics_AnalyzerName(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{}

	name := metrics.AnalyzerName()

	assert.Equal(t, "sentiment", name)
}

func TestComputedMetrics_ToJSON(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{
		Aggregate: AggregateData{
			TotalTicks:       5,
			AverageSentiment: 0.6,
		},
	}

	result := metrics.ToJSON()

	assert.Equal(t, metrics, result)
}

func TestComputedMetrics_ToYAML(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{
		Aggregate: AggregateData{
			TotalTicks:       5,
			AverageSentiment: 0.6,
		},
	}

	result := metrics.ToYAML()

	assert.Equal(t, metrics, result)
}

func TestComputeAllMetrics_Full(t *testing.T) {
	t.Parallel()

	emotions := map[int]float32{
		0: testSentimentPositive,
		1: testSentimentNegative,
		2: testSentimentNeutral,
	}
	comments := map[int][]string{
		0: {testComment1},
		1: {testComment2},
	}
	commits := map[int][]gitlib.Hash{
		0: {testHash("abc")},
	}

	report := analyze.Report{
		"emotions_by_tick": emotions,
		"comments_by_tick": comments,
		"commits_by_tick":  commits,
	}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)

	// TimeSeries.
	require.Len(t, result.TimeSeries, 3)

	// Trend.
	assert.Equal(t, 0, result.Trend.StartTick)
	assert.Equal(t, 2, result.Trend.EndTick)
	assert.Equal(t, "declining", result.Trend.TrendDirection) // 0.8 -> 0.5

	// LowSentimentPeriods - only tick 1 with 0.3 sentiment.
	require.Len(t, result.LowSentimentPeriods, 1)
	assert.Equal(t, 1, result.LowSentimentPeriods[0].Tick)

	// Aggregate.
	assert.Equal(t, 3, result.Aggregate.TotalTicks)
	assert.Equal(t, 1, result.Aggregate.PositiveTicks)
	assert.Equal(t, 1, result.Aggregate.NeutralTicks)
	assert.Equal(t, 1, result.Aggregate.NegativeTicks)
}
