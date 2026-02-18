package observability

import (
	"context"
	"fmt"
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
	reqTotal, err := mt.Int64Counter(metricRequestsTotal,
		metric.WithDescription("Total number of requests"),
		metric.WithUnit("{request}"),
	)
	if err != nil {
		return nil, fmt.Errorf("create %s: %w", metricRequestsTotal, err)
	}

	reqDuration, err := mt.Float64Histogram(metricRequestDuration,
		metric.WithDescription("Request duration in seconds"),
		metric.WithUnit("s"),
		metric.WithExplicitBucketBoundaries(durationBucketBoundaries...),
	)
	if err != nil {
		return nil, fmt.Errorf("create %s: %w", metricRequestDuration, err)
	}

	errTotal, err := mt.Int64Counter(metricErrorsTotal,
		metric.WithDescription("Total number of errors"),
		metric.WithUnit("{error}"),
	)
	if err != nil {
		return nil, fmt.Errorf("create %s: %w", metricErrorsTotal, err)
	}

	inflight, err := mt.Int64UpDownCounter(metricInflightRequests,
		metric.WithDescription("Number of in-flight requests"),
		metric.WithUnit("{request}"),
	)
	if err != nil {
		return nil, fmt.Errorf("create %s: %w", metricInflightRequests, err)
	}

	return &REDMetrics{
		requestsTotal:    reqTotal,
		requestDuration:  reqDuration,
		errorsTotal:      errTotal,
		inflightRequests: inflight,
	}, nil
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
