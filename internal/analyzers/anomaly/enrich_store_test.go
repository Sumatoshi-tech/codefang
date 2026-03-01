package anomaly

// FRD: specs/frds/FRD-20260301-anomaly-enrich-from-store.md.

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// buildTestStoreForEnrich creates a store with a single analyzer entry.
func buildTestStoreForEnrich(t *testing.T, analyzerID string) string {
	t.Helper()

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)

	meta := analyze.ReportMeta{AnalyzerID: analyzerID}

	w, beginErr := store.Begin(analyzerID, meta)
	require.NoError(t, beginErr)

	// Write a dummy record so the analyzer shows up in AnalyzerIDs().
	require.NoError(t, w.Write("dummy", struct{}{}))
	require.NoError(t, w.Close())
	require.NoError(t, store.Close())

	return storeDir
}

func TestEnrichFromStore_Basic(t *testing.T) {
	t.Parallel()

	storeDir := buildTestStoreForEnrich(t, "test-source")

	storeExtractors := map[string]StoreTimeSeriesExtractor{
		"test-source": func(_ analyze.ReportReader) ([]int, map[string][]float64) {
			return []int{0, 1, 2, 3, 4}, map[string][]float64{
				"metric_a": {1.0, 1.0, 1.0, 1.0, 100.0},
			}
		},
	}

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	const windowSize = 3

	const threshold = 2.0

	allAnomalies, _ := runStoreEnrichment(readStore, windowSize, threshold, storeExtractors)
	assert.NotEmpty(t, allAnomalies)

	// The spike at tick 4 should be detected.
	found := false

	for _, a := range allAnomalies {
		if a.Source == "test-source" && a.Dimension == "metric_a" && a.Tick == 4 {
			found = true

			assert.Greater(t, a.ZScore, threshold)
			assert.InDelta(t, 100.0, a.RawValue, 0.001)
		}
	}

	assert.True(t, found, "expected anomaly at tick 4")
}

func TestEnrichFromStore_Equivalence(t *testing.T) {
	t.Parallel()

	ticks := []int{0, 1, 2, 3, 4, 5}
	dimensions := map[string][]float64{
		"dim_a": {1.0, 1.0, 1.0, 1.0, 50.0, 1.0},
		"dim_b": {5.0, 5.0, 5.0, 5.0, 5.0, 5.0},
	}

	const windowSize = 3

	const threshold = 2.0

	// Expected: compute anomalies directly via detectExternalAnomalies.
	expectedAnomalies, expectedSummaries := detectExternalAnomalies("equiv-src", ticks, dimensions, windowSize, threshold)

	// Store path: enrichment through the store.
	storeExtractors := map[string]StoreTimeSeriesExtractor{
		"equiv-src": func(_ analyze.ReportReader) ([]int, map[string][]float64) {
			return ticks, dimensions
		},
	}

	storeDir := buildTestStoreForEnrich(t, "equiv-src")

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	storeAnomalies, storeSummaries := runStoreEnrichment(readStore, windowSize, threshold, storeExtractors)

	require.Len(t, storeAnomalies, len(expectedAnomalies))

	for i := range expectedAnomalies {
		assert.Equal(t, expectedAnomalies[i].Source, storeAnomalies[i].Source)
		assert.Equal(t, expectedAnomalies[i].Dimension, storeAnomalies[i].Dimension)
		assert.Equal(t, expectedAnomalies[i].Tick, storeAnomalies[i].Tick)
		assert.InDelta(t, expectedAnomalies[i].ZScore, storeAnomalies[i].ZScore, 0.001)
	}

	require.Len(t, storeSummaries, len(expectedSummaries))

	for i := range expectedSummaries {
		assert.Equal(t, expectedSummaries[i].Source, storeSummaries[i].Source)
		assert.Equal(t, expectedSummaries[i].Dimension, storeSummaries[i].Dimension)
		assert.InDelta(t, expectedSummaries[i].Mean, storeSummaries[i].Mean, 0.001)
		assert.InDelta(t, expectedSummaries[i].StdDev, storeSummaries[i].StdDev, 0.001)
		assert.Equal(t, expectedSummaries[i].Anomalies, storeSummaries[i].Anomalies)
	}
}

