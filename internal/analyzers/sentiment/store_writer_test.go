package sentiment

// FRD: specs/frds/FRD-20260301-all-analyzers-store-based.md.

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

func buildTestSentimentTicks() []analyze.TICK {
	return []analyze.TICK{
		{
			Tick: 0,
			Data: &TickData{
				CommentsByCommit: map[string][]string{
					"000000000000000000000000000000000000000a": {
						"This is a wonderful improvement to the codebase",
						"Great work on this feature",
					},
					"000000000000000000000000000000000000000b": {"Fixed a terrible bug that was causing crashes everywhere"},
				},
			},
		},
		{
			Tick: 1,
			Data: &TickData{
				CommentsByCommit: map[string][]string{
					"000000000000000000000000000000000000000c": {"This is an absolutely amazing refactor of the core module"},
				},
			},
		},
	}
}

const testAnalyzerID = "history/sentiment"

func TestWriteToStore_RoundTrip(t *testing.T) {
	t.Parallel()

	ticks := buildTestSentimentTicks()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &Analyzer{}

	meta := analyze.ReportMeta{AnalyzerID: testAnalyzerID}
	w, beginErr := store.Begin(testAnalyzerID, meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStore(ctx, ticks, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	// Read back.
	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(testAnalyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	// Verify time_series records.
	timeSeries, tsErr := readTimeSeriesIfPresent(reader, reader.Kinds())
	require.NoError(t, tsErr)

	// 2 ticks of data.
	const expectedTickCount = 2

	require.Len(t, timeSeries, expectedTickCount)

	// Should be sorted by tick.
	assert.Equal(t, 0, timeSeries[0].Tick)
	assert.Equal(t, 1, timeSeries[1].Tick)

	// Sentiment scores should be in [0, 1].
	for _, ts := range timeSeries {
		assert.GreaterOrEqual(t, ts.Sentiment, float32(0))
		assert.LessOrEqual(t, ts.Sentiment, float32(1))
	}

	// Verify trend record.
	trend, trendErr := readTrendIfPresent(reader, reader.Kinds())
	require.NoError(t, trendErr)
	assert.Equal(t, 0, trend.StartTick)
	assert.Equal(t, 1, trend.EndTick)

	// Verify aggregate record.
	agg, aggErr := readAggregateIfPresent(reader, reader.Kinds())
	require.NoError(t, aggErr)
	assert.Equal(t, expectedTickCount, agg.TotalTicks)
	assert.Positive(t, agg.TotalComments)
}

func TestWriteToStore_EmptyTicks(t *testing.T) {
	t.Parallel()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &Analyzer{}

	meta := analyze.ReportMeta{AnalyzerID: testAnalyzerID}
	w, beginErr := store.Begin(testAnalyzerID, meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStore(ctx, nil, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(testAnalyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	timeSeries, tsErr := readTimeSeriesIfPresent(reader, reader.Kinds())
	require.NoError(t, tsErr)
	assert.Empty(t, timeSeries)
}

func TestWriteToStore_EquivalenceReference(t *testing.T) {
	t.Parallel()

	ticks := buildTestSentimentTicks()

	// Reference path: ticksToReport → ComputeAllMetrics.
	ctx := context.Background()
	refReport := ticksToReport(ctx, ticks, nil)
	refMetrics, metricsErr := ComputeAllMetrics(refReport)
	require.NoError(t, metricsErr)

	// Store path.
	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &Analyzer{}

	meta := analyze.ReportMeta{AnalyzerID: testAnalyzerID}
	w, beginErr := store.Begin(testAnalyzerID, meta)
	require.NoError(t, beginErr)

	writeErr := analyzer.WriteToStore(ctx, ticks, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(testAnalyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	storeTimeSeries, tsErr := readTimeSeriesIfPresent(reader, reader.Kinds())
	require.NoError(t, tsErr)

	require.Len(t, storeTimeSeries, len(refMetrics.TimeSeries))

	for i := range refMetrics.TimeSeries {
		assert.Equal(t, refMetrics.TimeSeries[i].Tick, storeTimeSeries[i].Tick)
		assert.InDelta(t, refMetrics.TimeSeries[i].Sentiment, storeTimeSeries[i].Sentiment, 0.001)
		assert.Equal(t, refMetrics.TimeSeries[i].CommentCount, storeTimeSeries[i].CommentCount)
		assert.Equal(t, refMetrics.TimeSeries[i].Classification, storeTimeSeries[i].Classification)
	}

	// Compare aggregate.
	storeAgg, aggErr := readAggregateIfPresent(reader, reader.Kinds())
	require.NoError(t, aggErr)

	assert.Equal(t, refMetrics.Aggregate.TotalTicks, storeAgg.TotalTicks)
	assert.Equal(t, refMetrics.Aggregate.TotalComments, storeAgg.TotalComments)
	assert.InDelta(t, refMetrics.Aggregate.AverageSentiment, storeAgg.AverageSentiment, 0.001)
}

func TestGenerateStoreSections_RoundTrip(t *testing.T) {
	t.Parallel()

	ticks := buildTestSentimentTicks()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &Analyzer{}

	meta := analyze.ReportMeta{AnalyzerID: testAnalyzerID}
	w, beginErr := store.Begin(testAnalyzerID, meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStore(ctx, ticks, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(testAnalyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	sections, secErr := GenerateStoreSections(reader)
	require.NoError(t, secErr)

	// Expects main chart + distribution chart.
	const expectedSectionCount = 2

	require.Len(t, sections, expectedSectionCount)
	assert.Equal(t, chartSectionTitle, sections[0].Title)
	assert.Equal(t, distributionTitle, sections[1].Title)
}

func TestGenerateStoreSections_EmptyStore(t *testing.T) {
	t.Parallel()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	meta := analyze.ReportMeta{AnalyzerID: testAnalyzerID}
	w, beginErr := store.Begin(testAnalyzerID, meta)
	require.NoError(t, beginErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(testAnalyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	sections, secErr := GenerateStoreSections(reader)
	require.NoError(t, secErr)
	assert.Empty(t, sections)
}
