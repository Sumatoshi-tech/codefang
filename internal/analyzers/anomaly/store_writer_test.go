package anomaly

// FRD: specs/frds/FRD-20260301-all-analyzers-store-based.md.

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

const testAnomalyAnalyzerID = "history/anomaly"

func buildTestAnomalyTicks() []analyze.TICK {
	return []analyze.TICK{
		{
			Tick: 0,
			Data: &TickData{
				CommitMetrics: map[string]*CommitAnomalyData{
					testHashA: {
						FilesChanged: 5, LinesAdded: 20, LinesRemoved: 10, NetChurn: 10,
						Files: []string{"main.go"}, Languages: map[string]int{"Go": 3}, AuthorID: 0,
					},
				},
			},
		},
		{
			Tick: 1,
			Data: &TickData{
				CommitMetrics: map[string]*CommitAnomalyData{
					testHashB: {
						FilesChanged: 3, LinesAdded: 15, LinesRemoved: 8, NetChurn: 7,
						Files: []string{"util.go"}, Languages: map[string]int{"Go": 2, "Python": 1}, AuthorID: 1,
					},
				},
			},
		},
	}
}

func newStoreTestAnalyzer(commitsByTick map[int][]gitlib.Hash) *Analyzer {
	a := NewAnalyzer()
	a.Threshold = DefaultAnomalyThreshold
	a.WindowSize = DefaultAnomalyWindowSize
	a.commitsByTick = commitsByTick

	return a
}

