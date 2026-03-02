package observability

// FRD: specs/frds/FRD-20260302-otel-metric-helper.md.

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

func TestMetricBuilder_Counter(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	c := b.counter(testMetricName, testMetricDesc, testMetricUnit)
	require.NoError(t, b.err)
	assert.NotNil(t, c)
}

func TestMetricBuilder_Histogram(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	h := b.histogram(testMetricName, testMetricDesc, "s", durationBucketBoundaries...)
	require.NoError(t, b.err)
	assert.NotNil(t, h)
}

func TestMetricBuilder_Histogram_NoBounds(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	h := b.histogram(testMetricName, testMetricDesc, testMetricUnit)
	require.NoError(t, b.err)
	assert.NotNil(t, h)
}

func TestMetricBuilder_UpDownCounter(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	c := b.upDownCounter(testMetricName, testMetricDesc, testMetricUnit)
	require.NoError(t, b.err)
	assert.NotNil(t, c)
}

func TestMetricBuilder_Gauge(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	g := b.gauge(testMetricName, testMetricDesc, testMetricUnit)
	require.NoError(t, b.err)
	assert.NotNil(t, g)
}

func TestMetricBuilder_ObservableCounter(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	c := b.observableCounter(testMetricName, testMetricDesc, testMetricUnit)
	require.NoError(t, b.err)
	assert.NotNil(t, c)
}

func TestMetricBuilder_ErrorAccumulation_CapturesFirst(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	b.setErr("first.metric", errTestCreation)

	require.Error(t, b.err)
	require.ErrorIs(t, b.err, errTestCreation)
	assert.Contains(t, b.err.Error(), "first.metric")
}

func TestMetricBuilder_ErrorAccumulation_IgnoresSubsequent(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	b.setErr("first.metric", errTestCreation)
	b.setErr("second.metric", errTestSecond)

	// Only the first error is retained.
	require.ErrorIs(t, b.err, errTestCreation)
	assert.NotErrorIs(t, b.err, errTestSecond)
}

func TestMetricBuilder_SetErr_NilError(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	b.setErr("no.problem", nil)
	assert.NoError(t, b.err)
}

func TestMetricBuilder_AllInstruments(t *testing.T) {
	t.Parallel()

	b := newMetricBuilder(testMeter())

	c := b.counter("test.counter", "counter desc", "{count}")
	h := b.histogram("test.histogram", "histogram desc", "ms")
	u := b.upDownCounter("test.updown", "updown desc", "{req}")
	g := b.gauge("test.gauge", "gauge desc", "{goroutine}")
	o := b.observableCounter("test.obs", "obs desc", "{goroutine}")

	require.NoError(t, b.err)
	assert.NotNil(t, c)
	assert.NotNil(t, h)
	assert.NotNil(t, u)
	assert.NotNil(t, g)
	assert.NotNil(t, o)
}
