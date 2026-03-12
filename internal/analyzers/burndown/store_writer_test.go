package burndown

// FRD: specs/frds/FRD-20260301-burndown-filehistory-store-writer.md.

import (
	"context"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

const testPeople = 2

var testNames = []string{"alice", "bob"}

// buildTestBurndownAggregator creates an aggregator with known burndown data.
// It returns the aggregator and the PathInterner used to intern file paths.
func buildTestBurndownAggregator(tb testing.TB) (*Aggregator, *PathInterner) {
	tb.Helper()

	pi := NewPathInterner()

	agg := newAggregator(
		analyze.AggregatorOptions{},
		testGranularity, testSampling, testPeople,
		true, // trackFiles.
		defaultTickSizeHours*time.Hour,
		testNames, pi,
	)

	fileA := pi.Intern("a.go")
	fileB := pi.Intern("b.go")

	// Simulate commits at ticks 0 and 30.
	require.NoError(tb, agg.Add(analyze.TC{
		Data: &CommitResult{
			GlobalDeltas: sparseHistory{
				0: {0: 100},
			},
			PeopleDeltas: map[int]sparseHistory{
				0: {0: {0: 100}},
			},
			MatrixDeltas: []map[int]int64{
				{authorSelf: 100},
			},
			FileDeltas: map[PathID]sparseHistory{
				fileA: {0: {0: 60}},
				fileB: {0: {0: 40}},
			},
			FileOwnership: map[PathID]map[int]int{
				fileA: {0: 60},
				fileB: {0: 40},
			},
		},
	}))

	require.NoError(tb, agg.Add(analyze.TC{
		Data: &CommitResult{
			GlobalDeltas: sparseHistory{
				testSampling: {0: -10, testSampling: 50},
			},
			PeopleDeltas: map[int]sparseHistory{
				1: {testSampling: {0: -10, testSampling: 50}},
			},
			MatrixDeltas: []map[int]int64{
				nil,
				{authorSelf: 50, 0: -10},
			},
			FileDeltas: map[PathID]sparseHistory{
				fileA: {testSampling: {0: -5, testSampling: 30}},
			},
			FileOwnership: map[PathID]map[int]int{
				fileA: {0: 55, 1: 30},
				fileB: {0: 40},
			},
		},
	}))

	agg.lastTick = testSampling
	agg.endTime = time.Date(2025, 1, 1, 0, 0, 0, 0, time.UTC)

	return agg, pi
}

func TestWriteToStoreFromAggregator_RoundTrip(t *testing.T) {
	t.Parallel()

	agg, _ := buildTestBurndownAggregator(t)
	defer agg.Close()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &HistoryAnalyzer{}
	analyzer.Granularity = testGranularity
	analyzer.Sampling = testSampling

	meta := analyze.ReportMeta{AnalyzerID: "burndown"}
	w, beginErr := store.Begin("history/burndown", meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStoreFromAggregator(ctx, agg, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	// Read back.
	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open("history/burndown")
	require.NoError(t, openErr)

	defer reader.Close()

	// Verify chart_data record exists.
	chartData, chartErr := readChartDataIfPresent(reader, reader.Kinds())
	require.NoError(t, chartErr)
	require.NotNil(t, chartData)
	assert.NotEmpty(t, chartData.GlobalHistory)
	assert.Equal(t, testSampling, chartData.Sampling)
	assert.Equal(t, testGranularity, chartData.Granularity)

	// Verify metrics record exists.
	metrics, metricsErr := readMetricsIfPresent(reader, reader.Kinds())
	require.NoError(t, metricsErr)
	require.NotNil(t, metrics)
	assert.Positive(t, metrics.Aggregate.TotalCurrentLines)
	assert.Positive(t, metrics.Aggregate.TotalPeakLines)
}

func TestWriteToStoreFromAggregator_WrongType(t *testing.T) {
	t.Parallel()

	analyzer := &HistoryAnalyzer{}
	ctx := context.Background()
	err := analyzer.WriteToStoreFromAggregator(ctx, &mockAggregator{}, nil)
	require.ErrorIs(t, err, ErrUnexpectedAggregator)
}

func TestWriteToStoreFromAggregator_EmptyAggregator(t *testing.T) {
	t.Parallel()

	agg := newAggregator(
		analyze.AggregatorOptions{},
		testGranularity, testSampling, 0,
		false, defaultTickSizeHours*time.Hour,
		nil, NewPathInterner(),
	)
	defer agg.Close()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &HistoryAnalyzer{}
	analyzer.Granularity = testGranularity
	analyzer.Sampling = testSampling

	meta := analyze.ReportMeta{AnalyzerID: "burndown"}
	w, beginErr := store.Begin("history/burndown", meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStoreFromAggregator(ctx, agg, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open("history/burndown")
	require.NoError(t, openErr)

	defer reader.Close()

	// Chart data should exist but have empty global history.
	chartData, chartErr := readChartDataIfPresent(reader, reader.Kinds())
	require.NoError(t, chartErr)
	require.NotNil(t, chartData)
	assert.Empty(t, chartData.GlobalHistory)
}

func TestWriteToStoreFromAggregator_MetricsEquivalence(t *testing.T) {
	t.Parallel()

	agg, pi := buildTestBurndownAggregator(t)
	defer agg.Close()

	// Reference path: FlushAllTicks → ticksToReport → ComputeAllMetrics.
	ticks, flushErr := agg.FlushAllTicks()
	require.NoError(t, flushErr)

	ctx := context.Background()
	refReport := ticksToReport(ctx, ticks,
		testGranularity, testSampling, testPeople,
		true, defaultTickSizeHours*time.Hour,
		testNames, pi,
	)

	refMetrics, metricsErr := ComputeAllMetrics(refReport)
	require.NoError(t, metricsErr)

	// Store path.
	agg2, _ := buildTestBurndownAggregator(t)
	defer agg2.Close()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &HistoryAnalyzer{}
	analyzer.Granularity = testGranularity
	analyzer.Sampling = testSampling

	meta := analyze.ReportMeta{AnalyzerID: "burndown"}
	w, beginErr := store.Begin("history/burndown", meta)
	require.NoError(t, beginErr)

	writeErr := analyzer.WriteToStoreFromAggregator(ctx, agg2, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open("history/burndown")
	require.NoError(t, openErr)

	defer reader.Close()

	storeMetrics, storeErr := readMetricsIfPresent(reader, reader.Kinds())
	require.NoError(t, storeErr)
	require.NotNil(t, storeMetrics)

	// Compare aggregate.
	assert.Equal(t, refMetrics.Aggregate.TotalCurrentLines, storeMetrics.Aggregate.TotalCurrentLines)
	assert.Equal(t, refMetrics.Aggregate.TotalPeakLines, storeMetrics.Aggregate.TotalPeakLines)
	assert.InDelta(t, refMetrics.Aggregate.OverallSurvivalRate, storeMetrics.Aggregate.OverallSurvivalRate, 0.01)

	// Compare global survival length.
	assert.Len(t, storeMetrics.GlobalSurvival, len(refMetrics.GlobalSurvival))
}

func TestGenerateStoreSections_RoundTrip(t *testing.T) {
	t.Parallel()

	agg, _ := buildTestBurndownAggregator(t)
	defer agg.Close()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := &HistoryAnalyzer{}
	analyzer.Granularity = testGranularity
	analyzer.Sampling = testSampling

	meta := analyze.ReportMeta{AnalyzerID: "burndown"}
	w, beginErr := store.Begin("history/burndown", meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStoreFromAggregator(ctx, agg, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open("history/burndown")
	require.NoError(t, openErr)

	defer reader.Close()

	sections, secErr := GenerateStoreSections(reader)
	require.NoError(t, secErr)

	// Should have 2 sections: summary + chart.
	const expectedSectionCount = 2

	require.Len(t, sections, expectedSectionCount)

	titles := make([]string, len(sections))
	for i, s := range sections {
		titles[i] = s.Title
	}

	assert.Contains(t, titles, "Burndown Summary")
	assert.Contains(t, titles, "Code Burndown Chart")
}

// mockAggregator satisfies analyze.Aggregator but is not *burndown.Aggregator.
type mockAggregator struct{}

func (m *mockAggregator) Add(_ analyze.TC) error { return nil }

func (m *mockAggregator) FlushTick(_ int) (analyze.TICK, error) { return analyze.TICK{}, nil }

func (m *mockAggregator) FlushAllTicks() ([]analyze.TICK, error) { return nil, nil }

func (m *mockAggregator) DiscardState() {}

func (m *mockAggregator) Spill() (int64, error) { return 0, nil }

func (m *mockAggregator) Collect() error { return nil }

func (m *mockAggregator) EstimatedStateSize() int64 { return 0 }

func (m *mockAggregator) SpillState() analyze.AggregatorSpillInfo {
	return analyze.AggregatorSpillInfo{}
}

func (m *mockAggregator) RestoreSpillState(_ analyze.AggregatorSpillInfo) {}

func (m *mockAggregator) Close() error { return nil }
