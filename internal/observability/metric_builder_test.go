package observability

import (
	"errors"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"go.opentelemetry.io/otel/metric"
	noopmetric "go.opentelemetry.io/otel/metric/noop"
)

const (
	testMetricName = "test.metric"
	testMetricDesc = "A test metric"
	testMetricUnit = "{item}"
)

// Sentinel errors for testing error accumulation.
var (
	errTestCreation = errors.New("test: creation failed")
	errTestSecond   = errors.New("second error")
)

func testMeter() metric.Meter {
	return noopmetric.NewMeterProvider().Meter("test")
}

func TestCreateMetric_Counter(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	c := createMetric(b, testMetricName, func() (metric.Int64Counter, error) {
		return b.meter.Int64Counter(testMetricName,
			metric.WithDescription(testMetricDesc), metric.WithUnit(testMetricUnit))
	})

	require.NoError(t, b.err)
	assert.NotNil(t, c)
}

func TestCreateMetric_Histogram(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	h := createMetric(b, testMetricName, func() (metric.Float64Histogram, error) {
		return b.meter.Float64Histogram(testMetricName,
			metric.WithDescription(testMetricDesc),
			metric.WithUnit("s"),
			metric.WithExplicitBucketBoundaries(durationBucketBoundaries...))
	})

	require.NoError(t, b.err)
	assert.NotNil(t, h)
}

func TestCreateMetric_Histogram_NoBounds(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	h := createMetric(b, testMetricName, func() (metric.Float64Histogram, error) {
		return b.meter.Float64Histogram(testMetricName,
			metric.WithDescription(testMetricDesc), metric.WithUnit(testMetricUnit))
	})

	require.NoError(t, b.err)
	assert.NotNil(t, h)
}

func TestCreateMetric_UpDownCounter(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	c := createMetric(b, testMetricName, func() (metric.Int64UpDownCounter, error) {
		return b.meter.Int64UpDownCounter(testMetricName,
			metric.WithDescription(testMetricDesc), metric.WithUnit(testMetricUnit))
	})

	require.NoError(t, b.err)
	assert.NotNil(t, c)
}

func TestCreateMetric_Gauge(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	g := createMetric(b, testMetricName, func() (metric.Int64ObservableGauge, error) {
		return b.meter.Int64ObservableGauge(testMetricName,
			metric.WithDescription(testMetricDesc), metric.WithUnit(testMetricUnit))
	})

	require.NoError(t, b.err)
	assert.NotNil(t, g)
}

func TestCreateMetric_ObservableCounter(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	c := createMetric(b, testMetricName, func() (metric.Int64ObservableCounter, error) {
		return b.meter.Int64ObservableCounter(testMetricName,
			metric.WithDescription(testMetricDesc), metric.WithUnit(testMetricUnit))
	})

	require.NoError(t, b.err)
	assert.NotNil(t, c)
}

func TestCreateMetric_ErrorAccumulation_CapturesFirst(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	b.setErr("first.metric", errTestCreation)

	require.Error(t, b.err)
	require.ErrorIs(t, b.err, errTestCreation)
	assert.Contains(t, b.err.Error(), "first.metric")
}

func TestCreateMetric_ErrorAccumulation_IgnoresSubsequent(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	b.setErr("first.metric", errTestCreation)
	b.setErr("second.metric", errTestSecond)

	// Only the first error is retained.
	require.ErrorIs(t, b.err, errTestCreation)
	assert.NotErrorIs(t, b.err, errTestSecond)
}

func TestCreateMetric_SetErr_NilError(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	b.setErr("no.problem", nil)
	assert.NoError(t, b.err)
}

func TestCreateMetric_AllInstruments(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	c := createMetric(b, "test.counter", func() (metric.Int64Counter, error) {
		return b.meter.Int64Counter("test.counter",
			metric.WithDescription("counter desc"), metric.WithUnit("{count}"))
	})
	h := createMetric(b, "test.histogram", func() (metric.Float64Histogram, error) {
		return b.meter.Float64Histogram("test.histogram",
			metric.WithDescription("histogram desc"), metric.WithUnit("ms"))
	})
	u := createMetric(b, "test.updown", func() (metric.Int64UpDownCounter, error) {
		return b.meter.Int64UpDownCounter("test.updown",
			metric.WithDescription("updown desc"), metric.WithUnit("{req}"))
	})
	g := createMetric(b, "test.gauge", func() (metric.Int64ObservableGauge, error) {
		return b.meter.Int64ObservableGauge("test.gauge",
			metric.WithDescription("gauge desc"), metric.WithUnit("{goroutine}"))
	})
	o := createMetric(b, "test.obs", func() (metric.Int64ObservableCounter, error) {
		return b.meter.Int64ObservableCounter("test.obs",
			metric.WithDescription("obs desc"), metric.WithUnit("{goroutine}"))
	})

	require.NoError(t, b.err)
	assert.NotNil(t, c)
	assert.NotNil(t, h)
	assert.NotNil(t, u)
	assert.NotNil(t, g)
	assert.NotNil(t, o)
}

func TestBuildMetrics_Success(t *testing.T) {
	t.Parallel()

	type testMetrics struct {
		counter metric.Int64Counter
	}

	result, err := buildMetrics(testMeter(), func(b *metricBuilder) *testMetrics {
		return &testMetrics{
			counter: createMetric(b, testMetricName, func() (metric.Int64Counter, error) {
				return b.meter.Int64Counter(testMetricName,
					metric.WithDescription(testMetricDesc), metric.WithUnit(testMetricUnit))
			}),
		}
	})

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.NotNil(t, result.counter)
}

func TestBuildMetrics_PropagatesError(t *testing.T) {
	t.Parallel()

	type emptyMetrics struct{}

	result, err := buildMetrics(testMeter(), func(b *metricBuilder) *emptyMetrics {
		b.setErr("forced.failure", errTestCreation)

		return &emptyMetrics{}
	})

	require.Error(t, err)
	require.ErrorIs(t, err, errTestCreation)
	assert.Nil(t, result)
}
