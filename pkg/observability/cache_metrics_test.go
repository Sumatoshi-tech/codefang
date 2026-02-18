package observability_test

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	sdkmetric "go.opentelemetry.io/otel/sdk/metric"
	"go.opentelemetry.io/otel/sdk/metric/metricdata"

	"github.com/Sumatoshi-tech/codefang/pkg/observability"
)

// stubCacheStats implements observability.CacheStatsProvider for testing.
type stubCacheStats struct {
	hits   int64
	misses int64
}

func (s *stubCacheStats) CacheHits() int64   { return s.hits }
func (s *stubCacheStats) CacheMisses() int64 { return s.misses }

func TestCacheMetrics_Exported(t *testing.T) {
	t.Parallel()

	reader := sdkmetric.NewManualReader()
	mp := sdkmetric.NewMeterProvider(sdkmetric.WithReader(reader))
	meter := mp.Meter("test")

	blob := &stubCacheStats{hits: 10, misses: 3}
	diff := &stubCacheStats{hits: 7, misses: 5}

	err := observability.RegisterCacheMetrics(meter, blob, diff)
	require.NoError(t, err)

	var rm metricdata.ResourceMetrics

	err = reader.Collect(context.Background(), &rm)
	require.NoError(t, err)

	hits := findMetric(rm, "codefang.cache.hits")
	require.NotNil(t, hits, "codefang.cache.hits metric not found")

	misses := findMetric(rm, "codefang.cache.misses")
	require.NotNil(t, misses, "codefang.cache.misses metric not found")

	// Verify blob and diff data points are present.
	hitsGauge, ok := hits.Data.(metricdata.Gauge[int64])
	require.True(t, ok, "expected Gauge data type for hits")

	hitsMap := dataPointsByAttr(hitsGauge.DataPoints)
	assert.Equal(t, int64(10), hitsMap["blob"])
	assert.Equal(t, int64(7), hitsMap["diff"])

	missesGauge, ok := misses.Data.(metricdata.Gauge[int64])
	require.True(t, ok, "expected Gauge data type for misses")

	missesMap := dataPointsByAttr(missesGauge.DataPoints)
	assert.Equal(t, int64(3), missesMap["blob"])
	assert.Equal(t, int64(5), missesMap["diff"])
}

func TestCacheMetrics_NilProviders(t *testing.T) {
	t.Parallel()

	reader := sdkmetric.NewManualReader()
	mp := sdkmetric.NewMeterProvider(sdkmetric.WithReader(reader))
	meter := mp.Meter("test")

	// Should not error with nil providers.
	err := observability.RegisterCacheMetrics(meter, nil, nil)
	require.NoError(t, err)
}

// dataPointsByAttr extracts data points keyed by the "cache" attribute value.
func dataPointsByAttr(dps []metricdata.DataPoint[int64]) map[string]int64 {
	m := make(map[string]int64, len(dps))

	for _, dp := range dps {
		for _, attr := range dp.Attributes.ToSlice() {
			if string(attr.Key) == "cache" {
				m[attr.Value.AsString()] = dp.Value
			}
		}
	}

	return m
}
