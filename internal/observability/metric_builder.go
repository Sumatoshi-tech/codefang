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

// createMetric creates an OTel instrument using fn and records any error in the builder.
// This single generic function replaces the former counter, histogram, upDownCounter,
// gauge, and observableCounter convenience methods.
func createMetric[T any](b *metricBuilder, name string, fn func() (T, error)) T {
	inst, err := fn()
	b.setErr(name, err)

	return inst
}

// buildMetrics constructs a metrics struct by delegating instrument creation to fn.
// It handles builder lifecycle (creation + error check) so callers avoid boilerplate.
func buildMetrics[T any](mt metric.Meter, fn func(*metricBuilder) *T) (*T, error) {
	b := newMetricBuilder(mt)

	result := fn(b)
	if b.err != nil {
		return nil, b.err
	}

	return result, nil
}

// setErr records the first instrument creation error.
func (b *metricBuilder) setErr(name string, err error) {
	if err != nil && b.err == nil {
		b.err = fmt.Errorf("create %s: %w", name, err)
	}
}
