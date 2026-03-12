package observability_test

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	sdkmetric "go.opentelemetry.io/otel/sdk/metric"
	"go.opentelemetry.io/otel/sdk/metric/metricdata"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
	"go.opentelemetry.io/otel/sdk/trace/tracetest"

	"github.com/Sumatoshi-tech/codefang/internal/observability"
)

func TestEndToEnd_TraceExported(t *testing.T) {
	t.Parallel()
	// Set up an in-memory span exporter to capture spans.
	exporter := tracetest.NewInMemoryExporter()
	tp := sdktrace.NewTracerProvider(sdktrace.WithSyncer(exporter))

	t.Cleanup(func() { require.NoError(t, tp.Shutdown(context.Background())) })

	tracer := tp.Tracer("codefang")

	// Simulate a pipeline: root span with child phase spans.
	ctx, rootSpan := tracer.Start(context.Background(), "codefang.run")

	_, initSpan := tracer.Start(ctx, "codefang.init")
	initSpan.End()

	_, analysisSpan := tracer.Start(ctx, "codefang.analysis")
	analysisSpan.End()

	_, reportSpan := tracer.Start(ctx, "codefang.report")
	reportSpan.End()

	rootSpan.End()

	// Verify spans were captured.
	spans := exporter.GetSpans()
	require.Len(t, spans, 4)

	// All child spans should share the root's trace ID.
	rootTraceID := spans[3].SpanContext.TraceID()
	for _, span := range spans[:3] {
		assert.Equal(t, rootTraceID, span.SpanContext.TraceID(),
			"child span %q should share root trace ID", span.Name)
	}

	// Verify span names.
	spanNames := make([]string, len(spans))
	for i, span := range spans {
		spanNames[i] = span.Name
	}

	assert.Contains(t, spanNames, "codefang.run")
	assert.Contains(t, spanNames, "codefang.init")
	assert.Contains(t, spanNames, "codefang.analysis")
	assert.Contains(t, spanNames, "codefang.report")

	// Verify parent-child relationship: init/analysis/report have root as parent.
	rootSpanID := spans[3].SpanContext.SpanID()
	for _, span := range spans[:3] {
		assert.Equal(t, rootSpanID, span.Parent.SpanID(),
			"child span %q should have root as parent", span.Name)
	}
}

func TestEndToEnd_MetricsExported(t *testing.T) {
	t.Parallel()
	// Set up an in-memory metric reader.
	reader := sdkmetric.NewManualReader()
	mp := sdkmetric.NewMeterProvider(sdkmetric.WithReader(reader))
	meter := mp.Meter("codefang")

	red, err := observability.NewREDMetrics(meter)
	require.NoError(t, err)

	ctx := context.Background()

	// Simulate a CLI run recording.
	red.RecordRequest(ctx, "cli.run", "ok", time.Second)

	// Simulate a streaming chunk recording.
	red.RecordRequest(ctx, "streaming.chunk", "ok", time.Millisecond*500)

	// Simulate an error.
	red.RecordRequest(ctx, "cli.run", "error", time.Second*2)

	// Collect metrics.
	var rm metricdata.ResourceMetrics

	err = reader.Collect(ctx, &rm)
	require.NoError(t, err)

	// Verify request counter exists and has recordings.
	reqTotal := findMetric(rm, "codefang.requests.total")
	require.NotNil(t, reqTotal, "codefang.requests.total metric not found")

	// Verify duration histogram exists.
	reqDuration := findMetric(rm, "codefang.request.duration.seconds")
	require.NotNil(t, reqDuration, "codefang.request.duration.seconds metric not found")

	// Verify error counter exists.
	errTotal := findMetric(rm, "codefang.errors.total")
	require.NotNil(t, errTotal, "codefang.errors.total metric not found")
}

func TestEndToEnd_MiddlewareProducesSpans(t *testing.T) {
	t.Parallel()
	// Full integration: Init-like setup with in-memory exporter, HTTP
	// middleware creates spans, spans are captured.
	exporter := tracetest.NewInMemoryExporter()
	tp := sdktrace.NewTracerProvider(sdktrace.WithSyncer(exporter))

	t.Cleanup(func() { require.NoError(t, tp.Shutdown(context.Background())) })

	tracer := tp.Tracer("codefang")

	// Wire middleware around a handler that creates a child span.
	inner := http.HandlerFunc(func(rw http.ResponseWriter, hr *http.Request) {
		_, child := tracer.Start(hr.Context(), "codefang.analyze")
		child.End()

		rw.WriteHeader(http.StatusOK)
	})

	mw := observability.HTTPMiddleware(tracer, discardLogger, inner)

	req := httptest.NewRequest(http.MethodPost, "/v1/analyze", http.NoBody)
	rec := httptest.NewRecorder()

	mw.ServeHTTP(rec, req)

	spans := exporter.GetSpans()
	require.Len(t, spans, 2)

	// Verify parent-child: analyze is child of middleware span.
	middlewareSpan := spans[1] // middleware span ends last.
	analyzeSpan := spans[0]

	assert.Equal(t, "POST /v1/analyze", middlewareSpan.Name)
	assert.Equal(t, "codefang.analyze", analyzeSpan.Name)
	assert.Equal(t, middlewareSpan.SpanContext.SpanID(), analyzeSpan.Parent.SpanID())
}