func TestEnrichFromStore_EmptyStore(t *testing.T) {
	t.Parallel()

	storeExtractors := map[string]StoreTimeSeriesExtractor{
		"nonexistent": func(_ analyze.ReportReader) ([]int, map[string][]float64) {
			return []int{0, 1}, map[string][]float64{"dim": {1.0, 2.0}}
		},
	}

	storeDir := t.TempDir()
	store := analyze.NewFileReportStore(storeDir)
	require.NoError(t, store.Close())

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	allAnomalies, allSummaries := runStoreEnrichment(readStore, 3, 2.0, storeExtractors)
	assert.Empty(t, allAnomalies)
	assert.Empty(t, allSummaries)
}

func TestEnrichFromStore_SkipsAnomalyAnalyzer(t *testing.T) {
	t.Parallel()

	storeExtractors := map[string]StoreTimeSeriesExtractor{
		"anomaly": func(_ analyze.ReportReader) ([]int, map[string][]float64) {
			return []int{0, 1}, map[string][]float64{"dim": {1.0, 100.0}}
		},
	}

	storeDir := buildTestStoreForEnrich(t, "anomaly")

	readStore := analyze.NewFileReportStore(storeDir)
	defer readStore.Close()

	allAnomalies, _ := runStoreEnrichment(readStore, 3, 2.0, storeExtractors)
	assert.Empty(t, allAnomalies, "anomaly analyzer should be skipped during enrichment")
}

func TestEnrichAndRewrite_RoundTrip(t *testing.T) {
	t.Parallel()

	// First, write anomaly data to the store.
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

	// Also write a "test-ext-rw" analyzer so enrichment has data to enrich from.
	RegisterStoreTimeSeriesExtractor("test-ext-rw", func(_ analyze.ReportReader) ([]int, map[string][]float64) {
		return []int{0, 1, 2, 3, 4}, map[string][]float64{
			"churn": {1.0, 1.0, 1.0, 1.0, 100.0},
		}
	})

	extMeta := analyze.ReportMeta{AnalyzerID: "test-ext-rw"}
	ew, ewErr := store.Begin("test-ext-rw", extMeta)
	require.NoError(t, ewErr)
	require.NoError(t, ew.Write("dummy", struct{}{}))
	require.NoError(t, ew.Close())

	// Run enrichment.
	const enrichWindowSize = 3

	const enrichThreshold = 2.0

	enrichErr := EnrichAndRewrite(store, testAnomalyAnalyzerID, enrichWindowSize, enrichThreshold)
	require.NoError(t, enrichErr)

	// Read back and verify enrichment data was written.
	reader, openErr := store.Open(testAnomalyAnalyzerID)
	require.NoError(t, openErr)

	defer reader.Close()

	kinds := reader.Kinds()

	// Original kinds should still be present.
	timeSeries, tsErr := ReadTimeSeriesIfPresent(reader, kinds)
	require.NoError(t, tsErr)

	const expectedTickCount = 2

	require.Len(t, timeSeries, expectedTickCount)

	agg, aggErr := ReadAggregateIfPresent(reader, kinds)
	require.NoError(t, aggErr)
	assert.Equal(t, expectedTickCount, agg.TotalTicks)

	// External anomalies should be present (spike at tick 4 in test-ext-rw).
	externalAnomalies, eaErr := ReadExternalAnomaliesIfPresent(reader, kinds)
	require.NoError(t, eaErr)
	assert.NotEmpty(t, externalAnomalies)

	// External summaries should be present.
	externalSummaries, esErr := ReadExternalSummariesIfPresent(reader, kinds)
	require.NoError(t, esErr)
	assert.NotEmpty(t, externalSummaries)

	// Verify the anomaly was detected from the right source.
	found := false

	for _, ea := range externalAnomalies {
		if ea.Source == "test-ext-rw" && ea.Dimension == "churn" {
			found = true

			break
		}
	}

	assert.True(t, found, "expected external anomaly from test-ext-rw/churn")

	require.NoError(t, store.Close())
}
