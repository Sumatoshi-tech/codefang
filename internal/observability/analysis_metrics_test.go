package observability_test

import (
	"context"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	sdkmetric "go.opentelemetry.io/otel/sdk/metric"
	"go.opentelemetry.io/otel/sdk/metric/metricdata"

	"github.com/Sumatoshi-tech/codefang/internal/observability"
)

func setupAnalysisMeter(t *testing.T) (*observability.AnalysisMetrics, *sdkmetric.ManualReader) {
	t.Helper()

	reader := sdkmetric.NewManualReader()
	mp := sdkmetric.NewMeterProvider(sdkmetric.WithReader(reader))
	meter := mp.Meter("test")

	am, err := observability.NewAnalysisMetrics(meter)
	require.NoError(t, err)

	return am, reader
}

func TestNewAnalysisMetrics(t *testing.T) {
	t.Parallel()

	am, _ := setupAnalysisMeter(t)
	assert.NotNil(t, am)
}

func TestAnalysisMetrics_RecordRun(t *testing.T) {
	t.Parallel()

	am, reader := setupAnalysisMeter(t)
	ctx := context.Background()

	am.RecordRun(ctx, observability.AnalysisStats{
		Commits:         100,
		Chunks:          5,
		ChunkDurations:  []time.Duration{time.Second, 2 * time.Second, 3 * time.Second},
		BlobCacheHits:   50,
		BlobCacheMisses: 10,
		DiffCacheHits:   30,
		DiffCacheMisses: 5,
	})

	rm := collectMetrics(t, reader)

	commits := findMetric(rm, "codefang.analysis.commits.total")
	require.NotNil(t, commits, "commits counter should exist")

	chunks := findMetric(rm, "codefang.analysis.chunks.total")
	require.NotNil(t, chunks, "chunks counter should exist")

	chunkDur := findMetric(rm, "codefang.analysis.chunk.duration.seconds")
	require.NotNil(t, chunkDur, "chunk duration histogram should exist")

	// Verify histogram has data points with correct count.
	hist, ok := chunkDur.Data.(metricdata.Histogram[float64])
	require.True(t, ok, "expected Histogram data type")
	require.NotEmpty(t, hist.DataPoints)
	assert.Equal(t, uint64(3), hist.DataPoints[0].Count, "should have 3 duration recordings")

	cacheHits := findMetric(rm, "codefang.analysis.cache.hits.total")
	require.NotNil(t, cacheHits, "cache hits counter should exist")

	cacheMisses := findMetric(rm, "codefang.analysis.cache.misses.total")
	require.NotNil(t, cacheMisses, "cache misses counter should exist")
}

func TestAnalysisMetrics_RecordRun_NilReceiver(t *testing.T) {
	t.Parallel()

	var am *observability.AnalysisMetrics

	// Should not panic.
	am.RecordRun(context.Background(), observability.AnalysisStats{
		Commits: 10,
		Chunks:  1,
	})
}
