package quality

// FRD: specs/frds/FRD-20260301-all-analyzers-store-based.md.

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

const testAnalyzerID = "history/quality"

func buildTestQualityTicks() []analyze.TICK {
	return []analyze.TICK{
		{
			Tick: 0,
			Data: &TickData{
				CommitQuality: map[string]*TickQuality{
					"000000000000000000000000000000000000000a": {
						Complexities:    []float64{5.0, 10.0},
						Cognitives:      []float64{3.0, 7.0},
						MaxComplexities: []int{8, 12},
						Functions:       []int{4, 6},
						HalsteadVolumes: []float64{100.0, 200.0},
						HalsteadEfforts: []float64{50.0, 100.0},
						DeliveredBugs:   []float64{0.5, 1.0},
						CommentScores:   []float64{0.8, 0.6},
						DocCoverages:    []float64{0.7, 0.5},
						CohesionScores:  []float64{0.9, 0.7},
					},
				},
			},
		},
		{
			Tick: 1,
			Data: &TickData{
				CommitQuality: map[string]*TickQuality{
					"000000000000000000000000000000000000000b": {
						Complexities:    []float64{15.0, 20.0, 25.0},
						Cognitives:      []float64{10.0, 14.0, 18.0},
						MaxComplexities: []int{18, 22, 30},
						Functions:       []int{8, 10, 12},
						HalsteadVolumes: []float64{300.0, 400.0, 500.0},
						HalsteadEfforts: []float64{150.0, 200.0, 250.0},
						DeliveredBugs:   []float64{1.5, 2.0, 2.5},
						CommentScores:   []float64{0.5, 0.4, 0.3},
						DocCoverages:    []float64{0.4, 0.3, 0.2},
						CohesionScores:  []float64{0.6, 0.5, 0.4},
					},
				},
			},
		},
	}
}

func TestWriteToStore_RoundTrip(t *testing.T) {
	t.Parallel()

	ticks := buildTestQualityTicks()

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

	const expectedTickCount = 2

	require.Len(t, timeSeries, expectedTickCount)

	// Should be sorted by tick.
	assert.Equal(t, 0, timeSeries[0].Tick)
	assert.Equal(t, 1, timeSeries[1].Tick)

	// Tick 0: 2 files with complexities [5, 10].
	assert.InDelta(t, 7.5, timeSeries[0].Stats.ComplexityMean, 0.01)
	assert.Equal(t, 2, timeSeries[0].Stats.FilesAnalyzed)

	// Tick 1: 3 files with complexities [15, 20, 25].
	assert.InDelta(t, 20.0, timeSeries[1].Stats.ComplexityMean, 0.01)
	assert.Equal(t, 3, timeSeries[1].Stats.FilesAnalyzed)

	// Verify aggregate record.
	agg, aggErr := readAggregateIfPresent(reader, reader.Kinds())
	require.NoError(t, aggErr)
	assert.Equal(t, expectedTickCount, agg.TotalTicks)
	assert.Equal(t, 5, agg.TotalFilesAnalyzed)
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

	ticks := buildTestQualityTicks()

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
		assert.InDelta(t, refMetrics.TimeSeries[i].Stats.ComplexityMean, storeTimeSeries[i].Stats.ComplexityMean, 0.001)
		assert.InDelta(t, refMetrics.TimeSeries[i].Stats.ComplexityMedian, storeTimeSeries[i].Stats.ComplexityMedian, 0.001)
		assert.InDelta(t, refMetrics.TimeSeries[i].Stats.HalsteadVolMean, storeTimeSeries[i].Stats.HalsteadVolMean, 0.001)
		assert.Equal(t, refMetrics.TimeSeries[i].Stats.FilesAnalyzed, storeTimeSeries[i].Stats.FilesAnalyzed)
	}

	// Compare aggregate.
	storeAgg, aggErr := readAggregateIfPresent(reader, reader.Kinds())
	require.NoError(t, aggErr)

	assert.Equal(t, refMetrics.Aggregate.TotalTicks, storeAgg.TotalTicks)
	assert.Equal(t, refMetrics.Aggregate.TotalFilesAnalyzed, storeAgg.TotalFilesAnalyzed)
	assert.InDelta(t, refMetrics.Aggregate.ComplexityMedianMean, storeAgg.ComplexityMedianMean, 0.001)
	assert.InDelta(t, refMetrics.Aggregate.TotalDeliveredBugs, storeAgg.TotalDeliveredBugs, 0.001)
}

func TestGenerateStoreSections_RoundTrip(t *testing.T) {
	t.Parallel()

	ticks := buildTestQualityTicks()

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

	// Expects complexity chart + Halstead chart + stats grid.
	const expectedSectionCount = 3

	require.Len(t, sections, expectedSectionCount)
	assert.Equal(t, "Cyclomatic Complexity Over Time", sections[0].Title)
	assert.Equal(t, "Halstead Volume Over Time", sections[1].Title)
	assert.Equal(t, "Code Quality Summary", sections[2].Title)
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
