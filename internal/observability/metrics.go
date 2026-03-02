package observability

import (
	"context"
	"time"

	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/metric"
)

const (
	metricRequestsTotal    = "codefang.requests.total"
	metricRequestDuration  = "codefang.request.duration.seconds"
	metricErrorsTotal      = "codefang.errors.total"
	metricInflightRequests = "codefang.inflight.requests"

	attrOp     = "op"
	attrStatus = "status"

	statusError = "error"
)

// durationBucketBoundaries covers 10ms to 600s for analysis workloads
// that range from sub-second static checks to multi-minute history pipelines.
var durationBucketBoundaries = []float64{0.01, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10, 30, 60, 120, 300, 600}

// REDMetrics holds the OTel instruments for Rate, Error, Duration metrics.
type REDMetrics struct {
	requestsTotal    metric.Int64Counter
	requestDuration  metric.Float64Histogram
	errorsTotal      metric.Int64Counter
	inflightRequests metric.Int64UpDownCounter
}

// NewREDMetrics creates RED metric instruments from the given meter.
func NewREDMetrics(mt metric.Meter) (*REDMetrics, error) {
	b := newMetricBuilder(mt)

	rm := &REDMetrics{
		requestsTotal:    b.counter(metricRequestsTotal, "Total number of requests", "{request}"),
		requestDuration:  b.histogram(metricRequestDuration, "Request duration in seconds", "s", durationBucketBoundaries...),
		errorsTotal:      b.counter(metricErrorsTotal, "Total number of errors", "{error}"),
		inflightRequests: b.upDownCounter(metricInflightRequests, "Number of in-flight requests", "{request}"),
	}

	if b.err != nil {
		return nil, b.err
	}

	return rm, nil
}

// RecordRequest records a completed request with its operation, status, and duration.
func (rm *REDMetrics) RecordRequest(ctx context.Context, op, status string, duration time.Duration) {
	attrs := metric.WithAttributes(
		attribute.String(attrOp, op),
		attribute.String(attrStatus, status),
	)

	rm.requestsTotal.Add(ctx, 1, attrs)
	rm.requestDuration.Record(ctx, duration.Seconds(), attrs)

	if status == statusError {
		rm.errorsTotal.Add(ctx, 1, metric.WithAttributes(
			attribute.String(attrOp, op),
		))
	}
}

// TrackInflight increments the in-flight gauge and returns a function to decrement it.
func (rm *REDMetrics) TrackInflight(ctx context.Context, op string) func() {
	attrs := metric.WithAttributes(attribute.String(attrOp, op))
	rm.inflightRequests.Add(ctx, 1, attrs)

	return func() {
		rm.inflightRequests.Add(ctx, -1, attrs)
	}
}
