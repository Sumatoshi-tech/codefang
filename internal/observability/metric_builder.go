package observability

import (
	"fmt"

	"go.opentelemetry.io/otel/metric"
)

// metricBuilder accumulates OTel instrument creation errors,
// enabling batch construction with a single error check.
type metricBuilder struct {
	meter metric.Meter
	err   error
}

// newMetricBuilder creates a builder for the given meter.
func newMetricBuilder(mt metric.Meter) *metricBuilder {
	return &metricBuilder{meter: mt}
}

// counter creates an Int64Counter instrument.
func (b *metricBuilder) counter(name, desc, unit string) metric.Int64Counter {
	c, err := b.meter.Int64Counter(name, metric.WithDescription(desc), metric.WithUnit(unit))
	b.setErr(name, err)

	return c
}

// histogram creates a Float64Histogram instrument with optional explicit bucket boundaries.
func (b *metricBuilder) histogram(name, desc, unit string, bounds ...float64) metric.Float64Histogram {
	opts := []metric.Float64HistogramOption{
		metric.WithDescription(desc),
		metric.WithUnit(unit),
	}

	if len(bounds) > 0 {
		opts = append(opts, metric.WithExplicitBucketBoundaries(bounds...))
	}

	h, err := b.meter.Float64Histogram(name, opts...)
	b.setErr(name, err)

	return h
}

// upDownCounter creates an Int64UpDownCounter instrument.
func (b *metricBuilder) upDownCounter(name, desc, unit string) metric.Int64UpDownCounter {
	c, err := b.meter.Int64UpDownCounter(name, metric.WithDescription(desc), metric.WithUnit(unit))
	b.setErr(name, err)

	return c
}

// gauge creates an Int64ObservableGauge instrument.
func (b *metricBuilder) gauge(name, desc, unit string) metric.Int64ObservableGauge {
	g, err := b.meter.Int64ObservableGauge(name, metric.WithDescription(desc), metric.WithUnit(unit))
	b.setErr(name, err)

	return g
}

// observableCounter creates an Int64ObservableCounter instrument.
func (b *metricBuilder) observableCounter(name, desc, unit string) metric.Int64ObservableCounter {
	c, err := b.meter.Int64ObservableCounter(name, metric.WithDescription(desc), metric.WithUnit(unit))
	b.setErr(name, err)

	return c
}

// setErr records the first instrument creation error.
func (b *metricBuilder) setErr(name string, err error) {
	if err != nil && b.err == nil {
		b.err = fmt.Errorf("create %s: %w", name, err)
	}
}
