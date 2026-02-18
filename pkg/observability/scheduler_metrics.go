package observability

import (
	"context"
	"fmt"
	"math"
	runtimemetrics "runtime/metrics"

	"go.opentelemetry.io/otel/metric"
)

const (
	metricGoroutines        = "codefang.runtime.goroutines"
	metricThreads           = "codefang.runtime.threads"
	metricGoroutinesCreated = "codefang.runtime.goroutines.created"

	// runtime/metrics sample names (Go 1.26+).
	sampleGoroutines        = "/sched/goroutines:goroutines"
	sampleThreads           = "/sched/threads:threads"
	sampleGoroutinesCreated = "/sched/goroutines-created:goroutines"
)

// SchedulerMetrics exposes Go runtime scheduler metrics as OTel instruments.
// Goroutine and thread counts are read from runtime/metrics on each collection cycle.
type SchedulerMetrics struct {
	goroutines        metric.Int64ObservableGauge
	threads           metric.Int64ObservableGauge
	goroutinesCreated metric.Int64ObservableCounter
}

// NewSchedulerMetrics creates OTel instruments backed by Go 1.26 runtime/metrics.
// The meter's periodic reader invokes the callback automatically; no manual polling is needed.
func NewSchedulerMetrics(mt metric.Meter) (*SchedulerMetrics, error) {
	goroutines, err := mt.Int64ObservableGauge(metricGoroutines,
		metric.WithDescription("Current number of live goroutines"),
		metric.WithUnit("{goroutine}"),
	)
	if err != nil {
		return nil, fmt.Errorf("create %s: %w", metricGoroutines, err)
	}

	threads, err := mt.Int64ObservableGauge(metricThreads,
		metric.WithDescription("Current number of OS threads created by the Go runtime"),
		metric.WithUnit("{thread}"),
	)
	if err != nil {
		return nil, fmt.Errorf("create %s: %w", metricThreads, err)
	}

	created, err := mt.Int64ObservableCounter(metricGoroutinesCreated,
		metric.WithDescription("Total goroutines created since process start"),
		metric.WithUnit("{goroutine}"),
	)
	if err != nil {
		return nil, fmt.Errorf("create %s: %w", metricGoroutinesCreated, err)
	}

	sm := &SchedulerMetrics{
		goroutines:        goroutines,
		threads:           threads,
		goroutinesCreated: created,
	}

	_, err = mt.RegisterCallback(sm.observe, goroutines, threads, created)
	if err != nil {
		return nil, fmt.Errorf("register scheduler metrics callback: %w", err)
	}

	return sm, nil
}

// observe reads runtime/metrics samples and reports them to the OTel observer.
func (sm *SchedulerMetrics) observe(_ context.Context, obs metric.Observer) error {
	samples := []runtimemetrics.Sample{
		{Name: sampleGoroutines},
		{Name: sampleThreads},
		{Name: sampleGoroutinesCreated},
	}

	runtimemetrics.Read(samples)

	for idx := range samples {
		val, ok := sampleInt64Value(samples[idx].Value)
		if !ok {
			continue
		}

		switch samples[idx].Name {
		case sampleGoroutines:
			obs.ObserveInt64(sm.goroutines, val)
		case sampleThreads:
			obs.ObserveInt64(sm.threads, val)
		case sampleGoroutinesCreated:
			obs.ObserveInt64(sm.goroutinesCreated, val)
		}
	}

	return nil
}

// sampleInt64Value extracts an int64 from a runtime/metrics value,
// handling both Uint64 and Float64 kinds.
func sampleInt64Value(val runtimemetrics.Value) (int64, bool) {
	switch val.Kind() {
	case runtimemetrics.KindUint64:
		u := val.Uint64()
		if u > uint64(math.MaxInt64) {
			return math.MaxInt64, true
		}

		return int64(u), true
	case runtimemetrics.KindFloat64:
		return int64(val.Float64()), true
	case runtimemetrics.KindBad, runtimemetrics.KindFloat64Histogram:
		return 0, false
	default:
		return 0, false
	}
}
