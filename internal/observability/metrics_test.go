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

func setupTestMeter(t *testing.T) (*observability.REDMetrics, *sdkmetric.ManualReader) {
	t.Helper()

	reader := sdkmetric.NewManualReader()
	mp := sdkmetric.NewMeterProvider(sdkmetric.WithReader(reader))
	meter := mp.Meter("test")

	red, err := observability.NewREDMetrics(meter)
	require.NoError(t, err)

	return red, reader
}

func collectMetrics(t *testing.T, reader *sdkmetric.ManualReader) metricdata.ResourceMetrics {
	t.Helper()

	var rm metricdata.ResourceMetrics

	err := reader.Collect(context.Background(), &rm)
	require.NoError(t, err)

	return rm
}

func findMetric(rm metricdata.ResourceMetrics, name string) *metricdata.Metrics {
	for idx := range rm.ScopeMetrics {
		for midx := range rm.ScopeMetrics[idx].Metrics {
			if rm.ScopeMetrics[idx].Metrics[midx].Name == name {
				return &rm.ScopeMetrics[idx].Metrics[midx]
			}
		}
	}

	return nil
}

func TestREDMetrics_RecordRequest(t *testing.T) {
	t.Parallel()
	red, reader := setupTestMeter(t)
	ctx := context.Background()

	red.RecordRequest(ctx, "analyze", "ok", time.Millisecond*100)

	rm := collectMetrics(t, reader)

	reqTotal := findMetric(rm, "codefang.requests.total")
	require.NotNil(t, reqTotal, "codefang.requests.total metric not found")

	reqDuration := findMetric(rm, "codefang.request.duration.seconds")
	require.NotNil(t, reqDuration, "codefang.request.duration.seconds metric not found")
}

func TestREDMetrics_RecordRequestError(t *testing.T) {
	t.Parallel()
	red, reader := setupTestMeter(t)
	ctx := context.Background()

	red.RecordRequest(ctx, "history", "error", time.Second)

	rm := collectMetrics(t, reader)

	errTotal := findMetric(rm, "codefang.errors.total")
	require.NotNil(t, errTotal, "codefang.errors.total metric not found")
}

func TestREDMetrics_TrackInflight(t *testing.T) {
	t.Parallel()
	red, reader := setupTestMeter(t)
	ctx := context.Background()

	done := red.TrackInflight(ctx, "parse")

	rm := collectMetrics(t, reader)

	inflight := findMetric(rm, "codefang.inflight.requests")
	require.NotNil(t, inflight, "codefang.inflight.requests metric not found")

	done()

	rm = collectMetrics(t, reader)
	inflight = findMetric(rm, "codefang.inflight.requests")
	require.NotNil(t, inflight)
}

func TestREDMetrics_HistogramBuckets_Extended(t *testing.T) {
	t.Parallel()

	red, reader := setupTestMeter(t)
	ctx := context.Background()

	red.RecordRequest(ctx, "cli.run", "ok", time.Second)

	rm := collectMetrics(t, reader)

	reqDuration := findMetric(rm, "codefang.request.duration.seconds")
	require.NotNil(t, reqDuration)

	hist, ok := reqDuration.Data.(metricdata.Histogram[float64])
	require.True(t, ok, "expected Histogram data type")
	require.NotEmpty(t, hist.DataPoints)

	bounds := hist.DataPoints[0].Bounds

	// Verify explicit boundaries match the expected set for long-running analysis.
	expectedBounds := []float64{0.01, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10, 30, 60, 120, 300, 600}
	assert.Equal(t, expectedBounds, bounds, "histogram should use custom bucket boundaries")
}

func TestNewREDMetrics_WithNilMeter(t *testing.T) {
	t.Parallel()
	// Should not panic with a no-op meter.
	cfg := observability.DefaultConfig()

	providers, err := observability.Init(cfg)
	require.NoError(t, err)

	t.Cleanup(func() { require.NoError(t, providers.Shutdown(context.Background())) })

	red, err := observability.NewREDMetrics(providers.Meter)
	require.NoError(t, err)
	assert.NotNil(t, red)

	// Should not panic on recording.
	red.RecordRequest(context.Background(), "test", "ok", time.Millisecond)
}