func TestWriteToStore_RoundTrip(t *testing.T) {
	t.Parallel()

	ticks := buildTestAnomalyTicks()

	commitsByTick := map[int][]gitlib.Hash{
		0: {gitlib.NewHash(testHashA)},
		1: {gitlib.NewHash(testHashB)},
	}

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := newStoreTestAnalyzer(commitsByTick)

	meta := analyze.ReportMeta{AnalyzerID: testAnomalyAnalyzerID}
	w, beginErr := store.Begin(testAnomalyAnalyzerID, meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStore(ctx, ticks, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	// Read back.
	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(testAnomalyAnalyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	kinds := reader.Kinds()

	// Verify time_series records.
	timeSeries, tsErr := ReadTimeSeriesIfPresent(reader, kinds)
	require.NoError(t, tsErr)

	const expectedTickCount = 2

	require.Len(t, timeSeries, expectedTickCount)

	// Should be sorted by tick.
	assert.Equal(t, 0, timeSeries[0].Tick)
	assert.Equal(t, 1, timeSeries[1].Tick)

	// Verify aggregate record.
	agg, aggErr := ReadAggregateIfPresent(reader, kinds)
	require.NoError(t, aggErr)
	assert.Equal(t, expectedTickCount, agg.TotalTicks)
}

func TestWriteToStore_EmptyTicks(t *testing.T) {
	t.Parallel()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := newStoreTestAnalyzer(nil)

	meta := analyze.ReportMeta{AnalyzerID: testAnomalyAnalyzerID}
	w, beginErr := store.Begin(testAnomalyAnalyzerID, meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStore(ctx, nil, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(testAnomalyAnalyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	timeSeries, tsErr := ReadTimeSeriesIfPresent(reader, reader.Kinds())
	require.NoError(t, tsErr)
	assert.Empty(t, timeSeries)
}

func TestWriteToStore_EquivalenceReference(t *testing.T) {
	t.Parallel()

	ticks := buildTestAnomalyTicks()

	commitsByTick := map[int][]gitlib.Hash{
		0: {gitlib.NewHash(testHashA)},
		1: {gitlib.NewHash(testHashB)},
	}

	// Reference path: ticksToReport → ComputeAllMetrics.
	ctx := context.Background()
	refReport := ticksToReport(ctx, ticks, DefaultAnomalyThreshold, DefaultAnomalyWindowSize, commitsByTick)
	refMetrics, metricsErr := ComputeAllMetrics(refReport)
	require.NoError(t, metricsErr)

	// Store path.
	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := newStoreTestAnalyzer(commitsByTick)

	meta := analyze.ReportMeta{AnalyzerID: testAnomalyAnalyzerID}
	w, beginErr := store.Begin(testAnomalyAnalyzerID, meta)
	require.NoError(t, beginErr)

	writeErr := analyzer.WriteToStore(ctx, ticks, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(testAnomalyAnalyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	kinds := reader.Kinds()

	storeTimeSeries, tsErr := ReadTimeSeriesIfPresent(reader, kinds)
	require.NoError(t, tsErr)

	require.Len(t, storeTimeSeries, len(refMetrics.TimeSeries))

	for i := range refMetrics.TimeSeries {
		assert.Equal(t, refMetrics.TimeSeries[i].Tick, storeTimeSeries[i].Tick)
		assert.Equal(t, refMetrics.TimeSeries[i].IsAnomaly, storeTimeSeries[i].IsAnomaly)
		assert.InDelta(t, refMetrics.TimeSeries[i].ChurnZScore, storeTimeSeries[i].ChurnZScore, 0.001)
	}

	// Compare anomaly records.
	storeAnomalies, anomErr := ReadAnomaliesIfPresent(reader, kinds)
	require.NoError(t, anomErr)

	require.Len(t, storeAnomalies, len(refMetrics.Anomalies))

	for i := range refMetrics.Anomalies {
		assert.Equal(t, refMetrics.Anomalies[i].Tick, storeAnomalies[i].Tick)
		assert.InDelta(t, refMetrics.Anomalies[i].MaxAbsZScore, storeAnomalies[i].MaxAbsZScore, 0.001)
	}

	// Compare aggregate.
	storeAgg, aggErr := ReadAggregateIfPresent(reader, kinds)
	require.NoError(t, aggErr)

	assert.Equal(t, refMetrics.Aggregate.TotalTicks, storeAgg.TotalTicks)
	assert.Equal(t, refMetrics.Aggregate.TotalAnomalies, storeAgg.TotalAnomalies)
	assert.InDelta(t, refMetrics.Aggregate.AnomalyRate, storeAgg.AnomalyRate, 0.001)
}

func TestGenerateStoreSections_RoundTrip(t *testing.T) {
	t.Parallel()

	ticks := buildTestAnomalyTicks()

	commitsByTick := map[int][]gitlib.Hash{
		0: {gitlib.NewHash(testHashA)},
		1: {gitlib.NewHash(testHashB)},
	}

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	analyzer := newStoreTestAnalyzer(commitsByTick)

	meta := analyze.ReportMeta{AnalyzerID: testAnomalyAnalyzerID}
	w, beginErr := store.Begin(testAnomalyAnalyzerID, meta)
	require.NoError(t, beginErr)

	ctx := context.Background()
	writeErr := analyzer.WriteToStore(ctx, ticks, w)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(testAnomalyAnalyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	sections, secErr := GenerateStoreSections(reader)
	require.NoError(t, secErr)

	// Expects main chart + stats section (no external anomalies).
	const expectedSectionCount = 2

	require.Len(t, sections, expectedSectionCount)
	assert.Equal(t, "Net Churn Over Time with Anomalies", sections[0].Title)
	assert.Equal(t, "Anomaly Detection Summary", sections[1].Title)
}

func TestGenerateStoreSections_EmptyStore(t *testing.T) {
	t.Parallel()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	meta := analyze.ReportMeta{AnalyzerID: testAnomalyAnalyzerID}
	w, beginErr := store.Begin(testAnomalyAnalyzerID, meta)
	require.NoError(t, beginErr)
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	reader, openErr := readStore.Open(testAnomalyAnalyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	sections, secErr := GenerateStoreSections(reader)
	require.NoError(t, secErr)
	assert.Empty(t, sections)
}
